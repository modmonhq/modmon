---
title: Status, logs, and commands
description: Check running services, follow logs, and execute a command in a container.
---

## Check services

```sh
mm service status
mm service status web
```

A service is all of its containers: several on one host when it is scaled,
and one on each node it is placed on when the environment runs on a
[cluster target](./remote-daemon.md#deploy-to-a-cluster). Status lists each
service once, with a row for every container. On a cluster the rows go node
by node:

```text
shop [production] -> production
  runtime:     DockerCompose (available)
  deployed:    commit 3f2c9a1e5b7d  2026-11-12T18:03:22Z  by alice  plan 8a41c0de91f2  build receipt 5d0e7b12c9a4

  STACK  SERVICE  STATE         NODE   CONTAINER  HEALTH   STATUS                    IMAGE
  app    db       running 1/1   db-1   db-1       healthy  Up 3 days                 postgres:17
  app    web      running 2/2   app-1  web-1      healthy  Up 2 hours                registry.example.com/shop/web:1.4.2
                                app-2  web-1      healthy  Up 2 hours                registry.example.com/shop/web:1.4.2
  app    worker   degraded 1/2  app-2  worker-1   running  Up 5 minutes, 2 restarts  registry.example.com/shop/worker:1.4.2
                                db-1   -          -        no container              -
```

| Column      | Meaning                                                                                                                                                                                                                  |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `STATE`     | The service as a whole, then how many of its containers are ready (running, and passing their health check when they have one) out of how many it should have. `starting` means every container runs and a health check has not passed yet, as during a deployment. `degraded` means some run and some do not, one is unhealthy, or a node that should run the service did not answer. |
| `NODE`      | The node the container is on. Only a cluster target has this column.                                                                                                                                                     |
| `CONTAINER` | The container's short name, as Compose prints it: `web-1`.                                                                                                                                                               |
| `HEALTH`    | The health check's verdict. `running` for a running container without a health check; the engine's state, such as `exited`, for one that does not run.                                                                    |
| `STATUS`    | The engine's own words: `Up 2 hours`, `Exited (1) 3 minutes ago`, with the restart count when there is one. `no container` on a node where the service is placed and has none. `no answer` on a node that did not answer: the service is placed there and nobody knows whether it runs. `waits for shared storage` for a container the engine could not mount a [shared volume](./remote-daemon.md#shared-volumes-across-nodes) for; it starts by itself when the storage node answers again. |

A container that a newer one replaced, or that ran once for a job
(`compose run`), is history. It has no row and does not make the service
degraded.

On a remote environment the `deployed:` line is the deployment the containers
were started from: the commit, when and by whom it was applied, the plan, and
the build receipt when there was one. When the nodes of a cluster do not all
run the same release, there is one line for each node, ending in `on app-1`.

Below the table, a cluster status lists
[what each service name resolves to](./remote-daemon.md#see-what-a-name-resolves-to)
and then what is wrong with a node:

```text
  node app-2: has not answered after 5 seconds. Check `systemctl status mmd` on the node; `mm target nodes` asks it again
  node db-1: differs from its last deployment: service 'worker' is deployed here and has no container. `mm deploy plan -e production` shows the repair and applying it puts the node right
```

- A node that cannot be reached is listed with the reason. A node that says
  nothing is waited for until five seconds after the first node answered. The
  rows of the other nodes are shown either way. Each service this checkout
  places on that node counts one container there that is not ready, so a
  service on three nodes reads `degraded 2/3`, with a `no answer` row for the
  node, instead of `running 2/2`.
- `differs from its last deployment` names a service that was deployed to the
  node and has no container there, or a service that runs there although the
  deployed configuration places it on other nodes. Plan and apply a
  deployment to put it right.
- `cluster networks differ from the last deployment` is described under
  [when a node drifts](./remote-daemon.md#when-a-node-drifts).

Status reads what runs and waits for nothing. It takes no deployment lock, so
it answers while a deployment is in progress, and each node answers for
itself.

With `--output json`, each service under `stacks[].services[]` has its
`containers`, and on a cluster `nodes`: one entry per node with `node`,
`state`, `starting`, `desired_count`, `running_count`, and `healthy_count`. Each container
has `node`, and `"history": true` when it is history. `deployment` (`commit`,
`deployed_at_unix_ms`, `deployed_by`, `plan_hash`, `build_receipt`) is set
when every host runs the same release. A service has no `node` field: a
script that read `service.node` reads `service.nodes[].node` or
`containers[].node` instead.

A service's `state` is one of `running`, `degraded`, `stopped`, `missing`,
and `unknown`. Every service, and every entry of its `nodes`, also has the
boolean `starting`. A service that the table calls `starting` is `degraded`
in JSON with `"starting": true`: `"state": "degraded"` with
`"starting": true` is a rollout in progress or a service still coming up,
not a failure, and `"state": "degraded"` with `"starting": false` is a
failure to look at. While it starts, `running_count` equals `desired_count`,
`healthy_count` is smaller, and the container that is not ready has
`"health": "starting"`. The MCP read `services` returns the same fields. On a node that did not answer, the service's entry in `nodes` has
`"state": "unknown"`, `"desired_count": 1`, and both other counts `0`.

The top-level `nodes` list has one entry per node of a cluster target, and
every entry has the same six keys:

| Key             | Value                                                                                          |
| --------------- | ---------------------------------------------------------------------------------------------- |
| `name`          | The node's name.                                                                               |
| `reachable`     | Whether the node answered.                                                                     |
| `problem`       | Why it did not answer; `null` for a node that answered.                                        |
| `deployment`    | The deployment the node last finished, with the fields above; `null` when there is none or the node did not answer. |
| `drift`         | What differs on the node from its last deployment, one string each; `[]` when nothing does.    |
| `network_drift` | The same for its cluster networks; `[]` when nothing does.                                     |

The [terminal UI](./terminal-ui.md) and dashboard show the same service
information.

## Read logs

Replace `web` with your service name:

```sh
mm service logs web
mm service logs web --tail 200 --timestamps
mm service logs web --follow
mm service logs
```

The lines of every container of the service come as one stream, in the order
the engine recorded them. Each line starts with the container that wrote it:

```text
web-1 | GET / 200 12ms
web-2 | GET /cart 200 31ms
web-1 | GET /health 200 1ms
```

On a cluster target the stream has the service's containers on every node,
and each line names its node first:

```text
app-1 web-1 | GET / 200 12ms
app-2 web-1 | GET /cart 200 31ms
app-1 web-1 | GET /health 200 1ms
```

Without a service name you get every service of the environment in one
stream.

| Option                    | Effect                                                                                                                              |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `--tail COUNT`            | The last `COUNT` lines of the whole stream, oldest first. The default is 100. With `--follow`, the stream starts with up to that many lines from each container. |
| `--follow`                | Keep printing new lines until you press Ctrl+C.                                                                                     |
| `--node NODE`             | On a cluster target, read only this node. Repeat it for more nodes.                                                                 |
| `--since`, `--until`      | Only lines at or after, or before, a time in Unix milliseconds.                                                                     |
| `--timestamps`            | Print the engine's timestamp after the bar: `app-1 web-1 \| 2026-11-12T18:04:07.512345678Z GET / 200 12ms`.                         |
| `--stdout`, `--stderr`    | Only one of the two streams.                                                                                                        |
| `--stack STACK`           | The named service in one stack, when two stacks have a service of that name.                                                        |
| `--max-line-bytes`        | Cut a line longer than this many bytes. The default is 65536 and the most is 1048576. A line that was cut ends with ` [line truncated]`.       |

`--follow` also picks up containers that start later, so a deployment that
replaces a container does not end the stream. To keep lines in time order
while they arrive, it holds each line back briefly: up to 0.1 seconds for
another container on the same host, and up to 0.4 seconds for another node.

The order comes from the engines' timestamps, and each node stamps its lines
with its own clock. Two nodes whose clocks differ can swap lines that are
closer together than that difference, so keep the nodes' clocks synchronised
with NTP or chrony. Docker stores a line longer than 16 KiB in pieces and
counts each piece toward `--tail`, so a small tail can begin in the middle of
a very long line.

### When a node does not answer

A node that cannot be read is one line in the stream, marked with `!` instead
of `|`. The lines of the other nodes are still printed:

```text
app-1 web-1 | GET / 200 12ms
app-2 ! no answer: accepts SSH, but mmd is not running there (the relay found no daemon socket). Start it on the host with `systemctl start mmd`; `journalctl -u mmd` shows why it stopped
```

| Line                                                                             | Meaning                                                                         |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| `app-2 ! no answer: …`                                                           | The node could not be reached. The line says what is wrong and what to do.      |
| `app-2 ! no answer after 5 seconds; the other nodes' lines are shown without it` | The node said nothing for five seconds after another node answered.             |
| `app-2 ! the stream ended: …`                                                    | The node answered and its stream broke off later.                               |

With `--follow` the line appears when it happens; otherwise it comes after
the log lines. The command exits with a non-zero status only when no node
answered.

### Read logs from a script

With `--output json`, each line is one JSON object:

```text
{"identity":{"project_id":"0b6f4c1e-5a3d-4e2b-9c7a-1f2e3d4c5b6a","environment":"production","stack":"app","module":"shop","service":"web"},"sequence":1,"source":"stdout","node":"app-1","container":"shop-production-app-web-1","timestamp":"2026-11-12T18:04:07.512345678Z","message":"GET / 200 12ms","lossy_utf8":false,"truncated":false,"reconnect":0}
```

| Field                    | Value                                                                                                             |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------- |
| `identity`               | The service: `project_id`, `environment`, `stack`, `module`, `service`.                                           |
| `sequence`               | The line's number in this stream, from 1.                                                                         |
| `source`                 | `stdout` or `stderr`, or `runtime` for a line mm adds about a node that does not answer.                          |
| `node`                   | The node the line comes from. Absent on a single host.                                                            |
| `container`              | The engine's full name of the container.                                                                          |
| `timestamp`              | When the engine recorded the line, in RFC 3339, by the node's clock.                                              |
| `message`                | The line.                                                                                                         |
| `lossy_utf8`             | `true` when the line had bytes that are not UTF-8 and were replaced.                                              |
| `truncated`              | `true` when the line was cut at `--max-line-bytes`.                                                               |
| `reconnect`              | How often the stream of this container was opened again.                                                          |

### Secret values are replaced

mm replaces configured secret values with `[REDACTED]`. If it cannot check a
configured secret, it refuses to open the redacted stream. A value of eight
characters or more is replaced wherever it appears. A shorter value is replaced
only where it stands alone, so a secret `hunter2` is hidden in
`password=hunter2` and left in the path `/srv/hunter2/`. Values under four
characters, and words such as `null` or `true`, are not replaced: ordinary
output cannot be told apart from them, so give such a secret a real value.
This cannot protect arbitrary sensitive data your application writes to logs.
Local raw access requires both `--raw` and `--approve-high-risk`.

High output rates are limited.

On a remote host, reading every service at once needs access to the log of
each service. A key that is limited to some services is refused and told to
name a service it may read.

## Run a command

Put the command after `--`:

```sh
mm service exec api --stack app -- ./app --version
mm service exec api --stack app --user 1000:1000 --workdir /workspace -- ./tool check
```

Arguments are passed directly, without adding a shell. Pipes and redirects
are not interpreted inside the container. If the service has several running
containers, select one with `--container`. The command's exit code is returned
to your shell.

For an interactive shell on a local target:

```sh
mm service shell api --stack app --approve-high-risk
```

An exec `--env` value must be a secret reference such as
`NAME=provider://location`, not a plaintext value. When secret environment
values are supplied, output is suppressed and TTY mode is unavailable.

## Restart a service

```sh
mm service restart api
```

This restarts every container of the service, including containers that have
exited, and waits until they run and pass their health checks again, as long
as a deployment would wait (`environments.<name>.deploy.convergence-timeout`,
10 minutes by default). The image, configuration, and volumes stay as
deployed: to apply a changed variable or image, deploy again instead.

On a remote target, restarting needs the `operate` level (or higher) in the
[users file](./remote-access.md#levels); `deploy` is not needed. On a cluster
target the service restarts on one node at a time, in the order the target
lists its nodes, and on each node one container at a time, each healthy again
before the next. While another container of the service answers, a running
container first
[leaves without dropping a request](./remote-daemon.md#take-a-container-out-without-dropping-a-request):
no node sends it anything new, it serves what it has for its service's `drain`
(`1m` by default), and only then restarts; it is answered again once healthy.
The service's only container restarts at once, as on a single host. A node
that runs none of an [autoscaled](./remote-daemon.md#autoscale-a-service)
service has nothing to restart and is left out of the list.

```text
Restarted web on app-1, then app-2, then app-3, one node at a time, each healthy before the next (3 containers).
```

When a node's containers do not become healthy, the restart stops there; the
error names the node, the nodes that were restarted before it, and the nodes
it did not reach. Each restart is in the host's audit log with who asked.

To restart one container of a service, select it in the
[terminal UI](./terminal-ui.md#act-on-a-service-or-a-container).

## Runtime and remote support

Docker Compose and Podman Compose support logs and exec.

mm reads status and logs from the Docker Engine API on the engine's local
socket, which starts no `docker` process. On a cluster node, `mmd` reads the
containers on its networks and the containers that wait for shared storage
the same way every few seconds. It asks the `docker` command instead
when that command would talk to another engine: `DOCKER_HOST` names something
other than a `unix://` socket, or a Docker context other than `default` is
selected. Set [`MM_ENGINE_API=off`](./reference/environment-variables.md) to
always ask the command. Podman is always asked through its command.

SSH targets use the [remote daemon](./remote-daemon.md), with the permissions
granted by the host administrator. A remote host answers a status from the
containers its engine runs and renders no Compose file for it, so it answers
within tens of milliseconds, also in the middle of a deployment. Remote exec currently supports output and
cancellation, but not `--stdin`, `--tty`, or the interactive shell wrapper.
The container runtime itself never connects to a remote engine socket.
