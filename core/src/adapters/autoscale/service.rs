//! One step of the autoscaler for one service.

use std::time::{Duration, Instant};

use super::{Autoscaler, DeployedScaling, ScaleHost, heard, store};
use crate::domain::autoscale::{
    Decision, Moment, NodeStep, OwnContainer, Rule, ScaleChange, ScalePolicy, ScaleReport,
    ScaledService, ServiceScale, Want, cluster_tally, decide, decider, newest_want, node_step,
    settled_everywhere,
};
use crate::domain::error::MmError;
use crate::domain::metrics::{Condition, MetricQuery, Subject, Window};

/// What the decider knows of the service at this step.
struct Cluster<'a> {
    node: &'a str,
    service: &'a ScaledService,
    policy: &'a ScalePolicy,
    own: &'a ScaleReport,
    heard: &'a [(&'a str, &'a ScaleReport)],
    /// The nodes of the service that answer, this one included.
    answering: Vec<&'a str>,
    want: Option<Want>,
    count: u32,
}

impl Autoscaler {
    /// Looks at the service, takes one step toward this node's share, and
    /// returns what the node tells the others.
    pub(super) async fn service_step(
        &self,
        host: &dyn ScaleHost,
        deployment: &DeployedScaling,
        service: &ScaledService,
        peers: &[(String, Vec<ScaleReport>)],
    ) -> Result<ScaleReport, MmError> {
        let Some(policy) = service.policy() else {
            return Err(MmError::State {
                message: "the service does not autoscale".to_owned(),
            });
        };
        let node = deployment.scope.node.as_deref().unwrap_or_default();
        let key = service.key();
        let containers = self.containers(service).await?;
        let heard = heard(peers, &service.nodes, &key);
        // The newest count any node knows of is the one to go by.
        let own = store::want(&self.directory, &key)?;
        let want = newest_want(
            own.iter()
                .chain(heard.iter().filter_map(|(_, report)| report.want.as_ref())),
            &service.nodes,
        )
        .cloned();
        if let Some(newer) = want.as_ref().filter(|want| Some(*want) != own.as_ref()) {
            store::keep_want(&self.directory, &key, newer)?;
        }
        let count = service.count(want.as_ref().map(|want| want.count));
        let deploying = host.deploying(&deployment.scope);
        let step = node_step(service.share(node, Some(count)), &containers);
        let mut leaving = containers
            .iter()
            .filter(|container| container.leaving)
            .map(|container| container.id.clone())
            .collect::<Vec<_>>();
        if !deploying {
            self.act(host, deployment, service, &step, &mut leaving)
                .await;
        }
        self.tracked().entry(key.clone()).or_default().leaving = leaving;
        let own = self.report(policy, &key, &containers, &step, deploying, want.clone());
        let answering = service
            .nodes
            .iter()
            .map(String::as_str)
            .filter(|name| *name == node || peers.iter().any(|(peer, _)| peer == name))
            .collect();
        let decided = self.decide_for(
            host,
            deployment,
            &Cluster {
                node,
                service,
                policy,
                own: &own,
                heard: &heard,
                answering,
                want,
                count,
            },
        )?;
        Ok(ScaleReport {
            want: decided.or(own.want.clone()),
            ..own
        })
    }

    /// The one thing this node does now about its own containers.
    async fn act(
        &self,
        host: &dyn ScaleHost,
        deployment: &DeployedScaling,
        service: &ScaledService,
        step: &NodeStep,
        leaving: &mut Vec<String>,
    ) {
        match step {
            NodeStep::Start { scale_to } => {
                tracing::info!(service = %service.key(), containers = scale_to, "starting a container of an autoscaled service");
                if let Err(error) = host.start(deployment, service, *scale_to).await {
                    tracing::warn!(service = %service.key(), %error, "could not start a container of an autoscaled service");
                }
            }
            NodeStep::Leave { id } => {
                tracing::info!(service = %service.key(), container = %id, "a container of an autoscaled service leaves");
                let drain = Duration::from_millis(service.drain_ms);
                match self.leaves.begin(std::slice::from_ref(id), drain).await {
                    Ok(ids) => leaving.extend(ids),
                    Err(error) => {
                        tracing::warn!(service = %service.key(), %error, "could not have a container leave");
                    }
                }
            }
            NodeStep::Wait { .. } | NodeStep::Settled => {}
        }
    }

    /// What this node tells the other nodes about the service.
    fn report(
        &self,
        policy: &ScalePolicy,
        key: &str,
        containers: &[OwnContainer],
        step: &NodeStep,
        deploying: bool,
        want: Option<Want>,
    ) -> ScaleReport {
        let active = containers
            .iter()
            .filter(|container| container.running && !container.leaving)
            .map(|container| container.id.chars().take(12).collect::<String>())
            .collect::<Vec<_>>();
        ScaleReport {
            service: key.to_owned(),
            tally: self.metrics.tally(policy.metric, &active),
            running: u32::try_from(active.len()).unwrap_or(u32::MAX),
            settled: *step == NodeStep::Settled && !deploying,
            want,
        }
    }

    /// On the decider: the cluster's step of the service's series, and a
    /// new count when a rule says so. Elsewhere: what the status shows.
    /// Returns the new want when the count changed.
    fn decide_for(
        &self,
        host: &dyn ScaleHost,
        deployment: &DeployedScaling,
        cluster: &Cluster<'_>,
    ) -> Result<Option<Want>, MmError> {
        let key = cluster.service.key();
        let decider = decider(&cluster.service.nodes, |name| {
            cluster.answering.contains(&name)
        });
        let mut status = status_of(cluster, decider);
        let now = Instant::now();
        let mut tracked = self.tracked();
        let entry = tracked.entry(key.clone()).or_default();
        if decider != Some(cluster.node) {
            // A node that stops deciding drops the service's series: when
            // it decides again, it watches a whole `for:` first.
            if entry.deciding_since.take().is_some() {
                self.metrics.forget_service(&key);
            }
            entry.settled_since = None;
            entry.status = Some(status);
            return Ok(None);
        }
        let deciding_since = *entry.deciding_since.get_or_insert(now);
        let total = cluster_tally(cluster.own.tally, cluster.heard);
        self.metrics
            .record_service(&key, cluster.policy.metric, total);
        if settled_everywhere(
            cluster.count,
            &cluster.service.nodes,
            (cluster.node, cluster.own),
            cluster.heard,
            &cluster.answering,
        ) {
            entry.settled_since.get_or_insert(now);
        } else {
            entry.settled_since = None;
        }
        let moment = self.moment(cluster, &key, (deciding_since, entry.settled_since), now);
        status.reading = self.reading(&key, cluster.policy, cluster.policy.up);
        let (to, reason) = match decide(cluster.policy, &moment) {
            Decision::Change { to, reason } => (to, reason),
            Decision::Keep { reason } => {
                status.holding = Some(reason);
                entry.status = Some(status);
                return Ok(None);
            }
        };
        let change = ScaleChange {
            from: cluster.count,
            to,
            reason,
            at_unix_ms: crate::adapters::node_files::now_unix_ms(),
        };
        let next = Want {
            count: to,
            generation: status.generation.saturating_add(1),
            by: cluster.node.to_owned(),
            change: Some(change.clone()),
        };
        store::keep_want(&self.directory, &key, &next)?;
        entry.settled_since = None;
        status.wanted = to;
        status.generation = next.generation;
        status.last_change = Some(change.clone());
        entry.status = Some(status);
        drop(tracked);
        tracing::info!(service = %key, from = change.from, to, reason = %change.reason, "autoscale changed the count");
        host.audit(deployment, cluster.service, &change);
        Ok(Some(next))
    }

    /// The decider's view of the moment.
    fn moment(
        &self,
        cluster: &Cluster<'_>,
        key: &str,
        (deciding_since, settled_since): (Instant, Option<Instant>),
        now: Instant,
    ) -> Moment {
        let subject = Subject::Service(key.to_owned());
        let metric = cluster.policy.metric;
        let average = |rule: Rule| {
            self.reading(key, cluster.policy, rule)
                .and_then(|reading| reading.average)
        };
        Moment {
            count: cluster.count,
            up_held: self.metrics.held(
                &subject,
                metric,
                Condition::Greater(cluster.policy.up.threshold),
            ),
            down_held: self.metrics.held(
                &subject,
                metric,
                Condition::Less(cluster.policy.down.threshold),
            ),
            settled: settled_since.map(|since| now.saturating_duration_since(since)),
            deciding: now.saturating_duration_since(deciding_since),
            up_average: average(cluster.policy.up),
            down_average: average(cluster.policy.down),
        }
    }

    /// The service's metric over `rule`'s window.
    fn reading(
        &self,
        key: &str,
        policy: &ScalePolicy,
        rule: Rule,
    ) -> Option<crate::domain::metrics::MetricReading> {
        self.metrics.read(&MetricQuery {
            subject: Subject::Service(key.to_owned()),
            metric: policy.metric,
            window: Window::of(rule.hold()).unwrap_or(Window::NEWEST),
            samples: false,
        })
    }
}

/// What the status shows of the service, before the decider adds its own.
fn status_of(cluster: &Cluster<'_>, decider: Option<&str>) -> ServiceScale {
    ServiceScale {
        wanted: cluster.count,
        running: cluster
            .heard
            .iter()
            .fold(cluster.own.running, |sum, (_, report)| {
                sum.saturating_add(report.running)
            }),
        min: cluster.policy.min,
        max: cluster.policy.max,
        autoscale: true,
        generation: cluster.want.as_ref().map_or(0, |want| want.generation),
        decider: decider.map(str::to_owned),
        last_change: cluster.want.as_ref().and_then(|want| want.change.clone()),
        holding: None,
        reading: None,
    }
}
