macro_rules! managed_vault_install { () => {
    async fn install(&self, plan: &ManagedVaultPlan) -> Result<(), MmError> {
        if let Some(mut state) = self.load_state()?
            && state.journal == Journal::Retained
        {
            return self.reinstall_retained(plan, &mut state).await;
        }
        if self.connections.has_connection(&plan.connection)?
            || self.connections.has_recovery(&plan.installation)?
        {
            return Err(MmError::Security {
                message: "managed Vault private connection or recovery name is already in use"
                    .to_owned(),
            });
        }
        TcpListener::bind((Ipv4Addr::LOCALHOST, plan.host_port)).map_err(|source| {
            MmError::Security {
                message: format!(
                    "managed Vault loopback port {} is unavailable: {source}",
                    plan.host_port
                ),
            }
        })?;
        let config_directory = self.root.join("installations").join(&plan.installation);
        let ca_pem = prepare_configuration(
            &config_directory,
            plan.host_port,
            &plan.installation,
            &plan.ingress_host,
            &plan.service.identity.service_name,
        )?;
        prepare_ingress(&config_directory, &plan.service, &plan.ingress_host)?;
        let state = ManagedVaultState {
            schema: STATE_SCHEMA,
            installation: plan.installation.clone(),
            connection: plan.connection.clone(),
            project_id: plan.project_id,
            environment: plan.environment.clone(),
            locations: plan.locations.clone(),
            image: plan.image.clone(),
            host_port: plan.host_port,
            container: plan.service.identity.service_name.clone(),
            volume: plan.volume.clone(),
            config_volume: plan.config_volume.clone(),
            config_directory,
            ingress_host: plan.ingress_host.clone(),
            service: plan.service.clone(),
            plan_hash: plan.plan_hash.clone(),
            journal: Journal::Applying,
            previous: None,
        };
        self.save_state(&state)?;
        let result = self.initialize_install(&state, ca_pem).await;
        if let Err(error) = result {
            let cleanup = self.cleanup_failed_install(&state).await;
            return Err(with_cleanup_failures(
                error,
                cleanup,
                &plan.plan_hash,
                false,
            ));
        }
        Ok(())
    }

    async fn reinstall_retained(
        &self,
        plan: &ManagedVaultPlan,
        state: &mut ManagedVaultState,
    ) -> Result<(), MmError> {
        state.service = plan.service.clone();
        state.container = plan.service.identity.service_name.clone();
        state.volume = plan.service.identity.data_volume.clone();
        state.config_volume = plan.service.identity.config_volume.clone();
        state.ingress_host = plan.ingress_host.clone();
        prepare_ingress(&state.config_directory, &state.service, &state.ingress_host)?;
        self.start(state).await?;
        let connection = self.ephemeral_connection(state)?;
        self.wait_for_health(&connection, true).await?;
        if self.health(&connection).await?.sealed {
            let recovery = self.connections.recovery(&state.installation)?;
            self.submit_unseal(&connection, recovery.unseal_key.as_str()).await?;
        }
        self.configure_scopes(state, &plan.locations).await?;
        state.locations = plan.locations.clone();
        state.journal = Journal::Applied;
        state.plan_hash = plan.plan_hash.clone();
        self.save_state(state)
    }

    async fn initialize_install(&self, state: &ManagedVaultState, ca_pem: String) -> Result<(), MmError> {
        self.start(state).await?;
        let connection = self.ephemeral_connection_with_ca(state, ca_pem.clone());
        self.wait_for_health(&connection, true).await?;
        let init = self.transport.request(
            &connection,
            Method::POST,
            "sys/init",
            None,
            Some(&serde_json::json!({"secret_shares": 1, "secret_threshold": 1})),
        ).await?;
        if !(200..300).contains(&init.status) {
            return Err(MmError::Secret {
                message: format!("managed Vault initialization failed with HTTP {}", init.status),
            });
        }
        let mut initialized: InitializeResponse = serde_json::from_slice(&init.body)
            .map_err(|_| MmError::Secret {
                message: "managed Vault initialization response was malformed".to_owned(),
            })?;
        let recovery = VaultRecoveryMaterial {
            root_token: initialized.root_token,
            unseal_key: initialized.keys_base64.pop().ok_or_else(|| MmError::Security {
                message: "managed Vault returned no unseal key".to_owned(),
            })?,
        };
        // Persist recovery outside the Raft volume before first unseal.
        self.connections.save_recovery(&state.installation, recovery.clone())?;
        self.submit_unseal(&connection, recovery.unseal_key.as_str()).await?;
        self.wait_for_health(&connection, false).await?;
        VaultConnectionAdministrator::new(Arc::clone(&self.connections), Arc::clone(&self.transport))
            .setup(VaultSetupRequest {
                name: state.connection.clone(), mode: VaultMode::Managed,
                address: connection.address.clone(), namespace: None,
                mount: DEFAULT_MOUNT.to_owned(), auth_mount: DEFAULT_AUTH_MOUNT.to_owned(),
                ca_pem, expected_ca_sha256: connection.ca_sha256,
                project_id: state.project_id, environment: state.environment.clone(),
                locations: state.locations.clone(), developer_token: recovery.root_token,
                allow_mount_bootstrap: true, managed_installation: Some(state.installation.clone()),
            }).await?;
        let mut applied = state.clone();
        applied.journal = Journal::Applied;
        self.save_state(&applied)
    }

    async fn cleanup_failed_install(&self, state: &ManagedVaultState) -> Vec<String> {
        let mut cleanup = Vec::new();
        if let Err(error) = self.runtime.remove(&state.service, true).await {
            cleanup.push(error.to_string());
        }
        if let Err(error) = self.connections.remove_connection(&state.connection) {
            cleanup.push(error.to_string());
        }
        if let Err(error) = self.connections.remove_recovery(&state.installation) {
            cleanup.push(error.to_string());
        }
        if state.config_directory.exists() && let Err(error) = fs::remove_dir_all(&state.config_directory) {
            cleanup.push(error.to_string());
        }
        if let Err(error) = self.remove_state() {
            cleanup.push(error.to_string());
        }
        cleanup
    }

    async fn restart(&self, plan: &ManagedVaultPlan) -> Result<(), MmError> {
        let mut state = self.applied_state()?;
        state.service = plan.service.clone();
        state.container = plan.service.identity.service_name.clone();
        prepare_ingress(&state.config_directory, &state.service, &state.ingress_host)?;
        self.start(&state).await?;
        let connection = self.ephemeral_connection(&state)?;
        self.wait_for_health(&connection, true).await?;
        if self.health(&connection).await?.sealed {
            self.submit_unseal(
                &connection,
                self.connections
                    .recovery(&state.installation)?
                    .unseal_key
                    .as_str(),
            )
            .await?;
        }
        self.wait_for_health(&connection, false).await?;
        self.configure_scopes(&state, &plan.locations).await?;
        state.locations = plan.locations.clone();
        state.plan_hash = plan.plan_hash.clone();
        self.save_state(&state)
    }

    async fn upgrade(&self, plan: &ManagedVaultPlan) -> Result<(bool, bool), MmError> {
        let previous = self.applied_state()?;
        let rollback_backup = self.root.join("rollback.age");
        self.backup(&rollback_backup).await?;
        let mut transition = previous.clone();
        transition.image = plan.image.clone();
        transition.locations = plan.locations.clone();
        transition.service = plan.service.clone();
        transition.container = plan.service.identity.service_name.clone();
        transition.plan_hash = plan.plan_hash.clone();
        transition.journal = Journal::Applying;
        transition.previous = Some(Box::new(previous.clone()));
        self.save_state(&transition)?;
        self.runtime.remove(&previous.service, false).await?;
        let result = async {
            prepare_ingress(
                &transition.config_directory,
                &transition.service,
                &transition.ingress_host,
            )?;
            self.start(&transition).await?;
            let connection = self.ephemeral_connection(&transition)?;
            self.wait_for_health(&connection, true).await?;
            if self.health(&connection).await?.sealed {
                self.submit_unseal(
                    &connection,
                    self.connections
                        .recovery(&transition.installation)?
                        .unseal_key
                        .as_str(),
                )
                .await?;
            }
            self.wait_for_health(&connection, false).await?;
            self.configure_scopes(&transition, &plan.locations).await
        }
        .await;
        if let Err(error) = result {
            let mut cleanup = Vec::new();
            if let Err(cleanup_error) = self.runtime.remove(&transition.service, false).await {
                cleanup.push(cleanup_error.to_string());
            }
            let rollback = async {
                self.start(&previous).await?;
                let connection = self.ephemeral_connection(&previous)?;
                self.wait_for_health(&connection, true).await?;
                if self.health(&connection).await?.sealed {
                    self.submit_unseal(
                        &connection,
                        self.connections
                            .recovery(&previous.installation)?
                            .unseal_key
                            .as_str(),
                    )
                    .await?;
                }
                self.save_state(&previous)
            }
            .await;
            let restored = rollback.is_ok();
            if let Err(rollback_error) = rollback {
                cleanup.push(rollback_error.to_string());
            }
            if let Err(cleanup_error) = fs::remove_file(&rollback_backup) {
                cleanup.push(cleanup_error.to_string());
            }
            let primary = MmError::Security {
                message: if restored {
                    format!(
                        "managed Vault upgrade failed and the pinned previous image was restored: {error}"
                    )
                } else {
                    format!("managed Vault upgrade failed and requires recovery: {error}")
                },
            };
            return Err(with_cleanup_failures(
                primary,
                cleanup,
                &plan.plan_hash,
                restored,
            ));
        }
        transition.journal = Journal::Applied;
        transition.previous = None;
        self.save_state(&transition)?;
        fs::remove_file(&rollback_backup).map_err(|source| MmError::Io {
            context: "removing managed Vault rollback backup".to_owned(),
            source,
        })?;
        Ok((false, false))
    }
} }
