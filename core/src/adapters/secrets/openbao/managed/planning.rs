macro_rules! managed_openbao_planning { () => {
    #[must_use]
    pub fn new(
        root: PathBuf,
        connections: Arc<OpenBaoConnectionStore>,
        transport: Arc<dyn OpenBaoHttpTransport>,
        runtime: Arc<dyn ManagedServiceRuntime>,
    ) -> Self {
        Self {
            root,
            connections,
            transport,
            runtime,
        }
    }

    pub fn plan(&self, request: ManagedOpenBaoPlanRequest) -> Result<ManagedOpenBaoPlan, MmError> {
        let current = self.load_state()?;
        let locations = Self::validate_plan_request(&request, current.as_ref())?;
        let previous_state_digest = current
            .as_ref()
            .map(ManagedOpenBaoState::digest)
            .transpose()?
            .unwrap_or_else(|| "absent".to_owned());
        let ingress_host = managed_host(&request.connection);
        let generation = lifecycle_generation(&request, &previous_state_digest)?;
        let config_directory = self.root.join("installations").join(&request.installation);
        let service = self.runtime.plan(service_request(
            &request,
            &config_directory,
            generation,
            self.runtime.kind(),
        ))?;
        let mut plan = ManagedOpenBaoPlan {
            schema: MANAGED_STATE_SCHEMA,
            action: request.action,
            installation: request.installation,
            connection: request.connection,
            project_id: request.project_id,
            environment: request.environment,
            locations,
            image: request.image,
            previous_image: current.as_ref().map(|state| state.image.clone()),
            host_port: request.host_port,
            volume: service.identity.data_volume.clone(),
            config_volume: service.identity.config_volume.clone(),
            ingress_host,
            management_access: "local-loopback-only".to_owned(),
            purge_data: request.purge_data,
            previous_state_digest,
            service,
            plan_hash: String::new(),
        };
        plan.plan_hash = plan.canonical_hash()?;
        plan.validate()?;
        Ok(plan)
    }

    fn validate_plan_request(
        request: &ManagedOpenBaoPlanRequest,
        current: Option<&ManagedOpenBaoState>,
    ) -> Result<Vec<String>, MmError> {
        super::connection_store::validate_name(&request.installation)?;
        super::connection_store::validate_name(&request.connection)?;
        super::connection_store::validate_path_segment(&request.environment, "environment")?;
        if !supported_image(&request.image) {
            return Err(MmError::Security {
                message: "managed OpenBao image is not in the tested pinned 2.3.x/2.4.x matrix".to_owned(),
            });
        }
        match request.action {
            ManagedOpenBaoAction::Install
                if current
                    .as_ref()
                    .is_some_and(|state| state.journal != ManagedJournal::Retained) =>
            {
                return Err(MmError::State {
                    message: "managed provider is already installed".to_owned(),
                });
            }
            ManagedOpenBaoAction::Install => {}
            _ if current.is_none() => {
                return Err(MmError::State {
                    message: format!(
                        "cannot {} an uninstalled managed provider",
                        request.action.as_str()
                    ),
                });
            }
            _ => {}
        }
        if request.purge_data && request.action != ManagedOpenBaoAction::Remove {
            return Err(MmError::Security {
                message: "managed provider data purge is valid only for remove".to_owned(),
            });
        }
        if request.action == ManagedOpenBaoAction::Upgrade
            && current
                .as_ref()
                .is_some_and(|state| state.image == request.image)
        {
            return Err(MmError::State {
                message: "managed provider already uses the requested pinned image".to_owned(),
            });
        }
        if let Some(state) = &current {
            if request.action == ManagedOpenBaoAction::Upgrade
                && (state.image != OPENBAO_2_3_IMAGE || request.image != OPENBAO_2_4_IMAGE)
            {
                return Err(MmError::Security {
                    message: "managed provider supports only the reviewed 2.3-to-2.4 upgrade path"
                        .to_owned(),
                });
            }
            if request.action != ManagedOpenBaoAction::Upgrade && state.image != request.image {
                return Err(MmError::Security {
                    message:
                        "managed provider image changes require an explicit supported upgrade plan"
                            .to_owned(),
                });
            }
        }
        if let Some(state) = &current
            && (state.installation != request.installation
                || state.connection != request.connection
                || state.project_id != request.project_id
                || state.environment != request.environment
                || state.host_port != request.host_port)
        {
            return Err(MmError::Security {
                message: "managed provider ownership, connection, environment, or port differs from recorded state"
                    .to_owned(),
            });
        }
        if current.is_none() || matches!(request.action, ManagedOpenBaoAction::Install | ManagedOpenBaoAction::Restart | ManagedOpenBaoAction::Upgrade) {
            super::admin::normalized_locations(request.locations.clone())
        } else {
            Ok(current.map(|state| state.locations.clone()).unwrap_or_default())
        }
    }

    pub async fn execute_reviewed(
        &self,
        request: ManagedOpenBaoPlanRequest,
        expected_plan_hash: &str,
    ) -> Result<ManagedOpenBaoResult, MmError> {
        let plan = self.plan(request)?;
        if plan.plan_hash != expected_plan_hash {
            return Err(MmError::State {
                message: format!(
                    "managed provider plan changed (expected {expected_plan_hash}, current {}); review the new plan",
                    plan.plan_hash
                ),
            });
        }
        plan.validate()?;
        match plan.action {
            ManagedOpenBaoAction::Install => self.install(&plan).await,
            ManagedOpenBaoAction::Restart => self.restart(&plan).await,
            ManagedOpenBaoAction::Upgrade => self.upgrade(&plan).await,
            ManagedOpenBaoAction::Seal => self.seal(&plan).await,
            ManagedOpenBaoAction::Unseal => self.unseal(&plan).await,
            ManagedOpenBaoAction::Remove => self.remove(&plan).await,
        }
    }

    pub async fn status(&self) -> Result<ManagedOpenBaoStatus, MmError> {
        let Some(state) = self.load_state()? else {
            return Ok(ManagedOpenBaoStatus {
                installed: false,
                installation: None,
                connection: None,
                image: None,
                volume: None,
                ingress_url: format!("https://{OPENBAO_MANAGED_HOST}"),
                management_url: None,
                management_access: "local-loopback-only".to_owned(),
                health: None,
                recovery_available_off_instance: false,
                journal: None,
                diagnostics: vec!["managed provider is not installed".to_owned()],
            });
        };
        if state.journal == ManagedJournal::Retained {
            let recovery = self.connections.has_recovery(&state.installation)?;
            return Ok(ManagedOpenBaoStatus {
                installed: false,
                installation: Some(state.installation),
                connection: Some(state.connection),
                image: Some(state.image),
                volume: Some(state.volume),
                ingress_url: format!("https://{}", state.ingress_host),
                management_url: None,
                management_access: "local-loopback-only".to_owned(),
                health: None,
                recovery_available_off_instance: recovery,
                journal: Some("retained".to_owned()),
                diagnostics: vec![
                    "managed provider is removed with owned data retained; apply an install plan to reattach it"
                        .to_owned(),
                ],
            });
        }
        let connection = self.ephemeral_connection(&state)?;
        let mut diagnostics = Vec::new();
        let runtime_status = match self.runtime.observe(&state.service).await {
            Ok(status) => Some(status),
            Err(error) => {
                diagnostics.push(format!("managed provider runtime observation failed: {error}"));
                None
            }
        };
        let health = match self.health(&connection).await {
            Ok(health) => Some(health),
            Err(error) => {
                diagnostics.push(format!("managed provider health probe failed: {error}"));
                None
            }
        };
        let recovery = self.connections.has_recovery(&state.installation)?;
        if state.journal != ManagedJournal::Applied {
            diagnostics.push("managed provider journal requires recovery".to_owned());
        }
        if health.as_ref().is_none_or(|health| !health.healthy) {
            diagnostics.push("managed provider is unavailable, unhealthy, or sealed".to_owned());
        }
        if runtime_status.as_ref().is_none_or(|status| !status.running) {
            diagnostics
                .push("selected runtime does not report the managed service running".to_owned());
        }
        if runtime_status
            .as_ref()
            .and_then(|status| status.image.as_deref())
            != Some(state.image.as_str())
        {
            diagnostics.push("selected runtime reports managed provider image drift".to_owned());
        }
        if !recovery {
            diagnostics.push("off-instance encrypted recovery material is missing".to_owned());
        }
        Ok(ManagedOpenBaoStatus {
            installed: true,
            installation: Some(state.installation),
            connection: Some(state.connection),
            image: Some(state.image),
            volume: Some(state.volume),
            ingress_url: format!("https://{}", state.ingress_host),
            management_url: Some(format!("https://127.0.0.1:{}", state.host_port)),
            management_access: "local-loopback-only".to_owned(),
            health,
            recovery_available_off_instance: recovery,
            journal: Some(format!("{:?}", state.journal).to_ascii_lowercase()),
            diagnostics,
        })
    }

    pub async fn backup(&self, destination: &Path) -> Result<String, MmError> {
        let state = self.applied_state()?;
        let connection = self.ephemeral_connection(&state)?;
        let recovery = self.connections.recovery(&state.installation)?;
        let response = self
            .transport
            .request_bytes(
                &connection,
                Method::GET,
                "sys/storage/raft/snapshot",
                recovery.root_token.as_str(),
                None,
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(MmError::Secret {
                message: format!(
                    "managed provider snapshot failed with HTTP {}",
                    response.status
                ),
            });
        }
        let digest = blake3::hash(&response.body).to_hex().to_string();
        let payload = Zeroizing::new(
            serde_json::to_vec(&ManagedBackup {
                schema: MANAGED_BACKUP_SCHEMA,
                installation: state.installation,
                project_id: state.project_id,
                environment: state.environment,
                connection: self.connections.connection(&state.connection)?,
                recovery,
                snapshot: response.body,
            })
            .map_err(|error| MmError::State {
                message: format!("could not serialize managed provider backup: {error}"),
            })?,
        );
        let encrypted = self.connections.encrypt_integrity_checked(&payload)?;
        write_private(destination, &encrypted)?;
        Ok(digest)
    }

    pub async fn restore(&self, source: &Path) -> Result<String, MmError> {
        let state = self.applied_state()?;
        let encrypted =
            read_regular_file(source, 128 * 1024 * 1024)?.ok_or_else(|| MmError::State {
                message: "managed provider backup does not exist".to_owned(),
            })?;
        let plaintext = self.connections.decrypt_integrity_checked(&encrypted)?;
        let backup =
            serde_json::from_slice::<ManagedBackup>(&plaintext).map_err(|_| MmError::Security {
                message: "managed provider backup envelope is invalid".to_owned(),
            })?;
        if backup.schema != MANAGED_BACKUP_SCHEMA
            || backup.installation != state.installation
            || backup.project_id != state.project_id
            || backup.environment != state.environment
            || backup.connection.mode != OpenBaoMode::Managed
            || backup.connection.managed_installation.as_deref()
                != Some(state.installation.as_str())
        {
            return Err(MmError::Security {
                message: "managed provider backup belongs to another installation or scope"
                    .to_owned(),
            });
        }
        let digest = blake3::hash(&backup.snapshot).to_hex().to_string();
        let connection = self.ephemeral_connection(&state)?;
        let current_recovery = self.connections.recovery(&state.installation)?;
        let response = self
            .transport
            .request_bytes(
                &connection,
                Method::POST,
                "sys/storage/raft/snapshot-force",
                current_recovery.root_token.as_str(),
                Some(&backup.snapshot),
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(MmError::Secret {
                message: format!(
                    "managed provider restore failed with HTTP {}",
                    response.status
                ),
            });
        }
        let mut sealed = false;
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if let Ok(health) = self.health(&connection).await
                && health.sealed
            {
                sealed = true;
                break;
            }
        }
        if !sealed {
            return Err(MmError::RuntimeResponse {
                message: "restored managed provider did not enter the sealed state".to_owned(),
            });
        }
        self.connections
            .save_recovery(&state.installation, backup.recovery.clone())?;
        self.connections
            .save_connection(&state.connection, backup.connection)?;
        self.submit_unseal(&connection, backup.recovery.unseal_key.as_str())
            .await?;
        self.wait_for_health(&connection, false).await?;
        Ok(digest)
    }

    pub(super) fn reviewed_state_digest(&self) -> Result<String, MmError> {
        self.applied_state()?.digest()
    }

    pub(super) fn reviewed_path_digest(&self, path: &Path) -> Result<Option<String>, MmError> {
        read_regular_file(path, 128 * 1024 * 1024)
            .map(|content| content.map(|content| blake3::hash(&content).to_hex().to_string()))
    }
} }
