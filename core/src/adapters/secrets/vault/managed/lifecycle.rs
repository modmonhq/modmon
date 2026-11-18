macro_rules! managed_vault_lifecycle { () => {
    async fn seal(&self, _plan: &ManagedVaultPlan) -> Result<(), MmError> {
        let state = self.applied_state()?;
        let connection = self.ephemeral_connection(&state)?;
        let recovery = self.connections.recovery(&state.installation)?;
        let response = self
            .transport
            .request(
                &connection,
                Method::POST,
                "sys/seal",
                Some(recovery.root_token.as_str()),
                None,
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(MmError::Secret {
                message: format!("managed Vault seal failed with HTTP {}", response.status),
            });
        }
        if !self.wait_for_health(&connection, true).await?.sealed {
            return Err(MmError::RuntimeResponse {
                message: "managed Vault did not seal".to_owned(),
            });
        }
        Ok(())
    }

    async fn unseal(&self, _plan: &ManagedVaultPlan) -> Result<(), MmError> {
        let state = self.applied_state()?;
        let connection = self.ephemeral_connection(&state)?;
        self.submit_unseal(
            &connection,
            self.connections
                .recovery(&state.installation)?
                .unseal_key
                .as_str(),
        )
        .await?;
        self.wait_for_health(&connection, false).await.map(|_| ())
    }

    async fn remove(&self, plan: &ManagedVaultPlan) -> Result<bool, MmError> {
        let mut state = self.applied_state()?;
        state.journal = Journal::Removing;
        self.save_state(&state)?;
        self.runtime.remove(&state.service, plan.purge_data).await?;
        if plan.purge_data {
            self.connections.remove_connection(&state.connection)?;
            self.connections.remove_recovery(&state.installation)?;
            if state.config_directory.exists() {
                fs::remove_dir_all(&state.config_directory).map_err(|source| MmError::Io {
                    context: "removing managed Vault TLS directory".to_owned(),
                    source,
                })?;
            }
            self.remove_state()?;
        } else {
            state.journal = Journal::Retained;
            state.plan_hash = plan.plan_hash.clone();
            self.save_state(&state)?;
        }
        Ok(plan.purge_data)
    }

    async fn start(&self, state: &ManagedVaultState) -> Result<(), MmError> {
        self.runtime.apply(&state.service).await
    }

    async fn configure_scopes(
        &self,
        state: &ManagedVaultState,
        locations: &[String],
    ) -> Result<(), MmError> {
        let connection = self.connections.connection(&state.connection)?;
        let recovery = self.connections.recovery(&state.installation)?;
        VaultConnectionAdministrator::new(
            Arc::clone(&self.connections),
            Arc::clone(&self.transport),
        )
        .setup(VaultSetupRequest {
            name: state.connection.clone(),
            mode: VaultMode::Managed,
            address: connection.address,
            namespace: connection.namespace,
            mount: connection.mount,
            auth_mount: connection.auth_mount,
            ca_pem: connection.ca_pem,
            expected_ca_sha256: connection.ca_sha256,
            project_id: state.project_id,
            environment: state.environment.clone(),
            locations: locations.to_vec(),
            developer_token: recovery.root_token,
            allow_mount_bootstrap: false,
            managed_installation: Some(state.installation.clone()),
        })
        .await
        .map(|_| ())
    }

    async fn submit_unseal(&self, connection: &VaultConnection, key: &str) -> Result<(), MmError> {
        let response = self
            .transport
            .request(
                connection,
                Method::POST,
                "sys/unseal",
                None,
                Some(&serde_json::json!({"key": key})),
            )
            .await?;
        if (200..300).contains(&response.status) {
            Ok(())
        } else {
            Err(MmError::Security {
                message: format!("managed Vault unseal failed with HTTP {}", response.status),
            })
        }
    }

    async fn wait_for_seal(&self, connection: &VaultConnection) -> Result<(), MmError> {
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if self
                .health(connection)
                .await
                .is_ok_and(|health| health.sealed)
            {
                return Ok(());
            }
        }
        Err(MmError::RuntimeResponse {
            message: "restored managed Vault did not enter sealed state".to_owned(),
        })
    }

    async fn wait_for_health(
        &self,
        connection: &VaultConnection,
        allow_sealed: bool,
    ) -> Result<VaultHealth, MmError> {
        let mut last = None;
        for _ in 0..80 {
            match self.health(connection).await {
                Ok(health) if allow_sealed || health.healthy => return Ok(health),
                Ok(health) => last = Some(format!("sealed={}", health.sealed)),
                Err(error) => last = Some(error.to_string()),
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Err(MmError::RuntimeResponse {
            message: format!(
                "managed Vault did not become healthy: {}",
                last.unwrap_or_else(|| "no health response".to_owned())
            ),
        })
    }

    async fn health(&self, connection: &VaultConnection) -> Result<VaultHealth, MmError> {
        VaultConnectionAdministrator::new(
            Arc::clone(&self.connections),
            Arc::clone(&self.transport),
        )
        .health(connection)
        .await
    }

    fn ephemeral_connection(&self, state: &ManagedVaultState) -> Result<VaultConnection, MmError> {
        let ca = fs::read_to_string(state.config_directory.join("ca.pem")).map_err(|source| {
            MmError::Io {
                context: "reading managed Vault CA".to_owned(),
                source,
            }
        })?;
        Ok(self.ephemeral_connection_with_ca(state, ca))
    }

    fn ephemeral_connection_with_ca(
        &self,
        state: &ManagedVaultState,
        ca_pem: String,
    ) -> VaultConnection {
        VaultConnection {
            mode: VaultMode::Managed,
            address: format!("https://127.0.0.1:{}", state.host_port),
            namespace: None,
            mount: DEFAULT_MOUNT.to_owned(),
            auth_mount: DEFAULT_AUTH_MOUNT.to_owned(),
            ca_sha256: ca_sha256(&ca_pem),
            ca_pem,
            project_id: state.project_id,
            credentials: BTreeMap::new(),
            allowed_locations: BTreeMap::new(),
            managed_installation: Some(state.installation.clone()),
        }
    }

    fn applied_state(&self) -> Result<ManagedVaultState, MmError> {
        let state = self.load_state()?.ok_or_else(|| MmError::State {
            message: "managed Vault is not installed".to_owned(),
        })?;
        if state.journal != Journal::Applied {
            return Err(MmError::State {
                message: "managed Vault journal requires recovery".to_owned(),
            });
        }
        Ok(state)
    }
    fn state_path(&self) -> PathBuf {
        self.root.join("state.json")
    }
    fn load_state(&self) -> Result<Option<ManagedVaultState>, MmError> {
        let path = self.state_path();
        if !path.exists() {
            return Ok(None);
        }
        ensure_private_file(&path)?;
        let bytes = read_regular_file(&path, 1024 * 1024)?.ok_or_else(|| MmError::State {
            message: "managed Vault state disappeared".to_owned(),
        })?;
        let state: ManagedVaultState =
            serde_json::from_slice(&bytes).map_err(|error| MmError::State {
                message: format!("managed Vault state is invalid: {error}"),
            })?;
        if state.schema != STATE_SCHEMA || !supported_image(&state.image) {
            return Err(MmError::Security {
                message: "managed Vault state schema or image is unsupported".to_owned(),
            });
        }
        Ok(Some(state))
    }
    fn save_state(&self, state: &ManagedVaultState) -> Result<(), MmError> {
        write_private(
            &self.state_path(),
            &serde_json::to_vec_pretty(state).map_err(|error| MmError::State {
                message: format!("could not serialize managed Vault state: {error}"),
            })?,
        )
    }
    fn remove_state(&self) -> Result<(), MmError> {
        let path = self.state_path();
        if path.exists() {
            ensure_private_file(&path)?;
            fs::remove_file(path).map_err(|source| MmError::Io {
                context: "removing managed Vault state".to_owned(),
                source,
            })?;
        }
        Ok(())
    }
} }
