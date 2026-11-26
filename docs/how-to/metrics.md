---
title: Collect node and service metrics
description: Scrape each cluster node's CPU, memory, disks, network, mesh and services from a collector that needs no host access.
---

`mmd` runs on every node of a cluster and can serve that node's metrics in the
Prometheus text format: the host's CPU, memory, load, filesystems, disks and
network interfaces, the node's mesh peers, and how many containers of each
service run against how many the last deployment wants. A collector such as
Grafana Alloy or Prometheus reads them like any other target. It runs as an
ordinary service of your project, with no host mounts, no host network and no
[host exceptions](../remote-access.md#collect-host-metrics).

## Requirements

- A [cluster target](./cluster.md): a target with `nodes:`. A one-node cluster
  works too. A target that names a single host has no cluster networks to serve
  metrics on.
- `mmd` from mm 0.1.3 or newer on every node; see [upgrade](./upgrade.md).

## Turn it on

Mark one network of `mm.yaml` with `metrics: true`, and put the collector on it
with `placement: all`, so one collector runs on every node:

```yaml
# .config/mm.yaml
networks:
  monitoring: { metrics: true }

stacks:
  monitoring:
    files: [monitoring/compose.yaml]

modules:
  monitoring:
    path: monitoring
    services:
      alloy: { stack: monitoring, placement: all, networks: [monitoring] }
      prometheus: { stack: monitoring, placement: db-1, networks: [monitoring] }
```

Deploy as usual. Every node that runs a service on `monitoring` then serves
its metrics on its own address on that network, port 9753. Inside a container
on the network, the name `mmd.internal` resolves to that address on the
container's own node, so the metrics are at `http://mmd.internal:9753/metrics`.

A scrape gets the host and the node, and the services of this environment
only. Another project on the same hosts, or another environment of this one,
never shows up in it, even when it also turns metrics on.

## Collect with Grafana Alloy

Each Alloy scrapes the node it runs on and sends the samples to one
Prometheus. The `node` label on every series tells the nodes apart.

```yaml
# monitoring/compose.yaml
services:
  alloy:
    image: grafana/alloy:v1.5.0
    command: [run, /etc/alloy/config.alloy, --storage.path=/var/lib/alloy/data]
    volumes:
      - ./config.alloy:/etc/alloy/config.alloy:ro
  prometheus:
    image: prom/prometheus:v3.1.0
    command:
      - --config.file=/etc/prometheus/prometheus.yml
      - --web.enable-remote-write-receiver
    volumes:
      - prometheus-data:/prometheus

volumes:
  prometheus-data: {}
```

```text
// monitoring/config.alloy
prometheus.scrape "mmd" {
  targets         = [{"__address__" = "mmd.internal:9753"}]
  scrape_interval = "60s"
  forward_to      = [prometheus.remote_write.cluster.receiver]
}

prometheus.remote_write "cluster" {
  endpoint {
    url = "http://prometheus:9090/api/v1/write"
  }
}
```

`prometheus` resolves on every node, because the network spans every node
that runs `alloy`.

## Collect with Prometheus

A Prometheus that runs on every node (`placement: all`) scrapes its own node
with this job:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: mmd
    scrape_interval: 60s
    static_configs:
      - targets: ["mmd.internal:9753"]
```

To keep one Prometheus with all the data, run the per-node ones in agent mode
(`--agent`) with a `remote_write` to the central one, or use Alloy as above.

## Require a token

The host's admin can make every scrape of a project's network carry a bearer
token. Put the token in a file only root can read, and name it in
`/etc/mm/host.yaml`:

```sh
openssl rand -hex 32 | sudo tee /etc/mm/metrics-token >/dev/null
sudo chmod 600 /etc/mm/metrics-token
```

```yaml
# /etc/mm/host.yaml
metrics:
  token-file: /etc/mm/metrics-token
```

`mmd` reads the file again within ten seconds of a change, and never logs it.
A scrape without the token gets `401 Unauthorized`. The projects on the host
share this token, and it shows each of them only its own services. Give the
collector the token, for example as a [secret file](../secret-delivery.md),
and send it:

```text
prometheus.scrape "mmd" {
  targets    = [{"__address__" = "mmd.internal:9753"}]
  forward_to = [prometheus.remote_write.cluster.receiver]
  authorization {
    type             = "Bearer"
    credentials_file = "/run/secrets/metrics-token"
  }
}
```

In Prometheus, the same is `authorization: {credentials_file: /run/secrets/metrics-token}`
under the job.

## Settings in the host policy

All keys are optional, under `metrics:` in `/etc/mm/host.yaml`:

| Key               | Default | What it does                                                                                                        |
| ----------------- | ------- | ------------------------------------------------------------------------------------------------------------------- |
| `enabled`         | `true`  | `false` serves no metrics on this host, whatever a project's `metrics: true` asks for.                            |
| `token-file`      | none    | A root-owned file, mode `0600`, whose content every scrape of a project's network sends as `Authorization: Bearer <content>`. |
| `mesh`            | `false` | Also serve every project's metrics on the node's mesh address, port 9753, to the other nodes. Needs `mesh-token-file`. |
| `mesh-token-file` | none    | The token scrapes on the mesh address send: a file of its own, never the projects' `token-file`.                  |

With `mesh: true` a Prometheus that runs on a node's host, outside any
container, can scrape the other nodes at their mesh addresses, such as
`10.90.0.2:9753` (`mm target nodes` lists them), with the mesh token, which no
project holds. Only the other nodes of the cluster reach that address: the
node's own host and its containers never do, because a service of any project
with `network_mode: host` runs there too. A container reaches only its own
node, through `mmd.internal`.

`mmd access validate` checks the file. A setting takes effect within ten
seconds, without a restart.

## What a scrape returns

Every series has the label `node`. Service series also have `project` (the
project's name in the host policy), `environment`, `stack` and `service`, so
host, node and service series join without relabelling.

| Metric                                             | Type    | Labels besides `node`                       | Meaning                                                                                   |
| -------------------------------------------------- | ------- | ------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `mm_node_info`                                     | gauge   | `version`, `engine`                         | Always 1: the node's `mmd` version and container engine.                                  |
| `mm_metrics_collected`                             | gauge   | `part` (`host`, `mesh`, `services`)         | 1 when the last collection read that part, 0 when it could not.                           |
| `mm_metrics_collect_duration_seconds`              | gauge   |                                             | How long the last collection took.                                                        |
| `mm_mesh_peer_up`                                  | gauge   | `peer`                                      | 1 when the peer is on the mesh interface and shook hands in the last three minutes.       |
| `mm_mesh_peer_handshake_age_seconds`               | gauge   | `peer`                                      | Seconds since the last handshake with the peer.                                           |
| `mm_mesh_peer_receive_bytes_total`                 | counter | `peer`                                      | Bytes received from the peer.                                                             |
| `mm_mesh_peer_transmit_bytes_total`                | counter | `peer`                                      | Bytes sent to the peer.                                                                   |
| `mm_host_cpu_seconds_total`                        | counter | `mode`                                      | CPU seconds in each mode, over every CPU.                                                 |
| `mm_host_cpus`                                     | gauge   |                                             | The number of CPUs.                                                                       |
| `mm_host_boot_time_seconds`                        | gauge   |                                             | When the host booted, as a Unix time.                                                     |
| `mm_host_load1`, `mm_host_load5`, `mm_host_load15` | gauge   |                                             | The load average.                                                                         |
| `mm_host_memory_*_bytes`                           | gauge   |                                             | `total`, `available`, `free`, `buffers` and `cached` memory.                              |
| `mm_host_swap_total_bytes`, `mm_host_swap_free_bytes` | gauge |                                             | Swap space.                                                                               |
| `mm_host_filesystem_size_bytes`                    | gauge   | `device`, `mountpoint`, `fstype`            | Each local filesystem the host mounted; also `_free_bytes`, `_avail_bytes`, `_files`, `_files_free` and `_readonly`. |
| `mm_host_disk_reads_completed_total`               | counter | `device`                                    | Per whole disk; also `_read_bytes_total`, `_writes_completed_total`, `_written_bytes_total` and `_io_time_seconds_total`. |
| `mm_host_network_receive_bytes_total`              | counter | `interface`                                 | Per host interface; also `_packets_total`, `_errors_total` and `_drop_total`, and the same four for `transmit`. |
| `mm_environment_last_deployment_timestamp_seconds` | gauge   | `project`, `environment`                    | When the environment's last deployment finished on this node.                             |
| `mm_environment_rollout`                           | gauge   | `project`, `environment`, `state`           | 1 for the state of the last rollout here: `running`, `finished`, `failed` or `rolled-back`. |
| `mm_service_desired_containers`                    | gauge   | `project`, `environment`, `stack`, `service` | The containers the last deployment runs of the service on this node.                     |
| `mm_service_running_containers`                    | gauge   | `project`, `environment`, `stack`, `service` | The service's containers that run here, healthy or not.                                  |
| `mm_service_containers`                            | gauge   | the same and `health`                       | The service's containers here by `healthy`, `starting`, `unhealthy` or `stopped`.        |
| `mm_service_restarts`                              | gauge   | `project`, `environment`, `stack`, `service` | How often the engine restarted the service's current containers here.                    |

The host series come from the host itself, not from a container: the real
devices, filesystem types and interfaces. Filesystems are the host's local ones
(ext4, xfs, btrfs, zfs and similar, and the `tmpfs` at `/tmp`, `/run` and
`/dev/shm`); network filesystems, container layers and engine mounts are left
out. Interfaces leave out the host ends of containers' links (`veth…`).

A table of nodes and services like Swarm's `docker node ls` and
`docker service ls` comes from `mm_mesh_peer_up`, `mm_node_info`, and the sum
of `mm_service_running_containers` against `mm_service_desired_containers` by
`service`.

## How it stays cheap and private

- One collection serves every scrape for five seconds, so a fast scrape
  interval does not load the node. Each part has a deadline, and the answer is
  at most 4 MiB.
- `mmd` listens on the node's address on the network and nowhere else, and
  answers only containers of that network: never the node's own addresses,
  which any process on the host reaches. Anyone else is closed at once. It
  answers `GET /metrics` only, at most four scrapes at a time on each address.

## Limits

- A node serves a network's metrics only while it holds a part of that network,
  which it does while it runs a service on it. Place the collector with
  `placement: all` to cover every node.
- A container reaches only its own node's metrics. To scrape every node from
  one place without a collector on each, use `mesh: true` from the hosts; a
  host's Prometheus then reads every node but its own.
- `role: job` services have no desired count: their containers come and go with
  their runs.
- Shared storage usage and reachability, and the latency between nodes, are not
  served yet. `mm target storage status` and `mm target mesh status` show them.
