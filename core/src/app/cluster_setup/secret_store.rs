//! The `secret-store` step of a cluster setup.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::ClusterSetupService;
use super::steps::{mmd, mmd_of};
use crate::app::ports::{
    ClusterNodeCommand, ClusterScript, ClusterStoreKeeper, ProgressReporter, StoreScope,
    progress_on_node,
};
use crate::domain::cluster_setup::{ClusterNodePlan, ClusterSetupPlan, ClusterSetupStep};
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::repo_target::RepoTargetConfig;
use crate::domain::secret_store::{
    ClusterStorePlan, PinnedRelease, StoreFacts, StoreInitAnswer, StoreInitRequest, StoreMember,
    StoreMemberAnswer, StoreMemberSpec, StorePeerRemoval, StoreStatus, declared_store,
    pinned_lines, pinned_release, rejoining, removal_through, store_id, store_members,
};

/// How long setup waits for every member to vote.
const VOTERS_WITHIN: std::time::Duration = std::time::Duration::from_secs(90);

fn parse<T: serde::de::DeserializeOwned>(
    output: &str,
    node: &str,
    command: &str,
) -> Result<T, MmError> {
    serde_json::from_str(output.trim()).map_err(|error| MmError::Target {
        message: format!(
            "`{command}` on node {node} printed unexpected output: {}",
            crate::domain::error::printed(&error.to_string())
        ),
    })
}

fn release_of(store: &ClusterStorePlan) -> Result<&'static PinnedRelease, MmError> {
    pinned_release(&store.release).ok_or_else(|| MmError::Target {
        message: format!(
            "providers.{}.release is '{}'; on a cluster mm installs OpenBao {}",
            store.provider,
            store.release,
            pinned_lines().join(" or ")
        ),
    })
}

impl ClusterSetupService {
    /// For `mm target setup` of a project with a managed secret store: what
    /// keeps the authority and the recovery copy on this machine.
    #[must_use]
    pub fn with_secret_store(mut self, keeper: Arc<dyn ClusterStoreKeeper>) -> Self {
        self.keeper = Some(keeper);
        self
    }

    fn store_keeper(&self) -> Result<&Arc<dyn ClusterStoreKeeper>, MmError> {
        self.keeper.as_ref().ok_or_else(|| MmError::State {
            message: "cluster setup has no keeper for the managed secret store".to_owned(),
        })
    }

    /// What setup does to the environment's managed store, read from the
    /// member records its nodes hold; `None` when it declares none.
    pub(super) async fn plan_store(
        &self,
        document: &ConfigDocument,
        (environment, target): (&str, &RepoTargetConfig),
        reading: &mut super::reading::NodeReading,
        user: &str,
    ) -> Result<Option<ClusterStorePlan>, MmError> {
        let Some(declared) = declared_store(&document.config) else {
            return Ok(None);
        };
        let at = reading
            .steps
            .iter()
            .position(|step| *step == ClusterSetupStep::PushSecrets)
            .unwrap_or(reading.steps.len());
        reading.steps.insert(at, ClusterSetupStep::SecretStore);
        let (nodes, skipped) = (&reading.nodes, &reading.skipped);
        let provider = declared.provider.as_str();
        let pinned = pinned_release(declared.release).ok_or_else(|| MmError::Target {
            message: format!(
                "providers.{provider}.release is '{}'; on a cluster mm installs OpenBao {}",
                declared.release,
                pinned_lines().join(" or ")
            ),
        })?;
        let names = target
            .nodes
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();
        let addresses = target
            .mesh_addresses()
            .map_err(|message| MmError::Target { message })?;
        let members = store_members(declared.nodes, &names, provider)
            .map_err(|message| MmError::Target { message })?
            .into_iter()
            .filter_map(|name| {
                let position = names.iter().position(|known| *known == name)?;
                Some(StoreMember {
                    name,
                    address: *addresses.get(position)?,
                })
            })
            .collect::<Vec<_>>();
        let scope = (document.config.project.id, environment);
        let facts = self.store_facts(scope, nodes, &members, user).await?;
        let installed = facts
            .iter()
            .filter_map(|(name, facts)| Some((name.clone(), facts.version()?.to_owned())))
            .collect::<BTreeMap<_, _>>();
        let initialised = facts.values().any(StoreFacts::holds_data);
        let rejoins = if initialised {
            self.stale_members(scope, nodes, (&members, &facts), user)
                .await
        } else {
            Vec::new()
        };
        if !initialised
            && let Some(left) = skipped
                .iter()
                .find(|node| members.iter().any(|member| member.name == node.name))
        {
            return Err(MmError::Target {
                message: format!(
                    "the secret store of '{environment}' is initialised once, with every member; {} is a member and is left out. Run `mm -e {environment} target setup plan` without --nodes",
                    left.name
                ),
            });
        }
        Ok(Some(ClusterStorePlan {
            provider: provider.to_owned(),
            release: pinned.line.to_owned(),
            version: pinned.version.to_owned(),
            members,
            installed,
            initialised,
            rejoins,
        }))
    }

    /// What each member node holds of its member, by the read-only script.
    async fn store_facts(
        &self,
        (project, environment): (crate::domain::workspace::ProjectId, &str),
        nodes: &[ClusterNodePlan],
        members: &[StoreMember],
        user: &str,
    ) -> Result<BTreeMap<String, StoreFacts>, MmError> {
        let command = ClusterNodeCommand::Script {
            script: ClusterScript::StoreFacts,
            arguments: vec![store_id(project, environment)],
        };
        let mut facts = BTreeMap::new();
        for node in nodes
            .iter()
            .filter(|node| members.iter().any(|member| member.name == node.name))
        {
            let output = self
                .dependencies
                .shell
                .run(&self.connection_as(user, node), &command, None)
                .await?;
            facts.insert(
                node.name.clone(),
                parse::<StoreFacts>(&output, &node.name, "store facts")?,
            );
        }
        Ok(facts)
    }

    /// The members that rejoin, by the Raft configuration the first member
    /// that holds its data and serves reports. None when no member serves.
    async fn stale_members(
        &self,
        scope: (crate::domain::workspace::ProjectId, &str),
        nodes: &[ClusterNodePlan],
        (members, facts): (&[StoreMember], &BTreeMap<String, StoreFacts>),
        user: &str,
    ) -> Vec<String> {
        let command = mmd_of(scope, &["openbao", "status"]);
        for node in nodes
            .iter()
            .filter(|node| facts.get(&node.name).is_some_and(StoreFacts::holds_data))
        {
            let status = match self
                .dependencies
                .shell
                .run(&self.connection_as(user, node), &command, None)
                .await
                .and_then(|output| parse::<StoreStatus>(&output, &node.name, "mmd openbao status"))
            {
                Ok(status) => status,
                Err(error) => {
                    tracing::debug!(%error, node = %node.name, "the store's member does not answer");
                    continue;
                }
            };
            if !status.sealed && !status.peers.is_empty() {
                return rejoining(members, facts, &status.peers);
            }
        }
        Vec::new()
    }

    /// The `secret-store` step.
    pub(super) async fn install_store(
        &self,
        plan: &ClusterSetupPlan,
        progress: &dyn ProgressReporter,
    ) -> Result<String, MmError> {
        let Some(store) = &plan.secret_store else {
            return Ok(String::new());
        };
        let keeper = self.store_keeper()?;
        let scope = StoreScope {
            project: plan.project_id,
            environment: &plan.environment,
            provider: &store.provider,
        };
        let ca_pem = keeper.authority(scope)?;
        let step = ClusterSetupStep::SecretStore;
        super::apply::nodes_wait(progress, step, plan);
        let upgrade = store.initialised
            && store
                .installed
                .values()
                .any(|version| version != &store.version);
        let mut answers = BTreeMap::new();
        for node in self.member_order(plan, store, upgrade).await {
            let Some(member) = store.members.iter().find(|member| member.name == node.name) else {
                continue;
            };
            let answer = progress_on_node(
                progress,
                (
                    step.label(),
                    &node.name,
                    &format!("OpenBao {} on {}", store.version, member.address),
                ),
                self.member(plan, node, (store, &ca_pem, scope)),
            )
            .await?;
            if upgrade {
                self.serving_again(
                    plan,
                    node,
                    "after its upgrade; the members after it were left on their release",
                )
                .await?;
            }
            answers.insert(node.name.clone(), answer);
        }
        let initialised = answers.values().any(|answer| answer.initialized);
        let summary = if initialised {
            self.give_keys(plan, (keeper, scope), store, &answers)
                .await?;
            format!(
                "OpenBao {} on {} members",
                store.version,
                store.members.len()
            )
        } else {
            self.initialise(plan, (keeper, scope), store).await?;
            format!(
                "OpenBao {} on {} members; initialised, recovery copy kept here",
                store.version,
                store.members.len()
            )
        };
        self.verify_store(plan, store).await?;
        Ok(summary)
    }

    /// The plan's nodes in the order their members change: the leader last on an upgrade.
    async fn member_order<'a>(
        &self,
        plan: &'a ClusterSetupPlan,
        store: &ClusterStorePlan,
        upgrade: bool,
    ) -> Vec<&'a ClusterNodePlan> {
        let mut nodes = plan.nodes.iter().collect::<Vec<_>>();
        if !upgrade {
            return nodes;
        }
        let mut leader = None;
        for node in nodes
            .iter()
            .filter(|node| store.members.iter().any(|member| member.name == node.name))
        {
            if let Ok(status) = self.status_of(plan, node).await
                && status.leader.is_some()
            {
                leader = status.leader;
                break;
            }
        }
        nodes.sort_by_key(|node| leader.as_deref() == Some(node.name.as_str()));
        nodes
    }

    /// After an upgrade restarted `node`'s member, or it rejoined: it serves
    /// unsealed, as a voter, with a leader, before the next member is touched.
    async fn serving_again(
        &self,
        plan: &ClusterSetupPlan,
        node: &ClusterNodePlan,
        after: &str,
    ) -> Result<(), MmError> {
        let deadline = tokio::time::Instant::now() + VOTERS_WITHIN;
        loop {
            let status = self.status_of(plan, node).await;
            let serving = status.as_ref().is_ok_and(|status| {
                !status.sealed
                    && status.leader.is_some()
                    && status
                        .peers
                        .iter()
                        .any(|peer| peer.name == node.name && peer.voter)
            });
            if serving {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(MmError::Target {
                    message: format!(
                        "the secret store's member on {} did not serve again within {} s {after}. `mm target nodes` shows each member",
                        node.name,
                        VOTERS_WITHIN.as_secs()
                    ),
                });
            }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    }

    /// One member: its release, its files and unit, and a certificate of
    /// the environment's authority for the key it made itself.
    async fn member(
        &self,
        plan: &ClusterSetupPlan,
        node: &ClusterNodePlan,
        (store, ca_pem, scope): (&ClusterStorePlan, &str, StoreScope<'_>),
    ) -> Result<StoreMemberAnswer, MmError> {
        let pinned = release_of(store)?;
        let connection = self.connection(plan, node);
        let shell = &self.dependencies.shell;
        let mut arguments = vec![
            store_id(plan.project_id, &plan.environment),
            pinned.version.to_owned(),
        ];
        for archive in &pinned.archives {
            arguments.extend([
                archive.machine.to_owned(),
                pinned.url(archive),
                archive.sha256.to_owned(),
            ]);
        }
        shell
            .run(
                &connection,
                &ClusterNodeCommand::Script {
                    script: ClusterScript::StoreInstall,
                    arguments,
                },
                None,
            )
            .await?;
        let mut spec = StoreMemberSpec {
            provider: store.provider.clone(),
            node: node.name.clone(),
            version: store.version.clone(),
            members: store.members.clone(),
            ca_pem: ca_pem.to_owned(),
            certificate_pem: None,
        };
        let command = mmd(plan, &["openbao", "member"]);
        for _ in 0..2 {
            let input = serde_json::to_string(&spec).map_err(|error| MmError::State {
                message: format!("serializing the member of node {}: {error}", node.name),
            })?;
            let output = shell
                .run(&connection, &command, Some(zeroize::Zeroizing::new(input)))
                .await?;
            let mut answer: StoreMemberAnswer = parse(&output, &node.name, "mmd openbao member")?;
            let Some(request) = answer.csr.take() else {
                return Ok(answer);
            };
            spec.certificate_pem = Some(keeper_sign(
                self.store_keeper()?,
                scope,
                &request,
                &node.name,
                store,
            )?);
        }
        Err(MmError::Target {
            message: format!(
                "node {} did not take the certificate the environment's authority signed for it",
                node.name
            ),
        })
    }

    async fn recipient(
        &self,
        plan: &ClusterSetupPlan,
        node: &ClusterNodePlan,
    ) -> Result<String, MmError> {
        let output = self
            .dependencies
            .shell
            .run(
                &self.connection(plan, node),
                &mmd(plan, &["secrets", "recipient"]),
                None,
            )
            .await?;
        let answer = parse::<serde_json::Value>(&output, &node.name, "mmd secrets recipient")?;
        answer["recipient"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| MmError::Target {
                message: format!("node {} reported no age recipient", node.name),
            })
    }

    async fn join(
        &self,
        plan: &ClusterSetupPlan,
        node: &ClusterNodePlan,
        sealed: zeroize::Zeroizing<String>,
    ) -> Result<(), MmError> {
        self.dependencies
            .shell
            .run(
                &self.connection(plan, node),
                &mmd(plan, &["openbao", "join"]),
                Some(sealed),
            )
            .await
            .map(drop)
    }

    /// The first initialisation, on the first member: every node gets the
    /// recovery document sealed to its own store, the operator a copy.
    async fn initialise(
        &self,
        plan: &ClusterSetupPlan,
        (keeper, scope): (&Arc<dyn ClusterStoreKeeper>, StoreScope<'_>),
        store: &ClusterStorePlan,
    ) -> Result<(), MmError> {
        let first = store
            .members
            .first()
            .and_then(|member| plan.nodes.iter().find(|node| node.name == member.name))
            .ok_or_else(|| MmError::State {
                message: "the secret store has no member in this setup".to_owned(),
            })?;
        let mut recipients = BTreeMap::new();
        for node in &plan.nodes {
            recipients.insert(node.name.clone(), self.recipient(plan, node).await?);
        }
        let request = StoreInitRequest {
            recipients,
            operator: keeper.operator_recipient(plan.project_id)?,
        };
        let input = serde_json::to_string(&request).map_err(|error| MmError::State {
            message: format!("serializing the store's initialisation: {error}"),
        })?;
        let output = self
            .dependencies
            .shell
            .run(
                &self.connection(plan, first),
                &mmd(plan, &["openbao", "init"]),
                Some(zeroize::Zeroizing::new(input)),
            )
            .await?;
        let answer = parse::<StoreInitAnswer>(&output, &first.name, "mmd openbao init")?;
        keeper.keep_recovery(scope, &answer.recovery)?;
        for node in plan.nodes.iter().filter(|node| node.name != first.name) {
            let sealed = answer
                .sealed
                .get(&node.name)
                .ok_or_else(|| MmError::Target {
                    message: format!(
                        "node {} sealed no recovery document for node {}",
                        first.name, node.name
                    ),
                })?;
            self.join(plan, node, zeroize::Zeroizing::new(sealed.clone()))
                .await?;
        }
        Ok(())
    }

    /// An initialised store: the nodes that hold no keys get them from the
    /// recovery copy on this machine, sealed to their own store. A member
    /// that rejoins and holds no data has its old Raft entry removed first,
    /// and serves again before the next node.
    async fn give_keys(
        &self,
        plan: &ClusterSetupPlan,
        (keeper, scope): (&Arc<dyn ClusterStoreKeeper>, StoreScope<'_>),
        store: &ClusterStorePlan,
        answers: &BTreeMap<String, StoreMemberAnswer>,
    ) -> Result<(), MmError> {
        for node in &plan.nodes {
            let answer = answers.get(&node.name);
            let serving = match answer {
                Some(answer) => answer.has_keys && !answer.sealed,
                None => self.status_of(plan, node).await?.has_keys,
            };
            if serving {
                continue;
            }
            if keeper.recovery(scope)?.is_none() {
                let environment = &plan.environment;
                return Err(MmError::Target {
                    message: format!(
                        "node {} holds no keys of the secret store of '{environment}', and this machine holds no recovery copy to give it. On the machine that initialised the store run `mm -e {environment} secret provider recovery export {provider} --recipient AGE_RECIPIENT --to FILE`, then here `mm -e {environment} secret provider recovery import {provider} FILE`, and run setup again",
                        node.name,
                        provider = scope.provider
                    ),
                });
            }
            let rejoin = store.rejoins.contains(&node.name)
                && answer.is_some_and(|answer| !answer.initialized);
            if rejoin {
                self.remove_stale_peer(plan, store, node).await?;
            }
            let recipient = self.recipient(plan, node).await?;
            let sealed = keeper.reseal_recovery(scope, &recipient)?;
            self.join(plan, node, sealed).await?;
            if rejoin {
                self.serving_again(plan, node, "after it rejoined").await?;
            }
        }
        Ok(())
    }

    /// Removes `stale`'s old Raft entry through another member that serves
    /// with a leader.
    async fn remove_stale_peer(
        &self,
        plan: &ClusterSetupPlan,
        store: &ClusterStorePlan,
        stale: &ClusterNodePlan,
    ) -> Result<(), MmError> {
        let others = plan.nodes.iter().filter(|node| {
            node.name != stale.name && store.members.iter().any(|member| member.name == node.name)
        });
        for node in others {
            let status = match self.status_of(plan, node).await {
                Ok(status) => status,
                Err(error) => {
                    tracing::debug!(%error, node = %node.name, "the store's member does not answer");
                    continue;
                }
            };
            if !removal_through(&status, &stale.name) {
                continue;
            }
            if !status.peers.iter().any(|peer| peer.name == stale.name) {
                return Ok(());
            }
            let command = mmd(plan, &["openbao", "remove-peer", "--peer", &stale.name]);
            let output = self
                .dependencies
                .shell
                .run(&self.connection(plan, node), &command, None)
                .await?;
            let removal =
                parse::<StorePeerRemoval>(&output, &node.name, "mmd openbao remove-peer")?;
            tracing::info!(peer = %removal.peer, through = %removal.through, removed = removal.removed, "a stale Raft entry of the secret store");
            return Ok(());
        }
        Err(MmError::Target {
            message: format!(
                "no member of the secret store of '{}' serves with a leader, so the old Raft entry of {} stays and it cannot join again. `mm target nodes` shows each member",
                plan.environment, stale.name
            ),
        })
    }

    async fn status_of(
        &self,
        plan: &ClusterSetupPlan,
        node: &ClusterNodePlan,
    ) -> Result<StoreStatus, MmError> {
        let output = self
            .dependencies
            .shell
            .run(
                &self.connection(plan, node),
                &mmd(plan, &["openbao", "status"]),
                None,
            )
            .await?;
        parse(&output, &node.name, "mmd openbao status")
    }

    /// Every member serves, unsealed, as a voter of one Raft cluster.
    async fn verify_store(
        &self,
        plan: &ClusterSetupPlan,
        store: &ClusterStorePlan,
    ) -> Result<(), MmError> {
        let deadline = tokio::time::Instant::now() + VOTERS_WITHIN;
        loop {
            match self.whole(plan, store).await {
                Err(error) if tokio::time::Instant::now() < deadline => {
                    tracing::debug!(%error, "the secret store is not whole yet");
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
                done => return done,
            }
        }
    }

    async fn whole(
        &self,
        plan: &ClusterSetupPlan,
        store: &ClusterStorePlan,
    ) -> Result<(), MmError> {
        let Some(first) = store
            .members
            .iter()
            .find_map(|member| plan.nodes.iter().find(|node| node.name == member.name))
        else {
            return Ok(());
        };
        let status = self.status_of(plan, first).await?;
        let missing = store
            .members
            .iter()
            .filter(|member| plan.nodes.iter().any(|node| node.name == member.name))
            .filter(|member| {
                !status
                    .peers
                    .iter()
                    .any(|peer| peer.name == member.name && peer.voter)
            })
            .map(|member| member.name.as_str())
            .collect::<Vec<_>>();
        let mut problems = Vec::new();
        if status.sealed {
            problems.push(format!("the member on {} is sealed", first.name));
        }
        if status.leader.is_none() {
            problems.push("no member leads".to_owned());
        }
        if !missing.is_empty() {
            problems.push(format!("not voters: {}", missing.join(", ")));
        }
        if !problems.is_empty() {
            return Err(MmError::Target {
                message: format!(
                    "the secret store of '{}' is not whole after setup: {}. On the node, `systemctl status {}` and `journalctl -u {}` say why",
                    plan.environment,
                    problems.join("; "),
                    status.unit,
                    status.unit
                ),
            });
        }
        Ok(())
    }
}

fn keeper_sign(
    keeper: &Arc<dyn ClusterStoreKeeper>,
    scope: StoreScope<'_>,
    request: &str,
    node: &str,
    store: &ClusterStorePlan,
) -> Result<String, MmError> {
    let address = store
        .members
        .iter()
        .find(|member| member.name == node)
        .map(|member| member.address)
        .ok_or_else(|| MmError::State {
            message: format!("node {node} is not a member of the secret store"),
        })?;
    keeper.sign(scope, request, (node, address))
}
