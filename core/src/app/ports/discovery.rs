use std::net::Ipv4Addr;
use std::time::Duration;

use crate::domain::cluster_ingress::IngressTable;
use crate::domain::discovery::{NameReport, Reach};
use crate::domain::error::MmError;

/// What the name resolver of `mmd`'s own node knows.
pub trait ServiceNames: Send + Sync {
    /// The service names on the cluster networks whose name across the
    /// cluster starts with `prefix`, with the containers behind each, and
    /// whatever keeps this node from answering them. Reads memory only.
    fn report(&self, prefix: &str) -> NameReport;

    /// The addresses of `service`'s healthy containers on the cluster
    /// network `network` (its name across the cluster), as this node
    /// answers with them: its own first ([`Reach::LocalFirst`], with
    /// whether they are its own) or every node's ([`Reach::EveryNode`]).
    /// For what runs next to the resolver and steers the same way, such as
    /// an ingress proxy's list of backends. Reads memory only.
    fn addresses(&self, _network: &str, _service: &str, _reach: Reach) -> (Vec<Ipv4Addr>, bool) {
        (Vec::new(), false)
    }
}

/// Containers that leave this node without dropping a request
/// (`domain::leaving`): out of every node's name answers and ingress
/// proxies first, then a drain, then the stop signal.
#[async_trait::async_trait]
pub trait ContainerLeaving: Send + Sync {
    /// Has each of `containers` (engine ids) leave with `drain`, and
    /// returns once every one of them is stopped and removed. What it began
    /// goes on when the caller stops waiting.
    async fn leave(&self, containers: &[String], drain: Duration) -> Result<(), MmError>;

    /// As `leave`, but returns once every node confirmed that it sends
    /// none of `containers` anything new; their drain and stop go on after.
    async fn leave_confirmed(&self, containers: &[String], drain: Duration) -> Result<(), MmError> {
        self.leave(containers, drain).await
    }

    /// Takes back the leaves of the containers of Compose project `project`
    /// that have not been stopped: those containers are given out for
    /// their names and routed to again. A leave whose container is being
    /// stopped right now ends first. Returns the names of the containers
    /// taken back. A rollback calls it before it puts the project back, so
    /// that no container is left out of routing or half removed.
    async fn recall(&self, project: &str) -> Result<Vec<String>, MmError>;
    /// As `leave`, but each container restarts after its drain instead of
    /// being removed, and is answered again once it is healthy: a restart
    /// that drops no request. Without `drain`, the drain its service's
    /// `drain` put on the container. Returns once each one has restarted.
    async fn restart(
        &self,
        _containers: &[String],
        _drain: Option<Duration>,
    ) -> Result<(), MmError> {
        Err(MmError::Environment {
            message: "this node cannot restart a container without dropping requests".to_owned(),
        })
    }
}

/// What a cluster node's `mmd` knows about the number of containers of
/// its scaled services (`domain::autoscale`). Reads memory or one small
/// file, never the engine.
pub trait ScaleState: Send + Sync {
    /// The count over every node that the nodes last agreed on for the
    /// service `key` (`domain::autoscale::service_key`); `None` before
    /// anything scaled it.
    fn wanted(&self, key: &str) -> Option<u32>;

    /// How many of the service's containers on this node are leaving now.
    fn leaving(&self, key: &str) -> u32;
}

/// The ingress proxy of a cluster node (`domain::cluster_ingress`).
#[async_trait::async_trait]
pub trait IngressHost: Send + Sync {
    /// Starts the proxy unless it runs with what `routes` need besides the
    /// tables it serves, and attaches it to every environment's network.
    async fn ensure_proxy(&self, table: &str, routes: &IngressTable) -> Result<(), MmError>;

    /// Keeps an environment's certificate and key where the proxy reads
    /// them, and returns the name its table gives them.
    fn keep_certificate(
        &self,
        table: &str,
        certificate: &[u8],
        key: &[u8],
    ) -> Result<String, MmError>;

    /// Serves an environment's routes, and returns once the proxy runs
    /// them and, with a tunnel, once its connector here holds a connection
    /// to Cloudflare with `token`. A proxy or connector that does not take
    /// them keeps what it served.
    async fn publish(
        &self,
        table: &str,
        routes: &IngressTable,
        token: Option<&[u8]>,
    ) -> Result<(), MmError>;

    /// Takes an environment's routes away, detaches the proxy from the
    /// engine's network `network` when given, and stops the proxy once it
    /// serves no environment at all.
    async fn withdraw(&self, table: &str, network: Option<&str>) -> Result<(), MmError>;

    /// What the tunnel connector of an environment says, when the node
    /// serves the environment through a tunnel.
    async fn tunnel_status(
        &self,
        table: &str,
    ) -> Option<crate::domain::cluster_ingress::TunnelStatus>;

    /// The other environments the node serves on ports 80 and 443.
    fn public_tables(&self, except: &str) -> Result<Vec<String>, MmError>;
}
