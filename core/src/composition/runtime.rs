use super::*;

/// Typed process and clock capabilities required to assemble runtime adapters.
pub(crate) struct RuntimeCapabilities {
    pub(crate) process: Arc<dyn ProcessRunner>,
    pub(crate) streaming: Arc<dyn StreamingProcessRunner>,
    pub(crate) clock: Arc<dyn Clock>,
}

#[derive(Clone)]
pub(crate) struct RuntimeComposition {
    process: Arc<dyn ProcessRunner>,
    pub(super) factory: RuntimeFactory,
    clock: Arc<dyn Clock>,
}

impl RuntimeComposition {
    pub(crate) fn new() -> Self {
        let runner = Arc::new(TokioProcessRunner);
        let mut composition = Self::from_capabilities(RuntimeCapabilities {
            process: runner.clone(),
            streaming: runner,
            clock: Arc::new(SystemClock),
        });
        // Reads go to the engine's API where its socket answers on this
        // host; they start no process then.
        composition.factory = composition
            .factory
            .reading_engine_api(super::engine_api::local())
            .reading_podman_api(super::engine_api::local_podman());
        composition
    }

    pub(crate) fn from_capabilities(capabilities: RuntimeCapabilities) -> Self {
        Self {
            factory: RuntimeFactory::with_streaming(
                Arc::clone(&capabilities.process),
                capabilities.streaming,
            ),
            process: capabilities.process,
            clock: capabilities.clock,
        }
    }

    pub(crate) fn process(&self) -> Arc<dyn ProcessRunner> {
        Arc::clone(&self.process)
    }

    pub(crate) fn clock(&self) -> Arc<dyn Clock> {
        Arc::clone(&self.clock)
    }

    pub(crate) fn driver(&self, kind: RuntimeKind) -> Arc<dyn RuntimeDriver> {
        self.factory.driver(kind)
    }

    fn deployment_environment(
        &self,
        document: &ConfigDocument,
        home: &MmHome,
        documents: Arc<dyn ProjectDocumentStore>,
        state: Arc<dyn StateStore>,
        values: Arc<dyn EnvironmentValueStore>,
    ) -> Result<(EnvironmentManager, Arc<FilesystemEnvironmentArtifactStore>), MmError> {
        let age = Arc::new(AgeSecretStore::new(
            identity_path(home),
            home.secrets_directory
                .join(format!("{}.age", document.config.project.id.0)),
        ));
        let revision_key = crate::adapters::secrets::load_or_create_environment_revision_key(
            &home.identities_directory.join("environment-revisions.key"),
        )?;
        let secrets = secret_provider_registry(
            age,
            revision_key,
            home,
            document.config.project.id,
            &document.config,
        )?;
        let artifacts = Arc::new(FilesystemEnvironmentArtifactStore::new(
            home.run_directory.join("env"),
        )?);
        let environments = EnvironmentManager::new(EnvironmentDependencies {
            documents,
            sources: environment_sources(),
            writer: materialized_environment_writer(),
            state,
            values,
            inspector: secrets.clone(),
            resolver: secrets.clone(),
            storage_capabilities: secrets,
            delivery_capabilities: artifacts.clone(),
            clock: self.clock(),
        });
        Ok((environments, artifacts))
    }

    /// The environment manager a selection's deployment uses.
    pub(crate) fn environment_manager(
        &self,
        selection: &WorkspaceSelection,
        explicit_home: Option<&Path>,
    ) -> Result<EnvironmentManager, MmError> {
        let document = ConfigLoader.load_from(&selection.start)?;
        let home = MmHome::resolve(explicit_home)?;
        let sqlite = Arc::new(SqliteStateStore::open(&home.state_database)?);
        let values = super::host_values::values_for(selection, &home, &document, sqlite.clone());
        let (environments, _) = self.deployment_environment(
            &document,
            &home,
            documents_on(selection.document_scope()),
            sqlite,
            values,
        )?;
        Ok(environments)
    }

    pub(crate) fn build_manager(&self) -> BuildManager {
        let source: Arc<dyn BuildSourceInspector> =
            Arc::new(GitBuildSourceInspector::new(self.process()));
        BuildManager::new(project_documents(), source)
    }

    pub(crate) fn build_service(
        &self,
        planner: BuildManager,
        selection: &WorkspaceSelection,
        explicit_home: Option<&Path>,
    ) -> Result<BuildService, MmError> {
        let document = ConfigLoader.load_from(&selection.start)?;
        let home = MmHome::resolve(explicit_home)?;
        let state: Arc<dyn StateStore> = Arc::new(SqliteStateStore::open(&home.state_database)?);
        let age = Arc::new(AgeSecretStore::new(
            identity_path(&home),
            home.secrets_directory
                .join(format!("{}.age", document.config.project.id.0)),
        ));
        let revision_key = crate::adapters::secrets::load_or_create_environment_revision_key(
            &home.identities_directory.join("environment-revisions.key"),
        )?;
        let secrets = secret_provider_registry(
            age,
            revision_key,
            &home,
            document.config.project.id,
            &document.config,
        )?;
        let secret_inspector: Arc<dyn SecretMetadataInspector> = secrets.clone();
        let secret_resolver: Arc<dyn SecretMaterialResolver> = secrets;
        let builder: Arc<dyn ImageBuilder> = Arc::new(DockerBuildxBuilder::new(
            self.process(),
            home.builds_directory,
        ));
        let locks: Arc<dyn BuildOperationLock> = Arc::new(FilesystemTargetOperationLock::new(
            home.target_locks_directory.join("builds"),
        ));
        Ok(BuildService::new(
            planner,
            BuildExecutionPorts {
                documents: project_documents(),
                state,
                builder,
                secret_inspector,
                secret_resolver,
                locks,
                clock: self.clock(),
            },
        ))
    }

    /// Where services have user ids mm knows (rootful Podman), a file per
    /// revision is given to the container's user; elsewhere Podman keeps
    /// secrets in its own store.
    fn runtime_secrets(
        &self,
        runtime: RuntimeKind,
        selection: &WorkspaceSelection,
        home: &MmHome,
    ) -> Result<Arc<dyn RuntimeSecretDelivery>, MmError> {
        let directory = home.run_directory.join("runtime-secrets");
        Ok(match (runtime, &selection.user_ids) {
            (RuntimeKind::PodmanCompose, None) => {
                Arc::new(PodmanSecretDelivery::new(self.process(), directory)?)
            }
            (RuntimeKind::PodmanCompose, Some(_)) => {
                Arc::new(DockerComposeFileSecretDelivery::for_podman(directory)?)
            }
            (RuntimeKind::DockerCompose, _) => {
                Arc::new(DockerComposeFileSecretDelivery::new(directory)?)
            }
        })
    }

    pub(crate) fn deployment_service(
        &self,
        selection: &WorkspaceSelection,
        explicit_home: Option<&Path>,
    ) -> Result<DeploymentService, MmError> {
        let document = ConfigLoader.load_from(&selection.start)?;
        let home = MmHome::resolve(explicit_home)?;
        let services_check = development_services_check(&home, self)?;
        let sqlite = Arc::new(SqliteStateStore::open(&home.state_database)?);
        let state: Arc<dyn StateStore> = sqlite.clone();
        let values = super::host_values::values_for(selection, &home, &document, sqlite.clone());
        let routes: Arc<dyn IngressRouteRegistry> = sqlite.clone();
        let scheduler: Arc<dyn SchedulerRepository> = sqlite;
        let targets: Arc<dyn TargetStore> =
            Arc::new(FilesystemTargetStore::new(home.targets_file.clone()));
        let documents = documents_on(selection.document_scope());
        let stacks = StackManager::new(
            Arc::clone(&documents),
            Arc::new(FilesystemStackArtifactStore::new(
                home.compose_directory.clone(),
            )),
            Arc::clone(&targets),
        )
        .with_user_ids(selection.user_ids.clone());
        let target_ingress = target_ingress_manager(&home, Arc::clone(&targets));
        let dynamic_configs = dynamic_config_store(&home);
        let (environments, environment_artifacts) = self.deployment_environment(
            &document,
            &home,
            Arc::clone(&documents),
            Arc::clone(&state),
            values,
        )?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let (environment_pref, project_pref) =
            crate::app::load_selection_preferences(Some(state.as_ref()), &document, &environment)?;
        let (resolved, _) = crate::app::resolve_selection(
            &document,
            &environment,
            selection.runtime,
            selection.target.as_deref(),
            environment_pref.as_ref(),
            project_pref.as_ref(),
            targets.as_ref(),
        )?;
        let runtime = self.factory.deployment(resolved.runtime)?;
        let inspector = self.factory.driver(resolved.runtime);
        let runtime_secrets = self.runtime_secrets(resolved.runtime, selection, &home)?;
        let planner = DeploymentManager::new(DeploymentManagerDependencies {
            installation: installation_store(&home),
            stacks,
            routes: Arc::clone(&routes),
            state: Arc::clone(&state),
            dynamic_configs,
            target_ingress,
            environments: environments.clone(),
            runtime: Arc::clone(&inspector),
            local_ca: Arc::new(FilesystemLocalCaStore::new(home.ca_directory.clone())),
        })
        .with_prerequisite(services_check);
        Ok(DeploymentService::new(
            planner,
            DeploymentExecutionPorts {
                documents,
                routes,
                state,
                targets,
                runtime,
                inspector,
                locks: Arc::new(FilesystemTargetOperationLock::new(
                    home.target_locks_directory,
                )),
                verifier: Arc::new(TcpIngressVerifier::default()),
                rootless: rootless_ingress_installer(self.process())?,
                privileged: privileged_ingress_installer(self.process())?,
                clock: self.clock(),
                environments,
                environment_artifacts,
                runtime_secrets,
                scheduler,
            },
        )
        .with_jobs(Arc::new(super::services::SchedulerJobs {
            home: explicit_home.map(Path::to_path_buf),
            runtime: resolved.runtime,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::clock::FixedClock;
    use crate::app::ports::{CommandOutput, CommandSpec, ProcessStream, StreamCommandSpec};
    use crate::domain::observability::OperationCancellation;

    struct FakeProcess;

    #[async_trait::async_trait]
    impl ProcessRunner for FakeProcess {
        async fn run(&self, _specification: CommandSpec) -> Result<CommandOutput, MmError> {
            Err(MmError::State {
                message: "fake process should not run during composition".to_owned(),
            })
        }
    }

    #[async_trait::async_trait]
    impl StreamingProcessRunner for FakeProcess {
        async fn spawn(
            &self,
            _specification: StreamCommandSpec,
            _cancellation: OperationCancellation,
        ) -> Result<ProcessStream, MmError> {
            Err(MmError::State {
                message: "fake process should not stream during composition".to_owned(),
            })
        }
    }

    #[test]
    fn runtime_composition_accepts_typed_fake_capabilities() {
        let fake = Arc::new(FakeProcess);
        let process: Arc<dyn ProcessRunner> = fake.clone();
        let streaming: Arc<dyn StreamingProcessRunner> = fake;
        let composition = RuntimeComposition::from_capabilities(RuntimeCapabilities {
            process,
            streaming,
            clock: Arc::new(FixedClock { unix_ms: 42 }),
        });

        assert_eq!(composition.clock().now_unix_ms(), 42);
        assert_eq!(
            composition.driver(RuntimeKind::DockerCompose).kind(),
            RuntimeKind::DockerCompose
        );
    }
}
