use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{Datelike, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::config::{JobConfig, MisfirePolicy, ScheduleConcurrency, ScheduleConfig};
use crate::domain::runtime::{RuntimeEnvironment, RuntimeKind};
use crate::domain::workspace::ProjectId;

pub const JOB_PLAN_SCHEMA: &str = "mm-job-reconciliation-plan-v1";
pub const MAX_JOB_OUTPUT_BYTES: u32 = 1024 * 1024;
pub const MAX_JOB_HISTORY_ENTRIES: u32 = 10_000;
pub const MAX_CATCH_UP_RUNS: u16 = 100;
pub const MAX_JOB_ATTEMPTS: u16 = 100;
pub const MAX_SCHEDULE_SCAN_MINUTES: i64 = 10 * 366 * 24 * 60;
const MAX_DEFINITION_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobRunState {
    Queued,
    Launching,
    Running,
    RetryWait,
    Cancelling,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    Unknown,
}

impl JobRunState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::TimedOut | Self::Cancelled
        )
    }

    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Launching | Self::Running | Self::Cancelling | Self::Unknown
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobTrigger {
    Manual,
    Schedule,
    RunNow,
    Retry,
    /// A deployment ran it before its service was started or replaced.
    BeforeDeploy,
    /// A deployment ran it once it was in place.
    AfterDeploy,
    /// The first deployment of the environment with it started it.
    Once,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRun {
    pub id: Uuid,
    pub project_id: ProjectId,
    pub environment: String,
    pub job: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    pub trigger: JobTrigger,
    pub state: JobRunState,
    pub attempt: u16,
    pub maximum_attempts: u16,
    pub fence_token: u64,
    pub runtime_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_id: Option<String>,
    pub requested_at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_for_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_attempt_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub output: String,
    pub output_truncated: bool,
    pub cancellation_requested: bool,
    pub principal: String,
    pub source: String,
    /// The cluster node the run was on, as a client that asked several
    /// nodes labels it. A host never stores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
}

#[derive(Clone)]
pub struct JobLaunch {
    pub run_id: Uuid,
    pub fence_token: u64,
    pub runtime_name: String,
    pub runtime: RuntimeKind,
    pub target: crate::domain::runtime::RuntimeTarget,
    pub module: String,
    pub service: String,
    pub argv: Vec<String>,
    /// Who the command runs as, in place of the service's own user.
    pub user: Option<String>,
    pub timeout: Duration,
    pub output_limit_bytes: usize,
    pub environment: RuntimeEnvironment,
    pub redactions: Vec<zeroize::Zeroizing<String>>,
}

impl std::fmt::Debug for JobLaunch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JobLaunch")
            .field("run_id", &self.run_id)
            .field("fence_token", &self.fence_token)
            .field("runtime_name", &self.runtime_name)
            .field("runtime", &self.runtime)
            .field("module", &self.module)
            .field("service", &self.service)
            .field("argv_count", &self.argv.len())
            .field("user", &self.user)
            .field("timeout", &self.timeout)
            .field("output_limit_bytes", &self.output_limit_bytes)
            .field(
                "environment_keys",
                &self.environment.keys().collect::<Vec<_>>(),
            )
            .field("redaction_count", &self.redactions.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeJobState {
    Running,
    Exited {
        exit_code: i32,
        output: String,
        truncated: bool,
    },
    Missing,
    Unknown {
        diagnostic_code: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScheduleRecord {
    pub name: String,
    pub project_id: ProjectId,
    pub environment: String,
    pub job: String,
    pub enabled: bool,
    pub retired: bool,
    pub next_due_at_unix_ms: Option<i64>,
    pub definition_hash: String,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationAction {
    CreateJob,
    UpdateJob,
    RetireJob,
    CreateSchedule,
    UpdateSchedule,
    RetireSchedule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReconciliationChange {
    pub action: ReconciliationAction,
    pub name: String,
    pub previous_hash: Option<String>,
    pub desired_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReconciliationPlan {
    pub schema: String,
    pub project_id: ProjectId,
    pub environment: String,
    pub config_hash: String,
    pub changes: Vec<ReconciliationChange>,
    pub plan_hash: String,
}

impl ReconciliationPlan {
    pub fn calculate_hash(&self) -> Result<String, String> {
        let mut canonical = self.clone();
        canonical.plan_hash.clear();
        let bytes = serde_json::to_vec(&canonical)
            .map_err(|error| format!("could not serialize scheduler plan: {error}"))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredDefinitionHashes {
    pub jobs: BTreeMap<String, String>,
    pub schedules: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesiredDefinitions {
    pub jobs: BTreeMap<String, JobConfig>,
    pub schedules: BTreeMap<String, ScheduleConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueCalculation {
    pub runs: Vec<i64>,
    pub next_due_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationCommit {
    pub plan: ReconciliationPlan,
    pub desired: DesiredDefinitions,
    pub now_unix_ms: i64,
    pub principal: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewJobRun {
    pub id: Uuid,
    pub project_id: ProjectId,
    pub environment: String,
    pub job: String,
    pub schedule: Option<String>,
    pub trigger: JobTrigger,
    pub idempotency_key: String,
    pub job_definition: JobConfig,
    pub schedule_definition: Option<ScheduleConfig>,
    pub config_hash: String,
    pub requested_at_unix_ms: i64,
    pub scheduled_for_unix_ms: Option<i64>,
    pub principal: String,
    pub source: String,
    pub retry_of_run_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedSchedule {
    pub project_id: ProjectId,
    pub environment: String,
    pub name: String,
    pub definition: ScheduleConfig,
    pub job_definition: JobConfig,
    pub config_hash: String,
    pub next_due_at_unix_ms: i64,
    pub lease_owner: Uuid,
    pub fence_token: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleTickCommit {
    pub claim: ClaimedSchedule,
    pub due_at_unix_ms: Vec<i64>,
    pub next_due_at_unix_ms: Option<i64>,
    pub now_unix_ms: i64,
    pub principal: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedJobRun {
    pub run: JobRun,
    pub job_definition: JobConfig,
    pub schedule_definition: Option<ScheduleConfig>,
    pub lease_owner: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRunCompletion {
    pub run_id: Uuid,
    pub lease_owner: Uuid,
    pub fence_token: u64,
    pub state: JobRunState,
    pub finished_at_unix_ms: i64,
    pub exit_code: Option<i32>,
    pub error_code: Option<String>,
    pub diagnostic_code: Option<String>,
    pub output: String,
    pub output_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobAuditRecord {
    pub occurred_at_unix_ms: i64,
    pub project_id: ProjectId,
    pub environment: String,
    pub principal: String,
    pub source: String,
    pub action: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub definition_hash: Option<String>,
    pub details: BTreeMap<String, String>,
}

pub fn parse_duration(value: &str) -> Result<Duration, String> {
    if value.is_empty() || value.trim() != value {
        return Err("duration must be a compact value such as 30s, 10m, 2h, or 1d".to_owned());
    }
    let split = value
        .find(|character: char| !character.is_ascii_digit())
        .ok_or_else(|| "duration requires one of ms, s, m, h, or d".to_owned())?;
    let amount = value[..split]
        .parse::<u64>()
        .map_err(|_| "duration amount is invalid".to_owned())?;
    let multiplier = match &value[split..] {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => return Err("duration requires one of ms, s, m, h, or d".to_owned()),
    };
    let millis = amount
        .checked_mul(multiplier)
        .ok_or_else(|| "duration exceeds the supported range".to_owned())?;
    Ok(Duration::from_millis(millis))
}

pub fn parse_instant(value: &str) -> Result<i64, String> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.timestamp_millis())
        .map_err(|_| "timestamp must be RFC 3339 with an explicit offset".to_owned())
}

pub fn definition_hash<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("could not serialize scheduler definition: {error}"))?;
    if bytes.len() > MAX_DEFINITION_BYTES {
        return Err(format!(
            "scheduler definition exceeds the {MAX_DEFINITION_BYTES}-byte canonical limit"
        ));
    }
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

pub fn reconciliation_plan(
    project_id: ProjectId,
    environment: String,
    config_hash: String,
    desired: &DesiredDefinitions,
    stored: &StoredDefinitionHashes,
) -> Result<ReconciliationPlan, String> {
    let mut changes = Vec::new();
    append_definition_changes(
        &mut changes,
        &desired.jobs,
        &stored.jobs,
        ReconciliationAction::CreateJob,
        ReconciliationAction::UpdateJob,
        ReconciliationAction::RetireJob,
    )?;
    append_definition_changes(
        &mut changes,
        &desired.schedules,
        &stored.schedules,
        ReconciliationAction::CreateSchedule,
        ReconciliationAction::UpdateSchedule,
        ReconciliationAction::RetireSchedule,
    )?;
    let mut plan = ReconciliationPlan {
        schema: JOB_PLAN_SCHEMA.to_owned(),
        project_id,
        environment,
        config_hash,
        changes,
        plan_hash: String::new(),
    };
    plan.plan_hash = plan.calculate_hash()?;
    Ok(plan)
}

fn append_definition_changes<T: Serialize>(
    changes: &mut Vec<ReconciliationChange>,
    desired: &BTreeMap<String, T>,
    stored: &BTreeMap<String, String>,
    create: ReconciliationAction,
    update: ReconciliationAction,
    retire: ReconciliationAction,
) -> Result<(), String> {
    for (name, value) in desired {
        let hash = definition_hash(value)?;
        match stored.get(name) {
            None => changes.push(ReconciliationChange {
                action: create.clone(),
                name: name.clone(),
                previous_hash: None,
                desired_hash: Some(hash),
            }),
            Some(previous) if previous != &hash => changes.push(ReconciliationChange {
                action: update.clone(),
                name: name.clone(),
                previous_hash: Some(previous.clone()),
                desired_hash: Some(hash),
            }),
            Some(_) => {}
        }
    }
    for (name, hash) in stored {
        if !desired.contains_key(name) {
            changes.push(ReconciliationChange {
                action: retire.clone(),
                name: name.clone(),
                previous_hash: Some(hash.clone()),
                desired_hash: None,
            });
        }
    }
    Ok(())
}

pub fn next_occurrence(
    expression: &str,
    timezone: &str,
    after_unix_ms: i64,
    starts_at_unix_ms: Option<i64>,
    ends_at_unix_ms: Option<i64>,
) -> Result<Option<i64>, String> {
    let cron = CronExpression::parse(expression)?;
    let timezone = timezone
        .parse::<Tz>()
        .map_err(|_| format!("unknown IANA timezone '{timezone}'"))?;
    let after = starts_at_unix_ms.map_or(after_unix_ms, |start| {
        after_unix_ms.max(start.saturating_sub(1))
    });
    let mut candidate = after
        .div_euclid(60_000)
        .saturating_mul(60_000)
        .saturating_add(60_000);
    for _ in 0..MAX_SCHEDULE_SCAN_MINUTES {
        if ends_at_unix_ms.is_some_and(|end| candidate > end) {
            return Ok(None);
        }
        let Some(utc) = Utc.timestamp_millis_opt(candidate).single() else {
            return Err("schedule timestamp is outside the supported calendar range".to_owned());
        };
        let local = utc.with_timezone(&timezone);
        if cron.matches(&local) {
            return Ok(Some(candidate));
        }
        candidate = candidate.saturating_add(60_000);
    }
    Err("cron expression has no occurrence within ten years".to_owned())
}

pub fn apply_jitter(schedule_identity: &str, due_unix_ms: i64, maximum: Duration) -> i64 {
    let maximum_ms = i64::try_from(maximum.as_millis()).unwrap_or(i64::MAX);
    if maximum_ms <= 0 {
        return due_unix_ms;
    }
    let digest = blake3::hash(format!("{schedule_identity}\0{due_unix_ms}").as_bytes());
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest.as_bytes()[..8]);
    let offset = u64::from_be_bytes(bytes) % (maximum_ms as u64 + 1);
    due_unix_ms.saturating_add(i64::try_from(offset).unwrap_or(i64::MAX))
}

pub fn due_occurrences(
    schedule: &ScheduleConfig,
    schedule_identity: &str,
    next_due_at_unix_ms: i64,
    now_unix_ms: i64,
) -> Result<DueCalculation, String> {
    let start = schedule
        .starts_at
        .as_deref()
        .map(parse_instant)
        .transpose()?;
    let end = schedule.ends_at.as_deref().map(parse_instant).transpose()?;
    let jitter = schedule.jitter.parse()?;
    let mut due = Vec::new();
    let mut cursor = Some(next_due_at_unix_ms);
    while let Some(value) = cursor {
        if value > now_unix_ms || due.len() > usize::from(MAX_CATCH_UP_RUNS) {
            break;
        }
        due.push(value);
        cursor = next_occurrence(&schedule.cron, &schedule.timezone, value, start, end)?
            .map(|next| apply_jitter(schedule_identity, next, jitter));
    }
    let runs = match schedule.misfire {
        MisfirePolicy::Skip if due.len() > 1 => Vec::new(),
        MisfirePolicy::RunOnce if due.len() > 1 => due.last().copied().into_iter().collect(),
        MisfirePolicy::BoundedCatchUp { maximum } => due
            .into_iter()
            .take(usize::from(maximum.min(MAX_CATCH_UP_RUNS)))
            .collect(),
        MisfirePolicy::Skip | MisfirePolicy::RunOnce => due,
    };
    while cursor.is_some_and(|value| value <= now_unix_ms) {
        cursor = next_occurrence(
            &schedule.cron,
            &schedule.timezone,
            cursor.unwrap_or(now_unix_ms),
            start,
            end,
        )?
        .map(|next| apply_jitter(schedule_identity, next, jitter));
    }
    Ok(DueCalculation {
        runs,
        next_due_at_unix_ms: cursor,
    })
}

#[must_use]
pub const fn concurrency_action(
    policy: ScheduleConcurrency,
    active_runs: usize,
) -> ConcurrencyAction {
    match (policy, active_runs) {
        (_, 0) | (ScheduleConcurrency::Allow, _) => ConcurrencyAction::Launch,
        (ScheduleConcurrency::Forbid, _) => ConcurrencyAction::Suppress,
        (ScheduleConcurrency::Replace, _) => ConcurrencyAction::CancelAndLaunch,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcurrencyAction {
    Launch,
    Suppress,
    CancelAndLaunch,
}

#[derive(Debug, Clone)]
struct CronExpression {
    minute: Field,
    hour: Field,
    day: Field,
    month: Field,
    weekday: Field,
}

impl CronExpression {
    fn parse(value: &str) -> Result<Self, String> {
        let fields = value.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 5 {
            return Err(
                "cron must contain exactly five fields: minute hour day month weekday".to_owned(),
            );
        }
        Ok(Self {
            minute: Field::parse(fields[0], 0, 59, &[])?,
            hour: Field::parse(fields[1], 0, 23, &[])?,
            day: Field::parse(fields[2], 1, 31, &[])?,
            month: Field::parse(
                fields[3],
                1,
                12,
                &[
                    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov",
                    "dec",
                ],
            )?,
            weekday: Field::parse(
                fields[4],
                0,
                7,
                &["sun", "mon", "tue", "wed", "thu", "fri", "sat"],
            )?,
        })
    }

    fn matches(&self, value: &chrono::DateTime<Tz>) -> bool {
        let day_matches = self.day.contains(value.day());
        let weekday = value.weekday().num_days_from_sunday();
        let weekday_matches =
            self.weekday.contains(weekday) || (weekday == 0 && self.weekday.contains(7));
        let calendar_matches = if !self.day.wildcard && !self.weekday.wildcard {
            day_matches || weekday_matches
        } else {
            day_matches && weekday_matches
        };
        self.minute.contains(value.minute())
            && self.hour.contains(value.hour())
            && self.month.contains(value.month())
            && calendar_matches
    }
}

#[derive(Debug, Clone)]
struct Field {
    minimum: u32,
    values: Vec<bool>,
    wildcard: bool,
}

impl Field {
    fn parse(value: &str, minimum: u32, maximum: u32, names: &[&str]) -> Result<Self, String> {
        let mut field = Self {
            minimum,
            values: vec![false; (maximum - minimum + 1) as usize],
            wildcard: value == "*",
        };
        for part in value.split(',') {
            if part.is_empty() {
                return Err("cron contains an empty list item".to_owned());
            }
            let (range, step) = part.split_once('/').map_or((part, 1_u32), |(range, step)| {
                (range, step.parse::<u32>().unwrap_or(0))
            });
            if step == 0 {
                return Err("cron step must be a positive integer".to_owned());
            }
            let (start, end) = if range == "*" {
                (minimum, maximum)
            } else if let Some((start, end)) = range.split_once('-') {
                (
                    field_value(start, minimum, maximum, names)?,
                    field_value(end, minimum, maximum, names)?,
                )
            } else {
                let parsed = field_value(range, minimum, maximum, names)?;
                (parsed, parsed)
            };
            if start > end {
                return Err("cron range start is after its end".to_owned());
            }
            let mut current = start;
            while current <= end {
                field.values[(current - minimum) as usize] = true;
                let Some(next) = current.checked_add(step) else {
                    break;
                };
                current = next;
            }
        }
        Ok(field)
    }

    fn contains(&self, value: u32) -> bool {
        value
            .checked_sub(self.minimum)
            .and_then(|index| self.values.get(index as usize))
            .copied()
            .unwrap_or(false)
    }
}

fn field_value(value: &str, minimum: u32, maximum: u32, names: &[&str]) -> Result<u32, String> {
    let normalized = value.to_ascii_lowercase();
    let parsed = if let Some(index) = names.iter().position(|name| *name == normalized) {
        minimum + index as u32
    } else {
        normalized
            .parse::<u32>()
            .map_err(|_| format!("invalid cron value '{value}'"))?
    };
    if (minimum..=maximum).contains(&parsed) {
        Ok(parsed)
    } else {
        Err(format!(
            "cron value '{value}' is outside {minimum}..={maximum}"
        ))
    }
}

pub fn retry_delay(base: Duration, maximum: Duration, attempt: u16) -> Duration {
    let exponent = u32::from(attempt.saturating_sub(1)).min(31);
    base.checked_mul(2_u32.saturating_pow(exponent))
        .unwrap_or(maximum)
        .min(maximum)
}

#[must_use]
pub fn runtime_job_name(project_id: ProjectId, run_id: Uuid) -> String {
    let project = project_id.0.simple().to_string();
    let run = run_id.simple().to_string();
    format!("mm-job-{}-{}", &project[..8], &run[..20])
}

#[path = "scheduler/requests.rs"]
mod requests;
pub use requests::{
    JobOutcome, JobRunRequest, JobRunsRequest, MAX_LISTED_RUNS, OnceRecord, StartedRuns,
    deployment_label, remote_schedules, run_in_flight, runs_now, undeclared_job,
};
