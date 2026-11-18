// What a host does once its part of a deployment is in place: it runs the
// deployment's `after-deploy` jobs that belong to this node, starts its
// `once` jobs that never ran in the environment, and keeps the record of
// every `once` job the deployment knows of, here or on another node. A job
// that fails does not undo the deployment: it is reported with it.

impl RemoteDaemon {
    async fn after_deployment(
        &self,
        context: &RemoteRequestContext,
        plan: &crate::domain::deployment::DeploymentPlan,
        cluster: Option<&crate::domain::rollout::ClusterRollout>,
        (timeout, pass): (Duration, &MutationPass),
    ) -> Result<Vec<crate::domain::scheduler::JobOutcome>, MmError> {
        use crate::domain::config::JobWhen;
        use crate::domain::scheduler::{JobOutcome, JobRunRequest, JobTrigger, OnceRecord};
        // The runs, and the records of `once` jobs, are filed under the
        // deployment: the cluster plan on a cluster node.
        let cluster_plan = cluster.and(context.request_plan_hash.as_deref());
        let label = crate::domain::scheduler::deployment_label(cluster_plan, &plan.plan_hash);
        let known = self.keep_once_records(context, label, cluster)?;
        let mut outcomes = Vec::new();
        let node = context.scope.node.clone();
        for job in plan.rollout.jobs.iter().filter(|job| job.run != JobWhen::BeforeDeploy) {
            let once = job.run == JobWhen::Once;
            // A `once` job runs where nothing says it ran: on a cluster,
            // the plan says it is due.
            let due = match cluster {
                Some(cluster) => cluster.once.iter().any(|due| due.job == job.job),
                None => !known.contains_key(&job.job),
            };
            if once && !due {
                continue;
            }
            let request = JobRunRequest {
                job: job.job.clone(),
                wait: !once,
                trigger: if once { JobTrigger::Once } else { JobTrigger::AfterDeploy },
                deployment: Some(label.to_owned()),
                run: None,
            };
            let running = self.use_cases.run_job(context, request);
            let outcome = match self.watched(context, pass, timeout, running).await {
                Err(error @ MmError::Security { .. }) => return Err(error),
                Ok(run) => {
                    if once {
                        let record = OnceRecord::started(
                            node.clone(),
                            Some(run.id),
                            (cluster_plan, &plan.plan_hash),
                            now_unix_ms()?,
                        );
                        self.access.update_deployments(context.scope.project_id, &context.scope.environment, |stored| {
                            stored.once.insert(job.job.clone(), record.clone());
                            Ok(())
                        })?;
                    }
                    JobOutcome::of(job.run, &run, node.clone())
                }
                Err(error) => JobOutcome::not_started(
                    &job.job,
                    job.run,
                    node.clone(),
                    bounded_message(&redacted_error_message(&error)),
                ),
            };
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }

    /// Keeps, in this host's state, the record of every `once` job the
    /// cluster plan knows of: those that ran before, and those it starts
    /// on their nodes now. Returns every record this host keeps.
    fn keep_once_records(
        &self,
        context: &RemoteRequestContext,
        plan_hash: &str,
        cluster: Option<&crate::domain::rollout::ClusterRollout>,
    ) -> Result<std::collections::BTreeMap<String, crate::domain::scheduler::OnceRecord>, MmError> {
        let scope = &context.scope;
        let now = now_unix_ms()?;
        self.access.update_deployments(scope.project_id, &scope.environment, |stored| {
            if let Some(cluster) = cluster {
                for (job, record) in &cluster.ran_once {
                    stored.once.entry(job.clone()).or_insert_with(|| record.clone());
                }
                for due in &cluster.once {
                    stored.once.entry(due.job.clone()).or_insert_with(|| crate::domain::scheduler::OnceRecord {
                        node: Some(due.node.clone()),
                        run: None,
                        plan_hash: plan_hash.to_owned(),
                        at_unix_ms: now,
                    });
                }
            }
            Ok(stored.once.clone())
        })
    }
}
