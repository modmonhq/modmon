use std::sync::Arc;

use crate::app::ports::GuidedConfigEditor;
use crate::domain::error::MmError;
use crate::domain::onboarding::{
    ConfigLayoutPlan, ConfigLayoutRequest, ConfigLayoutResult, ConfigMutationAction,
    ConfigMutationPlan, ConfigMutationRequest, ConfigMutationResult, ConfigResourceKind,
    GuidedJourney, InitializationPlan, WorkflowEffect, WorkflowEvent, WorkflowRequest,
    WorkflowReview, WorkflowState, WorkflowStep,
};

#[derive(Clone)]
pub struct OnboardingService {
    editor: Arc<dyn GuidedConfigEditor>,
}

impl std::fmt::Debug for OnboardingService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OnboardingService")
            .finish_non_exhaustive()
    }
}

impl OnboardingService {
    #[must_use]
    pub fn new(editor: Arc<dyn GuidedConfigEditor>) -> Self {
        Self { editor }
    }

    pub fn plan_initialization(
        &self,
        root: &std::path::Path,
        requested_name: Option<&str>,
    ) -> Result<InitializationPlan, MmError> {
        let preview = self.editor.preview_initialization(root, requested_name)?;
        Self::initialization_plan(preview)
    }

    pub(crate) fn initialization_plan(
        preview: crate::domain::onboarding::InitializationPreview,
    ) -> Result<InitializationPlan, MmError> {
        let result_hash = blake3::hash(preview.config_source.as_bytes())
            .to_hex()
            .to_string();
        let mut affected_files = vec![preview.config_path];
        affected_files.extend(preview.starter_files);
        let mut canonical_diff = format!(
            ".config/mm.yaml\n{}",
            canonical_diff("", &preview.config_source)
        );
        if preview.gitignore.changed() {
            affected_files.push(preview.gitignore.path.clone());
            canonical_diff.push_str(&format!(
                "\n.gitignore\n{}",
                canonical_diff_for_ignore(&preview.gitignore)
            ));
        }
        let canonical = serde_json::to_vec(&(
            &preview.root,
            &preview.project_name,
            preview.project_id,
            &affected_files,
            &result_hash,
            &preview.gitignore,
            &preview.config_hash,
        ))
        .map_err(|error| MmError::State {
            message: format!("could not canonicalize initialization plan: {error}"),
        })?;
        let plan_hash = blake3::hash(&canonical).to_hex().to_string();
        Ok(InitializationPlan {
            root: preview.root,
            project_name: preview.project_name,
            project_id: preview.project_id,
            affected_files,
            canonical_diff,
            generated_resources: vec!["target:local".to_owned(), "environment:local".to_owned()],
            generated_routes: Vec::new(),
            privilege_prompts: Vec::new(),
            result_hash,
            plan_hash,
        })
    }

    pub fn verify_initialization(
        &self,
        root: &std::path::Path,
        requested_name: Option<&str>,
        reviewed_plan_hash: &str,
    ) -> Result<InitializationPlan, MmError> {
        let plan = self.plan_initialization(root, requested_name)?;
        if plan.plan_hash != reviewed_plan_hash {
            return Err(MmError::State {
                message: format!(
                    "initialization plan changed (expected {reviewed_plan_hash}, current {}); review the new plan",
                    plan.plan_hash
                ),
            });
        }
        Ok(plan)
    }

    /// One resource as the configuration files spell it today, or `None`
    /// when it is not configured. An edit starts from this so it keeps the
    /// keys it does not change.
    pub fn current_definition(
        &self,
        start: &std::path::Path,
        resource: ConfigResourceKind,
        name: &str,
        parent: Option<&str>,
    ) -> Result<Option<serde_json::Value>, MmError> {
        self.editor.definition(
            start,
            &ConfigMutationRequest {
                action: ConfigMutationAction::Edit,
                resource,
                name: name.to_owned(),
                parent: parent.map(str::to_owned),
                definition: None,
            },
        )
    }

    pub fn plan_mutation(
        &self,
        start: &std::path::Path,
        request: ConfigMutationRequest,
    ) -> Result<ConfigMutationPlan, MmError> {
        validate_mutation_request(&request)?;
        let preview = self.editor.preview(start, &request)?;
        let result_hash = blake3::hash(preview.after.as_bytes()).to_hex().to_string();
        let canonical_diff = canonical_diff_for(
            &config_source_label(&preview.config_path),
            &preview.before,
            &preview.after,
        );
        let generated_routes = if request.resource == ConfigResourceKind::Ingress
            && request.action != ConfigMutationAction::Remove
        {
            vec![request.name.clone()]
        } else {
            Vec::new()
        };
        let generated_resources = if request.action == ConfigMutationAction::Remove {
            Vec::new()
        } else {
            vec![format!("{}:{}", request.resource.label(), request.name)]
        };
        let plan_hash = mutation_plan_hash(&request, &preview.source_hash, &result_hash)?;
        Ok(ConfigMutationPlan {
            request,
            affected_files: vec![preview.config_path],
            canonical_diff,
            generated_resources,
            generated_routes,
            privilege_prompts: Vec::new(),
            source_hash: preview.source_hash,
            result_hash,
            plan_hash,
        })
    }

    pub fn apply_mutation(
        &self,
        start: &std::path::Path,
        request: ConfigMutationRequest,
        reviewed_plan_hash: &str,
    ) -> Result<ConfigMutationResult, MmError> {
        let plan = self.plan_mutation(start, request)?;
        if plan.plan_hash != reviewed_plan_hash {
            return Err(MmError::State {
                message: format!(
                    "configuration plan changed (expected {reviewed_plan_hash}, current {}); review the new plan",
                    plan.plan_hash
                ),
            });
        }
        let preview = self.editor.preview(start, &plan.request)?;
        let document = self
            .editor
            .commit(&preview, &plan.source_hash, &plan.result_hash)?;
        Ok(ConfigMutationResult {
            config_path: preview.config_path,
            source_hash: document.source_hash,
            plan_hash: plan.plan_hash,
        })
    }

    /// Plans moving one top-level key to or from `.config/mm/<key>.yaml`.
    pub fn plan_layout(
        &self,
        start: &std::path::Path,
        request: ConfigLayoutRequest,
    ) -> Result<ConfigLayoutPlan, MmError> {
        let preview = self.editor.preview_layout(start, &request)?;
        let result_hash = preview.result_hash();
        let mut canonical_diff = String::new();
        for change in &preview.changes {
            let label = config_source_label(&change.path);
            let status = match (&change.before, &change.after) {
                (None, Some(_)) => " (new file)",
                (Some(_), None) => " (removed)",
                _ => "",
            };
            canonical_diff.push_str(&format!("{label}{status}\n"));
            canonical_diff.push_str(&canonical_diff_for(
                &label,
                change.before.as_deref().unwrap_or_default(),
                change.after.as_deref().unwrap_or_default(),
            ));
        }
        let plan_hash = layout_plan_hash(&request, &preview.source_hash, &result_hash)?;
        Ok(ConfigLayoutPlan {
            request,
            affected_files: preview
                .changes
                .iter()
                .map(|change| change.path.clone())
                .collect(),
            canonical_diff,
            notes: preview.notes,
            source_hash: preview.source_hash,
            result_hash,
            plan_hash,
        })
    }

    pub fn apply_layout(
        &self,
        start: &std::path::Path,
        request: ConfigLayoutRequest,
        reviewed_plan_hash: &str,
    ) -> Result<ConfigLayoutResult, MmError> {
        let preview = self.editor.preview_layout(start, &request)?;
        let result_hash = preview.result_hash();
        let plan_hash = layout_plan_hash(&request, &preview.source_hash, &result_hash)?;
        if plan_hash != reviewed_plan_hash {
            return Err(MmError::State {
                message: format!(
                    "configuration plan changed (expected {reviewed_plan_hash}, current {plan_hash}); review the new plan"
                ),
            });
        }
        let document = self
            .editor
            .commit_layout(&preview, &preview.source_hash, &result_hash)?;
        Ok(ConfigLayoutResult {
            config_path: document.source(&request.key).to_path_buf(),
            request,
            source_hash: document.source_hash,
            plan_hash,
        })
    }

    pub fn plan_workflow(&self, request: WorkflowRequest) -> Result<WorkflowReview, MmError> {
        let discovery = self.editor.discover_project(&request.root)?;
        let mutation = request
            .mutation
            .clone()
            .map(|mutation| self.plan_mutation(&request.root, mutation))
            .transpose()?;
        let mut affected_files = Vec::new();
        let mut canonical_diff = String::new();
        let mut resources = Vec::new();
        let mut routes = Vec::new();
        if let Some(plan) = &mutation {
            affected_files.clone_from(&plan.affected_files);
            canonical_diff.clone_from(&plan.canonical_diff);
            resources.clone_from(&plan.generated_resources);
            routes.clone_from(&plan.generated_routes);
        }
        let privilege_prompts = privilege_prompts(request.journey);
        let next_actions = next_actions(request.journey);
        let plan_hash = workflow_plan_hash(
            &request,
            &discovery,
            mutation.as_ref().map(|plan| plan.plan_hash.as_str()),
            &privilege_prompts,
        )?;
        Ok(WorkflowReview {
            journey: request.journey,
            title: request.journey.title().to_owned(),
            proposals: discovery.proposals,
            affected_files,
            canonical_diff,
            generated_resources: resources,
            generated_routes: routes,
            privilege_prompts,
            next_actions,
            plan_hash,
        })
    }

    pub fn transition(
        &self,
        state: &mut WorkflowState,
        event: WorkflowEvent,
    ) -> Result<WorkflowEffect, MmError> {
        transition(state, event)
    }
}

fn canonical_diff_for_ignore(
    change: &crate::domain::onboarding::InitializationFileChange,
) -> String {
    canonical_diff_for(
        ".gitignore",
        change.before.as_deref().unwrap_or(""),
        &change.after,
    )
}

pub fn transition(
    state: &mut WorkflowState,
    event: WorkflowEvent,
) -> Result<WorkflowEffect, MmError> {
    let effect = match event {
        WorkflowEvent::Start(request) => {
            if let Some(mutation) = &request.mutation {
                validate_mutation_request(mutation)?;
            }
            state.step = WorkflowStep::Discover;
            state.request = Some(request.clone());
            state.review = None;
            state.message = None;
            WorkflowEffect::Discover(request)
        }
        WorkflowEvent::Discovered(review) if state.step == WorkflowStep::Discover => {
            state.step = WorkflowStep::Configure;
            state.review = Some(review);
            WorkflowEffect::None
        }
        WorkflowEvent::Configure if state.step == WorkflowStep::Discover => {
            state.step = WorkflowStep::Configure;
            WorkflowEffect::None
        }
        WorkflowEvent::Review if state.step == WorkflowStep::Configure => {
            state.step = WorkflowStep::Review;
            WorkflowEffect::None
        }
        WorkflowEvent::Apply if state.step == WorkflowStep::Review => {
            let request = state.request.clone().ok_or_else(|| MmError::State {
                message: "workflow request is missing".to_owned(),
            })?;
            let plan_hash = state
                .review
                .as_ref()
                .map(|review| review.plan_hash.clone())
                .ok_or_else(|| MmError::State {
                    message: "workflow review is missing".to_owned(),
                })?;
            state.step = WorkflowStep::Applying;
            WorkflowEffect::ApplyReviewed { request, plan_hash }
        }
        WorkflowEvent::Applied(message) if state.step == WorkflowStep::Applying => {
            state.step = WorkflowStep::Complete;
            state.message = Some(message);
            WorkflowEffect::None
        }
        WorkflowEvent::Back => {
            state.step = match state.step {
                WorkflowStep::Configure => WorkflowStep::Discover,
                WorkflowStep::Review => WorkflowStep::Configure,
                WorkflowStep::Error => WorkflowStep::Review,
                WorkflowStep::ChooseJourney
                | WorkflowStep::Discover
                | WorkflowStep::Applying
                | WorkflowStep::Complete
                | WorkflowStep::Cancelled => WorkflowStep::ChooseJourney,
            };
            state.message = None;
            WorkflowEffect::None
        }
        WorkflowEvent::Cancel => {
            state.step = WorkflowStep::Cancelled;
            state.message = Some("Cancelled before mutation; no changes were made.".to_owned());
            WorkflowEffect::None
        }
        WorkflowEvent::Resume if state.step == WorkflowStep::Cancelled => {
            state.step = if state.review.is_some() {
                WorkflowStep::Review
            } else {
                WorkflowStep::Discover
            };
            state.message = None;
            WorkflowEffect::None
        }
        WorkflowEvent::Failed(message) => {
            state.step = WorkflowStep::Error;
            state.message = Some(message);
            WorkflowEffect::None
        }
        WorkflowEvent::Retry if state.step == WorkflowStep::Error => {
            let request = state.request.clone().ok_or_else(|| MmError::State {
                message: "workflow request is missing".to_owned(),
            })?;
            state.step = WorkflowStep::Discover;
            state.message = None;
            WorkflowEffect::Discover(request)
        }
        _ => {
            return Err(MmError::State {
                message: format!("workflow event is not valid at step {:?}", state.step),
            });
        }
    };
    Ok(effect)
}

fn validate_mutation_request(request: &ConfigMutationRequest) -> Result<(), MmError> {
    if request.name.trim().is_empty() || request.name.chars().any(char::is_whitespace) {
        return Err(MmError::ConfigParse {
            path: std::path::PathBuf::from(".config/mm.yaml"),
            message: "resource names must be non-empty and contain no whitespace".to_owned(),
        });
    }
    match request.action {
        ConfigMutationAction::Add | ConfigMutationAction::Edit if request.definition.is_none() => {
            return Err(MmError::Environment {
                message: format!(
                    "{} {} requires a public metadata definition",
                    action_label(request),
                    request.resource.label()
                ),
            });
        }
        ConfigMutationAction::Remove if request.definition.is_some() => {
            return Err(MmError::Environment {
                message: "remove requests must not contain a definition".to_owned(),
            });
        }
        _ => {}
    }
    if request.resource == ConfigResourceKind::Secret
        && request
            .definition
            .as_ref()
            .is_some_and(contains_secret_plaintext_field)
    {
        return Err(MmError::Security {
            message: "secret declarations may contain provider/generation/input metadata only; submit plaintext through the dedicated hidden-input effect"
                .to_owned(),
        });
    }
    Ok(())
}

const fn action_label(request: &ConfigMutationRequest) -> &'static str {
    match request.action {
        ConfigMutationAction::Add => "adding",
        ConfigMutationAction::Edit => "editing",
        ConfigMutationAction::Remove => "removing",
    }
}

fn contains_secret_plaintext_field(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(values) => values.iter().any(|(key, value)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "value" | "password" | "plaintext" | "material" | "token"
            ) || contains_secret_plaintext_field(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(contains_secret_plaintext_field),
        _ => false,
    }
}

fn mutation_plan_hash(
    request: &ConfigMutationRequest,
    source_hash: &str,
    result_hash: &str,
) -> Result<String, MmError> {
    let canonical = serde_json::to_vec(&(request, source_hash, result_hash)).map_err(|error| {
        MmError::State {
            message: format!("could not canonicalize configuration plan: {error}"),
        }
    })?;
    Ok(blake3::hash(&canonical).to_hex().to_string())
}

fn layout_plan_hash(
    request: &ConfigLayoutRequest,
    source_hash: &str,
    result_hash: &str,
) -> Result<String, MmError> {
    let canonical = serde_json::to_vec(&("config-layout-v1", request, source_hash, result_hash))
        .map_err(|error| MmError::State {
            message: format!("could not canonicalize configuration layout plan: {error}"),
        })?;
    Ok(blake3::hash(&canonical).to_hex().to_string())
}

fn workflow_plan_hash(
    request: &WorkflowRequest,
    discovery: &crate::domain::onboarding::ProjectDiscovery,
    mutation_hash: Option<&str>,
    privileges: &[String],
) -> Result<String, MmError> {
    let canonical =
        serde_json::to_vec(&(request, discovery, mutation_hash, privileges)).map_err(|error| {
            MmError::State {
                message: format!("could not canonicalize guided workflow: {error}"),
            }
        })?;
    Ok(blake3::hash(&canonical).to_hex().to_string())
}

fn canonical_diff(before: &str, after: &str) -> String {
    canonical_diff_for(".config/mm.yaml", before, after)
}

/// The project-relative name of a configuration source: the anchor or one of
/// its split files.
fn config_source_label(path: &std::path::Path) -> String {
    let parent = path.parent();
    match (
        parent.and_then(std::path::Path::file_name),
        path.file_name(),
    ) {
        (Some(directory), Some(name)) if directory == "mm" => {
            format!(".config/mm/{}", name.to_string_lossy())
        }
        _ => ".config/mm.yaml".to_owned(),
    }
}

fn canonical_diff_for(label: &str, before: &str, after: &str) -> String {
    if before == after {
        return "(no changes)\n".to_owned();
    }
    let mut output = format!("--- a/{label}\n+++ b/{label}\n");
    let before_lines = before.lines().collect::<Vec<_>>();
    let after_lines = after.lines().collect::<Vec<_>>();
    let prefix = before_lines
        .iter()
        .zip(&after_lines)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = before_lines[prefix..]
        .iter()
        .rev()
        .zip(after_lines[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let old_end = before_lines.len().saturating_sub(suffix);
    let new_end = after_lines.len().saturating_sub(suffix);
    output.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        prefix + 1,
        old_end.saturating_sub(prefix),
        prefix + 1,
        new_end.saturating_sub(prefix)
    ));
    for line in &before_lines[prefix..old_end] {
        output.push('-');
        output.push_str(line);
        output.push('\n');
    }
    for line in &after_lines[prefix..new_end] {
        output.push('+');
        output.push_str(line);
        output.push('\n');
    }
    output
}

fn privilege_prompts(journey: GuidedJourney) -> Vec<String> {
    match journey {
        GuidedJourney::GlobalSetup => vec![
            "Installing the local CA may invoke the platform trust tool with elevated privileges."
                .to_owned(),
            "Installing ingress may bind privileged ports 80 and 443 through an explicit helper."
                .to_owned(),
        ],
        GuidedJourney::NodeSetup => vec![
            "Applying this plan runs the reviewed Ansible playbook and mutates the selected host."
                .to_owned(),
            "Remote setup may request SSH authentication and privilege escalation.".to_owned(),
        ],
        GuidedJourney::NewProject
        | GuidedJourney::ExistingProject
        | GuidedJourney::ProjectEditing => Vec::new(),
    }
}

fn next_actions(journey: GuidedJourney) -> Vec<String> {
    match journey {
        GuidedJourney::GlobalSetup => vec![
            "Repair/create local trust".to_owned(),
            "Apply the shared service plan".to_owned(),
            "Open https://mm.test and https://mail.mm.test".to_owned(),
        ],
        GuidedJourney::NewProject => vec![
            "Review discovered Compose, services, builds, templates, ports, and mounts".to_owned(),
            "Apply the exact configuration diff".to_owned(),
        ],
        GuidedJourney::ExistingProject => vec![
            "Choose the development environment".to_owned(),
            "Resolve missing public values and secret status".to_owned(),
            "Initialize routes, mounts, networks, and local dependencies".to_owned(),
            "Deploy and open the project dashboard".to_owned(),
        ],
        GuidedJourney::NodeSetup => vec![
            "Choose local or SSH target".to_owned(),
            "Review bootstrap requirements, ingress, relay, and access policy".to_owned(),
            "Apply only the displayed plan hash".to_owned(),
        ],
        GuidedJourney::ProjectEditing => vec![
            "Review the minimal YAML diff".to_owned(),
            "Apply only if mm.yaml still matches the source hash".to_owned(),
        ],
    }
}

#[cfg(test)]
mod tests;
