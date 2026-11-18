fn parse_owned_resource(
    output: CommandOutput,
    kind: PodmanResourceKind,
    expected_name: &str,
    target_id: TargetId,
    resource: &str,
) -> Result<Value, MmError> {
    if output.truncated {
        return Err(MmError::RuntimeResponse {
            message: format!(
                "Podman {} inspect output was truncated",
                kind.command_name()
            ),
        });
    }
    let mut rows = serde_json::from_str::<Vec<Value>>(&output.stdout).map_err(|error| {
        MmError::RuntimeResponse {
            message: format!(
                "Podman returned invalid {} inspect JSON: {error}",
                kind.command_name()
            ),
        }
    })?;
    if rows.len() != 1 {
        return Err(MmError::RuntimeResponse {
            message: format!(
                "Podman {} inspect returned {} resources for '{expected_name}'",
                kind.command_name(),
                rows.len()
            ),
        });
    }
    let inspected = rows.pop().ok_or_else(|| MmError::RuntimeResponse {
        message: format!(
            "Podman {} inspect returned no resource for '{expected_name}'",
            kind.command_name()
        ),
    })?;
    let actual_name =
        string_for_keys(&inspected, &["Name", "name"]).ok_or_else(|| MmError::RuntimeResponse {
            message: format!(
                "Podman {} inspect omitted the resource name",
                kind.command_name()
            ),
        })?;
    if actual_name != expected_name {
        return Err(MmError::RuntimeResponse {
            message: format!(
                "Podman {} inspect returned an unexpected resource name for '{expected_name}'",
                kind.command_name()
            ),
        });
    }
    let labels = value_for_keys(&inspected, &["Labels", "labels"])
        .and_then(Value::as_object)
        .map(|labels| {
            labels
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_owned()))
                })
                .collect::<BTreeMap<_, _>>()
        });
    ResourceOwnership {
        project_id: None,
        environment: None,
        target_id,
        kind: kind.ownership_kind(),
        logical_name: expected_name.to_owned(),
        resource: resource.to_owned(),
        generation: crate::domain::ownership::OWNERSHIP_GENERATION,
    }
    .require_owned(expected_name, labels.as_ref())?;
    Ok(inspected)
}

fn validate_resource_name(name: &str) -> Result<(), MmError> {
    if name.is_empty()
        || name.len() > 255
        || !name.bytes().enumerate().all(|(index, value)| {
            value.is_ascii_alphanumeric() || (index > 0 && matches!(value, b'_' | b'.' | b'-'))
        })
    {
        return Err(MmError::Security {
            message: format!("invalid Podman resource name '{name}'"),
        });
    }
    Ok(())
}

fn validate_resource_label(resource: &str) -> Result<(), MmError> {
    if resource.is_empty() || resource.len() > 255 || resource.chars().any(char::is_control) {
        return Err(MmError::Security {
            message: "mm runtime resource label is empty, too long, or contains control characters"
                .to_owned(),
        });
    }
    Ok(())
}

fn validate_volume_file_name(file_name: &str) -> Result<PathBuf, MmError> {
    let path = Path::new(file_name);
    let mut components = path.components();
    let valid = !file_name.is_empty()
        && matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none();
    if !valid {
        return Err(MmError::Security {
            message: format!("invalid volume file name '{file_name}'"),
        });
    }
    Ok(path.to_path_buf())
}

fn unix_mode(mode: u32) -> Result<String, MmError> {
    if mode & !0o777 != 0 {
        return Err(MmError::Security {
            message: format!("invalid Unix file mode {mode:#o}"),
        });
    }
    Ok(format!("{mode:04o}"))
}

fn validate_mountpoint(mountpoint: &str, volume_name: &str) -> Result<PathBuf, MmError> {
    let path = Path::new(mountpoint);
    if mountpoint.is_empty()
        || mountpoint.chars().any(char::is_control)
        || !path.is_absolute()
        || path == Path::new("/")
        || path.components().any(|component| {
            matches!(
                component,
                Component::CurDir | Component::ParentDir | Component::Prefix(_)
            )
        })
    {
        return Err(MmError::RuntimeResponse {
            message: format!("Podman returned an invalid mountpoint for volume '{volume_name}'"),
        });
    }
    Ok(path.to_path_buf())
}

fn verified_local_source(source: &Path) -> Result<PathBuf, MmError> {
    ensure_no_symlink_components(source)?;
    let metadata = fs::symlink_metadata(source).map_err(|source_error| MmError::Io {
        context: format!("inspecting volume-file source {}", source.display()),
        source: source_error,
    })?;
    if !metadata.file_type().is_file() {
        return Err(MmError::Security {
            message: format!(
                "volume-file source '{}' must be a regular, non-symlink file",
                source.display()
            ),
        });
    }
    let canonical = dunce::canonicalize(source).map_err(|source_error| MmError::Io {
        context: format!("resolving volume-file source {}", source.display()),
        source: source_error,
    })?;
    ensure_no_symlink_components(&canonical)?;
    Ok(canonical)
}

fn path_argument(path: &Path, description: &str) -> Result<String, MmError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| MmError::Security {
            message: format!("{description} '{}' is not valid Unicode", path.display()),
        })
}

fn require_local_target(target: &RuntimeTarget) -> Result<TargetId, MmError> {
    let resolved = target
        .target
        .as_ref()
        .ok_or_else(|| MmError::RuntimeResponse {
            message: "runtime target has no resolved stable target identity".to_owned(),
        })?;
    require_local_named_target(resolved)
}

fn require_local_named_target(target: &NamedTarget) -> Result<TargetId, MmError> {
    match target.definition.transport {
        TargetTransport::Local => Ok(target.definition.id),
        TargetTransport::Ssh { .. } => Err(MmError::RuntimeResponse {
            message: format!(
                "target '{}' uses SSH, but this runtime driver supports only local targets",
                target.name
            ),
        }),
    }
}

impl PodmanComposeRuntime {
    async fn populate_health(
        &self,
        root: &std::path::Path,
        containers: &mut [ContainerObservation],
    ) -> Result<(), MmError> {
        let ids = containers
            .iter()
            .map(|container| container.id.clone())
            .filter(|id| !id.is_empty())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(());
        }

        for batch in ids.chunks(32) {
            if let Some(health) = self.query_health(root, batch).await? {
                apply_health(containers, health);
                continue;
            }

            // A container may disappear between `ps` and `inspect`; retrying each ID
            // preserves health for the containers that still exist.
            for id in batch {
                if let Some(health) = self.query_health(root, std::slice::from_ref(id)).await? {
                    apply_health(containers, health);
                }
            }
        }
        Ok(())
    }

    async fn query_health(
        &self,
        root: &std::path::Path,
        ids: &[String],
    ) -> Result<Option<Vec<(String, Option<String>)>>, MmError> {
        const FORMAT: &str =
            "{{.Id}}\t{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}";
        let output = self
            .process
            .run(
                self.command_spec(
                    [
                        "inspect".to_owned(),
                        "--format".to_owned(),
                        FORMAT.to_owned(),
                    ]
                    .into_iter()
                    .chain(ids.iter().cloned()),
                )
                .current_dir(root)
                .timeout(Duration::from_secs(30)),
            )
            .await?;
        if !output.success() {
            tracing::debug!(
                ids = ?ids,
                stderr = ?output.stderr.trim(),
                "container disappeared before health inspection"
            );
            return Ok(None);
        }
        if output.truncated {
            return Err(MmError::RuntimeResponse {
                message: "Podman inspect output exceeded the output limit".to_owned(),
            });
        }
        let mut health = Vec::new();
        for line in output.stdout.lines().filter(|line| !line.trim().is_empty()) {
            let (id, status) = line
                .split_once('\t')
                .ok_or_else(|| MmError::RuntimeResponse {
                    message: "Podman inspect returned malformed health output".to_owned(),
                })?;
            health.push((
                id.to_owned(),
                (status != "none" && !status.is_empty()).then(|| status.to_owned()),
            ));
        }
        Ok(Some(health))
    }
}

fn apply_health(containers: &mut [ContainerObservation], health: Vec<(String, Option<String>)>) {
    for (inspected_id, status) in health {
        if let Some(container) = containers.iter_mut().find(|container| {
            inspected_id.starts_with(&container.id) || container.id.starts_with(&inspected_id)
        }) && let Some(status) = status
        {
            container.health = Some(status.clone());
            if status.eq_ignore_ascii_case("unhealthy") {
                container.diagnostic_code = Some("MM_RUNTIME_UNHEALTHY".to_owned());
            } else if container.diagnostic_code.as_deref() == Some("MM_RUNTIME_UNHEALTHY") {
                container.diagnostic_code = None;
            }
        }
    }
}

fn first_nonempty_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

fn provider_name(stdout: &str) -> String {
    let normalized = stdout.to_ascii_lowercase();
    if normalized.contains("docker compose") || normalized.contains("docker-compose") {
        "docker-compose".to_owned()
    } else if normalized.contains("podman-compose") {
        "podman-compose".to_owned()
    } else {
        "compose-provider".to_owned()
    }
}

fn parse_provider_executable(stderr: &str) -> Option<PathBuf> {
    let marker = "compose provider ";
    let start = stderr.find(marker)? + marker.len();
    let quoted = &stderr[start..];
    if !quoted.starts_with('"') {
        return None;
    }
    let mut escaped = false;
    let mut closing = None;
    for (index, character) in quoted.char_indices().skip(1) {
        if character == '"' && !escaped {
            closing = Some(index);
            break;
        }
        if character == '\\' {
            escaped = !escaped;
        } else {
            escaped = false;
        }
    }
    let token = &quoted[..=closing?];
    match serde_json::from_str::<String>(token) {
        Ok(path) => Some(PathBuf::from(path)),
        Err(_) => None,
    }
}

#[cfg(test)]
fn container_from_json(value: &Value) -> Option<ContainerObservation> {
    let all_labels = value_for_keys(value, &["Labels", "labels"])
        .and_then(Value::as_object)
        .map(|labels| {
            labels
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_owned()))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let service = all_labels.get("com.docker.compose.service")?.clone();
    let labels = all_labels
        .into_iter()
        .filter(|(key, _)| {
            matches!(
                key.as_str(),
                "com.docker.compose.project"
                    | "com.docker.compose.service"
                    | "com.docker.compose.container-number"
                    | "com.docker.compose.oneoff"
                    | "io.podman.compose.project"
                    | "io.podman.compose.service"
            )
        })
        .collect();

    let status = string_for_keys(value, &["Status", "status"]).unwrap_or_default();
    let health = string_for_keys(value, &["Health", "health"])
        .filter(|health| !health.is_empty())
        .or_else(|| health_from_status(&status));
    let diagnostic_code =
        (health.as_deref() == Some("unhealthy")).then(|| "MM_RUNTIME_UNHEALTHY".to_owned());

    Some(ContainerObservation {
        id: string_for_keys(value, &["Id", "ID", "id"]).unwrap_or_default(),
        name: name_for_container(value),
        service,
        image: string_for_keys(value, &["Image", "image"]).unwrap_or_default(),
        state: string_for_keys(value, &["State", "state"]).unwrap_or_else(|| "unknown".to_owned()),
        status,
        health,
        restart_count: None,
        exit_code: None,
        created_at: string_for_keys(value, &["CreatedAt", "created_at"]),
        started_at: string_for_keys(value, &["StartedAt", "started_at"]),
        finished_at: string_for_keys(value, &["FinishedAt", "finished_at"]),
        revision: None,
        digest: string_for_keys(value, &["Image", "image"])
            .and_then(|image| image.rsplit_once('@').map(|(_, digest)| digest.to_owned())),
        diagnostic_code,
        error: None,
        labels,
        node: None,
        history: false,
    })
}

#[cfg(test)]
fn health_from_status(status: &str) -> Option<String> {
    let status = status.to_ascii_lowercase();
    if status.contains("unhealthy") {
        Some("unhealthy".to_owned())
    } else if status.contains("health: starting") || status.contains("(starting)") {
        Some("starting".to_owned())
    } else if status.contains("(healthy)") {
        Some("healthy".to_owned())
    } else {
        None
    }
}

#[cfg(test)]
fn name_for_container(value: &Value) -> String {
    let names = value_for_keys(value, &["Names", "names"]);
    if let Some(name) = names.and_then(Value::as_str) {
        return name.to_owned();
    }
    names
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn string_for_keys(value: &Value, keys: &[&str]) -> Option<String> {
    value_for_keys(value, keys)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn value_for_keys<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| value.get(key))
}
