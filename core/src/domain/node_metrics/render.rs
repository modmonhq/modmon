//! The text one scrape gets from one collection.

use super::procfs::HostSample;
use super::{
    EnvironmentSample, Exposition, Kind, MetricsScope, NodeSnapshot, Part, RolloutOutcome,
};
use crate::domain::discovery::Health;

const HEALTH: [Health; 4] = [
    Health::Healthy,
    Health::Starting,
    Health::Unhealthy,
    Health::Stopped,
];

/// The metrics of `snapshot` that `scope` sees, at most `limit` bytes.
#[must_use]
pub fn render(snapshot: &NodeSnapshot, scope: &MetricsScope, limit: usize) -> String {
    let mut out = Exposition::new(limit);
    node(&mut out, snapshot);
    if let Some(host) = &snapshot.host {
        let node = snapshot.node.as_str();
        machine(&mut out, node, host);
        filesystems(&mut out, node, host);
        disks(&mut out, node, host);
        interfaces(&mut out, node, host);
    }
    if let Some(environments) = &snapshot.environments {
        let seen = environments
            .iter()
            .filter(|environment| scope.sees(environment.project_id, &environment.environment))
            .collect::<Vec<_>>();
        services(&mut out, &snapshot.node, &seen);
    }
    out.finish()
}

fn node(out: &mut Exposition, snapshot: &NodeSnapshot) {
    let node = snapshot.node.as_str();
    out.family(
        "mm_node_info",
        Kind::Gauge,
        "The node, its mmd version, and its container engine.",
    );
    out.sample(
        "mm_node_info",
        &[
            ("node", node),
            ("version", snapshot.version.as_str()),
            ("engine", snapshot.engine.as_str()),
        ],
        1.0,
    );
    out.family(
        "mm_metrics_collected",
        Kind::Gauge,
        "Whether the last collection read this part: host, mesh, or services.",
    );
    for part in Part::ALL {
        let collected = f64::from(u8::from(snapshot.collected(part)));
        out.sample(
            "mm_metrics_collected",
            &[("node", node), ("part", part.as_str())],
            collected,
        );
    }
    out.family(
        "mm_metrics_collect_duration_seconds",
        Kind::Gauge,
        "How long the last collection took.",
    );
    out.sample(
        "mm_metrics_collect_duration_seconds",
        &[("node", node)],
        snapshot.collect_seconds,
    );
    let Some(peers) = &snapshot.peers else {
        return;
    };
    out.family(
        "mm_mesh_peer_up",
        Kind::Gauge,
        "Whether the mesh peer is on the interface and shook hands in the last three minutes.",
    );
    for peer in peers {
        out.sample(
            "mm_mesh_peer_up",
            &[("node", node), ("peer", peer.peer.as_str())],
            f64::from(u8::from(peer.up())),
        );
    }
    out.family(
        "mm_mesh_peer_handshake_age_seconds",
        Kind::Gauge,
        "Seconds since the last handshake with the mesh peer.",
    );
    for peer in peers {
        if let Some(age) = peer.handshake_age_seconds {
            out.sample(
                "mm_mesh_peer_handshake_age_seconds",
                &[("node", node), ("peer", peer.peer.as_str())],
                age as f64,
            );
        }
    }
    out.family(
        "mm_mesh_peer_receive_bytes_total",
        Kind::Counter,
        "Bytes received from the mesh peer.",
    );
    for peer in peers {
        out.sample(
            "mm_mesh_peer_receive_bytes_total",
            &[("node", node), ("peer", peer.peer.as_str())],
            peer.received_bytes as f64,
        );
    }
    out.family(
        "mm_mesh_peer_transmit_bytes_total",
        Kind::Counter,
        "Bytes sent to the mesh peer.",
    );
    for peer in peers {
        out.sample(
            "mm_mesh_peer_transmit_bytes_total",
            &[("node", node), ("peer", peer.peer.as_str())],
            peer.sent_bytes as f64,
        );
    }
}

fn machine(out: &mut Exposition, node: &str, host: &HostSample) {
    let labels = [("node", node)];
    if let Some(cpu) = &host.cpu {
        out.family(
            "mm_host_cpu_seconds_total",
            Kind::Counter,
            "Seconds the host's CPUs spent in each mode, over every CPU.",
        );
        for (mode, seconds) in &cpu.modes {
            out.sample(
                "mm_host_cpu_seconds_total",
                &[("node", node), ("mode", *mode)],
                *seconds,
            );
        }
        out.family("mm_host_cpus", Kind::Gauge, "The host's CPUs.");
        out.sample("mm_host_cpus", &labels, f64::from(cpu.cpus));
        if let Some(boot) = cpu.boot_time {
            out.family(
                "mm_host_boot_time_seconds",
                Kind::Gauge,
                "When the host booted, in seconds since the Unix epoch.",
            );
            out.sample("mm_host_boot_time_seconds", &labels, boot as f64);
        }
    }
    if let Some(load) = host.load {
        for (name, value) in [
            ("mm_host_load1", load[0]),
            ("mm_host_load5", load[1]),
            ("mm_host_load15", load[2]),
        ] {
            out.family(name, Kind::Gauge, "The host's load average.");
            out.sample(name, &labels, value);
        }
    }
    if let Some(memory) = &host.memory {
        for (name, help, value) in [
            (
                "mm_host_memory_total_bytes",
                "The host's memory.",
                memory.total,
            ),
            (
                "mm_host_memory_available_bytes",
                "Memory the host can give out without swapping.",
                memory.available,
            ),
            (
                "mm_host_memory_free_bytes",
                "Memory nothing uses.",
                memory.free,
            ),
            (
                "mm_host_memory_buffers_bytes",
                "Memory in block device buffers.",
                memory.buffers,
            ),
            (
                "mm_host_memory_cached_bytes",
                "Memory in the page cache.",
                memory.cached,
            ),
            (
                "mm_host_swap_total_bytes",
                "The host's swap space.",
                memory.swap_total,
            ),
            (
                "mm_host_swap_free_bytes",
                "Swap space nothing uses.",
                memory.swap_free,
            ),
        ] {
            out.family(name, Kind::Gauge, help);
            out.sample(name, &labels, value as f64);
        }
    }
}

fn filesystems(out: &mut Exposition, node: &str, host: &HostSample) {
    type Field = fn(&super::procfs::FilesystemSample) -> f64;
    let fields: [(&str, &str, Field); 6] = [
        (
            "mm_host_filesystem_size_bytes",
            "The filesystem's size.",
            |sample| sample.usage.size as f64,
        ),
        (
            "mm_host_filesystem_free_bytes",
            "Bytes free on the filesystem, root's reserve included.",
            |sample| sample.usage.free as f64,
        ),
        (
            "mm_host_filesystem_avail_bytes",
            "Bytes free on the filesystem for anyone but root.",
            |sample| sample.usage.available as f64,
        ),
        (
            "mm_host_filesystem_files",
            "The filesystem's inodes.",
            |sample| sample.usage.files as f64,
        ),
        (
            "mm_host_filesystem_files_free",
            "The filesystem's free inodes.",
            |sample| sample.usage.files_free as f64,
        ),
        (
            "mm_host_filesystem_readonly",
            "Whether the host mounted the filesystem read-only.",
            |sample| f64::from(u8::from(sample.mount.readonly)),
        ),
    ];
    for (name, help, value) in fields {
        out.family(name, Kind::Gauge, help);
        for sample in &host.filesystems {
            let mount = &sample.mount;
            out.sample(
                name,
                &[
                    ("node", node),
                    ("device", mount.device.as_str()),
                    ("mountpoint", mount.mountpoint.as_str()),
                    ("fstype", mount.fstype.as_str()),
                ],
                value(sample),
            );
        }
    }
}

fn disks(out: &mut Exposition, node: &str, host: &HostSample) {
    type Field = fn(&super::procfs::DiskSample) -> f64;
    let fields: [(&str, &str, Field); 5] = [
        (
            "mm_host_disk_reads_completed_total",
            "Reads the disk completed.",
            |disk| disk.reads as f64,
        ),
        (
            "mm_host_disk_read_bytes_total",
            "Bytes read from the disk.",
            |disk| disk.read_bytes as f64,
        ),
        (
            "mm_host_disk_writes_completed_total",
            "Writes the disk completed.",
            |disk| disk.writes as f64,
        ),
        (
            "mm_host_disk_written_bytes_total",
            "Bytes written to the disk.",
            |disk| disk.written_bytes as f64,
        ),
        (
            "mm_host_disk_io_time_seconds_total",
            "Seconds the disk was busy.",
            |disk| disk.io_milliseconds as f64 / 1000.0,
        ),
    ];
    for (name, help, value) in fields {
        out.family(name, Kind::Counter, help);
        for disk in &host.disks {
            out.sample(
                name,
                &[("node", node), ("device", disk.device.as_str())],
                value(disk),
            );
        }
    }
}

fn interfaces(out: &mut Exposition, node: &str, host: &HostSample) {
    type Field = fn(&super::procfs::InterfaceSample) -> u64;
    let fields: [(&str, &str, Field); 8] = [
        (
            "mm_host_network_receive_bytes_total",
            "Bytes the interface received.",
            |sample| sample.receive_bytes,
        ),
        (
            "mm_host_network_receive_packets_total",
            "Packets the interface received.",
            |sample| sample.receive_packets,
        ),
        (
            "mm_host_network_receive_errors_total",
            "Receive errors on the interface.",
            |sample| sample.receive_errors,
        ),
        (
            "mm_host_network_receive_drop_total",
            "Received packets the interface dropped.",
            |sample| sample.receive_drops,
        ),
        (
            "mm_host_network_transmit_bytes_total",
            "Bytes the interface sent.",
            |sample| sample.transmit_bytes,
        ),
        (
            "mm_host_network_transmit_packets_total",
            "Packets the interface sent.",
            |sample| sample.transmit_packets,
        ),
        (
            "mm_host_network_transmit_errors_total",
            "Transmit errors on the interface.",
            |sample| sample.transmit_errors,
        ),
        (
            "mm_host_network_transmit_drop_total",
            "Packets the interface dropped instead of sending.",
            |sample| sample.transmit_drops,
        ),
    ];
    for (name, help, value) in fields {
        out.family(name, Kind::Counter, help);
        for sample in &host.interfaces {
            out.sample(
                name,
                &[("node", node), ("interface", sample.interface.as_str())],
                value(sample) as f64,
            );
        }
    }
}

fn services(out: &mut Exposition, node: &str, environments: &[&EnvironmentSample]) {
    out.family(
        "mm_environment_last_deployment_timestamp_seconds",
        Kind::Gauge,
        "When the environment's last deployment finished on this node.",
    );
    for environment in environments {
        if let Some(at) = environment.deployed_at_unix_ms {
            let labels = [
                ("node", node),
                ("project", environment.project.as_str()),
                ("environment", environment.environment.as_str()),
            ];
            out.sample(
                "mm_environment_last_deployment_timestamp_seconds",
                &labels,
                at as f64 / 1000.0,
            );
        }
    }
    out.family(
        "mm_environment_rollout",
        Kind::Gauge,
        "The state of the environment's last rollout on this node: 1 for the state it is in.",
    );
    for environment in environments {
        let Some(outcome) = environment.rollout else {
            continue;
        };
        for state in RolloutOutcome::ALL {
            let labels = [
                ("node", node),
                ("project", environment.project.as_str()),
                ("environment", environment.environment.as_str()),
                ("state", state.as_str()),
            ];
            out.sample(
                "mm_environment_rollout",
                &labels,
                f64::from(u8::from(state == outcome)),
            );
        }
    }
    service_counts(out, node, environments);
}

fn service_counts(out: &mut Exposition, node: &str, environments: &[&EnvironmentSample]) {
    let each =
        |out: &mut Exposition, name: &str, value: &dyn Fn(&super::ServiceSample) -> Option<f64>| {
            for environment in environments {
                for service in &environment.services {
                    if let Some(value) = value(service) {
                        let labels = [
                            ("node", node),
                            ("project", environment.project.as_str()),
                            ("environment", environment.environment.as_str()),
                            ("stack", service.stack.as_str()),
                            ("service", service.service.as_str()),
                        ];
                        out.sample(name, &labels, value);
                    }
                }
            }
        };
    out.family(
        "mm_service_desired_containers",
        Kind::Gauge,
        "The containers the environment's last deployment runs of the service on this node.",
    );
    each(out, "mm_service_desired_containers", &|service| {
        service.desired.map(f64::from)
    });
    out.family(
        "mm_service_running_containers",
        Kind::Gauge,
        "The service's containers that run on this node, healthy or not.",
    );
    each(out, "mm_service_running_containers", &|service| {
        Some(f64::from(service.running()))
    });
    out.family(
        "mm_service_restarts",
        Kind::Gauge,
        "How often the engine restarted the service's current containers on this node.",
    );
    each(out, "mm_service_restarts", &|service| {
        Some(service.restarts as f64)
    });
    out.family("mm_service_containers", Kind::Gauge, "The service's containers on this node by health: healthy, starting, unhealthy, or stopped.");
    for environment in environments {
        for service in &environment.services {
            for (health, count) in HEALTH.iter().zip(service.containers) {
                let labels = [
                    ("node", node),
                    ("project", environment.project.as_str()),
                    ("environment", environment.environment.as_str()),
                    ("stack", service.stack.as_str()),
                    ("service", service.service.as_str()),
                    ("health", health.as_str()),
                ];
                out.sample("mm_service_containers", &labels, f64::from(count));
            }
        }
    }
}
