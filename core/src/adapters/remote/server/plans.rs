/// What a commit is planned for: a deployment, with the build receipt whose
/// images it runs and the release's name and inputs, or an access sync.
enum PlanRequest {
    Deployment(Option<Box<BuildReceipt>>, crate::domain::release_facts::ReleaseRequest),
    TeamSync,
}

/// A reviewed deployment as a one-call apply takes it from its stored plan.
type ReviewedDeployment = (
    RemotePlanBinding,
    Option<crate::domain::deployment::DeploymentPlan>,
    (Option<BuildReceipt>, Option<crate::domain::release_facts::ReleaseFacts>),
    Vec<crate::domain::staged::StagedPlanItem>,
);

impl RemoteDaemon {
    /// Fetches `commit`, checks it may run here, checks it out, and binds a
    /// reviewed plan: the deployment (for deploys) plus the access change.
    async fn plan<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        commit: &str,
        request: PlanRequest,
        timeout: Duration,
    ) -> Result<(), MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        let (kind, receipt, release) = match request {
            PlanRequest::Deployment(receipt, release) => (
                RemotePlanKind::Deployment,
                receipt.map(|receipt| *receipt),
                Some(crate::domain::release_facts::ReleaseFacts::new(Some(commit.to_owned()), release)),
            ),
            PlanRequest::TeamSync => (RemotePlanKind::TeamSync, None, None),
        };
        let scope = &context.scope;
        // Reject a receipt for another commit or project before fetching
        // anything. One built for another environment is checked against this
        // environment's build plan once the commit is checked out.
        if let Some(receipt) = &receipt {
            receipt
                .validate()
                .and_then(|()| check_receipt_scope(receipt, scope.project_id, commit))
                .map_err(|message| MmError::Security { message })?;
        }
        let CheckedOutAccess { root, access, document, bundle_hash, allow, .. } = self
            .checkout_commit(scope.project_id, &scope.environment, &context.principal, commit, timeout)
            .await?;
        let mut planned = context.clone();
        planned.project_root = root;
        planned.release.clone_from(&release);
        // On a cluster node the engine plans when a service is placed here,
        // or when the last deployment left containers to remove. A node
        // with neither takes the access change and runs nothing.
        let placed = self.node_services(scope, &document, commit)?;
        let deployment = match kind {
            RemotePlanKind::Deployment
                if placed.as_ref().is_some_and(NodeServices::is_empty)
                    && !self.runs_containers(scope)?
                    && !scope.node.as_deref().is_some_and(|node| {
                        crate::domain::placement::runs_ingress(&document.config, &scope.target, node, &scope.environment)
                    }) =>
            {
                None
            }
            RemotePlanKind::Deployment => Some(
                timed(timeout, self.use_cases.plan_deployment(&planned, receipt.as_ref())).await?,
            ),
            RemotePlanKind::TeamSync => None,
        };
        refuse_unscoped_names(&document, &scope.environment, deployment.as_ref(), &allow)?;
        let access_hash = access.canonical_hash().map_err(|message| MmError::State { message })?;
        let deployment_hash = match (kind, &deployment) {
            (RemotePlanKind::TeamSync, _) => access_hash.clone(),
            (RemotePlanKind::Deployment, Some(plan)) => plan.plan_hash.clone(),
            (RemotePlanKind::Deployment, None) => crate::domain::remote::nothing_placed_hash(),
        };
        let staged = match kind {
            RemotePlanKind::Deployment => timed(timeout, self.use_cases.staged_plan(&planned)).await?,
            RemotePlanKind::TeamSync => Vec::new(),
        };
        let hashes = (deployment_hash, access_hash, bundle_hash);
        let binding = self.bind_plan(context, kind, commit, hashes)?;
        let release = deployment.as_ref().map_or(release, |plan| plan.release.clone());
        let details = planned_details(&binding, receipt.as_ref(), release.as_ref());
        let (stored, result) = match kind {
            RemotePlanKind::Deployment => {
                let stored = self.access.deployments(scope.project_id, &scope.environment)?;
                let once = stored.once.clone();
                let unfinished = stored
                    .report(now_unix_ms()?)
                    .rollout
                    .filter(|rollout| rollout.unfinished() && !rollout.active);
                let remote = RemoteDeploymentPlan {
                    binding, plan: deployment, access, receipt, unfinished, once, staged, release,
                };
                (
                    StoredRemotePlan::Deployment(Box::new(remote.clone())),
                    RemoteResult::DeploymentPlan(Box::new(remote)),
                )
            }
            RemotePlanKind::TeamSync => {
                let remote = RemoteAccessPlan { binding, access };
                (
                    StoredRemotePlan::TeamSync(Box::new(remote.clone())),
                    RemoteResult::AccessPlan(Box::new(remote)),
                )
            }
        };
        self.journal.save_plan(&stored)?;
        self.audit_planned(context, &stored, details)?;
        send_result(writer, context, result).await
    }

    /// Binds a plan to this daemon, the request's scope, principal, and
    /// authorization, for one apply within the plan lifetime.
    fn bind_plan(
        &self,
        context: &RemoteRequestContext,
        kind: RemotePlanKind,
        commit: &str,
        (deployment_plan_hash, access_change_hash, bundle_hash): (String, String, Option<String>),
    ) -> Result<RemotePlanBinding, MmError> {
        let scope = &context.scope;
        let mut binding = RemotePlanBinding {
            protocol_version: context.protocol,
            daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
            daemon_fingerprint: self.identity.fingerprint.clone(),
            target_fingerprint: scope.target_fingerprint.clone(),
            target: scope.target.clone(),
            node: scope.node.clone(),
            kind,
            project_id: scope.project_id,
            environment: scope.environment.clone(),
            runtime: scope.runtime,
            commit: commit.to_owned(),
            deployment_plan_hash,
            access_change_hash,
            principal_id: context.principal_id,
            authorization_revision: context.authorization.policy_revision,
            authorization_hash: context.authorization.authorization_hash.clone(),
            plan_nonce: uuid::Uuid::new_v4(),
            expires_at_unix_ms: now_unix_ms()?.saturating_add(PLAN_TTL_MS),
            bundle_hash,
            plan_hash: String::new(),
        };
        binding.plan_hash = binding.canonical_hash().map_err(|message| MmError::Security { message })?;
        Ok(binding)
    }

    /// The audit record of a stored plan, with what it plans in `details`.
    fn audit_planned(
        &self,
        context: &RemoteRequestContext,
        stored: &StoredRemotePlan,
        details: serde_json::Value,
    ) -> Result<(), MmError> {
        let scope = &context.scope;
        let mut record = serde_json::json!({
            "timestamp_unix_ms": now_unix_ms()?, "request_id": context.request_id,
            "operation_nonce": context.operation_nonce, "principal_id": context.principal_id,
            "principal": context.principal,
            "key_fingerprint": context.key_fingerprint, "session_id": context.session_id,
            "source": context.source, "project_id": scope.project_id,
            "environment": scope.environment, "target": scope.target,
            "capability": context.authorization.capability, "plan_hash": stored.binding().plan_hash,
            "result": "planned", "daemon_version": env!("CARGO_PKG_VERSION"),
        });
        if let (Some(record), serde_json::Value::Object(details)) = (record.as_object_mut(), details) {
            record.extend(details);
        }
        self.journal.audit(&record)
    }

    /// Reads a stored plan, checks it is for this operation, principal, and
    /// authorization, and claims it so it cannot be applied twice.
    fn claim_reviewed(
        &self,
        context: &RemoteRequestContext,
        (remote_plan_hash, expected): (&str, RemotePlanKind),
        (plan_authorization, reviewed_at): (&Authorization, i64),
    ) -> Result<StoredRemotePlan, MmError> {
        // A cluster apply names the cluster plan; this node applies the
        // section the seal holds for it and nothing else.
        let node_plan_hash = self.sealed_section(&context.scope, remote_plan_hash)?;
        let remote_plan_hash = node_plan_hash.as_deref().unwrap_or(remote_plan_hash);
        let stored = self.journal.read_plan(remote_plan_hash, &context.scope, reviewed_at)?;
        let binding = stored.binding();
        if binding.kind != expected {
            return Err(MmError::Security {
                message: "the reviewed plan is for a different operation".to_owned(),
            });
        }
        if binding.principal_id != context.principal_id
            || binding.authorization_hash != plan_authorization.authorization_hash
        {
            return Err(MmError::Security {
                message: "reviewed plan principal or authorization is stale; produce a new plan".to_owned(),
            });
        }
        // A plan made from a pushed bundle runs only on that bundle's files.
        if let Some(planned) = &binding.bundle_hash {
            let held = self.repositories.delivered(binding.project_id, &binding.environment, &binding.commit)?;
            if held.map(|record| record.bundle_hash).as_ref() != Some(planned) {
                return Err(MmError::Security {
                    message: format!(
                        "the files this host holds for commit {} are not the bundle the plan was made from; produce a new plan",
                        binding.commit
                    ),
                });
            }
        }
        let stored = self.journal.claim_validated_plan(remote_plan_hash, stored)?;
        self.journal.set_request_phase(context.operation_nonce, "running")?;
        Ok(stored)
    }

    /// Fetches `commit` (or takes the files a client pushed for it), checks
    /// it may run in `environment`, checks it out,
    /// reconciles its users file as `principal`, and applies the workload
    /// guardrails. A team sync over the relay and `mmd access sync` both
    /// start here, so they enforce the same rules.
    async fn checkout_commit(
        &self,
        project: crate::domain::workspace::ProjectId,
        environment: &str,
        principal: &str,
        commit: &str,
        timeout: Duration,
    ) -> Result<CheckedOutAccess, MmError> {
        let snapshot = self.access.load()?;
        let enrolled = snapshot.host.projects.get(&project).ok_or_else(|| {
            MmError::Security { message: "project is not enrolled on this host".to_owned() }
        })?;
        if !enrolled.environments.contains(environment) {
            return Err(MmError::Security {
                message: not_set_up(environment),
            });
        }
        let current = snapshot
            .records
            .iter()
            .find(|record| record.project_id == project && record.environment == environment)
            .cloned();
        // The ratchet: a commit must pass the rule in force now, including a
        // commit that changes the rule.
        let allowed = current
            .as_ref()
            .and_then(|record| record.deploy_from.clone())
            .or_else(|| enrolled.initial_deploy_from.get(environment).cloned());
        let (root, bundle_hash) = if enrolled.delivery.is_push() {
            // The commit's files came from a client and were proven to be
            // the commit; nothing is fetched, and the branch is the one
            // that client stated.
            let (root, record) = self.repositories.pushed_checkout(project, environment, commit)?;
            check_stated_branches(commit, &record.branches, allowed.as_deref())?;
            (root, Some(record.bundle_hash))
        } else {
            timed(timeout, self.repositories.fetch(project, enrolled)).await?;
            self.repositories
                .check_reachable(project, enrolled, commit, allowed.as_deref())
                .await?;
            let checkout = self.repositories.checkout(project, enrolled, environment, commit);
            (timed(timeout, checkout).await?, None)
        };
        let (access, document) =
            reconcile_from_checkout(&root, (project, environment), commit, principal, current.as_ref())?;
        let document = Box::new(document);
        // Trusting the repo means trusting it within the host's limits: no
        // Compose feature that reaches the host runs unless host.yaml allows
        // it for that service.
        let findings = crate::app::workload_guardrails(&document, environment, &enrolled.allow)?;
        refuse_findings(environment, &findings)?;
        let allow = enrolled.allow.clone();
        Ok(CheckedOutAccess { root, current, access, document, bundle_hash, allow })
    }

    /// Applies a reviewed plan: access first, so revocations hold even if the
    /// stacks fail, then the deployment.
    async fn apply<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        timeout: Duration,
        (remote_plan_hash, expected): (&str, RemotePlanKind),
        plan_authorization: &Authorization,
        pass: &MutationPass,
    ) -> Result<(), MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        if expected == RemotePlanKind::Deployment {
            self.refuse_while_rolling(&context.scope)?;
        }
        let stored = self.claim_reviewed(
            context,
            (remote_plan_hash, expected),
            (plan_authorization, now_unix_ms()?),
        )?;
        let (access, deployment) = match stored {
            StoredRemotePlan::Deployment(plan) => {
                (plan.access, Some((plan.binding, plan.plan, (plan.receipt, plan.release), plan.staged)))
            }
            StoredRemotePlan::TeamSync(plan) => (plan.access, None),
        };
        let Some(deployment) = deployment else {
            self.change_access(context, pass, &access).await?;
            return send_result(writer, context, RemoteResult::AccessApplied(Box::new(access))).await;
        };
        // The deployment is on record from here on, whatever becomes of
        // it or of its client: `mm deploy status` shows it running,
        // finished, or failed with its error.
        self.begin_one_call(context, remote_plan_hash, &deployment.0)?;
        let applied = self
            .apply_here(context, (timeout, pass), (remote_plan_hash, &access), deployment)
            .await;
        let cancelled = pass.running().and_then(|running| running.stopping()).is_some_and(|stop| stop.is_cancellation());
        self.end_one_call(context, remote_plan_hash, (applied.as_ref().err(), cancelled))?;
        match applied? {
            Some(result) => {
                send_result(writer, context, RemoteResult::DeploymentApplied(Box::new(result))).await
            }
            None => send_result(writer, context, RemoteResult::NothingPlaced).await,
        }
    }

    /// The deployment of a one-call apply: access, the cluster's network
    /// addresses, the engine, and the record of what the host now runs.
    /// `None` when the plan places nothing on this host.
    async fn apply_here(
        &self,
        context: &RemoteRequestContext,
        (timeout, pass): (Duration, &MutationPass),
        (remote_plan_hash, access): (&str, &AccessChange),
        (binding, plan, (receipt, release), staged): ReviewedDeployment,
    ) -> Result<Option<crate::domain::deployment::DeploymentResult>, MmError> {
        self.change_access(context, pass, access).await?;
        let root = self
            .repositories
            .checkout_path(binding.project_id, &binding.environment, &binding.commit);
        // The addresses the cluster plan gives the networks: stored before
        // the engine creates a network on them. A node the plan places
        // nothing on still learns the other nodes' slices.
        let stored = self.store_sealed_networks(context, remote_plan_hash, plan.is_none());
        self.watched(context, pass, timeout, stored).await?;
        let mut applying = context.clone();
        applying.project_root = root.clone();
        applying.release.clone_from(&release);
        let Some(plan) = plan else {
            self.record_deployed(context, &binding, root, (false, remote_plan_hash, None, release.as_ref())).await?;
            self.go_live(&applying, &staged).await;
            return Ok(None);
        };
        let applied = self.use_cases.apply_deployment(&applying, &plan.plan_hash, receipt.as_ref());
        let result = self.watched(context, pass, timeout, applied).await.map_err(|error| {
            // Given up because access changed, or stopped: that is the
            // answer itself.
            if matches!(error, MmError::Security { .. } | MmError::Cancelled { .. } | MmError::Timeout { .. }) {
                return error;
            }
            // The host keeps the full cause; the deploying principal gets it
            // too, bounded, so a failed deploy can be diagnosed remotely.
            tracing::error!(
                request_id = %context.request_id,
                environment = %binding.environment,
                code = error.code(),
                %error,
                "remote deployment failed"
            );
            if let MmError::OperationFailure { operation_id, recovery, primary, .. } = &error
                && error.kept()
            {
                return MmError::operation_failure(
                    operation_id.clone(),
                    crate::domain::error::MutationStatus::Kept,
                    MmError::Remote {
                        code: error.code(),
                        message: format!("the deployment failed on the host: {}", bounded_message(&primary.to_string())),
                    },
                    Vec::new(),
                    recovery.clone(),
                );
            }
            // Its own code, not a security refusal: the deploy was allowed
            // and something on the host failed.
            MmError::Remote {
                code: error.code(),
                message: format!(
                    "the deployment failed on the host: {}. The host returned to what it ran before where it could, and nothing is retried automatically; `mm deploy status` shows what it runs and `mm deploy plan` plans from there",
                    bounded_message(&error.to_string())
                ),
            }
        })?;
        // A node still runs containers of the environment while a service
        // that moved away waits to be stopped.
        let runs_services = plan.node.as_ref().is_none_or(|node| node.keeps_containers());
        self.record_deployed(
            context,
            &binding,
            root,
            (runs_services, remote_plan_hash, receipt.as_ref(), release.as_ref()),
        )
        .await?;
        self.go_live(&applying, &staged).await;
        let mut result = result;
        let following = remaining(context)?;
        result.jobs = self
            .after_deployment(&applying, &plan, None, (following, pass))
            .await?;
        Ok(Some(result))
    }

    /// Puts a one-call apply on record as the environment's deployment in
    /// progress: it holds the environment until it ends or its deadline.
    fn begin_one_call(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        binding: &RemotePlanBinding,
    ) -> Result<(), MmError> {
        let scope = &context.scope;
        let now = now_unix_ms()?;
        self.access.update_deployments(scope.project_id, &scope.environment, |stored| {
            if let Some(running) = stored.rollout.as_ref().filter(|rollout| rollout.holds_lock(now)) {
                return Err(deploy_running(running, &scope.environment, now));
            }
            stored.rollout = Some(crate::adapters::remote::access::StoredRollout {
                plan_hash: hash.to_owned(),
                commit: binding.commit.clone(),
                principal_id: context.principal_id,
                started_by: context.principal.clone(),
                started_at_unix_ms: now,
                heard_at_unix_ms: now,
                expires_at_unix_ms: context.deadline_unix_ms,
                state: RolloutState::Running,
                stacks: Vec::new(),
                failure: None,
                interrupted: None,
                interrupted_at_unix_ms: None,
                changes: None,
                begun_under: None,
            });
            Ok(())
        })
    }

    /// Records how a one-call apply ended. One that was stopped, or whose
    /// engine work failed without returning the host, stays unfinished
    /// with its error, so `mm deploy status` and the next plan say so; one
    /// that failed otherwise was rolled back by the engine. One that was
    /// `cancelled` keeps the environment for the cancellation, which frees
    /// it once it has undone the apply.
    fn end_one_call(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        (failed, cancelled): (Option<&MmError>, bool),
    ) -> Result<(), MmError> {
        let failure = failed.map(|error| StepFailure {
            stack: String::new(),
            service: None,
            reason: bounded_message(&redacted_error_message(error)),
            log: Vec::new(),
            code: Some(error.code().to_owned()),
        });
        let unfinished = failed.is_some_and(|error| {
            matches!(error, MmError::Security { .. } | MmError::Cancelled { .. } | MmError::Timeout { .. })
                || error.code() == "MM_STATE"
                || error.kept()
        });
        self.note_rollout(context, hash, |rollout, now| {
            if !cancelled {
                rollout.expires_at_unix_ms = now;
            }
            rollout.state = match (&failure, unfinished) {
                (None, _) => RolloutState::Finished,
                (Some(_), true) => RolloutState::Running,
                (Some(_), false) => RolloutState::RolledBack,
            };
            rollout.failure = failure;
        })
    }

    /// Records the commit this host now runs for the environment, and
    /// keeps the deployment before it as the one a rollback returns to.
    /// `plan_hash` is the plan the client applied: for a cluster, the hash
    /// every node of the deployment shares. This records what the engine
    /// already did, so it is written whatever happened to access meanwhile.
    async fn record_deployed(
        &self,
        context: &RemoteRequestContext,
        binding: &RemotePlanBinding,
        root: std::path::PathBuf,
        (runs_services, plan_hash, receipt, release): (
            bool,
            &str,
            Option<&BuildReceipt>,
            Option<&crate::domain::release_facts::ReleaseFacts>,
        ),
    ) -> Result<(), MmError> {
        let now = now_unix_ms()?;
        // A host that was deployed to before it kept a history still knows
        // what it runs: that becomes the deployment to return to.
        let known = self
            .access
            .deployed(binding.project_id, &binding.environment)?
            .map(|deployed| crate::domain::rollout::DeploymentRecord {
                commit: deployed.commit,
                deployed_at_unix_ms: deployed.deployed_at_unix_ms,
                deployed_by: deployed.deployed_by,
                runs_services: deployed.runs_services,
                plan_hash: None,
                receipt: None,
                release: None,
            });
        let deployed = DeployedState {
            project_id: binding.project_id,
            environment: binding.environment.clone(),
            commit: binding.commit.clone(),
            root,
            deployed_at_unix_ms: now,
            deployed_by: context.principal.clone(),
            runs_services,
        };
        self.access
            .locked("mmd, recording a deployment", |guard| self.access.write_deployed(guard, &deployed))
            .await?;
        self.access.update_deployments(binding.project_id, &binding.environment, |stored| {
            if stored.current.is_none() {
                stored.current = known;
            }
            stored.finished(crate::domain::rollout::DeploymentRecord {
                commit: binding.commit.clone(),
                deployed_at_unix_ms: now,
                deployed_by: context.principal.clone(),
                runs_services,
                plan_hash: Some(plan_hash.to_owned()),
                receipt: receipt.cloned().map(Box::new),
                release: release.cloned(),
            });
            Ok(())
        })
    }

    /// Applies the reviewed access change: writes this host's record, lifts
    /// emergency blocks the repo dropped, and renders `authorized_keys`.
    /// `principal` is recorded as who reconciled it. Returns the revision
    /// of the policy it leaves.
    fn apply_access(
        &self,
        principal: &str,
        guard: &AccessGuard,
        access: &AccessChange,
    ) -> Result<u64, MmError> {
        let mut record = access.record.clone();
        record.reconciled_at_unix_ms = now_unix_ms()?;
        principal.clone_into(&mut record.reconciled_by);
        self.access.write_record(guard, &record)?;
        // An emergency block lasts until the repo stops listing the key.
        let listed = record
            .users
            .values()
            .flat_map(|user| user.keys.iter().map(|key| key.fingerprint.as_str()))
            .collect::<std::collections::BTreeSet<_>>();
        let project_blocks = guard
            .snapshot()
            .blocks
            .iter()
            .filter(|block| block.project_id == record.project_id)
            .collect::<Vec<_>>();
        let kept = project_blocks
            .iter()
            .filter(|block| {
                block.environment != record.environment || listed.contains(block.fingerprint.as_str())
            })
            .map(|block| (*block).clone())
            .collect::<Vec<_>>();
        if kept.len() != project_blocks.len() {
            self.access.write_blocks(guard, record.project_id, &kept)?;
        }
        let snapshot = self.access.load()?;
        self.access.render_authorized_keys(&snapshot.policy)?;
        Ok(snapshot.policy.revision)
    }

    async fn whoami<W>(&self, writer: &mut W, context: &RemoteRequestContext) -> Result<(), MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        let scope = &context.scope;
        let snapshot = self.access.load()?;
        let record = snapshot.records.iter().find(|record| {
            record.project_id == scope.project_id && record.environment == scope.environment
        });
        let break_glass = context.principal.starts_with("break-glass:");
        let access = if break_glass {
            Some(crate::domain::host_policy::EnvironmentAccess::level(
                crate::domain::host_policy::AccessLevel::Admin,
            ).effective())
        } else {
            let ceiling = snapshot
                .host
                .projects
                .get(&scope.project_id)
                .map(|project| project.ceiling_for(&scope.environment));
            record
                .zip(ceiling)
                .and_then(|(record, ceiling)| {
                    record.access_of_key(&context.key_fingerprint, &ceiling)
                })
        };
        let identity = crate::domain::remote::RemoteIdentity {
            principal: context.principal.clone(),
            principal_id: context.principal_id,
            key_fingerprint: context.key_fingerprint.clone(),
            break_glass,
            environment: scope.environment.clone(),
            access,
            access_commit: record.map(|record| record.commit.clone()),
            deployed_commit: self
                .access
                .deployed(scope.project_id, &scope.environment)?
                .map(|deployed| deployed.commit),
        };
        send_result(writer, context, RemoteResult::Identity(Box::new(identity))).await
    }
}

/// A commit checked out for one environment: where, the access record it
/// replaces, and the reconciled change.
/// Refuses a deployment whose volume or network `name:` is outside its
/// stack's Compose project name here.
fn refuse_unscoped_names(
    document: &crate::domain::config::ConfigDocument,
    environment: &str,
    deployment: Option<&crate::domain::deployment::DeploymentPlan>,
    allow: &[crate::domain::host_policy::GuardrailException],
) -> Result<(), MmError> {
    let Some(plan) = deployment else {
        return Ok(());
    };
    let names = plan
        .stacks
        .iter()
        .map(|stack| (stack.stack.clone(), stack.compose_project_name.clone()))
        .collect();
    let findings = crate::app::named_resource_guardrails(document, environment, &names, allow)?;
    refuse_findings(environment, &findings)
}

/// Refuses a plan with guardrail findings, naming each one.
fn refuse_findings(environment: &str, findings: &[crate::app::GuardrailFinding]) -> Result<(), MmError> {
    if findings.is_empty() {
        return Ok(());
    }
    let next = if findings.iter().any(crate::app::GuardrailFinding::grantable) {
        ". A host admin can allow a feature for one service under `allow` in /etc/mm/host.yaml"
    } else {
        ""
    };
    Err(MmError::Security {
        message: format!(
            "this host refuses Compose features that reach the host in environment '{environment}': {}{next}",
            findings.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
        ),
    })
}

struct CheckedOutAccess {
    root: std::path::PathBuf,
    current: Option<crate::domain::host_policy::EnvironmentAccessRecord>,
    access: AccessChange,
    /// The commit's configuration.
    document: Box<crate::domain::config::ConfigDocument>,
    /// The bundle the checkout was made from, on a host the project is
    /// pushed to.
    bundle_hash: Option<String>,
    /// The host's guardrail exceptions for the project.
    allow: Vec<crate::domain::host_policy::GuardrailException>,
}

/// What a plan's audit record says it plans: the commit, the bundle its
/// files came in when they were pushed, and, with a build receipt, the
/// images.
fn planned_details(
    binding: &RemotePlanBinding,
    receipt: Option<&BuildReceipt>,
    release: Option<&crate::domain::release_facts::ReleaseFacts>,
) -> serde_json::Value {
    let mut details = serde_json::json!({ "commit": binding.commit });
    if let (Some(release), Some(details)) = (release, details.as_object_mut()) {
        details.insert("release".to_owned(), release.release_name().unwrap_or_default().into());
        if !release.inputs.is_empty() {
            details.insert("release_inputs".to_owned(), serde_json::json!(release.inputs));
        }
    }
    if let (Some(bundle_hash), Some(details)) = (&binding.bundle_hash, details.as_object_mut()) {
        details.insert("bundle_hash".to_owned(), bundle_hash.clone().into());
    }
    if let (Some(receipt), Some(details)) = (receipt, details.as_object_mut()) {
        details.insert("build_receipt".to_owned(), receipt.receipt_hash.clone().into());
        details.insert(
            "build_receipt_environment".to_owned(),
            receipt.plan.environment.clone().into(),
        );
        details.insert(
            "images".to_owned(),
            receipt
                .result
                .images
                .iter()
                .map(|image| format!("{}@{}", image.repository, image.digest))
                .collect::<Vec<_>>()
                .into(),
        );
    }
    details
}

/// Loads the checked-out commit's configuration (never local overrides) and
/// reconciles its users file for one environment.
fn reconcile_from_checkout(
    root: &std::path::Path,
    (project, environment_name): (crate::domain::workspace::ProjectId, &str),
    commit: &str,
    principal: &str,
    current: Option<&crate::domain::host_policy::EnvironmentAccessRecord>,
) -> Result<(AccessChange, crate::domain::config::ConfigDocument), MmError> {
    let document = crate::adapters::config::ConfigLoader
        .load(&root.join(crate::adapters::config::CONFIG_RELATIVE_PATH))?;
    if document.config.project.id != project {
        return Err(MmError::Security {
            message: "the commit's configuration belongs to another project".to_owned(),
        });
    }
    let environment = document.config.environments.get(environment_name).ok_or_else(|| {
        MmError::Environment {
            message: format!("environment '{environment_name}' is not configured at commit {commit}"),
        }
    })?;
    let record = crate::domain::users::reconcile_users(
        (&document.config.users, &document.config.agents),
        environment.deploy_from.clone(),
        project,
        environment_name,
        commit,
        now_unix_ms()?,
        principal,
    )
    .map_err(|message| MmError::ConfigParse {
        path: document.source("users").to_path_buf(),
        message,
    })?;
    Ok((AccessChange::between(current, record), document))
}

/// A host error for the client: at most 4000 characters, without control
/// characters other than newlines.
fn bounded_message(message: &str) -> String {
    let mut bounded = message
        .chars()
        .filter(|character| *character == '\n' || !character.is_control())
        .take(4000)
        .collect::<String>();
    if message.chars().count() > 4000 {
        bounded.push('…');
    }
    bounded
}
