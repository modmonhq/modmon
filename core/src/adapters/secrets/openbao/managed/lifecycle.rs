macro_rules! managed_openbao_lifecycle { () => {
    async fn upgrade(&self, plan: &ManagedOpenBaoPlan) -> Result<ManagedOpenBaoResult, MmError> {
        let previous = self.applied_state()?;
        let rollback_backup = self.root.join("rollback.age");
        self.backup(&rollback_backup).await?;
        let mut transition = previous.clone();
        transition.image = plan.image.clone();
        transition.locations = plan.locations.clone();
        transition.service = plan.service.clone();
        transition.container = plan.service.identity.service_name.clone();
        transition.plan_hash = plan.plan_hash.clone();
        transition.journal = ManagedJournal::Applying;
        transition.previous = Some(Box::new(previous.clone()));
        self.save_state(&transition)?;
        self.runtime.remove(&previous.service, false).await?;
        let upgraded = async {
            prepare_ingress(
                &transition.config_directory,
                &transition.service,
                &transition.ingress_host,
            )?;
            self.start(&transition).await?;
            let connection = self.ephemeral_connection(&transition)?;
            self.wait_for_health(&connection, true).await?;
            let health = self.health(&connection).await?;
            if health.sealed {
                let recovery = self.connections.recovery(&transition.installation)?;
                self.submit_unseal(&connection, recovery.unseal_key.as_str())
                    .await?;
            }
            self.wait_for_health(&connection, false).await?;
            self.configure_scopes(&transition, &plan.locations).await
        }
        .await;
        if let Err(upgrade_error) = upgraded {
            let mut cleanup = Vec::new();
            if let Err(error) = self.runtime.remove(&transition.service, false).await {
                cleanup.push(error.to_string());
            }
            let rollback = async {
                self.start(&previous).await?;
                let connection = self.ephemeral_connection(&previous)?;
                self.wait_for_health(&connection, true).await?;
                let health = self.health(&connection).await?;
                if health.sealed {
                    let recovery = self.connections.recovery(&previous.installation)?;
                    self.submit_unseal(&connection, recovery.unseal_key.as_str())
                        .await?;
                }
                self.save_state(&previous)
            }
            .await;
            let restored = rollback.is_ok();
            if let Err(error) = rollback {
                cleanup.push(error.to_string());
            }
            if let Err(error) = fs::remove_file(&rollback_backup) {
                cleanup.push(error.to_string());
            }
            let primary = MmError::Security {
                message: if restored {
                    format!(
                        "managed provider upgrade failed and the pinned previous image was restored: {upgrade_error}"
                    )
                } else {
                    format!(
                        "managed provider upgrade failed and requires recovery: {upgrade_error}"
                    )
                },
            };
            return Err(with_cleanup_failures(
                primary,
                cleanup,
                &plan.plan_hash,
                restored,
            ));
        }
        transition.journal = ManagedJournal::Applied;
        transition.previous = None;
        self.save_state(&transition)?;
        fs::remove_file(&rollback_backup).map_err(|source| MmError::Io {
            context: "removing managed provider rollback backup".to_owned(),
            source,
        })?;
        Ok(ManagedOpenBaoResult {
            action: plan.action,
            plan_hash: plan.plan_hash.clone(),
            rolled_back: false,
            data_purged: false,
        })
    }

    async fn remove(&self, plan: &ManagedOpenBaoPlan) -> Result<ManagedOpenBaoResult, MmError> {
        let mut state = self.applied_state()?;
        state.journal = ManagedJournal::Removing;
        self.save_state(&state)?;
        self.runtime.remove(&state.service, plan.purge_data).await?;
        if plan.purge_data {
            self.connections.remove_connection(&state.connection)?;
            self.connections.remove_recovery(&state.installation)?;
            if state.config_directory.exists() {
                fs::remove_dir_all(&state.config_directory).map_err(|source| MmError::Io {
                    context: "removing managed provider TLS/config directory".to_owned(),
                    source,
                })?;
            }
            self.remove_state()?;
        } else {
            state.journal = ManagedJournal::Retained;
            state.plan_hash = plan.plan_hash.clone();
            self.save_state(&state)?;
        }
        Ok(ManagedOpenBaoResult {
            action: plan.action,
            plan_hash: plan.plan_hash.clone(),
            rolled_back: false,
            data_purged: plan.purge_data,
        })
    }

    async fn start(&self, state: &ManagedOpenBaoState) -> Result<(), MmError> {
        self.runtime.apply(&state.service).await
    }

    async fn configure_scopes(
        &self,
        state: &ManagedOpenBaoState,
        locations: &[String],
    ) -> Result<(), MmError> {
        let connection = self.connections.connection(&state.connection)?;
        let recovery = self.connections.recovery(&state.installation)?;
        OpenBaoConnectionAdministrator::new(
            Arc::clone(&self.connections),
            Arc::clone(&self.transport),
        )
        .setup(OpenBaoSetupRequest {
            name: state.connection.clone(),
            mode: OpenBaoMode::Managed,
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

    async fn submit_unseal(
        &self,
        connection: &OpenBaoConnection,
        key: &str,
    ) -> Result<(), MmError> {
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
        if !(200..300).contains(&response.status) {
            return Err(MmError::Security {
                message: format!(
                    "managed provider unseal failed with HTTP {}",
                    response.status
                ),
            });
        }
        Ok(())
    }

    async fn wait_for_health(
        &self,
        connection: &OpenBaoConnection,
        allow_sealed: bool,
    ) -> Result<OpenBaoHealth, MmError> {
        let administrator = OpenBaoConnectionAdministrator::new(
            Arc::clone(&self.connections),
            Arc::clone(&self.transport),
        );
        let mut last = None;
        for _ in 0..40 {
            match administrator.health(connection).await {
                Ok(health) if allow_sealed || health.healthy => return Ok(health),
                Ok(health) => last = Some(format!("sealed={}", health.sealed)),
                Err(error) => last = Some(error.to_string()),
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Err(MmError::RuntimeResponse {
            message: format!(
                "managed provider did not become healthy: {}",
                last.unwrap_or_else(|| "no health response".to_owned())
            ),
        })
    }

    async fn health(&self, connection: &OpenBaoConnection) -> Result<OpenBaoHealth, MmError> {
        OpenBaoConnectionAdministrator::new(
            Arc::clone(&self.connections),
            Arc::clone(&self.transport),
        )
        .health(connection)
        .await
    }

    fn ephemeral_connection(
        &self,
        state: &ManagedOpenBaoState,
    ) -> Result<OpenBaoConnection, MmError> {
        let ca = fs::read_to_string(state.config_directory.join("ca.pem")).map_err(|source| {
            MmError::Io {
                context: "reading managed provider CA".to_owned(),
                source,
            }
        })?;
        self.ephemeral_connection_with_ca(state, ca)
    }

    fn ephemeral_connection_with_ca(
        &self,
        state: &ManagedOpenBaoState,
        ca_pem: String,
    ) -> Result<OpenBaoConnection, MmError> {
        Ok(OpenBaoConnection {
            mode: OpenBaoMode::Managed,
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
        })
    }

    fn applied_state(&self) -> Result<ManagedOpenBaoState, MmError> {
        let state = self.load_state()?.ok_or_else(|| MmError::State {
            message: "managed provider is not installed".to_owned(),
        })?;
        if state.journal != ManagedJournal::Applied {
            return Err(MmError::State {
                message: "managed provider journal requires recovery before this operation"
                    .to_owned(),
            });
        }
        Ok(state)
    }

    fn state_path(&self) -> PathBuf {
        self.root.join("state.json")
    }

    fn load_state(&self) -> Result<Option<ManagedOpenBaoState>, MmError> {
        let path = self.state_path();
        if !path.exists() {
            return Ok(None);
        }
        ensure_private_file(&path)?;
        let bytes = read_regular_file(&path, 1024 * 1024)?.ok_or_else(|| MmError::State {
            message: "managed provider state disappeared".to_owned(),
        })?;
        let state = serde_json::from_slice::<ManagedOpenBaoState>(&bytes).map_err(|error| {
            MmError::State {
                message: format!("managed provider state is invalid: {error}"),
            }
        })?;
        if state.schema != MANAGED_STATE_SCHEMA || !supported_image(&state.image) {
            return Err(MmError::Security {
                message: "managed provider state schema or image is unsupported".to_owned(),
            });
        }
        Ok(Some(state))
    }

    fn save_state(&self, state: &ManagedOpenBaoState) -> Result<(), MmError> {
        let bytes = serde_json::to_vec_pretty(state).map_err(|error| MmError::State {
            message: format!("could not serialize managed provider state: {error}"),
        })?;
        write_private(&self.state_path(), &bytes)
    }

    fn remove_state(&self) -> Result<(), MmError> {
        let path = self.state_path();
        if path.exists() {
            ensure_private_file(&path)?;
            fs::remove_file(&path).map_err(|source| MmError::Io {
                context: "removing managed provider state".to_owned(),
                source,
            })?;
        }
        Ok(())
    }
} }
