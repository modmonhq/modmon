//! A service that grows and shrinks with load.
//!
//! The whole configuration, next to a service's `placement`:
//!
//! ```yaml
//! autoscale:
//!   scale:
//!     min: 1
//!     max: 4
//!   metric:
//!     type: cpu
//!     up:
//!       greater: 75%
//!       for: 30s
//!     down:
//!       less: 40%
//!       for: 5m
//! ```
//!
//! Only `scale.max` is required. `scale.min` defaults to the service's
//! `replicas`, and the metric to the values above.
//!
//! There is no scheduler and no consensus. Every node that the service is
//! placed on computes the same decider: the first node of the placement
//! that answers. The decider publishes how many containers the service
//! wants ([`Want`]) in the records the nodes already exchange, and every
//! node derives its own share of that number with [`share`] and converges
//! its own engine one container at a time ([`node_step`]).

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::config::DurationSpec;
use crate::domain::metrics::{Metric, Percent, RETAINED, Window};

#[path = "autoscale/decide.rs"]
mod decide;
#[path = "autoscale/report.rs"]
mod report;

pub use decide::{
    Decision, Moment, NodeStep, OwnContainer, cluster_tally, decide, decider, newest_want,
    node_step, settled_everywhere, share,
};
pub use report::{ScaleChange, ScaleReport, ScaledService, ServiceScale, Want, service_key};

/// The CPU share above which a container is added, when `up.greater` is
/// not set.
pub const DEFAULT_UP: Percent = Percent::from_whole(75);
/// How long the up condition holds before a container is added.
pub const DEFAULT_UP_FOR: Duration = Duration::from_secs(30);
/// The CPU share below which a container is removed.
pub const DEFAULT_DOWN: Percent = Percent::from_whole(40);
/// How long the down condition holds before a container is removed.
pub const DEFAULT_DOWN_FOR: Duration = Duration::from_secs(5 * 60);
/// The most containers of one service on one node: a cluster network has
/// addresses for 253 containers per node.
pub const MOST_PER_NODE: u32 = 253;

/// `autoscale:` on a service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AutoscaleConfig {
    /// How few and how many containers the service runs.
    pub scale: ScaleRange,
    /// What is measured and when a container is added or removed. Every
    /// key has a default: CPU, up above 75% for 30s, down below 40% for 5m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<ScaleMetricConfig>,
}

/// `autoscale.scale`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ScaleRange {
    /// The fewest containers; defaults to the service's `replicas`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<u32>,
    /// The most containers, over all the service's nodes.
    pub max: u32,
}

/// `autoscale.metric`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ScaleMetricConfig {
    /// `cpu` (the default) or `memory`: a share of the container's limit,
    /// or of one core (CPU) or of the node's memory without one.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Metric>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up: Option<UpRuleConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub down: Option<DownRuleConfig>,
}

/// `autoscale.metric.up`: add a container when the average is above
/// `greater` for `for`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct UpRuleConfig {
    /// Such as `75%`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub greater: Option<PercentSpec>,
    /// Such as `30s`; at most `4h`.
    #[serde(rename = "for", default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<DurationSpec>,
}

/// `autoscale.metric.down`: remove a container when the average is below
/// `less` for `for`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DownRuleConfig {
    /// Such as `40%`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub less: Option<PercentSpec>,
    /// Such as `5m`; at most `4h`.
    #[serde(rename = "for", default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<DurationSpec>,
}

/// A percentage as written in mm.yaml: `75%`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct PercentSpec(pub String);

impl JsonSchema for PercentSpec {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PercentSpec".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "A percentage, such as 75%.",
            "pattern": "^[0-9]+(\\.[0-9]{1,2})?%$"
        })
    }
}

impl<'de> Deserialize<'de> for PercentSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Written {
            Text(String),
            Number(f64),
        }
        match Written::deserialize(deserializer)? {
            Written::Text(text) => Ok(Self(text)),
            Written::Number(number) => Err(serde::de::Error::custom(format!(
                "{number} is a number; write it as a percentage, such as `{number}%`"
            ))),
        }
    }
}

impl PercentSpec {
    /// The share it names, above 0% and at most 655.34%.
    pub fn parse(&self) -> Result<Percent, String> {
        let text = self.0.trim();
        let number = text
            .strip_suffix('%')
            .filter(|number| {
                !number.is_empty()
                    && number
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || byte == b'.')
            })
            .and_then(|number| number.parse::<f64>().ok())
            .ok_or_else(|| format!("is '{}'; write a percentage, such as 75%", self.0))?;
        if number <= 0.0 || number > f64::from(Percent::MAX) {
            return Err(format!(
                "is {text}; it is a share above 0% and at most {}",
                Percent::MAX
            ));
        }
        Percent::try_from(number)
    }
}

/// One rule, resolved: the share and how long it has to hold, in whole
/// steps of the metrics store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Rule {
    pub threshold: Percent,
    pub hold_seconds: u64,
}

impl Rule {
    #[must_use]
    pub const fn hold(self) -> Duration {
        Duration::from_secs(self.hold_seconds)
    }
}

/// A service's autoscale settings with every default filled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ScalePolicy {
    pub min: u32,
    pub max: u32,
    /// What the service runs before anything scaled it: `replicas` when
    /// it is set, else `min`.
    pub initial: u32,
    pub metric: Metric,
    pub up: Rule,
    pub down: Rule,
}

/// A problem with a service's `replicas` or `autoscale`, with the key to
/// change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaleProblem {
    pub key: &'static str,
    pub message: String,
}

impl ScaleProblem {
    fn new(key: &'static str, message: String) -> Self {
        Self { key, message }
    }
}

impl std::fmt::Display for ScaleProblem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "`{}` {}", self.key, self.message)
    }
}

/// How many containers a service runs over its nodes when nothing scales
/// it: `replicas`, or one on each of its `nodes`.
pub fn replicas(replicas: Option<u32>, nodes: usize) -> Result<u32, ScaleProblem> {
    let default = u32::try_from(nodes).unwrap_or(u32::MAX).max(1);
    match replicas {
        None => Ok(default),
        Some(0) => Err(ScaleProblem::new(
            "replicas",
            "is 0; a service runs at least one container. To run it nowhere, leave this environment out of its `environments:`".to_owned(),
        )),
        Some(count) => Ok(count),
    }
}

impl ScalePolicy {
    /// The policy of `config` for a service that sets `replicas` (or not)
    /// and is placed on `nodes` nodes. Every problem names its key.
    pub fn resolve(
        config: &AutoscaleConfig,
        replicas: Option<u32>,
        nodes: usize,
    ) -> Result<Self, Vec<ScaleProblem>> {
        let mut problems = Vec::new();
        let base = match self::replicas(replicas, nodes) {
            Ok(base) => base,
            Err(problem) => {
                problems.push(problem);
                1
            }
        };
        let (min, max) = range(config, replicas, base, nodes, &mut problems);
        let metric = config.metric.clone().unwrap_or_default();
        let up = metric.up.unwrap_or_default();
        let down = metric.down.unwrap_or_default();
        let up = rule(
            (
                "autoscale.metric.up.greater",
                up.greater.as_ref(),
                DEFAULT_UP,
            ),
            ("autoscale.metric.up.for", up.hold.as_ref(), DEFAULT_UP_FOR),
            &mut problems,
        );
        let down = rule(
            (
                "autoscale.metric.down.less",
                down.less.as_ref(),
                DEFAULT_DOWN,
            ),
            (
                "autoscale.metric.down.for",
                down.hold.as_ref(),
                DEFAULT_DOWN_FOR,
            ),
            &mut problems,
        );
        if down.threshold >= up.threshold {
            problems.push(ScaleProblem::new(
                "autoscale.metric.down.less",
                format!(
                    "is {}, not below `autoscale.metric.up.greater` ({}): the service would add and remove containers in turn. Lower `down.less` or raise `up.greater`",
                    down.threshold, up.threshold
                ),
            ));
        }
        if !problems.is_empty() {
            return Err(problems);
        }
        Ok(Self {
            min,
            max,
            initial: replicas.unwrap_or(min),
            metric: metric.kind.unwrap_or(Metric::Cpu),
            up,
            down,
        })
    }

    /// A count brought into `min..=max`.
    #[must_use]
    pub fn bounded(&self, count: u32) -> u32 {
        count.clamp(self.min, self.max.max(self.min))
    }
}

/// `scale.min` and `scale.max`, checked against each other and `replicas`.
fn range(
    config: &AutoscaleConfig,
    replicas: Option<u32>,
    base: u32,
    nodes: usize,
    problems: &mut Vec<ScaleProblem>,
) -> (u32, u32) {
    let max = config.scale.max;
    let min = config.scale.min.unwrap_or(base);
    if max == 0 {
        problems.push(ScaleProblem::new(
            "autoscale.scale.max",
            "is 0; a service runs at least one container: set it to 1 or more".to_owned(),
        ));
    }
    if config.scale.min == Some(0) {
        problems.push(ScaleProblem::new(
            "autoscale.scale.min",
            "is 0; autoscale measures the containers the service runs, so it keeps at least one: set it to 1 or more".to_owned(),
        ));
    }
    if max > 0 && min > max {
        problems.push(match config.scale.min {
            Some(_) => ScaleProblem::new(
                "autoscale.scale.min",
                format!("is {min}, above `autoscale.scale.max` ({max}); lower `min` or raise `max`"),
            ),
            None => ScaleProblem::new(
                "autoscale.scale.min",
                format!(
                    "is not set, so it is the service's `replicas`, {min} here{}, above `autoscale.scale.max` ({max}). Set `autoscale.scale.min` (or `replicas`) to at most {max}, or raise `max`",
                    if replicas.is_none() { " (one container on each placement node)" } else { "" }
                ),
            ),
        });
    }
    if let (Some(count), Some(_)) = (replicas, config.scale.min)
        && (count < min || count > max)
    {
        problems.push(ScaleProblem::new(
            "replicas",
            format!(
                "is {count}, outside `autoscale.scale` {min}..{max}; set `replicas` within it, or drop it: the service then starts at `min`"
            ),
        ));
    }
    let nodes = u32::try_from(nodes).unwrap_or(u32::MAX).max(1);
    if max.div_ceil(nodes) > MOST_PER_NODE {
        problems.push(ScaleProblem::new(
            "autoscale.scale.max",
            format!(
                "is {max}, which puts up to {} containers on one of the service's {nodes} node(s); a node holds at most {MOST_PER_NODE} of one service. Lower `max` or place the service on more nodes",
                max.div_ceil(nodes)
            ),
        ));
    }
    (min, max)
}

/// One rule from its two keys, each with its default.
fn rule(
    (share_key, share, default_share): (&'static str, Option<&PercentSpec>, Percent),
    (hold_key, hold, default_hold): (&'static str, Option<&DurationSpec>, Duration),
    problems: &mut Vec<ScaleProblem>,
) -> Rule {
    let threshold = match share.map(PercentSpec::parse).transpose() {
        Ok(share) => share.unwrap_or(default_share),
        Err(message) => {
            problems.push(ScaleProblem::new(share_key, message));
            default_share
        }
    };
    let hold = match hold.map(|spec| (spec, spec.parse())) {
        None => default_hold,
        Some((_, Ok(hold))) if hold > RETAINED => {
            problems.push(ScaleProblem::new(
                hold_key,
                format!(
                    "is longer than 4h, the longest `for:`: mmd keeps four hours of measurements ({} s here). Set it to 4h or less",
                    hold.as_secs()
                ),
            ));
            default_hold
        }
        Some((spec, Ok(hold))) if hold.is_zero() => {
            problems.push(ScaleProblem::new(
                hold_key,
                format!(
                    "is {}; give the condition time to hold, such as 30s",
                    spec.0
                ),
            ));
            default_hold
        }
        Some((_, Ok(hold))) => hold,
        Some((spec, Err(problem))) => {
            problems.push(ScaleProblem::new(
                hold_key,
                format!("is '{}': {problem}", spec.0),
            ));
            default_hold
        }
    };
    Rule {
        threshold,
        // Whole steps of the store, so both sides of `held >= for` are.
        hold_seconds: Window::of(hold).map_or(hold.as_secs(), |window| window.duration().as_secs()),
    }
}
