// The typed remote operations over one SSH relay connection each.

const STORE_REQUEST_TIMEOUT_MS: u64 = 5 * 60 * 1_000;

#[async_trait]
impl RemoteTransport for SshRemoteTransport {
    async fn status(&self, scope: RemoteScope) -> Result<WorkspaceSnapshot, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::Status,
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::Status(snapshot) => Ok(*snapshot),
            _ => Err(protocol("remote status returned another result type")),
        }
    }

    async fn deliver(
        &self,
        scope: RemoteScope,
        offer: crate::domain::delivery::BundleOffer,
        bundle: Option<PathBuf>,
    ) -> Result<crate::domain::delivery::Delivered, MmError> {
        self.deliver_bundle(scope, offer, bundle).await
    }

    async fn logs(
        &self,
        scope: RemoteScope,
        request: LogRequest,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        let timeout = if request.follow {
            Duration::from_millis(MAX_REQUEST_TIMEOUT_MS)
        } else {
            Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS)
        };
        let stream = self
            .stream(
                scope,
                RemoteMethod::Logs(request),
                timeout,
                cancellation.clone(),
                StreamKind::Logs,
            )
            .await?;
        let (sender, receiver) = tokio::sync::mpsc::channel(128);
        tokio::spawn(async move {
            let mut stream = stream;
            while let Some(item) = stream.receiver.recv().await {
                let item = match item {
                    Ok(StreamValue::Log(record)) => Ok(record),
                    Ok(StreamValue::Exec(_)) => Err(protocol("exec event appeared in log stream")),
                    Err(error) => Err(error),
                };
                if sender.send(item).await.is_err() {
                    stream.cancellation.cancel();
                    break;
                }
            }
        });
        Ok(LogStream::new(receiver, cancellation))
    }

    async fn exec(
        &self,
        scope: RemoteScope,
        request: ExecRequest,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        if request.stdin != crate::domain::observability::ExecStdinMode::Closed
            || request.tty != crate::domain::observability::ExecTtyMode::Disabled
        {
            return Err(protocol(
                "this remote protocol does not transport exec stdin or TTY state yet",
            ));
        }
        let timeout = request
            .timeout
            .min(Duration::from_millis(MAX_REQUEST_TIMEOUT_MS));
        let stream = self
            .stream(
                scope,
                RemoteMethod::Exec(request),
                timeout,
                cancellation.clone(),
                StreamKind::Exec,
            )
            .await?;
        let (sender, receiver) = tokio::sync::mpsc::channel(128);
        tokio::spawn(async move {
            let mut stream = stream;
            while let Some(item) = stream.receiver.recv().await {
                let item = match item {
                    Ok(StreamValue::Exec(event)) => Ok(event),
                    Ok(StreamValue::Log(_)) => Err(protocol("log record appeared in exec stream")),
                    Err(error) => Err(error),
                };
                if sender.send(item).await.is_err() {
                    stream.cancellation.cancel();
                    break;
                }
            }
        });
        Ok(ExecStream::new(receiver, cancellation))
    }

    async fn plan_deployment(
        &self,
        scope: RemoteScope,
        commit: String,
        receipt: Option<crate::domain::build::BuildReceipt>,
        release: crate::domain::release_facts::ReleaseRequest,
    ) -> Result<RemoteDeploymentPlan, MmError> {
        // Planning fetches the commit on the host, so allow it more time.
        let sent = receipt.as_ref().map(|receipt| receipt.receipt_hash.clone());
        let asked = release.clone();
        let plan = match self
            .unary(
                scope,
                RemoteMethod::PlanDeployment {
                    commit,
                    receipt: receipt.map(Box::new),
                    release,
                },
                Duration::from_millis(PLAN_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::DeploymentPlan(plan) => *plan,
            _ => {
                return Err(protocol(
                    "remote deployment plan returned another result type",
                ));
            }
        };
        if plan.receipt.as_ref().map(|receipt| &receipt.receipt_hash) != sent.as_ref() {
            return Err(protocol(
                "remote deployment plan does not deploy the build receipt sent",
            ));
        }
        if plan
            .release
            .as_ref()
            .is_some_and(|release| release.request() != asked)
        {
            return Err(protocol(
                "remote deployment plan does not deploy the release asked for",
            ));
        }
        Ok(plan)
    }

    async fn apply_deployment(
        &self,
        scope: RemoteScope,
        remote_plan_hash: String,
    ) -> Result<Option<DeploymentResult>, MmError> {
        let node = scope.node.is_some();
        match self
            .unary(
                scope,
                RemoteMethod::ApplyDeployment { remote_plan_hash },
                Duration::from_millis(APPLY_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::DeploymentApplied(result) => Ok(Some(*result)),
            RemoteResult::NothingPlaced if node => Ok(None),
            _ => Err(protocol(
                "remote deployment apply returned another result type",
            )),
        }
    }

    async fn finish_moves(
        &self,
        scope: RemoteScope,
        cluster_plan_hash: String,
    ) -> Result<Vec<crate::domain::placement::PlacedService>, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::FinishMoves { cluster_plan_hash },
                Duration::from_millis(APPLY_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::MovesFinished(stopped) => Ok(stopped),
            _ => Err(protocol("the node returned another result than the services it stopped")),
        }
    }

    async fn rollout(
        &self,
        scope: RemoteScope,
        plan_hash: String,
        step: crate::domain::rollout::RolloutStep,
        patience: Duration,
    ) -> Result<crate::domain::rollout::RolloutAnswer, MmError> {
        let patience = patience.min(Duration::from_millis(MAX_REQUEST_TIMEOUT_MS));
        match self
            .unary(scope, RemoteMethod::Rollout { plan_hash, step }, patience)
            .await?
        {
            RemoteResult::Rollout(answer) => Ok(*answer),
            _ => Err(protocol("the host returned another result than a rollout step's")),
        }
    }

    async fn names(
        &self,
        scope: RemoteScope,
    ) -> Result<crate::domain::discovery::NameReport, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::Names,
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::Names(report) => Ok(*report),
            _ => Err(protocol("the host returned another result than its names")),
        }
    }

    async fn deployment_status(
        &self,
        scope: RemoteScope,
    ) -> Result<crate::domain::rollout::NodeDeploymentStatus, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::DeploymentStatus,
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::DeploymentStatus(status) => Ok(*status),
            _ => Err(protocol("the host returned another result than its deployments")),
        }
    }

    async fn cancel(
        &self,
        scope: RemoteScope,
        operation: Option<String>,
    ) -> Result<crate::domain::rollout::CancelOutcome, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::Cancel { operation },
                Duration::from_millis(CANCEL_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::Cancelled(outcome) => Ok(*outcome),
            _ => Err(protocol("the host returned another result than a cancellation's")),
        }
    }

    async fn seal_plan(
        &self,
        scope: RemoteScope,
        seal: crate::domain::remote::ClusterPlanSeal,
    ) -> Result<crate::domain::remote::ClusterPlanSeal, MmError> {
        let sent = seal.clone();
        match self
            .unary(
                scope,
                RemoteMethod::SealPlan(Box::new(seal)),
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::Sealed(sealed) if *sealed == sent => Ok(*sealed),
            _ => Err(protocol("the node did not keep the cluster plan it was sent")),
        }
    }

    async fn sealed_plan(
        &self,
        scope: RemoteScope,
        cluster_plan_hash: String,
    ) -> Result<crate::domain::remote::ClusterPlanSeal, MmError> {
        let expected = cluster_plan_hash.clone();
        match self
            .unary(
                scope,
                RemoteMethod::SealedPlan { cluster_plan_hash },
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::Sealed(sealed) if sealed.cluster_plan_hash == expected => {
                sealed
                    .validate()
                    .map_err(|message| MmError::Security { message })?;
                Ok(*sealed)
            }
            _ => Err(protocol("the node returned another cluster plan")),
        }
    }

    async fn node(&self, scope: RemoteScope) -> Result<crate::domain::remote::NodeFacts, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::Node,
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::Node(facts) => Ok(*facts),
            _ => Err(protocol("remote node report returned another result type")),
        }
    }

    async fn mesh_status(
        &self,
        scope: RemoteScope,
    ) -> Result<crate::domain::mesh::MeshNodeStatus, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::MeshStatus,
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::MeshStatus(status) => Ok(*status),
            _ => Err(protocol("remote mesh status returned another result type")),
        }
    }

    async fn networks(
        &self,
        scope: RemoteScope,
    ) -> Result<crate::domain::cluster_network::NodeNetworkStatus, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::Networks,
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::Networks(status) => Ok(*status),
            _ => Err(protocol("remote networks returned another result type")),
        }
    }

    async fn runtimes(
        &self,
        scope: RemoteScope,
    ) -> Result<crate::domain::hardening::HostRuntimeStatus, MmError> {
        // Each runtime runs a container; the first run pulls its image.
        match self
            .unary(scope, RemoteMethod::Runtimes, Duration::from_secs(300))
            .await?
        {
            RemoteResult::Runtimes(status) => Ok(*status),
            _ => Err(protocol("remote runtimes returned another result type")),
        }
    }

    async fn storage_status(
        &self,
        scope: RemoteScope,
        storage_node: String,
        sizes: bool,
    ) -> Result<crate::domain::shared_storage::StorageNodeStatus, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::StorageStatus {
                    storage_node,
                    sizes,
                },
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::StorageStatus(status) => Ok(*status),
            _ => Err(protocol("remote storage status returned another result type")),
        }
    }

    async fn plan_volume_delete(
        &self,
        scope: RemoteScope,
        volume: crate::domain::shared_storage::VolumeName,
    ) -> Result<crate::domain::shared_storage::VolumeDeletePlan, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::PlanVolumeDelete {
                    stack: volume.stack,
                    volume: volume.volume,
                },
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::VolumeDeletePlan(plan) => Ok(*plan),
            _ => Err(protocol("remote volume delete plan returned another result type")),
        }
    }

    async fn delete_volume(
        &self,
        scope: RemoteScope,
        volume: crate::domain::shared_storage::VolumeName,
        plan_hash: String,
    ) -> Result<crate::domain::shared_storage::VolumeDeleted, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::DeleteVolume {
                    stack: volume.stack,
                    volume: volume.volume,
                    plan_hash,
                },
                Duration::from_millis(PLAN_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::VolumeDeleted(deleted) => Ok(*deleted),
            _ => Err(protocol("remote volume delete returned another result type")),
        }
    }

    async fn plan_team_sync(
        &self,
        scope: RemoteScope,
        commit: String,
    ) -> Result<RemoteAccessPlan, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::PlanTeamSync { commit },
                Duration::from_millis(PLAN_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::AccessPlan(plan) => Ok(*plan),
            _ => Err(protocol(
                "remote team sync plan returned another result type",
            )),
        }
    }

    async fn apply_team_sync(
        &self,
        scope: RemoteScope,
        remote_plan_hash: String,
    ) -> Result<AccessChange, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::ApplyTeamSync { remote_plan_hash },
                // It may wait its turn behind a deployment step; the
                // client's heartbeat says so meanwhile.
                Duration::from_millis(TEAM_SYNC_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::AccessApplied(change) => Ok(*change),
            _ => Err(protocol("remote team sync returned another result type")),
        }
    }

    async fn forward(
        &self,
        scope: RemoteScope,
        service: String,
        port: u16,
        local: tokio::net::TcpStream,
    ) -> Result<crate::domain::forward::ForwardTotals, MmError> {
        self.forward_connection(scope, service, port, local).await
    }

    fn close_kept_connection(&self) {
        if let Some(kept) = &self.endpoint.kept {
            kept.close();
        }
    }

    async fn restart(
        &self,
        scope: RemoteScope,
        request: RestartRequest,
    ) -> Result<RestartOutcome, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::Restart(request),
                Duration::from_millis(RESTART_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::Restarted(outcome) => Ok(*outcome),
            _ => Err(protocol("remote restart returned another result type")),
        }
    }

    async fn run_job(
        &self,
        scope: RemoteScope,
        request: JobRunRequest,
        patience: Duration,
    ) -> Result<JobRun, MmError> {
        let patience = patience.min(Duration::from_millis(MAX_REQUEST_TIMEOUT_MS));
        match self
            .unary(scope, RemoteMethod::RunJob(request), patience)
            .await?
        {
            RemoteResult::JobRun(run) => Ok(*run),
            _ => Err(protocol("remote job run returned another result type")),
        }
    }

    async fn secret_recipient(&self, scope: RemoteScope) -> Result<String, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::SecretRecipient,
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::SecretRecipient(recipient) => Ok(recipient),
            _ => Err(protocol("remote secret recipient returned another result type")),
        }
    }

    async fn stage_values(
        &self,
        scope: RemoteScope,
        stage: crate::domain::staged::ValueStage,
    ) -> Result<crate::domain::staged::StageAnswer, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::StageValues(Box::new(stage)),
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::Staged(answer) => Ok(*answer),
            _ => Err(protocol("remote stage returned another result type")),
        }
    }

    async fn host_values(
        &self,
        scope: RemoteScope,
    ) -> Result<crate::domain::staged::NodeValues, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::HostValues,
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::HostValues(values) => Ok(*values),
            _ => Err(protocol("remote host values returned another result type")),
        }
    }

    async fn store_snapshot(&self, scope: RemoteScope, recipient: String) -> Result<String, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::StoreSnapshot { recipient },
                Duration::from_millis(STORE_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::StoreSnapshot(sealed) => Ok(sealed),
            _ => Err(protocol("remote store snapshot returned another result type")),
        }
    }

    async fn store_restore(
        &self,
        scope: RemoteScope,
        step: crate::domain::secret_store::StoreRestoreStep,
    ) -> Result<crate::domain::secret_store::StoreRestored, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::StoreRestore(Box::new(step)),
                Duration::from_millis(STORE_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::StoreRestored(restored) => Ok(*restored),
            _ => Err(protocol("remote store restore returned another result type")),
        }
    }

    async fn store_migrate(
        &self,
        scope: RemoteScope,
        provider: String,
    ) -> Result<crate::domain::secret_store::StoreMigrated, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::StoreMigrate { provider },
                Duration::from_millis(STORE_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::StoreMigrated(moved) => Ok(*moved),
            _ => Err(protocol("remote store migration returned another result type")),
        }
    }

    async fn job_runs(
        &self,
        scope: RemoteScope,
        request: JobRunsRequest,
    ) -> Result<Vec<JobRun>, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::JobRuns(request),
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::JobRuns(runs) => Ok(runs),
            _ => Err(protocol("remote job runs returned another result type")),
        }
    }

    async fn whoami(&self, scope: RemoteScope) -> Result<RemoteIdentity, MmError> {
        match self
            .unary(
                scope,
                RemoteMethod::WhoAmI,
                Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            )
            .await?
        {
            RemoteResult::Identity(identity) => Ok(*identity),
            _ => Err(protocol("remote whoami returned another result type")),
        }
    }
}
