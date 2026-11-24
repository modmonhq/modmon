impl RemoteDaemon {
    /// Refuses a deployment whose volume or network `name:` is outside its
    /// stack's Compose project name here, or that mounts a host path on an
    /// engine that may not mount it recursively read-only.
    async fn refuse_after_plan(
        &self,
        document: &crate::domain::config::ConfigDocument,
        scope: &crate::domain::remote::RemoteScope,
        deployment: Option<&crate::domain::deployment::DeploymentPlan>,
        allow: &[crate::domain::host_policy::GuardrailException],
    ) -> Result<(), MmError> {
        let Some(plan) = deployment else {
            return Ok(());
        };
        let environment = &scope.environment;
        let names = plan
            .stacks
            .iter()
            .map(|stack| (stack.stack.clone(), stack.compose_project_name.clone()))
            .collect();
        let mut findings = crate::app::named_resource_guardrails(document, environment, &names, allow)?;
        let why = match scope.runtime {
            crate::domain::runtime::RuntimeKind::PodmanCompose => Some(PODMAN_HOST_PATHS),
            crate::domain::runtime::RuntimeKind::DockerCompose => None,
        };
        let mut host_paths = crate::app::unsupported_host_path_guardrails(document, environment, why.unwrap_or(DOCKER_HOST_PATHS))?;
        if why.is_none()
            && (host_paths.is_empty()
                || self.use_cases.engine(scope.runtime).await.is_some_and(|engine| engine.recursive_read_only))
        {
            host_paths.clear();
        }
        findings.extend(host_paths);
        refuse_findings(environment, &findings)
    }
}

const PODMAN_HOST_PATHS: &str =
    "this host runs Podman, which mm cannot yet have mount a host path recursively read-only";
const DOCKER_HOST_PATHS: &str = "this host's Docker Engine cannot be relied on to mount a host path recursively read-only: that needs Docker Engine 25 (API 1.44) or later on Linux 5.12 or later";

/// Refuses a plan with guardrail findings, naming each one.
fn refuse_findings(environment: &str, findings: &[crate::app::GuardrailFinding]) -> Result<(), MmError> {
    if findings.is_empty() {
        return Ok(());
    }
    let next = if findings.iter().any(crate::app::GuardrailFinding::grantable) {
        ". A host admin can allow a feature for one service under `allow` in /etc/mm/host.yaml"
    } else {
        ""
    };
    Err(MmError::Security {
        message: format!(
            "this host refuses Compose features that reach the host in environment '{environment}': {}{next}",
            findings.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
        ),
    })
}
