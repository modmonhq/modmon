//! The firewall chains that keep the mesh between the hosts, loaded with
//! the mesh itself.
//!
//! [`GuardedMesh`] is the host's mesh with those chains: whenever the
//! interface is applied or brought up the chains `MM-MESH` and `MM-MESH-IN`
//! are brought to what the node's mesh state and stored cluster networks
//! ask for, before the interface is touched; a status says what differs;
//! leaving the mesh removes them. `mm-mesh.service` runs `mmd mesh up`
//! before the container engine starts, and `mmd network up` ends with the
//! same call, so the chains follow every change to the node's networks.
//!
//! Everything mm adds to the packet filter here is in those two chains and
//! the two rules that enter them. `MM-MESH-IN` is entered from the filter
//! table's `INPUT`; `MM-MESH` from where the host's engine lets mm's forward
//! rules in ([`forward_hook`]): Docker Engine's `DOCKER-USER`, or the mangle
//! table's `FORWARD` on Podman.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use zeroize::Zeroizing;

use crate::adapters::cluster_network::NetworkStore;
use crate::app::ports::{CommandOutput, CommandSpec, MeshHost, ProcessRunner};
use crate::domain::cluster_network::{ForwardHook, HostRange, forward_hook};
use crate::domain::error::MmError;
use crate::domain::mesh::{
    MESH_FORWARD_CHAIN, MESH_INPUT_CHAIN, MeshApplied, MeshFacts, MeshGuard, MeshNodeStatus,
    MeshState,
};

const TIMEOUT: Duration = Duration::from_secs(30);
/// `iptables` lives in an sbin directory, which the default search path of a
/// process started without an environment does not include.
const SEARCH_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// The host's mesh together with the firewall chains that guard it.
pub struct GuardedMesh {
    inner: Arc<dyn MeshHost>,
    /// The cluster networks the last deployment stored.
    networks: NetworkStore,
    process: Arc<dyn ProcessRunner>,
    /// Where `MM-MESH` is entered from on this host's engine.
    hook: ForwardHook,
}

impl std::fmt::Debug for GuardedMesh {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GuardedMesh")
            .finish_non_exhaustive()
    }
}

fn failed(program: &str, arguments: &[&str], output: &CommandOutput) -> MmError {
    MmError::ProcessFailed {
        program: format!("{program} {}", arguments.join(" ")),
        code: output.code,
        stderr: output.stderr.trim().to_owned(),
    }
}

impl GuardedMesh {
    /// `state_root` is `mmd`'s own state directory, where `inner` keeps the
    /// mesh and the last deployment stored the cluster networks.
    #[must_use]
    pub fn new(
        state_root: &Path,
        inner: Arc<dyn MeshHost>,
        process: Arc<dyn ProcessRunner>,
    ) -> Self {
        Self {
            inner,
            networks: NetworkStore::new(state_root.join("networks")),
            process,
            hook: forward_hook(crate::domain::runtime::RuntimeKind::DockerCompose),
        }
    }

    /// The host runs `engine`, whose packet filter `MM-MESH` enters.
    #[must_use]
    pub fn for_engine(mut self, engine: crate::domain::runtime::RuntimeKind) -> Self {
        self.hook = forward_hook(engine);
        self
    }

    /// Each chain with the table it lives in and the chain that enters it.
    fn hooks(&self) -> [(&'static str, &'static str, &'static str); 2] {
        [
            (self.hook.table, self.hook.chain, MESH_FORWARD_CHAIN),
            ("filter", "INPUT", MESH_INPUT_CHAIN),
        ]
    }

    async fn run(
        &self,
        program: &str,
        arguments: &[&str],
        stdin: Option<String>,
    ) -> Result<CommandOutput, MmError> {
        let mut specification = CommandSpec::new(program)
            .args(arguments.iter().copied())
            .environment("LC_ALL", "C")
            .environment("PATH", SEARCH_PATH)
            .timeout(TIMEOUT);
        if let Some(stdin) = stdin {
            specification = specification.sensitive_stdin(Zeroizing::new(stdin));
        }
        self.process
            .run(specification)
            .await
            .map_err(|error| match error {
                MmError::ProcessSpawn { program, .. } => MmError::Target {
                    message: format!(
                        "`{program}` is not installed on this host; keeping the mesh between the hosts needs the iptables package (`apt-get install iptables`), which also comes with Docker Engine"
                    ),
                },
                other => other,
            })
    }

    async fn iptables(&self, arguments: &[&str]) -> Result<(), MmError> {
        let output = self.run("iptables", arguments, None).await?;
        if output.success() {
            Ok(())
        } else {
            Err(failed("iptables", arguments, &output))
        }
    }

    /// The table of `MM-MESH` as `iptables -S` prints it, followed by the
    /// filter table's when that is another: one process for both chains
    /// and the rules that enter them on Docker Engine, two on Podman.
    async fn listing(&self) -> Result<String, MmError> {
        let mut listing = String::new();
        let mut tables = vec![self.hook.table];
        if self.hook.table != "filter" {
            tables.push("filter");
        }
        for table in tables {
            let arguments = ["-w", "-t", table, "-S"];
            let output = self.run("iptables", &arguments, None).await?;
            if !output.success() {
                return Err(failed("iptables", &arguments, &output));
            }
            listing.push_str(&output.stdout);
            if !listing.ends_with('\n') {
                listing.push('\n');
            }
        }
        Ok(listing)
    }

    /// The chains the node wants; `None` on a host that holds no mesh.
    fn wanted(&self, state: Option<&MeshState>) -> Result<Option<MeshGuard>, MmError> {
        let Some(state) = state else {
            return Ok(None);
        };
        Ok(Some(MeshGuard::of(state, self.networks.state()?.as_ref())))
    }

    /// Brings both chains to `wanted` and makes sure they are entered
    /// first. Returns what it did; nothing when they were in place.
    async fn load(&self, wanted: &MeshGuard) -> Result<Vec<String>, MmError> {
        let listing = self.listing().await?;
        let drift = wanted.drift(&listing, self.hook.chain);
        if drift.is_empty() {
            return Ok(Vec::new());
        }
        let arguments = ["--noflush", "--wait", "5"];
        let output = self
            .run(
                "iptables-restore",
                &arguments,
                Some(wanted.restore_input(self.hook)),
            )
            .await?;
        if !output.success() {
            return Err(failed("iptables-restore", &arguments, &output));
        }
        let has = |line: String| listing.lines().any(|held| held.trim() == line);
        // Before Docker Engine has started for the first time its chain
        // does not exist; the engine keeps a chain it finds.
        if self.hook.create && !has(format!("-N {}", self.hook.chain)) {
            self.iptables(&["-w", "-t", self.hook.table, "-N", self.hook.chain])
                .await?;
        }
        for (table, from, chain) in self.hooks() {
            if !has(format!("-A {from} -j {chain}")) {
                self.iptables(&["-w", "-t", table, "-I", from, "1", "-j", chain])
                    .await?;
            }
        }
        Ok(vec![format!(
            "loaded the firewall chains {MESH_FORWARD_CHAIN} and {MESH_INPUT_CHAIN} ({})",
            drift.join("; ")
        )])
    }

    /// Takes both chains and the rules that enter them away.
    async fn remove(&self) -> Result<Vec<String>, MmError> {
        let mut actions = Vec::new();
        let mut listing = self.listing().await?;
        for (table, from, chain) in self.hooks() {
            if !listing
                .lines()
                .any(|line| line.trim() == format!("-N {chain}"))
            {
                continue;
            }
            let entered = format!("-A {from} -j {chain}");
            while listing.lines().any(|line| line.trim() == entered) {
                self.iptables(&["-w", "-t", table, "-D", from, "-j", chain])
                    .await?;
                listing = self.listing().await?;
            }
            self.iptables(&["-w", "-t", table, "-F", chain]).await?;
            self.iptables(&["-w", "-t", table, "-X", chain]).await?;
            actions.push(format!("removed the firewall chain {chain}"));
        }
        Ok(actions)
    }

    /// `applied` with the guard's own actions in front.
    fn with(mut guard: Vec<String>, applied: MeshApplied) -> MeshApplied {
        guard.extend(applied.actions);
        MeshApplied {
            changed: !guard.is_empty(),
            actions: guard,
        }
    }
}

#[async_trait]
impl MeshHost for GuardedMesh {
    async fn facts(&self) -> Result<MeshFacts, MmError> {
        self.inner.facts().await
    }

    fn stored(&self) -> Result<Option<MeshState>, MmError> {
        self.inner.stored()
    }

    async fn host_ranges(&self) -> Result<Vec<HostRange>, MmError> {
        self.inner.host_ranges().await
    }

    async fn ensure_key(&self) -> Result<String, MmError> {
        self.inner.ensure_key().await
    }

    async fn apply(&self, desired: &MeshState) -> Result<MeshApplied, MmError> {
        desired
            .validate()
            .map_err(|message| MmError::Security { message })?;
        // The chains first: the interface never carries anything without
        // them.
        let guard = match self.wanted(Some(desired))? {
            Some(wanted) => self.load(&wanted).await?,
            None => Vec::new(),
        };
        Ok(Self::with(guard, self.inner.apply(desired).await?))
    }

    async fn up(&self) -> Result<MeshApplied, MmError> {
        let guard = match self.wanted(self.inner.stored()?.as_ref())? {
            Some(wanted) => self.load(&wanted).await?,
            None => Vec::new(),
        };
        Ok(Self::with(guard, self.inner.up().await?))
    }

    async fn status(&self) -> Result<MeshNodeStatus, MmError> {
        let mut status = self.inner.status().await?;
        if let Some(wanted) = self.wanted(self.inner.stored()?.as_ref())? {
            status.guard_drift = match self.listing().await {
                Ok(listing) => wanted.drift(&listing, self.hook.chain),
                Err(error) => vec![format!("the rules could not be read: {error}")],
            };
        }
        Ok(status)
    }

    async fn stage_key(&self) -> Result<String, MmError> {
        self.inner.stage_key().await
    }

    async fn activate_key(&self) -> Result<String, MmError> {
        self.inner.activate_key().await
    }

    async fn leave(&self) -> Result<MeshApplied, MmError> {
        let applied = self.inner.leave().await?;
        // After the interface: nothing arrives on it any more.
        let mut actions = applied.actions;
        actions.extend(self.remove().await?);
        Ok(MeshApplied {
            changed: !actions.is_empty(),
            actions,
        })
    }
}
