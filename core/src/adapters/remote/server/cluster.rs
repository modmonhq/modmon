// What makes this daemon one node of a cluster target: it answers only
// requests addressed to its own name, plans and serves only the services
// placed on it, and keeps the seal that binds its plan into a cluster plan.

use crate::domain::placement::{ClusterNodes, NodeServices};
use crate::domain::remote::{ClusterPlanSeal, NodeDeployment, NodeFacts, RemoteScope};

impl RemoteDaemon {
    /// Refuses a request addressed to another node, so an address that
    /// reaches the wrong host never applies another node's plan. Asking a
    /// host which node it is stays possible.
    fn check_addressed_node(&self, scope: &RemoteScope, method: &RemoteMethod) -> Result<(), MmError> {
        let Some(requested) = &scope.node else {
            return Ok(());
        };
        if matches!(method, RemoteMethod::Node) {
            return Ok(());
        }
        let node = self.config.node_env()?;
        let actual = node.as_ref().map(|node| node.name.clone());
        if actual.as_deref() != Some(requested.as_str()) {
            return Err(MmError::WrongNode { requested: requested.clone(), actual });
        }
        // The node's own work (names, storage, its firewall) follows the
        // engine setup installed; a deployment for the other engine would
        // run containers next to it that none of that sees. Only a plan
        // carries the environment's engine for certain: access and other
        // requests name the project's default.
        if !matches!(method, RemoteMethod::PlanDeployment { .. }) {
            return Ok(());
        }
        match node.and_then(|node| node.runtime) {
            Some(installed) if installed != scope.runtime => Err(MmError::Target {
                message: format!(
                    "node '{requested}' runs {installed}, as `mm target setup` installed it, but environment '{environment}' uses {wanted}; a node runs one engine for every environment, so set environments.{environment}.runtime.driver to {installed}",
                    installed = installed.name(),
                    wanted = scope.runtime.name(),
                    environment = scope.environment,
                ),
            }),
            _ => Ok(()),
        }
    }

    /// The services `document` places on the node a request is for, or
    /// `None` for a request that names no node (a single host runs them all).
    fn node_services(
        &self,
        scope: &RemoteScope,
        document: &crate::domain::config::ConfigDocument,
        commit: &str,
    ) -> Result<Option<NodeServices>, MmError> {
        let Some(node) = scope.node.as_deref() else {
            return Ok(None);
        };
        let short = commit.get(..12).unwrap_or(commit);
        let target = document
            .config
            .targets
            .get(&scope.target)
            .filter(|target| target.is_cluster())
            .ok_or_else(|| MmError::Target {
                message: format!(
                    "commit {short} does not list nodes under targets.{}, so it cannot be used on node '{node}'; use a commit that declares this cluster",
                    scope.target
                ),
            })?;
        let names = target.nodes.iter().map(|node| node.name.as_str()).collect::<Vec<_>>();
        crate::domain::placement::services_on_node(
            &document.config.modules,
            &scope.environment,
            ClusterNodes { nodes: &names, groups: &target.cluster.groups },
            node,
        )
        .map(Some)
        .map_err(|message| MmError::Placement {
            message: format!(
                "at commit {short}, {message}; add the node under targets.{}.nodes and commit it",
                scope.target
            ),
        })
    }

    /// The services the deployed commit places on this node.
    fn placed_services(&self, context: &RemoteRequestContext) -> Result<Option<NodeServices>, MmError> {
        let scope = &context.scope;
        if scope.node.is_none() {
            return Ok(None);
        }
        let document = crate::adapters::config::ConfigLoader
            .load(&context.project_root.join(crate::adapters::config::CONFIG_RELATIVE_PATH))?;
        let commit = self
            .access
            .deployed(scope.project_id, &scope.environment)?
            .map(|deployed| deployed.commit)
            .unwrap_or_default();
        self.node_services(scope, &document, &commit)
    }

    /// On a cluster node, a status lists only the services placed here.
    fn keep_placed(
        &self,
        context: &RemoteRequestContext,
        snapshot: &mut crate::domain::workspace::WorkspaceSnapshot,
    ) -> Result<(), MmError> {
        let Some(placed) = self.placed_services(context)? else {
            return Ok(());
        };
        for stack in &mut snapshot.stacks {
            stack.services.retain(|service| placed.contains(&service.stack, &service.name));
        }
        snapshot.stacks.retain(|stack| !stack.services.is_empty());
        Ok(())
    }

    /// What the daemon adds to a status and takes from it: on a cluster
    /// node only the services placed here, and the deployment the host
    /// last finished.
    fn finish_status(
        &self,
        context: &RemoteRequestContext,
        snapshot: &mut crate::domain::workspace::WorkspaceSnapshot,
    ) -> Result<(), MmError> {
        self.keep_placed(context, snapshot)?;
        // A container that is leaving says so.
        let leaving = crate::adapters::leaving::leaving_ids(&self.config.state_root);
        for container in snapshot
            .stacks
            .iter_mut()
            .flat_map(|stack| stack.services.iter_mut())
            .flat_map(|service| service.containers.iter_mut())
            .filter(|container| leaving.iter().any(|id| !container.id.is_empty() && id.starts_with(&container.id)))
        {
            container.labels.insert(crate::domain::leaving::LEAVING_LABEL.to_owned(), "true".to_owned());
        }
        snapshot.deployment = self.applied_deployment(&context.scope)?;
        self.add_scaling(&context.scope, snapshot);
        self.add_restarts(&context.scope, snapshot);
        Ok(())
    }

    /// The deployment this host last finished, for a status: what its
    /// containers were started from.
    fn applied_deployment(
        &self,
        scope: &RemoteScope,
    ) -> Result<Option<crate::domain::workspace::AppliedDeployment>, MmError> {
        use crate::domain::workspace::AppliedDeployment;
        if let Some(current) = self.access.deployments(scope.project_id, &scope.environment)?.current {
            return Ok(Some(AppliedDeployment {
                commit: current.commit,
                deployed_at_unix_ms: current.deployed_at_unix_ms,
                deployed_by: current.deployed_by,
                plan_hash: current.plan_hash,
                build_receipt: current.receipt.map(|receipt| receipt.receipt_hash),
            }));
        }
        // A host deployed before deployments were remembered knows the
        // commit and who deployed it.
        Ok(self
            .access
            .deployed(scope.project_id, &scope.environment)?
            .map(|deployed| AppliedDeployment {
                commit: deployed.commit,
                deployed_at_unix_ms: deployed.deployed_at_unix_ms,
                deployed_by: deployed.deployed_by,
                plan_hash: None,
                build_receipt: None,
            }))
    }

    /// What a log read needs beyond the grant its method was checked
    /// against. A node answers for the containers it has, so a service
    /// that is placed on other nodes is not refused: it has no lines here.
    fn authorize_log_read(
        &self,
        context: &RemoteRequestContext,
        request: &crate::domain::observability::LogRequest,
    ) -> Result<(), MmError> {
        if request.selector.is_none() {
            self.authorize_every_log(context)?;
        }
        Ok(())
    }

    /// A read of every service's log is for a principal that may read each
    /// of them. A grant that names services must not open the others, and
    /// a request that names no service matches such a grant, so each
    /// service of the environment is checked by name.
    fn authorize_every_log(&self, context: &RemoteRequestContext) -> Result<(), MmError> {
        let document = crate::adapters::config::ConfigLoader
            .load(&context.project_root.join(crate::adapters::config::CONFIG_RELATIVE_PATH))?;
        let policy = self.access.load()?.policy;
        let capability = Capability::new(AccessAction::Logs, AccessResourceKind::Service);
        let environment = &context.scope.environment;
        for (name, _) in document
            .config
            .modules
            .values()
            .flat_map(|module| &module.services)
            .filter(|(_, service)| service.is_in_environment(environment))
        {
            self.reauthorize_capability_with_policy(context, capability, Some(name), &policy)
                .map_err(|_| MmError::Security {
                    message: format!(
                        "{} may not read the log of service '{name}' in '{environment}', so not every service at once; name a service it may read: `mm service logs SERVICE`",
                        context.principal
                    ),
                })?;
        }
        Ok(())
    }

    /// Commands and forwards reach a service only on a node that runs it.
    fn require_placed(&self, context: &RemoteRequestContext, service: &str) -> Result<(), MmError> {
        match self.placed_services(context)? {
            Some(placed) if !placed.has_service(service) => Err(MmError::Environment {
                message: format!(
                    "service '{service}' does not run on node '{}'; `mm service status {service}` shows the node it runs on",
                    context.scope.node.as_deref().unwrap_or_default()
                ),
            }),
            _ => Ok(()),
        }
    }

    /// The hash of this node's own plan inside the cluster plan `hash`, or
    /// `None` for a request that names no node and so applies `hash` itself.
    fn sealed_section(&self, scope: &RemoteScope, hash: &str) -> Result<Option<String>, MmError> {
        let Some(node) = scope.node.as_deref() else {
            return Ok(None);
        };
        let seal = self.journal.read_seal(hash)?.ok_or_else(|| MmError::Security {
            message: format!(
                "node '{node}' holds no reviewed cluster plan {hash}; run `mm deploy plan` again and apply the hash it prints"
            ),
        })?;
        check_seal_scope(&seal, scope)?;
        seal.section(node)
            .map(|section| Some(section.plan_hash.clone()))
            .ok_or_else(|| MmError::Security {
                message: format!("cluster plan {hash} has no section for node '{node}'"),
            })
    }

    /// Has the node store the network addresses of the cluster plan `hash`,
    /// when the plan has any.
    async fn store_sealed_networks(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        converge: bool,
    ) -> Result<(), MmError> {
        let Some(node) = context.scope.node.as_deref() else {
            return Ok(());
        };
        let Some(networks) = self.journal.read_seal(hash)?.and_then(|seal| seal.networks) else {
            return Ok(());
        };
        self.use_cases.store_networks(node, &networks, converge).await
    }

    /// Takes the networks the cluster plan `hash` retired off this node,
    /// once its part of the rollout finished. What stays is left for a
    /// later deployment; the deployment itself is done either way.
    async fn retire_networks(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        (timeout, pass): (Duration, &MutationPass),
    ) {
        let retired = match self.journal.read_seal(hash) {
            Ok(seal) => seal.and_then(|seal| seal.networks).map(|networks| networks.retired),
            Err(error) => {
                tracing::warn!(%error, "could not read the plan's networks to retire");
                return;
            }
        };
        let Some(retired) = retired.filter(|retired| !retired.is_empty()) else {
            return;
        };
        let retiring = self.use_cases.retire_networks(&retired);
        match self.watched(context, pass, timeout, retiring).await {
            Ok(taken) => tracing::info!(
                request_id = %context.request_id,
                networks = ?taken,
                "took away networks no service of the environment uses"
            ),
            Err(error) => tracing::warn!(
                request_id = %context.request_id,
                %error,
                "could not take away the networks the deployment retired"
            ),
        }
    }

    /// Makes the shared volumes of the cluster plan `hash` ready on this
    /// node, when the plan has any: the storage node creates the missing
    /// directories, a node that mounts one checks that it reaches the
    /// storage node. Nothing is removed, here or by any later step.
    async fn ready_storage(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        timeout: Duration,
        pass: &MutationPass,
    ) -> Result<crate::domain::rollout::RolloutAnswer, MmError> {
        let ready = |created| crate::domain::rollout::RolloutAnswer::StorageReady { created };
        let Some(node) = context.scope.node.as_deref() else {
            return Ok(ready(Vec::new()));
        };
        let Some(storage) = self.journal.read_seal(hash)?.and_then(|seal| seal.storage) else {
            return Ok(ready(Vec::new()));
        };
        let created = self
            .watched(context, pass, timeout, self.use_cases.prepare_storage(node, &storage))
            .await
            .map_err(|error| host_failure(context, "storage", error))?;
        Ok(ready(created))
    }

    /// Each OCI runtime the host's policy offers, tried on its engine.
    async fn runtimes<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        timeout: Duration,
    ) -> Result<(), MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        let status = timed(timeout, self.use_cases.runtimes(context)).await?;
        send_result(writer, context.request_id, RemoteResult::Runtimes(Box::new(status))).await
    }

    /// Has an ingress node of the cluster plan `hash` serve its routes, and
    /// every other node take away what it served of the environment. A
    /// plan that leaves nodes out carries no ingress: nothing changes.
    async fn serve_routes(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        timeout: Duration,
        pass: &MutationPass,
    ) -> Result<(), MmError> {
        let Some(node) = context.scope.node.as_deref() else {
            return Ok(());
        };
        let Some(ingress) = self.journal.read_seal(hash)?.and_then(|seal| seal.ingress) else {
            return Ok(());
        };
        let serving = self.use_cases.apply_ingress(context, node, &ingress);
        self.watched(context, pass, timeout, serving).await
    }

    /// The requests about shared volumes: what this node says about them,
    /// what deleting one would remove, and deleting it, which is audited.
    async fn storage<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        method: RemoteMethod,
        pass: Option<&MutationPass>,
        timeout: Duration,
    ) -> Result<(), MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        use crate::domain::shared_storage::VolumeName;
        let scope = &context.scope;
        let result = match method {
            RemoteMethod::StorageStatus { storage_node, sizes } => {
                let status = timed(timeout, self.use_cases.storage_status(context, &storage_node, sizes)).await?;
                RemoteResult::StorageStatus(Box::new(status))
            }
            RemoteMethod::PlanVolumeDelete { stack, volume } => {
                let name = VolumeName { stack, volume };
                let plan = timed(timeout, self.use_cases.plan_volume_delete(context, &name)).await?;
                RemoteResult::VolumeDeletePlan(Box::new(plan))
            }
            RemoteMethod::DeleteVolume { stack, volume, plan_hash } => {
                let pass = pass.ok_or_else(|| MmError::State {
                    message: "deleting a shared volume ran without its turn".to_owned(),
                })?;
                let name = VolumeName { stack, volume };
                let deleting = self.use_cases.delete_volume(context, &name, &plan_hash);
                let deleted = self.watched(context, pass, timeout, deleting).await?;
                self.journal.audit(&serde_json::json!({
                    "timestamp_unix_ms": now_unix_ms()?, "request_id": context.request_id,
                    "principal": context.principal, "principal_id": context.principal_id,
                    "project_id": scope.project_id, "environment": scope.environment,
                    "target": scope.target, "node": scope.node, "plan_hash": plan_hash,
                    "result": "shared-volume-deleted", "volume": name.to_string(), "path": deleted.path,
                    "daemon_version": env!("CARGO_PKG_VERSION"),
                }))?;
                RemoteResult::VolumeDeleted(Box::new(deleted))
            }
            _ => {
                return Err(MmError::State {
                    message: "this is no shared storage request".to_owned(),
                });
            }
        };
        send_result(writer, context.request_id, result).await
    }

    /// Checks that the cluster hash covers the plan this node stored for the
    /// same caller, and keeps the seal for the apply.
    async fn seal<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        seal: ClusterPlanSeal,
    ) -> Result<(), MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        let scope = &context.scope;
        let node = scope.node.as_deref().ok_or_else(|| MmError::Security {
            message: "only a cluster node seals a plan; the request names no node".to_owned(),
        })?;
        seal.validate().map_err(|message| MmError::Security { message })?;
        check_seal_scope(&seal, scope)?;
        let section = seal.section(node).ok_or_else(|| MmError::Security {
            message: format!("the cluster plan has no section for node '{node}'"),
        })?;
        let stored = self.journal.read_plan(&section.plan_hash, scope, now_unix_ms()?)?;
        let binding = stored.binding();
        if binding.kind != seal.kind
            || binding.commit != seal.commit
            || binding.principal_id != context.principal_id
        {
            return Err(MmError::Security {
                message: "the cluster plan does not cover the plan this node made for you; produce a new plan".to_owned(),
            });
        }
        // The order the plan rolls out in is part of what was reviewed:
        // it has to match what this node's own plan renders.
        if let StoredRemotePlan::Deployment(remote) = &stored {
            let stacks = remote
                .plan
                .iter()
                .flat_map(|plan| plan.stacks.iter().map(|stack| stack.stack.clone()))
                .collect::<Vec<_>>();
            seal.rollout
                .as_ref()
                .ok_or_else(|| "the cluster plan carries no rollout order".to_owned())
                .and_then(|rollout| {
                    let jobs = remote.plan.as_ref().map_or(&[][..], |plan| plan.rollout.jobs.as_slice());
                    rollout.check_node(node, &stacks, jobs)
                })
                .map_err(|message| MmError::Security {
                    message: format!("{message}; produce a new plan"),
                })?;
        }
        self.journal.save_seal(&seal)?;
        self.journal.audit(&serde_json::json!({
            "timestamp_unix_ms": now_unix_ms()?, "request_id": context.request_id,
            "principal": context.principal, "principal_id": context.principal_id,
            "project_id": scope.project_id, "environment": scope.environment,
            "target": scope.target, "node": node, "plan_hash": section.plan_hash,
            "cluster_plan_hash": seal.cluster_plan_hash, "result": "sealed",
            "daemon_version": env!("CARGO_PKG_VERSION"),
        }))?;
        send_result(writer, context.request_id, RemoteResult::Sealed(Box::new(seal))).await
    }

    /// The seal kept for `hash`, while this node's section can still be
    /// applied: unexpired and not applied before.
    async fn sealed<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        hash: &str,
    ) -> Result<(), MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        let scope = &context.scope;
        let node = scope.node.as_deref().unwrap_or_default();
        let section = self.sealed_section(scope, hash)?.ok_or_else(|| MmError::Security {
            message: "only a cluster node keeps cluster plans; the request names no node".to_owned(),
        })?;
        if self.journal.is_claimed(&section)? {
            return Err(MmError::Security {
                message: format!(
                    "node '{node}' already applied its part of cluster plan {hash}; run `mm deploy plan` again"
                ),
            });
        }
        self.journal.read_plan(&section, scope, now_unix_ms()?)?;
        let seal = self.journal.read_seal(hash)?.ok_or_else(|| MmError::State {
            message: "the cluster plan seal disappeared while it was read".to_owned(),
        })?;
        send_result(writer, context.request_id, RemoteResult::Sealed(Box::new(seal))).await
    }

    /// This host's node name, version, and last applied deployment.
    async fn node_facts<W>(&self, writer: &mut W, context: &RemoteRequestContext) -> Result<(), MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        let scope = &context.scope;
        let node = self.config.node_env()?;
        let deployed = self
            .access
            .deployed(scope.project_id, &scope.environment)?
            .map(|deployed| NodeDeployment {
                commit: deployed.commit,
                deployed_at_unix_ms: deployed.deployed_at_unix_ms,
                deployed_by: deployed.deployed_by,
                runs_services: deployed.runs_services,
            });
        // Only a rollout that still matters: one that runs, or one that
        // was left unfinished.
        let rollout = self
            .access
            .deployments(scope.project_id, &scope.environment)?
            .report(now_unix_ms()?)
            .rollout
            .filter(|rollout| rollout.active || rollout.unfinished());
        // The engine setup installed on a cluster node; on a single host
        // the one the environment uses.
        let runtime = node.as_ref().and_then(|node| node.runtime).unwrap_or(scope.runtime);
        let facts = NodeFacts {
            private_address: node.as_ref().and_then(|node| node.private_address.clone()),
            node: node.map(|node| node.name),
            daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
            deployed,
            rollout,
            engine: self.use_cases.engine(runtime).await,
            daemon: Some(self.daemon_health()?),
            secret_store: self.use_cases.secret_store(scope).await,
        };
        send_result(writer, context.request_id, RemoteResult::Node(Box::new(facts))).await
    }

    /// How this daemon fares: when it started, what of it does not answer,
    /// what runs, and child processes still there after their deadline.
    fn daemon_health(&self) -> Result<crate::domain::rollout::DaemonHealth, MmError> {
        let (watchdog, mut not_answering) = crate::adapters::remote::liveness::last_look();
        if let Err(why) = crate::adapters::process::supervisor_answers(Duration::from_millis(500)) {
            not_answering.push(format!("the watch over child processes: {why}"));
        }
        Ok(crate::domain::rollout::DaemonHealth {
            started_at_unix_ms: self.started_at_unix_ms,
            watchdog_seconds: watchdog.map(|watchdog| watchdog.as_secs()),
            not_answering,
            operations: self.all_operations(),
            late_children: crate::adapters::process::late_children(now_unix_ms()?),
        })
    }
}

/// A seal is for one project, environment, and target.
fn check_seal_scope(seal: &ClusterPlanSeal, scope: &RemoteScope) -> Result<(), MmError> {
    if seal.project_id != scope.project_id
        || seal.environment != scope.environment
        || seal.target != scope.target
        || seal.target_fingerprint != scope.target_fingerprint
    {
        return Err(MmError::Security {
            message: "the cluster plan is for another project, environment, or target".to_owned(),
        });
    }
    Ok(())
}
