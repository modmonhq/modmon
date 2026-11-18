use super::*;

impl EnvironmentManager {
    #[must_use]
    pub fn new(dependencies: EnvironmentDependencies) -> Self {
        Self {
            documents: dependencies.documents,
            sources: dependencies.sources,
            writer: dependencies.writer,
            state: dependencies.state,
            values: dependencies.values,
            inspector: dependencies.inspector,
            resolver: dependencies.resolver,
            storage_capabilities: dependencies.storage_capabilities,
            delivery_capabilities: dependencies.delivery_capabilities,
            clock: dependencies.clock,
        }
    }

    pub async fn status(
        &self,
        selection: &WorkspaceSelection,
        check_secrets: bool,
    ) -> Result<EnvironmentStatusSnapshot, MmError> {
        let context = self.context(selection)?;
        let overrides = self.overrides(&context)?;
        let mut diagnostics = context.validation.diagnostics;
        let inspect_providers = check_secrets
            && !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error);
        let secret_metadata = self
            .inspect_secrets(
                &context.document,
                &context.environment,
                &context.index,
                inspect_providers,
                &mut diagnostics,
                &mut Vec::new(),
            )
            .await?;
        let modules = status_modules(
            &context.document,
            &context.environment,
            &context.index,
            &overrides,
            &secret_metadata,
            inspect_providers,
            &mut diagnostics,
        );
        let input_hash = status_input_hash(&context.index, &overrides, &secret_metadata);

        let delivery = resolve_environment_delivery(&context.document.config, &context.environment);
        Ok(EnvironmentStatusSnapshot {
            project_name: context.document.config.project.name,
            environment: context.environment.clone(),
            delivery,
            config_hash: context.document.source_hash,
            input_hash,
            modules,
            diagnostics,
        })
    }

    pub fn set_override(
        &self,
        selection: &WorkspaceSelection,
        key: &str,
        value: &str,
        expected_revision: Option<i64>,
    ) -> Result<EnvironmentValueOverride, MmError> {
        if !is_safe_environment_value(value) {
            return Err(MmError::Environment {
                message: "environment override values may not contain control characters"
                    .to_owned(),
            });
        }
        let context = self.context(selection)?;
        ensure_index_valid(&context.validation)?;
        ensure_override_allowed(&context.document, &context.environment, &context.index, key)?;
        refuse_release_override(&context, key)?;
        self.values.set_environment_override(
            context.workspace.id,
            &context.environment,
            key,
            value,
            self.clock.now_unix_ms(),
            expected_revision,
        )
    }

    pub fn unset_override(
        &self,
        selection: &WorkspaceSelection,
        key: &str,
    ) -> Result<bool, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let workspace = self.state.register_workspace(
            &WorkspaceRegistration {
                project_id: document.config.project.id,
                root: document.root,
                config_path: document.path,
            },
            self.clock.now_unix_ms(),
        )?;
        self.values
            .unset_environment_override(workspace.id, &environment, key)
    }

    pub async fn secret_status(
        &self,
        selection: &WorkspaceSelection,
        requested_key: Option<&str>,
    ) -> Result<SecretStatusSnapshot, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let mut diagnostics = validate(&document, &environment).diagnostics;
        let inspect_providers = !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error);
        let mut secrets = Vec::new();
        let mut runtime_metadata = BTreeMap::new();
        for (key, declaration) in &document.config.secrets {
            if requested_key.is_some_and(|requested| requested != key) {
                continue;
            }
            let reference = configured_reference(declaration)?;
            let metadata = if inspect_providers {
                let scope = SecretScope::new(&environment);
                self.inspector
                    .inspect(&scope, &reference)
                    .await
                    .unwrap_or(SecretMetadata {
                        status: SecretStatus::ProviderUnavailable,
                        revision: None,
                        created_at_unix_ms: None,
                        candidate_revision: None,
                    })
            } else {
                SecretMetadata {
                    status: SecretStatus::NotChecked,
                    revision: None,
                    created_at_unix_ms: None,
                    candidate_revision: None,
                }
            };
            if inspect_providers {
                append_secret_diagnostic(&reference.canonical(), &metadata, &mut diagnostics);
            }
            runtime_metadata.insert(key.clone(), metadata.clone());
            secrets.push(SecretBindingStatus {
                key: key.clone(),
                provider: reference.scheme().to_owned(),
                status: metadata.status,
                revision: metadata.revision,
            });
        }
        let runtime = selection
            .runtime
            .or_else(|| {
                document
                    .config
                    .environments
                    .get(&environment)
                    .and_then(|environment| environment.runtime.as_ref())
                    .and_then(|runtime| runtime.driver)
            })
            .or(document.config.runtime.driver)
            .unwrap_or(crate::domain::runtime::RuntimeKind::DockerCompose);
        let delivery = runtime_secret_driver(runtime, selection, &document.config, &environment);
        let runtime_delivery = self.runtime_secret_statuses(
            &document,
            &environment,
            requested_key,
            &runtime_metadata,
            delivery,
        );
        if let Some(key) = requested_key
            && secrets.is_empty()
        {
            return Err(MmError::Environment {
                message: format!("secret '{key}' is not configured"),
            });
        }
        Ok(SecretStatusSnapshot {
            project_name: document.config.project.name,
            environment,
            secrets,
            runtime_delivery,
            diagnostics,
        })
    }

    pub fn default_environment(&self, start: &Path) -> Result<Option<String>, MmError> {
        Ok(self.documents.load_from(start)?.config.default_environment)
    }

    pub fn set_default_environment(
        &self,
        start: &Path,
        environment: Option<&str>,
    ) -> Result<Option<String>, MmError> {
        Ok(self
            .documents
            .set_default_environment(start, environment)?
            .config
            .default_environment)
    }

    pub fn delivery(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<crate::domain::environment_delivery::ResolvedEnvironmentDelivery, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        Ok(resolve_environment_delivery(&document.config, &environment))
    }

    pub fn set_delivery(
        &self,
        start: &Path,
        environment: &str,
        mode: EnvironmentDeliveryMode,
    ) -> Result<crate::domain::environment_delivery::ResolvedEnvironmentDelivery, MmError> {
        let document = self
            .documents
            .set_environment_delivery(start, environment, mode)?;
        Ok(resolve_environment_delivery(&document.config, environment))
    }

    /// The environment's inputs and its secrets' versions, for `mm env
    /// review` and `mm dev`: a store that cannot answer leaves a secret
    /// without a version.
    pub async fn review(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<EnvironmentReview, MmError> {
        self.review_with(selection, |_, _| StoreOutage::Tolerate)
            .await
    }

    /// The review a deployment to `target` plans from. A store that cannot
    /// answer refuses it, except for the development environment on this
    /// machine, which `mm dev` deploys.
    pub async fn review_for_deployment(
        &self,
        selection: &WorkspaceSelection,
        target: &crate::domain::target::NamedTarget,
    ) -> Result<EnvironmentReview, MmError> {
        self.review_with(selection, |document, environment| {
            StoreOutage::for_deployment(document, environment, target)
        })
        .await
    }

    async fn review_with(
        &self,
        selection: &WorkspaceSelection,
        outage: impl FnOnce(&ConfigDocument, &str) -> StoreOutage,
    ) -> Result<EnvironmentReview, MmError> {
        let context = self.context(selection)?;
        let outage = outage(&context.document, &context.environment);
        ensure_index_valid(&context.validation)?;
        check_release_keys(&context)?;
        let overrides = self.overrides(&context)?;
        let mut secret_diagnostics = Vec::new();
        let mut refusals = Vec::new();
        let secret_metadata = self
            .inspect_secrets(
                &context.document,
                &context.environment,
                &context.index,
                true,
                &mut secret_diagnostics,
                &mut refusals,
            )
            .await?;
        // A store mm cannot read here (no connection, a refused login) has
        // no revision to plan from: say why now, not when the plan is applied.
        if let Some((provider, refusal)) = refusals.into_iter().next() {
            return Err(outage.refusal(&context.environment, &provider, refusal));
        }
        outage.check(&context.environment, &secret_metadata)?;
        if secret_diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error && diagnostic.code == "MM_SECRET_INVALID"
        }) {
            return Err(MmError::Validation { errors: 1 });
        }
        let input_hash = status_input_hash(&context.index, &overrides, &secret_metadata);
        let services = context
            .index
            .modules
            .iter()
            .flat_map(|module| {
                module
                    .services
                    .iter()
                    .map(|service| CompiledServiceManifest {
                        module: module.module.clone(),
                        stack: service.stack.clone(),
                        service: service.name.clone(),
                        variable_count: module.variables.len(),
                        secret_count: module
                            .variables
                            .iter()
                            .filter(|variable| {
                                variable
                                    .global_key
                                    .as_deref()
                                    .and_then(|key| {
                                        effective_binding(
                                            &context.document.config,
                                            &context.environment,
                                            key,
                                        )
                                    })
                                    .and_then(EnvironmentBinding::secret_name)
                                    .is_some()
                            })
                            .count()
                            + context.document.config.modules[&module.module].services
                                [&service.name]
                                .secret_files_in(&context.environment)
                                .count(),
                    })
            })
            .collect();
        let secret_revisions = secret_metadata
            .into_iter()
            .map(|(reference, metadata)| (reference, metadata.revision))
            .collect();
        Ok(EnvironmentReview {
            input_hash,
            services,
            secret_revisions,
            delivery: resolve_environment_delivery(&context.document.config, &context.environment),
        })
    }

    pub fn validate_secret_delivery(
        &self,
        selection: &WorkspaceSelection,
        runtime: crate::domain::runtime::RuntimeKind,
        secret_revisions: &BTreeMap<String, Option<String>>,
    ) -> Result<Vec<RuntimeSecretPlanItem>, MmError> {
        let context = self.context(selection)?;
        ensure_index_valid(&context.validation)?;
        self.validate_environment_secret_delivery(&context, runtime)?;
        let (config, environment) = (&context.document.config, &context.environment);
        let runtime_driver = runtime_secret_driver(runtime, selection, config, environment);
        let driver_id = DeliveryDriverId::new(runtime_driver)
            .map_err(|message| MmError::Security { message })?;
        let delivery = self.delivery_capabilities.delivery_capability(&driver_id)?;
        let mut items = BTreeMap::<String, RuntimeSecretPlanItem>::new();
        for (service_name, service) in config.active_services(environment) {
            for (target, configured) in service.secret_files_in(environment) {
                let declaration = context
                    .document
                    .config
                    .secrets
                    .get(&configured.secret)
                    .ok_or_else(|| MmError::Secret {
                        message: format!(
                            "logical secret '{}' is not configured",
                            configured.secret
                        ),
                    })?;
                let logical = LogicalSecretId::new(configured.secret.clone())
                    .map_err(|message| MmError::Secret { message })?;
                let materialization = MaterializationName::file(target.clone())
                    .map_err(|message| MmError::Secret { message })?;
                let backend = self
                    .storage_capabilities
                    .storage_capability(&declaration.storage.provider)?;
                if !backend.supports(StorageOperation::ResolveMaterial) {
                    return Err(MmError::Secret {
                        message: format!(
                            "secret '{logical}' backend '{}' cannot resolve material for runtime delivery",
                            backend.id
                        ),
                    });
                }
                plan_secret_compatibility(SecretCompatibilityRequest {
                    logical_secret: &logical,
                    backend: &backend,
                    delivery: &delivery,
                    runtime,
                    workload: SecretWorkloadKind::Service,
                    material_kind: SecretMaterialKind::Text,
                    materialization_name: &materialization,
                })?;
                let reference = configured_reference(declaration)?;
                let source_revision = secret_revisions
                        .get(&reference.canonical())
                        .and_then(Clone::clone)
                        .ok_or_else(|| MmError::Secret {
                            message: format!(
                                "secret '{logical}' has no stable observed revision for runtime delivery"
                            ),
                        })?;
                let consumer = RuntimeSecretConsumer {
                    stack: service.stack.clone(),
                    service: service_name.clone(),
                    target: target.clone(),
                    uid: configured.uid.clone(),
                    gid: configured.gid.clone(),
                    mode: configured.mode,
                };
                consumer
                    .validate()
                    .map_err(|message| MmError::Secret { message })?;
                let item = items.entry(configured.secret.clone()).or_insert_with(|| {
                    RuntimeSecretPlanItem {
                        logical_name: configured.secret.clone(),
                        backend: declaration.storage.provider.to_string(),
                        delivery: driver_id.clone(),
                        source_revision: source_revision.clone(),
                        previous_source_revision: None,
                        consumers: Vec::new(),
                    }
                });
                if item.backend != declaration.storage.provider.as_str()
                    || item.source_revision != source_revision
                {
                    return Err(MmError::Secret {
                        message: format!(
                            "logical secret '{}' resolved inconsistently while planning consumers",
                            configured.secret
                        ),
                    });
                }
                item.consumers.push(consumer);
            }
        }
        Ok(items.into_values().collect())
    }

    fn validate_environment_secret_delivery(
        &self,
        context: &EnvironmentContext,
        runtime: crate::domain::runtime::RuntimeKind,
    ) -> Result<(), MmError> {
        let mode =
            resolve_environment_delivery(&context.document.config, &context.environment).mode;
        let driver_id = DeliveryDriverId::new(mode.driver_id())
            .map_err(|message| MmError::Security { message })?;
        let delivery = self.delivery_capabilities.delivery_capability(&driver_id)?;
        for module in &context.index.modules {
            for variable in &module.variables {
                let Some(global_key) = &variable.global_key else {
                    continue;
                };
                let Some(secret_name) =
                    effective_binding(&context.document.config, &context.environment, global_key)
                        .and_then(EnvironmentBinding::secret_name)
                else {
                    continue;
                };
                let declaration = context
                    .document
                    .config
                    .secrets
                    .get(secret_name)
                    .ok_or_else(|| MmError::Secret {
                        message: format!("logical secret '{secret_name}' is not configured"),
                    })?;
                let logical_secret = LogicalSecretId::new(secret_name.to_owned())
                    .map_err(|message| MmError::Secret { message })?;
                let materialization_name = MaterializationName::environment(variable.name.clone())
                    .map_err(|message| MmError::Secret { message })?;
                let backend = self
                    .storage_capabilities
                    .storage_capability(&declaration.storage.provider)?;
                if !backend.supports(StorageOperation::ResolveMaterial) {
                    return Err(MmError::Secret {
                        message: format!(
                            "secret '{logical_secret}' backend '{}' cannot resolve material for delivery; supported operations are {:?}",
                            backend.id, backend.operations
                        ),
                    });
                }
                if declaration.storage.location.len() > backend.max_location_bytes {
                    return Err(MmError::Secret {
                        message: format!(
                            "secret '{logical_secret}' backend '{}' location exceeds its {}-byte limit",
                            backend.id, backend.max_location_bytes
                        ),
                    });
                }
                plan_secret_compatibility(SecretCompatibilityRequest {
                    logical_secret: &logical_secret,
                    backend: &backend,
                    delivery: &delivery,
                    runtime,
                    workload: SecretWorkloadKind::Service,
                    material_kind: SecretMaterialKind::Text,
                    materialization_name: &materialization_name,
                })?;
            }
        }
        Ok(())
    }

    pub async fn compile(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<CompiledEnvironmentBundle, MmError> {
        self.compile_with_document(selection)
            .await
            .map(|(bundle, _)| bundle)
    }

    pub async fn materialize(
        &self,
        selection: &WorkspaceSelection,
        module: &str,
        service: Option<&str>,
        force: bool,
    ) -> Result<MaterializedEnvironment, MmError> {
        let (bundle, document) = self.compile_with_document(selection).await?;
        let module_config =
            document
                .config
                .modules
                .get(module)
                .ok_or_else(|| MmError::Environment {
                    message: format!("module '{module}' is not configured"),
                })?;
        let selected = select_compiled_service(&bundle, module, service)?;
        let manifest = bundle
            .manifest
            .services
            .iter()
            .find(|manifest| {
                manifest.module == module
                    && manifest.stack == selected.stack
                    && manifest.service == selected.service
            })
            .ok_or_else(|| MmError::Environment {
                message: "compiled environment manifest is incomplete".to_owned(),
            })?;
        let unresolved_module_root = document.root.join(&module_config.path);
        ensure_no_symlink_components(&unresolved_module_root)?;
        let module_root =
            dunce::canonicalize(&unresolved_module_root).map_err(|source| MmError::Io {
                context: format!("resolving module directory for '{module}'"),
                source,
            })?;
        if !module_root.starts_with(&document.root) {
            return Err(MmError::Environment {
                message: format!("module '{module}' resolves outside the project root"),
            });
        }
        let path = module_root.join(".env");
        self.writer.write(&path, selected.bytes(), force)?;
        Ok(MaterializedEnvironment {
            module: module.to_owned(),
            stack: selected.stack.clone(),
            service: selected.service.clone(),
            path,
            variable_count: manifest.variable_count,
            secret_count: manifest.secret_count,
        })
    }

    pub async fn diff(
        &self,
        selection: &WorkspaceSelection,
        module: &str,
        service: Option<&str>,
    ) -> Result<EnvironmentDiff, MmError> {
        let (bundle, document) = self.compile_with_document(selection).await?;
        let module_config =
            document
                .config
                .modules
                .get(module)
                .ok_or_else(|| MmError::Environment {
                    message: format!("module '{module}' is not configured"),
                })?;
        let selected = select_compiled_service(&bundle, module, service)?;
        let unresolved_module_root = document.root.join(&module_config.path);
        ensure_no_symlink_components(&unresolved_module_root)?;
        let module_root =
            dunce::canonicalize(&unresolved_module_root).map_err(|source| MmError::Io {
                context: format!("resolving module directory for '{module}'"),
                source,
            })?;
        if !module_root.starts_with(&document.root) {
            return Err(MmError::Environment {
                message: format!("module '{module}' resolves outside the project root"),
            });
        }
        let path = module_root.join(".env");
        let (exists, changed) = match read_regular_file(&path, 4 * 1024 * 1024)? {
            Some(contents) => (true, contents != selected.bytes()),
            None => (false, true),
        };
        Ok(EnvironmentDiff {
            module: module.to_owned(),
            stack: selected.stack.clone(),
            service: selected.service.clone(),
            path,
            exists,
            changed,
        })
    }

    fn context(&self, selection: &WorkspaceSelection) -> Result<EnvironmentContext, MmError> {
        let mut document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let mut validation = validate(&document, &environment);
        let index = self.sources.index(&document, &environment);
        validation.diagnostics.extend(index.diagnostics.clone());
        document.config.release = selection.release.as_deref().cloned();
        let workspace = self.state.register_workspace(
            &WorkspaceRegistration {
                project_id: document.config.project.id,
                root: document.root.clone(),
                config_path: document.path.clone(),
            },
            self.clock.now_unix_ms(),
        )?;
        Ok(EnvironmentContext {
            document,
            environment,
            index,
            validation,
            workspace,
        })
    }

    fn overrides(
        &self,
        context: &EnvironmentContext,
    ) -> Result<BTreeMap<String, EnvironmentValueOverride>, MmError> {
        let referenced = context
            .index
            .modules
            .iter()
            .flat_map(|module| &module.variables)
            .filter_map(|variable| variable.global_key.clone())
            .collect::<BTreeSet<_>>();
        Ok(self
            .values
            .list_environment_overrides(context.workspace.id, &context.environment)?
            .into_iter()
            .filter(|value| {
                referenced.contains(&value.key)
                    && context
                        .document
                        .config
                        .environments
                        .get(&context.environment)
                        .and_then(|config| config.bindings.get(&value.key))
                        .is_none_or(|binding| binding.secret_name().is_none())
            })
            .map(|value| (value.key.clone(), value))
            .collect())
    }

    async fn compile_with_document(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<(CompiledEnvironmentBundle, ConfigDocument), MmError> {
        let context = self.context(selection)?;
        ensure_index_valid(&context.validation)?;
        let overrides = self.overrides(&context)?;
        let (secret_material, secret_metadata) = self
            .resolve_secrets(&context.document, &context.environment, &context.index)
            .await?;
        let mut diagnostics = context.validation.diagnostics.clone();
        for (reference, metadata) in &secret_metadata {
            append_secret_diagnostic(reference, metadata, &mut diagnostics);
        }
        let _modules = status_modules(
            &context.document,
            &context.environment,
            &context.index,
            &overrides,
            &secret_metadata,
            true,
            &mut diagnostics,
        );
        let error_count = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .count();
        if error_count > 0 {
            return Err(MmError::Validation {
                errors: error_count,
            });
        }
        let input_hash = status_input_hash(&context.index, &overrides, &secret_metadata);
        let bundle = compile_bundle(
            &context.document,
            &context.environment,
            &context.index,
            &input_hash,
            &overrides,
            &secret_material,
        )?;
        Ok((bundle, context.document))
    }

    async fn inspect_secrets(
        &self,
        document: &ConfigDocument,
        environment: &str,
        index: &EnvironmentIndex,
        check: bool,
        diagnostics: &mut Vec<Diagnostic>,
        refusals: &mut Vec<(String, MmError)>,
    ) -> Result<BTreeMap<String, SecretMetadata>, MmError> {
        let references = used_secret_references(document, environment, index)?;
        let scope = SecretScope::new(environment);
        let mut metadata = BTreeMap::new();
        for reference in references.values() {
            let result = if check {
                self.inspector.inspect(&scope, reference).await
            } else {
                Ok(SecretMetadata {
                    status: SecretStatus::NotChecked,
                    revision: None,
                    created_at_unix_ms: None,
                    candidate_revision: None,
                })
            };
            match result {
                Ok(value) => {
                    if check {
                        append_secret_diagnostic(&reference.canonical(), &value, diagnostics);
                    }
                    metadata.insert(reference.canonical(), value);
                }
                Err(error) => {
                    diagnostics.push(Diagnostic::error(
                        "MM_SECRET_PROVIDER_UNAVAILABLE",
                        format!(
                            "secret provider '{}' could not inspect a configured key: {error}",
                            reference.scheme()
                        ),
                    ));
                    metadata.insert(
                        reference.canonical(),
                        SecretMetadata {
                            status: SecretStatus::ProviderUnavailable,
                            revision: None,
                            created_at_unix_ms: None,
                            candidate_revision: None,
                        },
                    );
                    refusals.push((reference.scheme().to_owned(), error));
                }
            }
        }
        Ok(metadata)
    }

    async fn resolve_secrets(
        &self,
        document: &ConfigDocument,
        environment: &str,
        index: &EnvironmentIndex,
    ) -> Result<
        (
            BTreeMap<String, Arc<SecretMaterial>>,
            BTreeMap<String, SecretMetadata>,
        ),
        MmError,
    > {
        let references = used_secret_references(document, environment, index)?;
        let mut materials = BTreeMap::new();
        let mut metadata = BTreeMap::new();
        let scope = SecretScope::new(environment);
        for reference in references.values() {
            let resolution = self.resolver.resolve(&scope, reference).await?;
            let canonical = reference.canonical();
            if let Some(material) = resolution.material {
                materials.insert(canonical.clone(), Arc::new(material));
            }
            metadata.insert(canonical, resolution.metadata);
        }
        Ok((materials, metadata))
    }
}
