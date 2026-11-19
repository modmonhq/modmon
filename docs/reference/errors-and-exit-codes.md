---
title: Errors and exit codes
description: Stable process exits, error envelopes, and MM error codes.
---

## Exit categories

| Exit  | Meaning                                                                                 |
| ----- | --------------------------------------------------------------------------------------- |
| `0`   | Success, valid status, or no difference.                                                |
| `1`   | Local I/O, state, terminal, stale-plan, or difference result, depending on the command. |
| `2`   | Invalid request, configuration, environment, target, or build input.                    |
| `3`   | Security, secret-provider, runtime, process, or capability failure.                     |
| `130` | Interrupted by Ctrl-C or termination signal.                                            |

The category is stable; command-specific successful diagnostics can deliberately
return a non-zero status as described below.

## Error output

Human output uses `error[MM_CODE]: message`. With `--output json`, command
errors are written to stderr as a JSON error envelope and remain JSON:

```json
{ "error": { "code": "MM_CONFIG_NOT_FOUND", "message": "..." } }
```

CLI parsing failures, including unknown options, missing values, and invalid
enum values, use `MM_CLI_PARSE`. They honor `--output json` even though parsing
did not produce a command. A failure while rendering an error is reported as
`MM_OUTPUT` on stderr.

Recoverable setup errors also include `error.recovery`: a typed `issue`, a
plain-language `help` message, and a `command` argument array. Suggested commands
preserve explicit root, mm-home, environment, target, and runtime selections.
Human output prints the corresponding quoted command. Other errors may have
`recovery: null`.

## Stable command error codes

| Code                              |                    Exit | Category                                                                                                      |
| --------------------------------- | ----------------------: | ------------------------------------------------------------------------------------------------------------- |
| `MM_CONFIG_NOT_FOUND`             |                       2 | No `.config/mm.yaml` found from the selected root or its parents.                                             |
| `MM_CONFIG_READ`                  |                       2 | Configuration could not be read.                                                                              |
| `MM_CONFIG_PARSE`                 |                       2 | YAML, UTF-8, location, or typed-schema parse failure.                                                         |
| `MM_CONFIG_INVALID`               |                       2 | Validation returned one or more errors.                                                                       |
| `MM_CONFIG_EXISTS`                |                       2 | Initialization would replace an existing configuration.                                                       |
| `MM_ENVIRONMENT_REQUIRED`         |                       2 | No CLI, environment-variable, or default environment selection.                                               |
| `MM_ENVIRONMENT`                  |                       2 | Environment compilation or operation error.                                                                   |
| `MM_TARGET`                       |                       2 | Target selection or definition error.                                                                         |
| `MM_BUILD`                        |                       2 | Build planning or input error.                                                                                |
| `MM_SECRET`                       |                       3 | Secret provider or lifecycle error.                                                                           |
| `MM_SECRET_INCOMPATIBLE`          |                       3 | Storage, delivery, runtime, workload, or rollback combination is unsupported.                                 |
| `MM_SECRET_STORE_UNAVAILABLE`     |                       3 | A secret store a deployment needs did not answer or refused the login; nothing was planned.                   |
| `MM_TARGET_OPERATION_IN_PROGRESS` |                       3 | Another mutation holds the target operation lock.                                                             |
| `MM_SECURITY`                     |                       3 | A path, ownership, approval, or other security policy rejected the operation.                                 |
| `MM_PROCESS_SPAWN`                |                       3 | A required subprocess could not start.                                                                        |
| `MM_PROCESS_TIMEOUT`              |                       3 | A subprocess exceeded its bounded timeout.                                                                    |
| `MM_PROCESS_FAILED`               |                       3 | A subprocess exited unsuccessfully.                                                                           |
| `MM_RUNTIME_RESPONSE`             |                       3 | Runtime output could not be interpreted safely.                                                               |
| `MM_RUNTIME_UNSUPPORTED`          |                       3 | The selected runtime lacks a required advertised capability.                                                  |
| `MM_IO`                           |                       1 | Other local filesystem or stream I/O failure.                                                                 |
| `MM_STATE`                        |                       1 | Private state-store failure.                                                                                  |
| `MM_PLACEMENT`                    |                       2 | A service cannot be placed on the environment's cluster target: a `placement` name is not a node or group, a Compose service has no entry under `modules`, two services that share an engine are on different nodes, `--nodes` leaves out a node a service moves to, or a job's `placement` names a node where its service does not run. |
| `MM_RELEASE_INPUT`                |                       2 | A deployment plan's release does not fit the commit: a declared `release-inputs:` value is missing, one is given that is not declared, a value does not match its `pattern`, the `--release` name is not one word, or a template reads a release fact the deployment does not have, such as `MM.IMAGE.<module>` without a build. Also a `release-inputs:` entry with a bad name or pattern in `mm config validate`. |
| `MM_UNREACHABLE`                  |                       3 | A remote host or cluster node, or its `mmd`, did not answer. The message says what happened and what to run.  |
| `MM_NODE_MISMATCH`                |                       3 | A request for one cluster node reached a host with another node name. Nothing was applied there.              |
| `MM_NOT_DEPLOYED`                 |                       3 | A remote host never began a deployment of the environment, so it has nothing to show or run. The message names the command that plans one. |
| `MM_APPLY_STOPPED`                |                       3 | A cluster deployment failed on a node. The message names the node, the service, and its last log lines, and says which nodes were rolled back, left changed, or never reached. |
| `MM_DEPLOY_RUNNING`               |                       3 | Another deployment holds the environment. The message says who started it and when; `mm deploy status` shows where it is. |
| `MM_REMOTE_SCHEDULES`             |                       3 | A schedule of a remote environment was to be reconciled, toggled, or started from this machine. Its deployment reconciles the schedules on the host; change them in `mm.yaml` and deploy. |
| `MM_TIMEOUT`                      |                       3 | Something did not finish, or stopped showing progress, within its time: a step on a host past its deadline, a host that showed no progress for `deploy.progress-timeout`, a request a host did not answer in time, an engine that did not answer. The message names what, where, and for how long; `mm deploy status` shows what still runs. |
| `MM_CANCELLED`                    |                       3 | An operation was stopped before it finished: by `mm deploy cancel` (the message names who), or because the client that started it went away or was not heard from for two minutes. Its child processes were ended. |
| `MM_HOST_BUSY`                    |                       3 | A change to a remote host waited for another operation that is changing the same host and ran out of time. The message says whose operation and since when. Nothing was changed; run the command again. |
| `MM_TUNNEL_TOKEN_MISSING`         |                       3 | An environment with `ingress.type: cloudflared` cannot plan: the secret its `cloudflared.token` names is not declared, has no value on an ingress node, or does not hold a Cloudflare Tunnel token. The message names the `mm secret set` command. |
| `MM_TUNNEL_NOT_CONNECTED`         |                       3 | A node's Cloudflare Tunnel connector held no connection to Cloudflare within 90 seconds of a deployment. That node went back to what it ran before, and the nodes after it were not changed. The message names the connector's container, whose log says why. |
| `MM_REMOTE`                       |                       3 | A remote host reported an error under a code this `mm` does not know; the message starts with that code. Upgrade `mm`. |
| `MM_TERMINAL`                     |                       1 | Prompt or terminal contract failure.                                                                          |
| `MM_CLI_PARSE`                    | Clap status, normally 2 | Arguments could not be parsed.                                                                                |
| `MM_OUTPUT`                       |         original status | Fallback when mm cannot print the intended output.                                                            |

`mm` also exits with `MM_SECURITY` when an AI agent runs it directly instead of
through the MCP server. See
[mm refuses to run for an agent](../mcp.md#mm-refuses-to-run-for-an-agent).

Validation and status payloads also contain specific `MM_*` diagnostic codes.
Those codes identify individual findings; process status is determined from
their severity and category rather than from the presence of warnings.

An error from a remote host keeps the code and message it had on the host, and
on a cluster target it starts with the node's name. Only a refusal the host
made as policy, such as a missing access level, is reported as `MM_SECURITY`.

`MM_COMPOSE_DEPLOY_KEY` is the finding for a Compose `deploy` key that places
or multiplies a service, in an environment whose target lists `nodes:`. The
message says [what to write instead](../how-to/from-docker-swarm.md#compose-keys).
`MM_COMPOSE_DNS` is the finding for `dns`, `dns_search`, or `dns_opt` on a
service whose network spans the nodes of a cluster, where each node
[answers names itself](../remote-daemon.md#service-names-across-nodes) and the
key would have no effect. Remove the key.
`MM_SHARED_VOLUME` is the finding for a
[shared volume](../remote-daemon.md#shared-volumes-across-nodes) that cannot
be shared: no Compose file of the stack declares it, the Compose file sets
`external` or `name` on it, a service with `role: database` or `role: queue`
mounts it without `allow-shared-volumes: true`, or the target uses an
existing interface instead of mm's mesh.
`MM_HARDENING_REASON` is the finding for a service that leaves a default of
[`hardening: strict`](../hardening.md) out under `hardening.except` without
saying why. Write the reason as the key's value.
`MM_CONFIG_FILE` is the finding for a service's
[`config-files`](../config-files.md) entry that cannot be shipped: the path in
the container is not absolute, the entry names no source or two, the file is
missing or outside its module's directory, the owner or mode is not valid, or
a template has a placeholder that names no binding or names a secret.
`MM_COMPOSE_ENVIRONMENT` is the finding for a Compose file that sets
`environment` or `env_file` on a service; move the values to the service's
`env:` or its module's environment template.
`MM_HOST_NETWORK` is the finding for `host-network` without a reason.
`MM_COMPOSE_SOURCE` is the finding of `mm config validate` for a Compose
file that `mm deploy` or `mm dev up` would refuse in the environments it
names, such as `network_mode: host` on a service of this machine without
[`host-network`](./configuration.md#modules-services-and-builds); the
message names the file, the service, and what to change.
`MM_USER_NAMESPACE` is the finding for `host-user-namespace` without a
reason, and, on Podman hosts, where each service has [user ids of its
own](../remote-daemon.md#run-podman-on-the-hosts), for two services with ids
of their own that mount one volume and for a `uid` or `gid` of a config
file or secret file outside the service's ids 0 to 65535. A plan on such a
host fails with it when the host has no `mm-services` range in `/etc/subuid`
(run `mm target setup`), when the range is full, and when a service on a
shared volume cannot have the ids every other node gives it.
`MM_UPDATE_ORDER` is the finding for a service's `update:` that cannot work:
`order: start-first` on a service that publishes a fixed host port, sets
`container_name`, runs more than one container, or on Podman sets its own
`userns_mode` other than `host`, or a `parallelism` of 0. `MM_STACK_ORDER` is the finding for a stack's
`after:` that names no stack, the stack itself, or a loop,
`MM_DEPLOY_TIMEOUT` for a `deploy.convergence-timeout` or
`deploy.progress-timeout` that is not a time, `MM_DEPLOY_MM_VERSION` for a
`deploy.mm-version` that is not a version requirement, and `MM_DRAIN` for a
service's `drain` that is not a time or is longer than `15m`. `MM_SCALE` is
the finding for a service's `replicas` or `autoscale` that cannot work: on a
target of one host, a `scale.min` above `scale.max`, a `replicas` outside
that range, thresholds that overlap, a `for` longer than `4h`, an autoscaled
service on no network of `mm.yaml`, or a Compose `container_name`, `scale`,
or fixed host port that its count rules out. Each names the key to change.
`MM_HEALTH` is the finding for a service's `health:` that cannot work: an
`after` shorter than `10s`, a time that is not one, a `max-restarts` of 0, or
restarts on a target of one host.
`mm doctor` reports what each member of an environment's
[managed secret store](../how-to/managed-secret-store.md#check-the-store)
says: `MM_SECRET_STORE_SEALED` (a member is sealed), `MM_SECRET_STORE_DEGRADED`
(a member does not run or answer, is not a voter of the Raft cluster, or no
member leads), and the warning `MM_SECRET_STORE_VERSION_SKEW` (members run
different OpenBao releases). `mm target setup plan` and its apply repair each.
`MM_SECRET_STORE_INVALID` is the finding for a provider with `scope:
environment` that mm cannot run on a cluster: not `driver: openbao` with
`mode: managed`, a `release` mm does not install, a target or `nodes` list
with other than 3 or 5 members, or a second such provider. See
[the managed store](../how-to/managed-secret-store.md#declare-the-store).
`MM_ENVIRONMENT_TLS_INVALID` is the finding for an environment's `tls:` that
names a secret `secrets:` does not declare.
`MM_INGRESS_RULE_INVALID` is the finding for a route's `headers`, `allow`, or
`priority`, or an environment's `ingress.headers`, that mm cannot write: a
header name that is not one, a value with a control character, an `allow`
entry with neither `methods` nor `path`, a method not in capitals, a `path`
that is not a regular expression or has a backtick, or a `priority` outside 1
to 1000000. `MM_TUNNEL_IMAGE_INVALID` is the finding for an
`ingress.cloudflared.image` that is not an image reference.
`MM_JOB_ONLY_SERVICE` is the finding for a service with `role: job` that sets
what only a running container uses: Compose `ports`, an ingress route to it,
`update.order: start-first`, `replicas`, `autoscale`, or another service's
`depends_on` onto it. It is a warning when no job runs in the service.
`MM_DEVELOPMENT_DEPENDENCY` is the finding, in the development environment,
for a service that runs and needs, through Compose `depends_on`, `links`,
`volumes_from` or a `service:` mode, a service that
[`development:`](configuration.md#optional-in-development) keeps off on this
machine; the message names both and the `mm dev enable` command that turns the
other on. `MM_DEVELOPMENT_UNIT` is the warning for a name in the local
overrides' `development.enabled` that is no module, service or job;
`mm dev disable NAME` removes it.
`MM_SERVICE_ENVIRONMENT_UNKNOWN` is the finding for a service, or a
`secret-files` or `config-files` entry, whose `environments` names an
environment that `environments:` does not declare.
`MM_REMOTE_GUARDRAIL` is the finding of `mm config validate --for-remote` for
a Compose feature that [a remote host refuses](../remote-access.md#what-a-deploy-may-run)
in a service the named environment runs. With `--host-policy`, a feature the
policy's `allow` list grants is the information `MM_REMOTE_GUARDRAIL_ALLOWED`
instead, which does not fail the command.
A deployment that does not become healthy says why inside its message:
`MM_RUNTIME_UNHEALTHY` (the health check fails), `MM_RUNTIME_EXITED` (the
container exited), `MM_RUNTIME_RESTARTING` (the engine keeps restarting it),
`MM_RUNTIME_PAUSED` (its container is paused: only an unpause resumes it),
or `MM_CONVERGENCE_TIMEOUT` (not healthy within
[`deploy.convergence-timeout`](configuration.md#bindings-environments-and-secrets)),
next to the service and its last log lines. On a target that names one host
the command fails with that code; on a cluster with `MM_APPLY_STOPPED`, whose
message names the node.

Readiness findings include `MM_STATE_LEGACY`, `MM_TARGET_MISSING`,
`MM_RUNTIME_REQUIRED`, `MM_BUILD_REQUIRED`, and `MM_SERVICES_REQUIRED`. On an
environment whose target lists nodes, `mm doctor` also asks every node and
reports `MM_NODE_UNREACHABLE`, `MM_NODE_REFUSED` (its `mmd` answered and
refused, such as another remote protocol or a host policy it cannot read; the
message says what to do), `MM_NODE_NOT_SET_UP`, `MM_NODE_MISMATCH`,
`MM_NODE_VERSION_SKEW`, `MM_NODE_COMMIT_SKEW`, `MM_ROLLOUT_UNFINISHED` (a
rollout that was begun and neither finished nor rolled back; `mm deploy
rollback` undoes it), and `MM_SERVICE_ANSWERED_UNHEALTHY` (no container of a
service passes its health check, so the nodes answer and route to its running
ones anyway; fix the service or its `healthcheck:`), `MM_SERVICE_RESTART_LIMIT`
(a node stopped restarting a container that kept failing its health check;
`mm service logs` shows why, and a deployment or `mm service restart` starts
it again), what each node says about its container engine: `MM_NODE_ENGINE` (the engine
does not answer), `MM_PODMAN_VERSION` (Podman older than 5.0), and
`MM_PODMAN_COMPOSE` (`podman compose` does not run Docker Compose); a
target that names one host is asked for these three and for how its `mmd`
fares (below), not for the rest. For every remote environment, cluster or single host, it reports how each host's
`mmd` fares: `MM_DAEMON_NOT_ANSWERING` (a part of the daemon does not
answer), the note `MM_DAEMON_RESTARTED` (it started in the last 15 minutes),
the note `MM_OPERATION_IN_FLIGHT` and the warning `MM_OPERATION_STUCK` (an
operation past its deadline, or not at work for two minutes; `mm deploy
cancel` stops it), `MM_CHILD_PAST_DEADLINE` (a process `mmd` started is still
there after its deadline), and `MM_SERVICE_NOT_CONVERGED` (a service whose
containers on a node are not all ready). It also reports what
`mm target mesh status` finds between the nodes: `MM_MESH_NO_HANDSHAKE` (two
nodes do not hear each other; the help names the UDP port and both addresses),
`MM_MESH_PEER_MISSING`, `MM_MESH_KEY_MISMATCH`, `MM_MESH_DRIFT`,
`MM_MESH_DOWN`, `MM_MESH_NOT_SET_UP`, the warning `MM_MESH_EXTRA_PEER`, and
`MM_MESH_GUARD` (the firewall rules that
[keep the mesh between the hosts](../remote-daemon.md#what-the-mesh-carries)
are not in place on a node; the help says how to load them).
`MM_RUNTIME_NOT_WORKING` names a host whose policy offers an
[OCI runtime](../hardening.md#check-a-hosts-runtimes) that its engine does
not know or cannot start a container under; a remote environment on one host
is asked too. The warning `MM_RUNTIME_NOT_CHECKED` names a host that was
reached and could not check, such as one whose `mmd` is older than `mm`.
On every remote environment, `mm doctor` compares releases:
`MM_VERSION_PIN` names this mm or a host's `mmd` outside the environment's
[`deploy.mm-version`](configuration.md#bindings-environments-and-secrets)
(plans are refused then), and the warnings `MM_HOST_RELEASE_OLDER` and
`MM_HOST_RELEASE_NEWER` name the hosts that run another release than this mm,
with the upgrade that brings them together. A host whose policy file has a
mistake answers every request with `MM_CONFIG_PARSE`, naming the file, the
line, and `mmd access validate`; its containers keep their names.
For an environment that comes in through a Cloudflare Tunnel, it reports
`MM_TUNNEL_NOT_CONNECTED` for a node whose connector holds no connection to
Cloudflare or does not answer (the help names the connector's log and the
`mm secret set` command for a refused token), and the warning
`MM_TUNNEL_PUBLIC_PORTS` for a node that keeps ports 80 and 443 open for
another environment with public ingress.
`MM_NETWORK_DRIFT` names a node whose firewall rules, routes, bridge settings,
or network addresses differ from what its last deployment stored for the
[networks that span the nodes](../remote-daemon.md#networks-across-nodes); the
help says how to repair it. `mm doctor` also looks at this device's SSH keys:
the warning `MM_KEY_UNPROTECTED` names a plain key (`mm key protect` encrypts
it), the error `MM_KEY_PASSPHRASE_MISSING` an encrypted key whose passphrase
the vault no longer has (`mm key rotate`), the warning
`MM_KEY_ROTATION_PENDING` a rotation that was started and not finished
(`mm key rotate --finish`), and `MM_KEY_PASSPHRASE_IN_FILE` a
passphrase kept in a file beside the key: a warning where a system keychain
is available, a note where there is none or where `MM_KEY_VAULT=file` chose
the file, and the message says which. See
[remote access](../remote-access.md#keys-are-encrypted-and-only-mm-uses-them).
The warning `MM_WORKSPACE_STALE` names registered project folders, or their
`.config/mm.yaml`, that are gone; `mm workspace forget --stale`
[forgets them](../local-dashboard.md#forget-a-project-that-is-gone).

For shared storage it reports what `mm target storage status` finds:
`MM_STORAGE_UNREACHABLE` (a node cannot reach the storage node's storage
port; a warning while no volume is shared), `MM_STORAGE_NOT_SERVED` (the
storage node was never set up to serve: run `mm target setup`), and
`MM_STORAGE_NODE_SILENT` (a node's `mmd` does not answer). A deployment that
needs a shared volume fails with `MM_STORAGE_UNREACHABLE` or
`MM_STORAGE_NOT_SERVED` before it changes anything, and
`mm target storage delete` fails with `MM_STORAGE_IN_USE` while a container
has the volume mounted.
Doctor reports these findings together on stdout, including in JSON mode, and
marks dependent checks as `blocked` until they can be evaluated.

## Diagnostic commands

`mm config validate` returns `2` when validation has an error, otherwise `0`.

`mm doctor` returns `2` when its aggregate configuration, CA, local-route TLS,
or shared-service report contains an error. If that report is valid, it returns
`3` when the runtime is unavailable or has an error diagnostic, otherwise `0`.
An explicit `--check-updates` returns `1` when the stable manifest is unreachable
or invalid and includes an actionable unavailable report. Up-to-date,
update-available, installed-newer, and unsupported-platform states do not by
themselves fail unrelated doctor checks.

For an environment on a remote target, the CA, local-route TLS, and
shared-service findings describe local development on this machine, not the
environment. Doctor lists them as notes and they do not change the exit
status: `mm doctor -e production` returns `0` for a ready remote environment
on a machine where `mm dev setup` never ran, where it used to return `2`.
This machine's container engine is not asked either, because the
environment's containers run on the target's hosts: the `runtime` line reads
`on the target's hosts; this machine's engine is not used`, JSON keeps
`"runtime": {"available": false}` with the note
`MM_REMOTE_RUNTIME_NOT_INSPECTED`, and neither returns `3`. A workstation
with no container engine gets `0` for a ready remote environment, where it
used to get `3`.

`mm doctor recover-state` prints a read-only recovery review. Applying it requires
the exact `--plan HASH --yes`; successful recovery reports the backup path.
See [legacy-state recovery](../help/troubleshooting.md#state-uses-an-unsupported-pre-baseline-sqlite-schema)
for what is reset and preserved.

`mm target nodes` returns `3` when a node of the target is not ready, and `0`
when every node answers as itself. It prints every node either way.

`mm service status` returns `2` for configuration diagnostics, `3` for runtime or
provider diagnostics, and `0` otherwise. Warnings alone do not make either
command fail.

`mm service logs` on a cluster target fails only when no node answers. A node
that does not answer while another does is a line in the stream, and the
command returns `0`.

Environment status/check/setup operations return `3` for error diagnostics
`MM_SECRET_PROVIDER_UNAVAILABLE`, `MM_SECRET_ACCESS_DENIED`, or
`MM_SECRET_INVALID`; other error diagnostics return `2`; warnings return `0`.

`mm env diff` first applies those environment diagnostic rules. When status is
valid, it always prints the diff and returns `1` if the materialized file would
change or `0` if it is unchanged. Exit `1` is therefore a difference result,
not an execution error.
