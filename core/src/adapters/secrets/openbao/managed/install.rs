macro_rules! managed_openbao_install { () => {
    async fn install(&self, plan: &ManagedOpenBaoPlan) -> Result<ManagedOpenBaoResult, MmError> {
                if let Some(state) = self.load_state()?
            && state.journal == ManagedJournal::Retained
        {
            return self.reinstall_retained(plan, state).await;
        }
        if self.connections.has_connection(&plan.connection)?
            || self.connections.has_recovery(&plan.installation)?
        {
            return Err(MmError::Security {
                message: "managed provider private connection or recovery name is already in use"
                    .to_owned(),
            });
        }
        TcpListener::bind((Ipv4Addr::LOCALHOST, plan.host_port)).map_err(|source| {
            MmError::Security {
                message: format!(
                    "managed provider loopback port {} is unavailable before mutation: {source}",
                    plan.host_port
                ),
            }
        })?;
        let config_directory = self.root.join("installations").join(&plan.installation);
        let tls = prepare_configuration(
            &config_directory,
            plan.host_port,
            &plan.installation,
            &plan.ingress_host,
            &plan.service.identity.service_name,
        )?;
        prepare_ingress(&config_directory, &plan.service, &plan.ingress_host)?;
        let state = ManagedOpenBaoState {
            schema: MANAGED_STATE_SCHEMA,
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
            journal: ManagedJournal::Applying,
            previous: None,
        };
        self.save_state(&state)?;
        let result = self.initialize_install(plan, &state, tls).await;
        if let Err(error) = result {
            let cleanup = self.cleanup_failed_install(&state).await;
            return Err(with_cleanup_failures(
                error,
                cleanup,
                &plan.plan_hash,
                false,
            ));
        }
        result
    }

    async fn initialize_install(
        &self,
        plan: &ManagedOpenBaoPlan,
        state: &ManagedOpenBaoState,
        tls: ManagedTls,
    ) -> Result<ManagedOpenBaoResult, MmError> {
        self.start(state).await?;
        let connection = self.ephemeral_connection_with_ca(state, tls.ca_pem.clone())?;
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
                message: format!("managed provider initialization failed with HTTP {}", init.status),
            });
        }
        let mut initialized = serde_json::from_slice::<InitializeResponse>(&init.body)
            .map_err(|_| MmError::Secret {
                message: "managed provider initialization response was malformed".to_owned(),
            })?;
        let recovery = ManagedRecoveryMaterial {
            root_token: initialized.root_token,
            unseal_key: initialized.keys_base64.pop().ok_or_else(|| MmError::Security {
                message: "managed provider returned no recovery key".to_owned(),
            })?,
        };
        // Recovery is durably encrypted outside the Raft volume before the first unseal.
        self.connections.save_recovery(&state.installation, recovery.clone())?;
        self.submit_unseal(&connection, recovery.unseal_key.as_str()).await?;
        self.wait_for_health(&connection, false).await?;
        OpenBaoConnectionAdministrator::new(Arc::clone(&self.connections), Arc::clone(&self.transport))
            .setup(OpenBaoSetupRequest {
                name: state.connection.clone(), mode: OpenBaoMode::Managed,
                address: connection.address.clone(), namespace: None,
                mount: DEFAULT_MOUNT.to_owned(), auth_mount: DEFAULT_AUTH_MOUNT.to_owned(),
                ca_pem: tls.ca_pem, expected_ca_sha256: connection.ca_sha256,
                project_id: state.project_id, environment: state.environment.clone(),
                locations: state.locations.clone(), developer_token: recovery.root_token,
                allow_mount_bootstrap: true, managed_installation: Some(state.installation.clone()),
            }).await?;
        let mut applied = state.clone();
        applied.journal = ManagedJournal::Applied;
        self.save_state(&applied)?;
        Ok(ManagedOpenBaoResult {
            action: plan.action, plan_hash: plan.plan_hash.clone(),
            rolled_back: false, data_purged: false,
        })
    }

    async fn cleanup_failed_install(&self, state: &ManagedOpenBaoState) -> Vec<String> {
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

    async fn reinstall_retained(
        &self,
        plan: &ManagedOpenBaoPlan,
        mut state: ManagedOpenBaoState,
    ) -> Result<ManagedOpenBaoResult, MmError> {
        let result = async {
            state.service = plan.service.clone();
            state.container = plan.service.identity.service_name.clone();
            state.volume = plan.service.identity.data_volume.clone();
            state.config_volume = plan.service.identity.config_volume.clone();
            state.ingress_host = plan.ingress_host.clone();
            prepare_ingress(&state.config_directory, &state.service, &state.ingress_host)?;
            self.start(&state).await?;
            let connection = self.ephemeral_connection(&state)?;
            self.wait_for_health(&connection, true).await?;
            let health = self.health(&connection).await?;
            if health.sealed {
                let recovery = self.connections.recovery(&state.installation)?;
                self.submit_unseal(&connection, recovery.unseal_key.as_str())
                    .await?;
            }
            self.wait_for_health(&connection, false).await?;
            self.configure_scopes(&state, &plan.locations).await?;
            state.locations = plan.locations.clone();
            state.journal = ManagedJournal::Applied;
            state.plan_hash = plan.plan_hash.clone();
            self.save_state(&state)?;
            Ok(ManagedOpenBaoResult {
                action: plan.action,
                plan_hash: plan.plan_hash.clone(),
                rolled_back: false,
                data_purged: false,
            })
        }
        .await;
        match result {
            Ok(result) => Ok(result),
            Err(error) => {
                let cleanup = self.runtime.remove(&state.service, false).await.err();
                Err(with_cleanup_failures(
                    error,
                    cleanup
                        .into_iter()
                        .map(|failure| failure.to_string())
                        .collect(),
                    &plan.plan_hash,
                    false,
                ))
            }
        }
    }

    async fn restart(&self, plan: &ManagedOpenBaoPlan) -> Result<ManagedOpenBaoResult, MmError> {
        let mut state = self.applied_state()?;
        state.service = plan.service.clone();
        state.container = plan.service.identity.service_name.clone();
        prepare_ingress(&state.config_directory, &state.service, &state.ingress_host)?;
        self.start(&state).await?;
        let connection = self.ephemeral_connection(&state)?;
        self.wait_for_health(&connection, true).await?;
        let health = self.health(&connection).await?;
        if health.sealed {
            let recovery = self.connections.recovery(&state.installation)?;
            self.submit_unseal(&connection, recovery.unseal_key.as_str())
                .await?;
        }
        self.wait_for_health(&connection, false).await?;
        self.configure_scopes(&state, &plan.locations).await?;
        state.locations = plan.locations.clone();
        state.plan_hash = plan.plan_hash.clone();
        self.save_state(&state)?;
        Ok(ManagedOpenBaoResult {
            action: plan.action,
            plan_hash: plan.plan_hash.clone(),
            rolled_back: false,
            data_purged: false,
        })
    }

    async fn unseal(&self, plan: &ManagedOpenBaoPlan) -> Result<ManagedOpenBaoResult, MmError> {
        let state = self.applied_state()?;
        let connection = self.ephemeral_connection(&state)?;
        let recovery = self.connections.recovery(&state.installation)?;
        self.submit_unseal(&connection, recovery.unseal_key.as_str())
            .await?;
        self.wait_for_health(&connection, false).await?;
        Ok(ManagedOpenBaoResult {
            action: plan.action,
            plan_hash: plan.plan_hash.clone(),
            rolled_back: false,
            data_purged: false,
        })
    }

    async fn seal(&self, plan: &ManagedOpenBaoPlan) -> Result<ManagedOpenBaoResult, MmError> {
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
                message: format!("managed provider seal failed with HTTP {}", response.status),
            });
        }
        let health = self.wait_for_health(&connection, true).await?;
        if !health.sealed {
            return Err(MmError::RuntimeResponse {
                message: "managed provider did not enter the sealed state".to_owned(),
            });
        }
        Ok(ManagedOpenBaoResult {
            action: plan.action,
            plan_hash: plan.plan_hash.clone(),
            rolled_back: false,
            data_purged: false,
        })
    }
} }
