macro_rules! managed_vault_planning { () => {
    #[must_use]
    pub fn new(
        root: PathBuf,
        connections: Arc<VaultConnectionStore>,
        transport: Arc<dyn VaultHttpTransport>,
        runtime: Arc<dyn ManagedServiceRuntime>,
    ) -> Self {
        Self {
            root,
            connections,
            transport,
            runtime,
        }
    }

    pub(super) fn plan(&self, request: ManagedVaultRequest) -> Result<ManagedVaultPlan, MmError> {
        let current = self.load_state()?;
        let locations = Self::validate_plan_request(&request, current.as_ref())?;
        let previous_state_digest = current
            .as_ref()
            .map(ManagedVaultState::digest)
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
        let mut plan = ManagedVaultPlan {
            schema: STATE_SCHEMA,
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
        plan.plan_hash = plan.hash()?;
        plan.validate()?;
        Ok(plan)
    }

    fn validate_plan_request(
        request: &ManagedVaultRequest,
        current: Option<&ManagedVaultState>,
    ) -> Result<Vec<String>, MmError> {
        validate_name(&request.installation)?;
        validate_name(&request.connection)?;
        validate_segment(&request.environment, "Vault environment")?;
        if !supported_image(&request.image) {
            return Err(MmError::Security {
                message: "managed Vault image is outside the tested pinned 1.18.x/1.19.x matrix".to_owned(),
            });
        }
        match request.action {
            ManagedSecretBackendAction::Install
                if current
                    .as_ref()
                    .is_some_and(|state| state.journal != Journal::Retained) =>
            {
                return Err(MmError::State {
                    message: "managed Vault is already installed".to_owned(),
                });
            }
            ManagedSecretBackendAction::Install => {}
            _ if current.is_none() => {
                return Err(MmError::State {
                    message: format!(
                        "cannot {} an uninstalled managed Vault",
                        request.action.as_str()
                    ),
                });
            }
            _ => {}
        }
        if request.purge_data && request.action != ManagedSecretBackendAction::Remove {
            return Err(MmError::Security {
                message: "managed Vault data purge is valid only for remove".to_owned(),
            });
        }
        if let Some(state) = &current {
            if request.action == ManagedSecretBackendAction::Upgrade
                && (state.image != VAULT_1_18_IMAGE || request.image != VAULT_1_19_IMAGE)
            {
                return Err(MmError::Security {
                    message: "managed Vault supports only the reviewed 1.18-to-1.19 upgrade path"
                        .to_owned(),
                });
            }
            if request.action != ManagedSecretBackendAction::Upgrade && state.image != request.image
            {
                return Err(MmError::Security {
                    message: "managed Vault image changes require an explicit supported upgrade"
                        .to_owned(),
                });
            }
            if state.installation != request.installation
                || state.connection != request.connection
                || state.project_id != request.project_id
                || state.environment != request.environment
                || state.host_port != request.host_port
            {
                return Err(MmError::Security { message: "managed Vault ownership, connection, environment, or port differs from recorded state".to_owned() });
            }
        }
        if matches!(
            request.action,
            ManagedSecretBackendAction::Install
                | ManagedSecretBackendAction::Restart
                | ManagedSecretBackendAction::Upgrade
        ) {
            normalized_locations(request.locations.clone())
        } else {
            Ok(current.map(|state| state.locations.clone()).unwrap_or_default())
        }
    }

    pub(super) async fn execute(
        &self,
        request: ManagedVaultRequest,
        reviewed_hash: &str,
    ) -> Result<(bool, bool), MmError> {
        let plan = self.plan(request)?;
        if plan.plan_hash != reviewed_hash {
            return Err(MmError::State {
                message: format!(
                    "managed Vault plan changed (expected {reviewed_hash}, current {}); review again",
                    plan.plan_hash
                ),
            });
        }
        plan.validate()?;
        match plan.action {
            ManagedSecretBackendAction::Install => {
                self.install(&plan).await.map(|()| (false, false))
            }
            ManagedSecretBackendAction::Restart => {
                self.restart(&plan).await.map(|()| (false, false))
            }
            ManagedSecretBackendAction::Upgrade => self.upgrade(&plan).await,
            ManagedSecretBackendAction::Seal => self.seal(&plan).await.map(|()| (false, false)),
            ManagedSecretBackendAction::Unseal => self.unseal(&plan).await.map(|()| (false, false)),
            ManagedSecretBackendAction::Remove => {
                self.remove(&plan).await.map(|purged| (false, purged))
            }
        }
    }

    pub(super) async fn status(&self) -> Result<ManagedVaultStatus, MmError> {
        let Some(state) = self.load_state()? else {
            return Ok(ManagedVaultStatus {
                installed: false,
                installation: None,
                connection: None,
                image: None,
                volume: None,
                ingress_url: format!("https://{VAULT_MANAGED_HOST}"),
                management_url: None,
                health: None,
                recovery: false,
                journal: None,
                diagnostics: vec!["managed Vault is not installed".to_owned()],
            });
        };
        let recovery = self.connections.has_recovery(&state.installation)?;
        if state.journal == Journal::Retained {
            return Ok(ManagedVaultStatus {
                installed: false,
                installation: Some(state.installation),
                connection: Some(state.connection),
                image: Some(state.image),
                volume: Some(state.volume),
                ingress_url: format!("https://{}", state.ingress_host),
                management_url: None,
                health: None,
                recovery,
                journal: Some("retained".to_owned()),
                diagnostics: vec![
                    "managed Vault data is retained; install reattaches it".to_owned(),
                ],
            });
        }
        let mut diagnostics = Vec::new();
        let runtime_status = match self.runtime.observe(&state.service).await {
            Ok(status) => Some(status),
            Err(error) => {
                diagnostics.push(format!("managed Vault runtime observation failed: {error}"));
                None
            }
        };
        let health = match self.health(&self.ephemeral_connection(&state)?).await {
            Ok(health) => Some(health),
            Err(error) => {
                diagnostics.push(format!("managed Vault health probe failed: {error}"));
                None
            }
        };
        if state.journal != Journal::Applied {
            diagnostics.push("managed Vault journal requires recovery".to_owned());
        }
        if health.as_ref().is_none_or(|health| !health.healthy) {
            diagnostics.push("managed Vault is unavailable, unhealthy, or sealed".to_owned());
        }
        if runtime_status.as_ref().is_none_or(|status| !status.running) {
            diagnostics.push("selected runtime does not report managed Vault running".to_owned());
        }
        if runtime_status
            .as_ref()
            .and_then(|status| status.image.as_deref())
            != Some(state.image.as_str())
        {
            diagnostics.push("selected runtime reports managed Vault image drift".to_owned());
        }
        if !recovery {
            diagnostics
                .push("off-instance encrypted Vault recovery material is missing".to_owned());
        }
        Ok(ManagedVaultStatus {
            installed: true,
            installation: Some(state.installation),
            connection: Some(state.connection),
            image: Some(state.image),
            volume: Some(state.volume),
            ingress_url: format!("https://{}", state.ingress_host),
            management_url: Some(format!("https://127.0.0.1:{}", state.host_port)),
            health,
            recovery,
            journal: Some(format!("{:?}", state.journal).to_ascii_lowercase()),
            diagnostics,
        })
    }

    pub(super) fn state_digest(&self) -> Result<String, MmError> {
        self.applied_state()?.digest()
    }
    pub(super) fn path_digest(&self, path: &Path) -> Result<Option<String>, MmError> {
        read_regular_file(path, 128 * 1024 * 1024)
            .map(|value| value.map(|value| blake3::hash(&value).to_hex().to_string()))
    }

    pub(super) async fn backup(&self, destination: &Path) -> Result<String, MmError> {
        let state = self.applied_state()?;
        let recovery = self.connections.recovery(&state.installation)?;
        let response = self
            .transport
            .request_bytes(
                &self.ephemeral_connection(&state)?,
                Method::GET,
                "sys/storage/raft/snapshot",
                recovery.root_token.as_str(),
                None,
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(MmError::Secret {
                message: format!(
                    "managed Vault snapshot failed with HTTP {}",
                    response.status
                ),
            });
        }
        let digest = blake3::hash(&response.body).to_hex().to_string();
        let payload = Zeroizing::new(
            serde_json::to_vec(&ManagedBackup {
                schema: BACKUP_SCHEMA,
                installation: state.installation,
                project_id: state.project_id,
                environment: state.environment,
                connection: self.connections.connection(&state.connection)?,
                recovery,
                snapshot: response.body,
            })
            .map_err(|error| MmError::State {
                message: format!("could not serialize managed Vault backup: {error}"),
            })?,
        );
        write_private(
            destination,
            &self.connections.encrypt_integrity_checked(&payload)?,
        )?;
        Ok(digest)
    }

    pub(super) async fn restore(&self, source: &Path) -> Result<String, MmError> {
        let state = self.applied_state()?;
        let encrypted =
            read_regular_file(source, 128 * 1024 * 1024)?.ok_or_else(|| MmError::State {
                message: "managed Vault backup does not exist".to_owned(),
            })?;
        let plaintext = self.connections.decrypt_integrity_checked(&encrypted)?;
        if let Some(schema) = crate::domain::stored_state::stored_version(&plaintext, "schema") {
            check_schema(source, schema, BACKUP_SCHEMA, StoredOn::ThisMachine)?;
        }
        let backup: ManagedBackup =
            serde_json::from_slice(&plaintext).map_err(|_| MmError::Security {
                message: "managed Vault backup envelope is invalid".to_owned(),
            })?;
        if backup.schema != BACKUP_SCHEMA
            || backup.installation != state.installation
            || backup.project_id != state.project_id
            || backup.environment != state.environment
            || backup.connection.mode != VaultMode::Managed
            || backup.connection.managed_installation.as_deref()
                != Some(state.installation.as_str())
        {
            return Err(MmError::Security {
                message: "managed Vault backup belongs to another installation or scope".to_owned(),
            });
        }
        let digest = blake3::hash(&backup.snapshot).to_hex().to_string();
        let connection = self.ephemeral_connection(&state)?;
        let current = self.connections.recovery(&state.installation)?;
        let response = self
            .transport
            .request_bytes(
                &connection,
                Method::POST,
                "sys/storage/raft/snapshot-force",
                current.root_token.as_str(),
                Some(&backup.snapshot),
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(MmError::Secret {
                message: format!("managed Vault restore failed with HTTP {}", response.status),
            });
        }
        self.wait_for_seal(&connection).await?;
        self.connections
            .save_recovery(&state.installation, backup.recovery.clone())?;
        self.connections
            .save_connection(&state.connection, backup.connection)?;
        self.submit_unseal(&connection, backup.recovery.unseal_key.as_str())
            .await?;
        self.wait_for_health(&connection, false).await?;
        Ok(digest)
    }
} }
