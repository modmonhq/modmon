# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed

- `mm update` restarts the local user services that run `mm` (the dashboard
  control API under systemd or launchd) and checks they come back, lists
  `mm mcp serve` processes still on the old release, and, run in a project,
  ends with the release each remote environment's nodes run and the setup plan
  command for each environment with an older node.

- The development environment on this machine runs without Compose health
  checks, so a code change that breaks a service no longer fails each
  `mm dev up`. `environments.<name>.healthchecks` turns them on there, or off
  anywhere else; `service_healthy` dependencies then wait for
  `service_started`, and `mm service status` shows `check off`.

### Fixed

- A plan whose config-file template reads a release input it was not given
  fails with `MM_RELEASE_INPUT`, as one whose environment file does, instead
  of `MM_ENVIRONMENT`.
- On Linux, `mm ingress stop` keeps the public forwarder stopped across reboots
  and deploys, so a reverse proxy you run on ports 80 and 443 in front of mm's
  Traefik no longer makes `mm dev up` fail or ask for sudo. `mm ingress status`
  reports that state and Traefik's loopback ports.

- The apply command a cluster `mm target setup plan` prints carries every
  flag the plan was made with that `apply` needs again: `--upgrade`,
  `--downgrade`, `--commit`, and `--trust-new-host-keys` when the plan
  replaces a host key. A `--format json` plan lists them under `flags`.

## [0.1.2] - 2026-11-23

Remote protocol: 1. Every change in this release is additive: `mm` 0.1.2
works with hosts on 0.1.1, and hosts on 0.1.2 keep serving `mm` 0.1.1. Some
features need the host upgraded too (`mm target setup plan --upgrade`), and
remote hosts now refuse more Compose settings; see Changed and Fixed.

### Added

- A host exception for host paths: `host-path` in an `allow` entry of
  `/etc/mm/host.yaml`, with the paths in `paths`, lets one service bind-mount
  exactly those host paths recursively read-only (long syntax with
  `read_only: true` and `bind.recursive: readonly`), such as `/proc` and
  `/sys` for a metrics exporter. `mmd` plans one only on Docker Engine 25
  (API 1.44) or later on Linux 5.12 or later, as its engine reports; a Podman
  host refuses host paths for now. Allowing a path hands the repo
  what it holds: `/proc` shows every other project's processes, environment
  variables and files on the host, and a path with an engine socket or
  secrets in it, such as `/`, is root on the host. A policy that lists `/`,
  `/run`, `/var/run`, `/var/lib/docker`, `/var/lib/containers`,
  `/var/lib/containerd`, `/etc/mm`, `/var/lib/mm`, `/root`, `/home`,
  `/etc/shadow` or `/etc/ssh`, a path above or inside
  one, or a path inside `/proc`, is refused; `mmd` checks each path after
  resolving symlinks. See "Collect host metrics" in the remote access guide.
- `mm target preference set-address ENVIRONMENT NODE HOST[:PORT]` connects
  this device to a cluster node at an address of its own, such as a VPN
  address, without changing the repo. The node's recorded host key and daemon
  identity are checked as before. `unset-address` goes back to the repo's
  address, `show` lists them, and `mm target nodes`, the terminal UI and
  `MM_UNREACHABLE` say when one is in effect.
- `mm config validate`, `mm doctor` and the deployment plan warn
  (`MM_HEALTHCHECK_ARGUMENTS`) about a Compose health check in exec form
  (`CMD`) with an argument that contains whitespace or is empty, on Podman:
  Podman before 5.8 splits such an argument, so the check never passes. The
  warning names the service and file and gives the equivalent `CMD-SHELL`
  check.
- `deploy.failure-log-lines` sets how many last lines of a failed container's
  log a deployment keeps; `mm deploy status` shows them after a rollback
  removed the container.

### Changed

- `mm build --publish` also builds the modules that environments on remote
  targets run with the same build variant, so a receipt from a build-only
  environment such as `ci` deploys to an environment whose services it does
  not run. A CI publish can therefore build more images and take longer.
  Modules with build secrets outside `credential-only-secrets` are still
  built only where they run. The plan shows the environments each module is
  for, and `mm deploy plan --receipt` refuses a receipt that lacks a module
  the environment runs before asking any host. Builds without `--publish`
  are unchanged.
- A deployment keeps the last 200 lines of a failed container's log, bounded
  to 64 KiB with secret values hidden, instead of 20, also on a single host.
- A first deployment that fails while a stack starts is no longer removed:
  nothing ran before it, so the stacks it started keep running as they are,
  the failed one too, and the error names them with the commands to look at
  them and deploy again. The next deployment applies over them, so healthy
  stacks such as a database are not started again from scratch. A
  deployment that replaces one that ran still rolls back as before.

### Fixed

- Security: a remote host or cluster node no longer runs a service that
  reaches the host or another container without an exception.
  `container:NAME` in `pid`, `ipc` and `network_mode` is refused
  (`container-join`, which no exception allows; use `service:<service>`). A
  `service:` join that reaches a service with a host namespace needs the same
  exception as that service, such as `pid-host`. A guardrail key whose value
  comes from a variable, such as `privileged: ${PRIVILEGED:-true}`, a
  `security_opt` entry, `external` or a secret's `file`, is refused as the
  feature it could turn on; write the value itself. A quoted
  `privileged: "true"` and the `external: {name: ...}` form are caught too.
  `mm config validate --for-remote` reports the same.
- Security: a remote host checks more Compose keys against the values it
  runs rather than a list of known-bad ones. A config's or secret's `file`
  reached through a symlink or a `..` out of the project counts as
  `file-outside-project`, and `cap_add` entries in lower case, such as
  `cap_chown`, are no longer refused by mistake. **Breaking for remote
  targets**; each needs a change or a host exception:
  - `security_opt` accepts only `no-new-privileges` and `seccomp=builtin`
    without the `security-opt` exception, so a custom seccomp profile, SELinux
    label or AppArmor profile is refused.
  - A namespace mode other than the service's own, a `service:` join or
    `host` is `container-join`, which no exception allows; Podman's
    `userns_mode: keep-id` and `nomap` need `userns-host`, and
    `network_mode: slirp4netns` and `pasta` need `network-host`.
  - A volume or network `name:` must be the stack's Compose project name on
    that host or start with it and `_`; any other needs `external-volume` or
    `external-network`. `mm config validate --for-remote` warns
    (`MM_REMOTE_GUARDRAIL_NAME`). Do not just remove `name:` from a volume
    that holds data: Compose would create a new, empty volume. Have the host
    admin allow `external-volume` for `<stack>/volume:<key>`, or move the data
    first.
- A container that resolves its own `tasks.<service>` name as soon as it
  starts finds itself: `mmd` reads the engine for a lookup from a container it
  has not seen yet before answering it, for up to three seconds when the
  engine is slow to list its containers, instead of letting the name fall
  through to the host's resolvers.
- A build context reaches the engine with directories at 0755 and files at
  0644, or 0755 when executable, whatever the umask `mm build` runs under, so
  a non-root user in the image can read what `COPY` put there. The context
  hash now records these modes, so every module rebuilds once after the
  upgrade. A receipt promotes to another environment only when it and the
  hosts' `mmd` are both on this release or later; otherwise build that
  environment again.
- On Podman hosts, `mm config validate` and the plan refuse a config file or
  secret file entry of a service whose Compose file picks its own user
  namespace, such as `userns_mode: auto:size=1024`, when the entry names an
  owner other than root or a mode not every user can read
  (`MM_USER_NAMESPACE`). Such a file used to reach the container owned by an
  id it does not have, unreadable for a mode such as `0400`.
- On Podman hosts, a service whose Compose file sets a plain
  `userns_mode: auto` gets mm's own user ids, as a service without
  `userns_mode` does, so a secret file or config file with `uid: "1000"` and
  `mode: 0o400` reaches it owned by its uid 1000. A stack with several Compose
  files takes the `userns_mode` of the last file that sets one, a service
  takes the one of the service it `extends`, a tag such as `!override` stands
  for its value, and `userns_mode: ~` or `""` counts as not set.
- On Docker Engine with `userns-remap`, the plan refuses a secret file or
  config file that names an owner other than root or a mode not every user
  can read (`MM_USER_NAMESPACE`), instead of handing over a file the
  container's users cannot read. A service in the host's user namespace
  (`userns_mode: host` or `host-user-namespace`) keeps such files.
  0644, or 0755 when its owner may execute it (the bit Git records), whatever
  the umask `mm build` runs under, so a non-root user in the image can read
  what `COPY` put there. The context hash now records these modes, so every
  module rebuilds once after the upgrade. A receipt promotes to another
  environment only when it and the hosts' `mmd` are both on this release or
  later; otherwise build that environment again.
- On Podman hosts, a config file or secret file of a service whose Compose
  file sets its own `userns_mode`, such as `auto`, no longer reaches the
  container owned by an unknown id: `mm config validate` and the plan refuse
  an entry whose owner or mode cannot hold there (`MM_USER_NAMESPACE`) and say
  to remove `userns_mode`, which gives the service ids of its own.
- An `internal: true` network that spans cluster nodes no longer takes a
  container's default route, which cut a container that is also on an
  ordinary network off from the internet and from the other nodes. Podman
  creates it with `no_default_route=true` and Docker Engine as an internal
  network, where mm's firewall rules accept the traffic between its
  containers on different nodes, replies included. The first deployment
  after upgrading creates an existing one again and restarts the services on
  it.
- A network whose `internal` setting changes is created again with a reason
  in the plan: `with a default route` or `without a default route` after its
  old addresses, instead of the same addresses twice.
- A host pulls the images a stack or a job lacks in a step of its own before
  anything starts, with the registry logins, under `deploy.pull-timeout`
  (default `20m`). A large image on a slow registry no longer fails the
  deployment with `MM_PROCESS_TIMEOUT` after 120 seconds. `mm deploy apply`
  and `mm deploy status` show which image a host is pulling.
- `compose up`, `compose down`, and stopping replaced containers no longer
  stop after a fixed limit (two minutes on Podman, five on Docker). The limit
  now comes from the stack's `stop_grace_period` values, including ones set
  from variables or inherited through `extends`, and from the health checks
  other services wait for. A cluster rollout waits that long on top of its
  own steps.
- Processes in a container on a Podman host whose kernel runs AppArmor can
  signal each other again. `mm target setup` installs and loads the
  `mm-containers` profile, Podman's own plus signals and `ptrace (read)`
  between the processes a container sees, and points Podman at it. A
  service that shares the host's or another container's process namespace
  gets `mm-containers-shared-pid`, Podman's rules unchanged, and a plan
  that would run such a service under `mm-containers` is refused. `mm doctor`
  warns about a host whose containers run under Podman's default profile
  (`MM_PODMAN_APPARMOR`).
- A job whose image pull takes longer than 30 seconds keeps its claim on the
  run while it pulls, so a second worker no longer launches the same run
  again, and the job keeper of `mmd` looks after the other runs' timeouts and
  cancels meanwhile. A cancel stops the pull.
- `pull_policy: daily`, `weekly` and `every_<duration>` images are pulled on
  every deployment, like `always`, instead of only when missing; the time a
  host spends finding which images it lacks counts against
  `deploy.pull-timeout`.
- A job stopped at its timeout keeps the output it wrote, and `mm deploy logs`
  and `mm job history` show it.
- `mm job run` of a job with `retry` no longer waits forever after a failed
  attempt: reading the runs starts the next attempt once it is due.

## [0.1.1] - 2026-11-19

Remote protocol: 1. This release is the baseline: from it on, `mm` and the
`mmd` on your hosts use the newest remote protocol both speak, so a newer `mm`
works with hosts on 0.1.1 or later, and a host upgraded first keeps serving
older clients as long as it speaks their protocol. The protocol number
changes only for a change an older release cannot read. Builds from before
0.1.1 do not work with it: install `mm` with the installer and set their
hosts up again with `mm target setup plan --upgrade`. See
[compatibility between releases](docs/reference/compatibility.md);
[upgrade mm and your hosts](docs/how-to/upgrade.md) says which installations
`mm target setup` upgrades in place and what it refuses.

### Removed

- The separate release download site, the release certification step, and
  the PowerShell installer: releases are published on GitHub, and Windows is
  not a release target because `mm` does not build for Windows.
- The `docker-swarm` runtime driver and the `swarm-native-secret` delivery
  driver. The runtime drivers are `docker-compose` and `podman-compose`, and a
  configuration that still names a removed driver fails to load with a message
  naming its replacement. A target that lists several hosts under `nodes:`
  takes Docker Swarm's place: see
  [Coming from Docker Swarm](docs/how-to/from-docker-swarm.md).

### Changed

- A host's `mmd` that cannot read a request, such as one from a newer `mm`,
  answers `MM_REMOTE_UPGRADE_REQUIRED` with both releases and acts on none of
  it, instead of closing the connection. When `mm` and a host share no
  protocol, the refusal names the release and the protocols of each side, and
  the command for the side to upgrade.
- `mm target setup` orders releases by version first and remote protocol
  second, so a host from before 0.1.1 (version 0.1.0, protocol 16 to 19) is
  older than 0.1.1 and is replaced with `--upgrade`; before, setup took it
  for a newer release and refused it.
- A configuration key this `mm` or `mmd` does not read is ignored with a
  warning that names the file, the key, and the key it most likely meant,
  instead of failing to load:
  ``Warning: unknown key `modules.web.build.sufix` in .config/mm.yaml; this mm ignores it (did you mean `suffix`?)``.
  A project that uses a key a later release adds keeps planning with an older
  `mm` and on hosts with an older `mmd`, and adding a key never raises
  `schema-version`. Plans list the keys under `unknown_config_keys`, a remote
  plan with the nodes that ignore them; `mm config validate --strict` fails
  on them, for CI. The published JSON Schema still flags them in editors.
- The installer is served at `https://modmon.dev/install.sh` (the site build
  copies `packaging/release/install.sh`) and installs from the releases on
  GitHub: the latest by default, or the one `--version` names, release
  candidates (`vX.Y.Z-rc.N`) included. It checks the archive against the
  release's `SHA256SUMS` before it unpacks anything, and installs `mm`, on
  Linux `mm-ingress-helper` beside it, and `mmd` with `--with-daemon`. Under
  WSL it installs the Linux archive.
- `mm doctor --check-updates` reads `manifest.json` of the latest GitHub
  release, which the release workflow attaches and attests; a release
  candidate is never offered as an update. It reads the manifest of every
  later release: fields, platforms, and binaries a release adds are skipped,
  and a release that needs a newer reader says to run the installer.
- A client newer than its host now says to run `mm target setup plan
  --upgrade` and to apply that plan with `--upgrade`; before, it named the
  plan without the flag that a cluster's setup asks for.
- A build reads the ignore file next to its Dockerfile, such as
  `web/Dockerfile.dockerignore` for `web/Dockerfile`, instead of the
  context's `.dockerignore` when it exists, as Docker does, and matches
  ignore patterns by Docker's rules: from the top of the context, so
  `node_modules` no longer excludes `web/node_modules` (write
  `**/node_modules`). An image's context hash covers only what its own
  ignore file sends.
- `mm build` builds only the build variant the environment selects (every
  variant when it selects none of several), and a build receipt deploys to
  another environment only when it holds the variant that environment
  selects; the refusal names the variants on each side and the command that
  builds the missing one.
- A registered project whose folder or `.config/mm.yaml` was deleted no longer
  stops the dashboard or `mm dev setup --services`: it is skipped, and the
  dashboard and `mm doctor` (`MM_WORKSPACE_STALE`) name the command that
  forgets it.
- Commands that wait show what they do at once: on a terminal, the running
  step with its time, a row per node and a bar where an amount is known,
  each finished step left as one line; elsewhere one plain line per
  finished step with its time (`MM_PLAIN=1` asks for that on a terminal).
  `mm target setup plan` shows the plan as a card with short IDs and the
  changes first, reads every node, the secrets and the host binaries at
  once, and asks the secret store once instead of once per secret.
  `target setup apply --yes` plans once, not twice.
- `mm deploy plan` and `apply` show the plan as a card and the rollout live,
  node by node; scripts still read the same `==> node: step` lines. Status,
  rollback, cancel, team sync, `target nodes`, `target mesh status`,
  `target storage status`, `doctor`, `build`, `service status`, `service
  restart` and a followed `job run` show what they wait for.
- The terminal UI's Variables view lists the services on the left and shows
  the variables of the one selected, instead of every variable in one table.
  Search (`/`) and the problem filter (`p`) still look across every service,
  with the selected service's matches first.
- The website and the documentation lead with clusters: what a list of
  servers gets from mm, the smallest cluster target, and the terminal UI on a
  cluster come first, agents second, and local development after them. New
  guides: [Run a cluster](docs/how-to/cluster.md) and
  [Coming from Docker Swarm](docs/how-to/from-docker-swarm.md). The old
  address `/docs/how-to/swarm-cluster` leads to the cluster guide.
- A restart of `mmd` (an upgrade, a crash, the watchdog) keeps its sockets:
  `mmd` hands them to systemd (`FileDescriptorStoreMax=` in `mmd.service`),
  so name lookups and requests that arrive meanwhile are answered by the
  next `mmd` instead of refused. Upgrading with `mm target setup` replaces
  `mm` and `mmd` one node at a time without restarting a container, and puts
  the new relay in place only as `mmd` restarts (before, a node refused every
  client for about half a minute of each upgrade).
- A mistake in `/etc/mm/host.yaml` no longer stops `mmd` at its next start:
  it keeps answering its containers' names and refuses every request with the
  file, the line, and `mmd access validate` until the file loads.
- The audit log is rotated at 16 MiB and keeps about 80 MiB in five files;
  `mmd access audit` reads them all. Request records used against replay are
  removed after two days.
- `mm target setup` with `delivery: push` removes the deploy key, the pinned
  origin host key, and the clone that fetch mode left on a host, and the
  setup plan says so, for one host as for a cluster.
- `mm deploy apply` refused for a secret store outage says the apply planned
  again and was refused, and that nothing was changed.
- `mm deploy plan` and `mm deploy apply` stop with
  `MM_SECRET_STORE_UNAVAILABLE` when a secret store the deployment needs is
  unreachable, sealed, or refuses the login, on a remote host, a cluster node,
  and this machine alike. They used to plan without the secret's version, and
  the apply failed when it read the value. `mm dev` still plans the
  development environment while its store is stopped and reports the store in
  its status. See
  [when a secret store does not answer](docs/secret-delivery.md#when-a-secret-store-does-not-answer).
- A remote host refuses a Compose service's `runtime:` unless its host policy
  offers that runtime under `runtimes:`; the refusal lists what the host
  offers. Before, `runtime:` reached the engine unchecked.
- On a cluster node a container no longer reaches its own node's mesh
  address, nor a port another node publishes on its mesh address. Reach such
  a port from the hosts, or put both services on a network under `networks:`.
- `mm_read` with kind `jobs` or `schedules` on a remote environment returns
  the declared jobs and their runs on the hosts (empty when none are
  declared) instead of failing with `MM_BUILD`; reconciling or toggling a
  remote environment's schedules is refused with `MM_REMOTE_SCHEDULES`.
- `mm service status` lists each service once, with a row for each of its
  containers, node by node on a cluster target. The columns are `STACK`,
  `SERVICE`, `STATE` (`running 3/3`), `NODE` (cluster targets only),
  `CONTAINER`, `HEALTH`, `STATUS` in the engine's words, and `IMAGE`. In
  `--output json` a service has `nodes` (one entry per node with its state
  and counts) and each container has `node`. `service.node` is gone: a
  script that read it reads `service.nodes[].node` or `containers[].node`.
- `mm service logs` prefixes every line with the container that wrote it
  (`web-1 | ...`), and on a cluster target with the node before it
  (`app-1 web-1 | ...`). In `--output json` each record has `node`,
  `container`, and `timestamp`.
- Status and logs take no deployment lock and wait for no deployment. A
  remote host answers a status from the containers its engine runs, read
  from the Docker Engine API on its socket, without rendering a Compose
  file: it answers during a rollout and within tens of milliseconds. Remote
  status no longer prints the Compose plugin's version.
- `mm service status` on a cluster counts a service on a node that did not
  answer: a service placed on three nodes, one of them silent, reads
  `degraded 2/3` with a `no answer` row for that node instead of
  `running 2/2`, and a service placed only on a silent node is listed as
  `unknown 0/1`. In `--output json` that node's entry under the service's
  `nodes` has `"state": "unknown"`.
- The count under `STATE` is the containers that are ready: running, and
  passing their health check when they have one. A service whose containers
  all run while a health check has not passed yet reads `starting 2/3`
  instead of `degraded 3/3`; in `--output json` its `state` stays
  `degraded`, the new boolean `starting` is `true` (on the service and on
  each node's entry under `nodes`), and `healthy_count` says how many are
  ready. `"state": "degraded"` with `"starting": true` is a rollout in
  progress, not a failure.
- In `--output json` of a status, every entry of the top-level `nodes` list
  has `problem`, `deployment`, `drift`, and `network_drift`: `null` or `[]`
  when there is nothing to report. They used to be left out when empty.
- An MCP tool that cannot reach a host answers `MM_UNREACHABLE` with the
  host, what happened, and the next step, in mm's own words; it used to
  answer with a sentence that named no cause. A host that refuses the agent
  key, as after `mm key rotate --agent`, says `refused this device's agent
  key` and how to register the new key.
- `mm doctor` says why a key's passphrase is in a file beside the key when
  `MM_KEY_VAULT=file` chose that (`MM_KEY_PASSPHRASE_IN_FILE`); it used to
  say the machine has no system keychain.
- On a cluster node, `mmd` reads the containers on its networks and the
  containers that wait for shared storage from the Docker Engine API. It
  used to start `docker ps` and `docker container inspect` every ten
  seconds.
- `mmd`'s name resolver no longer starts `ip -j -4 route show table all`
  each time it looks at the node: it reads the mesh state `mmd` stored. A
  node whose stored networks name one that no deployment there uses any
  more (the engine has no bridge for it) tried that address every second
  for as long as `mmd` ran; it now tries every second for 30 seconds, as at
  boot, and then with its ordinary look every ten seconds.
- `mm team join --agent` after `mm key rotate --agent` puts the new agent
  key in your entry in place of the one the rotation replaced and says so
  (`This replaces the previous agent key SHA256:…`); it used to add the new
  key beside the old one. Only a key this device's own rotation replaced is
  taken out. `mm key rotate --agent` names that step and the old key's
  fingerprint, and `mm key rotate --output json` has `replaced`.
- `mm doctor` for an environment on a remote target exits non-zero only for
  findings about that environment and what this machine needs to reach it.
  The local certificate authority and the shared development services are
  listed as notes. `mm doctor -e ENVIRONMENT` on a machine where
  `mm dev setup` never ran returned `2` and now returns `0` when the
  environment is ready. This machine's container engine does not decide
  the exit code either: doctor returned `3` for every remote environment,
  because the engine of a remote environment is not asked and that read as
  unavailable. The `runtime` line now says
  `on the target's hosts; this machine's engine is not used`.
- The remote protocol is version 19 (0.1.0 spoke version 1): `mm` and the
  hosts' `mmd` must run the same version, and a host on another one answers
  with the side to upgrade. Run `mm target setup plan` and apply it after
  upgrading `mm`. Version 18 adds the Compose overlay that keeps a
  `role: job` service out of a deployment and its `job_only` status,
  `environments` on secret and config files, and the release (name, inputs,
  images) on plans, deployment records and each request. Version 19 adds a
  route's headers and allowlist and the environment's Cloudflare Tunnel to
  cluster plans, each node's tunnel connector to a status, each node's member
  of the managed secret store to its facts and report, and the requests that
  snapshot, restore and fill that store.
- The terminal UI lists each service of a cluster once, with a muted row for
  each node beneath it; `space` folds the rows. The service details list its
  nodes and what differs from the last deployment, the Overview shows the
  deployed release, the log view starts each line with its node and
  container in one colour per node and marks a node that does not answer,
  and the start review lists service moves and cluster network changes.
- The terminal UI's deploy review is the plan `mm deploy plan` prints for the
  same plan, line by line and scrollable, instead of three lines. While the
  plan is applied, each node's row says which step it is on and which
  stacks it has applied, as the apply reaches it; `esc` hides the review
  while the apply goes on and `u` shows it again. On a remote environment
  `b` plans a rollback, the plan `mm deploy rollback` makes, and shows it
  the same way. The MCP tool `mm_deploy_plan` takes `rollback: true` for the
  same plan, and `mm_deploy_apply` applies it.
- In the terminal UI the rows beneath a service on a cluster are rows to act
  on: `l` follows that node's container, as `mm service logs --node` does,
  and `x` runs a command in it.
- The terminal UI shows errors as the command line does, with their code:
  `error[MM_UNREACHABLE]: ...`.
- `mm deploy apply` on a cluster prints a line when a node's turn with a
  stack is over (`==> app-1: app: healthy`).

- `mmd.service` is `Type=notify` with `WatchdogSec=30s`. `mmd` reports
  itself alive only while a request to its own socket is answered, so
  systemd replaces a daemon that stopped answering within half a minute,
  two seconds after it killed it (`RestartSec` was five), and keeps trying
  for as long as it takes. On a cluster node `mmd` answers names on a thread
  of its own, so a busy or stuck request does not delay a container's
  lookup. A daemon that starts while a deployment was in progress on its
  host releases that deployment's lock at once and marks the rollout
  interrupted; `mm deploy status` says so, and the next deployment undoes
  it. Run `mm target setup plan` and apply it to install the new unit.

- The terminal UI draws at once and fills in. Bare `mm` used to read
  everything before its first frame: 0.6 seconds for an environment on this
  machine and over 2 seconds for a remote one. It now draws from the project
  configuration in under 50 ms and reads the environment afterwards, each part
  by itself: a view whose part has not arrived says what it is waiting for,
  and on a cluster each node is listed as it answers, so one slow or silent
  node no longer holds back the others. While the environment is being read,
  keys that look around work and any other key says it works as soon as the
  environment is here.
- `mm` keeps the SSH connection to a remote host open for five minutes and
  uses it for the next request, from the same command or the next one, instead
  of connecting again each time. Only a connection made with this device's
  key file is kept: never one made with a security key, the agent key, or
  `MM_SSH_KEY`. Kept connections are closed when `mm` creates a key, applies a
  team sync, a deployment, or a target setup, or accepts a target's new keys,
  and when the key file they were made with is removed or replaced;
  `MM_SSH_REUSE=off` turns them off. The host checks every request as before,
  whichever connection carries it. See
  [remote access](docs/remote-access.md#connections-stay-open-for-five-minutes).
- Reads on a host are quicker for the host too: a status asks the engine for
  its version, its state, and each stack at the same time and renders each
  stack once less; a node checks its engine's firewall once for all cluster
  networks instead of once per network; and `mm deploy plan` reads the nodes'
  stored network addresses while they plan.
- In the terminal UI, a cluster node that does not answer counts as an issue
  in the header, the Overview, and Diagnostics, and the Services view names
  it. It used to be visible only in the details of the Environments view,
  under a header that said `no issues`.
- A deployment waits up to ten minutes for a stack to become healthy (it was
  two), and no longer waits at all when waiting cannot help: a container that
  exited, a container the engine keeps restarting, or a failing health check
  fails the deployment at once, with the service's name and the container's
  last log lines in the error. This applies to local and remote deployments.
- On a host, a deployment that was interrupted is undone before the next one
  starts even when the next one deploys another commit. It used to be left as
  it was, because the host looked for interrupted deployments per checkout.
- A request to a cluster node names the node it is for, and a node refuses a
  request meant for another (`MM_NODE_MISMATCH`). A client and a host on
  different protocol versions refuse to work together, and the error says
  which side to upgrade.
- An error from a remote host is reported with the code and message it had on
  the host, and on a cluster target with the node's name. It used to be
  wrapped as `MM_SECURITY` "security policy rejected the operation: remote
  error ...", whatever it was. Only a refusal the host made as policy is still
  `MM_SECURITY`. A host error under a code this `mm` does not know is
  `MM_REMOTE`.
- A host that cannot be reached fails with `MM_UNREACHABLE` and a message that
  names the host and address, says what happened (no answer, `mmd` not
  running, a changed host key, a refused key), and what to run next. It used
  to report a protocol error.
- `mm target trust` lists the keys of a cluster target's nodes by node name.
- `mmd` owns the WireGuard mesh between cluster nodes. The interface is `mm0`
  (it was `wg0`, written as a `wg-quick` configuration by the setup scripts).
  Each node's `mmd` makes the node's key and stores the applied state under
  `/var/lib/mm/mesh`, and `mm-mesh.service` restores the interface at boot
  before `mmd`, the container engine, and the storage server. A host set up by
  an earlier development version moves to `mm0` on the next
  `mm target setup` apply and keeps its key; no manual step is needed.
- `mm target setup` on a cluster runs a new step, `connect-nodes`, after
  `provision-nodes`: the mesh is brought up by the `mmd` that provisioning
  installs. `mm target setup plan` now logs in to nodes whose SSH host key the
  repo records, to read their public WireGuard key and peers; it changes
  nothing there.
- `mmd.service` may open netlink sockets, which it needs to read the mesh
  interface for `mm target mesh status`.
- The MCP `mm_logs` and `mm_exec` tools and the `services` read go through the
  same path as `mm service`, so they work on cluster targets too.
- Bare `mm` opens the terminal UI whatever state the environment is in. It
  used to print the full `mm doctor` report and a menu, and would not open
  until every local check passed, so a broken local environment also blocked
  switching to staging or production. What a local environment still needs is
  now a short numbered list on the Overview; `enter` runs the selected step
  with the same handler as its command. An environment that cannot be read at
  all opens with the reason and empty views, and reloads when it answers.
- Bare `mm` registers this machine as the `local` target when it is missing,
  as `mm init` and `mm dev setup` do. Existing targets are not changed.
- Leaving the terminal UI exits with status 0. It used to exit with the status
  of the environment it opened on.
- An environment marked `not set up` in the Environments view opens on its
  setup steps instead of refusing to open.
- `mm doctor` prints passed checks as one count and ends with the steps to
  take, each once, with its command. JSON output is unchanged.
- `mm service logs --tail N` without `--follow`, and the `mm_logs` MCP tool,
  return the last N lines of the service in time order. A runtime applies the
  limit to each container or Swarm task, stopped ones included, so a service
  with several returned more lines than asked for, grouped by task.
- Log and job output redaction no longer replaces a short secret value inside
  unrelated text: a secret `null` turned `/dev/null` into `/dev/[REDACTED]`.
  Values of eight characters or more are replaced everywhere as before.
  Shorter values are replaced where they stand alone, and values under four
  characters or ordinary words (`null`, `true`, ...) are not replaced.
- The terminal UI's Environment view is now called Variables, and the views
  are renumbered: Environments is `2`, Services `3`, Ingress `4`, Variables
  `5`, Mounts `6`, Diagnostics `7`, TLS `8`, Access `9`, and Gates `0`.
- In the terminal UI's Jobs view, `space` enables or disables a schedule;
  `e` now edits it.
- On a remote environment the terminal UI refreshes every 10 seconds instead
  of every 3, shows the target and your access in place of the local control
  plane, and labels `u` as deploy.
- The website is a new Nuxt app in `site/` with a home page and the
  documentation under `/docs`. The documentation itself is now plain Markdown
  in `docs/`, and generated schemas live in `site/public/schema` (still served
  at `/schema`). Old documentation addresses redirect to their place under
  `/docs`.
- The local dashboard has a new design that follows the terminal interface:
  a health summary, a services table with replicas and routes, and the control
  plane and project details on every view. It shares its components with the
  website through `packages/ui`.
- Install instructions lead with the one-line release installer;
  `./install.sh` remains for building from a checkout.
- `mm` refuses to run directly for an AI agent. When a coding tool's
  environment variable is set (such as `CLAUDECODE`, `OPENCODE`, `CODEX_CI`,
  `CURSOR_AGENT`, `GEMINI_CLI`, `AI_AGENT`, `AGENT`, or `MM_AGENT`), every
  command except `mm mcp serve` exits with code 3. Run `mm` from your own
  terminal.
- The MCP server takes its limits from the project's `agents:` block. The
  private `mcp.json` is now optional, uses `schema_version` 1, and can only
  narrow the block with `ceiling`. Tool and resource contracts are
  `mm-mcp-tools-v1` and `mm-mcp-resources-v1`: a
  `scope` needs only `environment`, and plans report `applied_by`.
- The MCP server reaches remote environments only with the agent key. It no
  longer uses `MM_SSH_KEY`, a target's key file, or the device key.
- Agents can no longer apply a change above their level by confirming it
  themselves, and the users file cannot be edited through MCP.
- Remote access no longer uses per-developer Unix accounts or
  `/etc/mm/access.yaml`. `mmd` reads `/etc/mm/host.yaml` (enrolled projects,
  ceilings, break-glass keys) and `mmd.json` schema 2, and refuses the old
  files with a pointer to the migration steps.
- `mm secret generate --missing` creates only the generated secrets the
  selected environment uses: those its bindings name, its services mount, and
  its registries, TLS, tunnel and deploy key read. `--all` takes every secret
  with a `generate:` block, as `--missing` did before. A secret whose provider
  has no connection from this machine is listed as skipped with the reason,
  in the JSON output too, and the others are still created.
- `mm build` builds only the modules the selected environment runs, and the
  first-party images they build FROM; `mm build --all` builds every module,
  as before. A build receipt promotes to an environment that runs fewer
  modules than the one it was built for.
- A deployment plan needs only the `release-inputs:` that a template of the
  planned environment reads, and names every missing one with the template
  line that reads it, so a development environment plans without the values
  CI supplies for staging, from the command line and through MCP alike.
- A failed MCP call returns the message the command line prints for its
  error, redacted as the command log is, with the command line's help in
  `next_action`; what another program printed stays out, and a failed
  program is named with its exit code. `mm_read` `health` returns the
  findings of `mm config validate` when the environment's status cannot be
  read, and `MM_CONFIG_INVALID` names its first three findings.

### Added

- `mm update` updates `mm` from the latest stable release, or the one
  `--version` names, and `mmd` and `mm-ingress-helper` when they sit beside
  it. It checks the archive against the release's `manifest.json` before it
  unpacks anything, runs the new `mm`, and replaces every binary or none.
  `mm update --check` only reports, and the update check now says to run
  `mm update`. See [update mm](docs/start/install.md#update-mm).
- `mm target setup` reads each host's CPU while it plans and installs `mm`
  and `mmd` built for it: from the directory of the `mm` you run when it
  holds them, otherwise from this `mm`'s release, downloaded, checked against
  its manifest, and kept under the mm home. Setup works from macOS and for
  clusters whose nodes have different CPUs, and no longer needs the
  installer's `--with-daemon`. The plan names each host's source and the
  digests; `--host-binaries` still overrides, and binaries built for macOS,
  for two CPUs, or for another CPU than the host are refused before anything
  is installed. See
  [where setup takes mm and mmd from](docs/how-to/upgrade.md#where-setup-takes-mm-and-mmd-from).
- `mm workspace list` shows the project folders mm has run in on this machine,
  their project ID, when each was last used, and which are gone.
  `mm workspace forget --stale` (or `ROOT|ID`) forgets them through a reviewed
  plan: their operation history and `mm env set` values go, their files stay.
- A service may run on its host's network (`network_mode: host`), as a VPN
  or DNS server needs. On a remote host or a cluster node the host policy
  decides: `allow` with `features: [network-host]` for that `stack/service`.
  On your own machine the service says why with `host-network: <why>` under
  `modules.<module>.services.<service>`, and the plan shows the reason.
  Such a service has no routes, joins no network, and keeps the host's user
  namespace. See
  [run a service on the host's network](docs/remote-access.md#run-a-service-on-the-hosts-network).
- `mm_setup_plan` and `mm_setup_apply` let an agent set up this machine within
  its grant, through the services the command line uses: the shared local
  services (`mm dev setup --services`), a managed secret provider's install,
  the local age store (`mm secret init age`), and missing generated secrets
  (`mm secret generate`). The secret steps need `secrets: true` with level
  `deploy` on an environment that runs here; trusting the local certificate
  authority and `mm agent trust` stay the user's.
- Releases are published by the release workflow when a `vX.Y.Z` tag is
  pushed to `master`: the Linux x86-64 and ARM64 archives are built from the
  tag in pinned containers by `scripts/release/build-linux.sh` (which also
  reproduces them on any machine with Docker), the macOS archives on GitHub's
  macOS runners, and the GitHub release carries them with the source archive,
  `SHA256SUMS`, and a build provenance attestation for each file
  (`gh attestation verify`). A `vX.Y.Z-rc.N` tag publishes the same as a
  prerelease. The notes must start with the remote protocol the release
  speaks. The maintainer may add an SSH signature over `SHA256SUMS`
  (`scripts/release/sign.sh`). See [publish a release](docs/contribute/releases.md).
- Linux release archives carry `mm-ingress-helper` beside `mm` and `mmd`, and
  the installer puts it beside `mm`, so local HTTPS ingress (`mm ingress
  start`) works from a release install, not only from a checkout. When the
  helper is missing, the error says how to get it from a release.
- `development: on | optional | off` on a module, service or job keeps it
  off in local development until a developer runs `mm dev enable UNIT`
  (kept in the gitignored local overrides) or `mm dev up --with UNIT` (one
  run). What it needs through Compose `depends_on` comes on with it; its
  routes, jobs and schedules follow it; `mm dev disable` stops it and keeps
  its volumes. The plan lists it as `off locally (optional)`, the terminal
  UI turns it on and off with `o`, the dashboard greys it out, and agents
  with `deploy` on the development environment use `mm_dev_unit`.
- An environment on a cluster target can take its routes through a
  Cloudflare Tunnel with no port open on any node: `ingress.type:
  cloudflared` and `ingress.cloudflared.token` (a secret). Every ingress
  node runs a cloudflared connector next to its proxy, configured from the
  routes, with the token in a file only the connector reads; connectors
  change one node at a time, each once it is connected. `mm deploy plan`
  names the tunnel and each node's hosts, and `mm service status` and
  `mm doctor` show each connector's connections (`MM_TUNNEL_NOT_CONNECTED`,
  `MM_TUNNEL_TOKEN_MISSING`). See
  [Serve a cluster through Cloudflare Tunnel](docs/how-to/cloudflare-tunnel.md).
- Routes on a cluster target set response headers (`headers:` on a route,
  `ingress.headers` on an environment), let only the requests an `allow:`
  list names through (methods and a path pattern; the proxy answers 404 to
  the rest), and take a `priority:` among the routes of their host (by
  default the longer path goes first). The plan lists a host's routes in the
  order the proxy tries them, with what each lets through.
- A cluster environment can keep its deploy secrets in an OpenBao store mm
  runs on its nodes: `mode: managed` with `scope: environment` under
  `providers:`. `mm target setup` installs the pinned, checksum-verified
  release as a systemd unit on 3 or 5 members, joins them over the mesh with
  TLS from a per-environment authority whose key stays on your machine,
  initialises the store, and revokes the root token; each node's `mmd`
  unseals its member on boot. `mm secret set|generate|rotate|status` stage
  values as with the age store, and each becomes a new version once its
  deployment succeeded. `mm secret provider recovery export|import` hand the
  recovery copy over, `mm secret provider snapshot` and `restore` take the
  store to a file and back onto fresh nodes, and `mm secret provider status`,
  `mm target nodes`, `mm doctor` and MCP's read `secret_store` show each
  member's seal, Raft role and release
  (`MM_SECRET_STORE_SEALED`, `MM_SECRET_STORE_DEGRADED`,
  `MM_SECRET_STORE_VERSION_SKEW`). `mm secret migrate --from age --to NAME`
  moves the age store's values in, keeping their revisions, and setup
  upgrades the store (OpenBao 2.4 or 2.5) one member at a time, the leader
  last. A reinstalled node's member rejoins through setup: the plan shows it
  as `rejoins`, and the apply removes its old Raft entry through a serving
  member before it joins again; a member that is only down is never removed.
  The remote protocol is now 19. See
  [Keep deploy secrets in a managed store](docs/how-to/managed-secret-store.md).
- `mm -e ENV secret set KEY` and `mm -e ENV env set KEY` on a remote
  environment stage the value on every host through `mmd`, as your own
  principal: sealed with age to each host's store, all hosts or none, and
  only for a secret the deployed commit declares or a variable its
  templates use without a value in `.config/mm.yaml`. Running services keep
  their values until a deployment makes the change live; `unset` stages a
  removal and `discard` drops a staged change. `--local` writes this
  machine's store, which setup pushes, as these commands did before. The users file's `secrets:
  true` grants it per environment (admin includes it; an agent only when
  the `agents:` block says `secrets: true` too). `secret status` shows each
  host's live and staged revision and who changed it, never a value, as do
  the terminal UI's Variables view and MCP `mm_read` kind `secrets`.
  `mm deploy plan` lists the staged changes it makes live and refuses a
  host that would deploy without a required value; `mm deploy redeploy`
  (MCP: `mm_deploy_plan` with `redeploy`) plans the running commit again.
  Every stage is in each host's audit log. See
  [Change an environment's variable](docs/how-to/change-a-variable.md).
- MCP `mm_stage_value` stages a secret or variable for an agent whose
  grant has `secrets: true`; the value is sealed to the hosts and never
  returned.
- `mm target setup` refuses to replace the `mm` and `mmd` a node runs with
  another build of the same release or an older release unless
  `--upgrade` is given or `deploy.mm-version` pins the release; the plan
  names the build each node runs and the one it would get.
- A build target under `modules.<m>.build.targets` can be a mapping with
  `target` (the Dockerfile stage), `suffix` and `args`; its arguments merge
  over the module's, so two environments build two images from one stage
  with different public values. `mm build plan` and receipts list each
  variant's arguments. The string form still works.
- On a cluster, each node restarts its containers whose health check has
  failed for 3 minutes, one at a time as `mm service restart` does, never
  during a deployment, at most 5 times an hour; past that it keeps the
  container out of the names and routes while another can serve, and
  `mm service status` and `mm doctor` report `MM_SERVICE_RESTART_LIMIT`.
  `health:` on a service changes the times or turns it off; each restart is
  in the audit log, the status, and `mm service logs`.
- `ingress.balance: spread` on an environment, or `balance:` on one route,
  sends a cluster's ingress requests to the service's containers on every
  node instead of the ingress node's own first; a route to an autoscaled
  service spreads by default. Every cluster route now sends each request to
  the less busy of two containers, so a slow container gets fewer.
- A service with `role: job`, which changed nothing before, is now only the
  container its jobs run in: a deployment starts none of it, so a job can
  hold secrets no running container holds. Plans and `mm service status`
  list it as `job only`. See
  [Give jobs a service of their own](docs/jobs-and-schedules.md#give-jobs-a-service-of-their-own).
- `environments:` on a `secret-files` or `config-files` entry mounts the file
  only in those environments; elsewhere its secret is not required.
- Release facts: templates and bindings read `MM.COMMIT`,
  `MM.COMMIT_SHORT`, `MM.RELEASE`, `MM.IMAGE.<module>` and
  `MM.IMAGE_DIGEST.<module>`, and values declared under `release-inputs:`
  as `RELEASE.<NAME>`, given with `mm deploy plan --release NAME
  --release-input NAME=VALUE` (and `mm_deploy_plan`). They are part of the
  plan, recorded per node, shown by `mm deploy status`, restored by
  `mm deploy rollback` and kept by `mm deploy redeploy`; a missing or
  malformed input fails the plan with
  `MM_RELEASE_INPUT`.
- `mm history` lists the commands run in a project on this machine and
  `mm history show ID` prints what one printed, as plain text; agents read
  the same records with `mm_read` kind `commands`, and each change an agent
  makes through MCP is listed with its audit entry. Records stay under the
  mm home, with secret-bearing arguments and private keys redacted
  (`MM_NO_HISTORY=1` leaves a command out).
- `deploy.mm-version` on an environment pins the release of mm that may plan
  its deployments and of `mmd` on its hosts (`"0.9"`, `"=0.9.3"`). Plans from
  another mm, and for hosts that run another `mmd`, are refused, and
  `mm doctor` reports `MM_VERSION_PIN`. Without a pin, `mm doctor` warns when
  hosts run an older or newer release than mm (`MM_HOST_RELEASE_OLDER`,
  `MM_HOST_RELEASE_NEWER`).
- `mmd state export` and `mmd state import` back up and restore a host's own
  records: the host policy, the node's name, access, blocks, deployment
  history, cluster network addresses, and the audit log. An export never
  holds a key, a secret value, or a passphrase.
- A production checklist in the docs, with the test or release file behind
  each item.
- `delivery: push` on a target: mm sends each commit's tracked files to the
  hosts over its own connection, and the hosts never contact the git server
  and need no origin or deploy key. A host keeps a bundle only when its
  files hash to the commit's tree, the plan and the host's audit log name the
  bundle's hash, and on a cluster every node of a plan is sent it.
  `mm target setup --delivery push` (one host) and a `nodes:` target with
  `delivery: push` set a host up for it. The default stays `fetch`. With
  `push` a host cannot check which branch of the origin a commit is on:
  `deploy-from` is held against the branches the client states.
- `hardening: strict` at the top of `mm.yaml` gives every deployed service a
  read-only root filesystem, no Linux capabilities, no new privileges, the
  engine's default seccomp profile, and a tmpfs at `/tmp`. The plan lists
  what each service runs with. A service leaves one default out under
  `hardening: {except: {read-only: "<reason>"}}`, the plan prints the reason,
  and an exception without one fails validation (`MM_HARDENING_REASON`).
  `mm dev` is not hardened. See [harden services](docs/hardening.md).
- A remote host says which OCI runtimes it offers (`runtimes: [runsc]` in
  `/etc/mm/host.yaml`) and can run every service that publishes a port or
  takes ingress traffic under one of them (`exposed-runtime: runsc`), which
  the plan shows. `mm doctor` runs a container under each runtime a host
  offers and reports one that does not work as `MM_RUNTIME_NOT_WORKING`. See
  [choose an OCI runtime](docs/hardening.md#choose-an-oci-runtime).
- Remote hosts and cluster nodes can run rootful Podman: an environment with
  `runtime: {driver: podman-compose}` on a remote target is set up with
  Podman 5, Docker Compose as its Compose provider, and Podman's API socket,
  and everything a cluster does works there: networks across nodes, names,
  rolling deploys, shared volumes, status, logs, and exec. Every service
  gets 65536 user ids of its own, the same on every node and in every
  deployment, from a range setup reserves (`mm-services` in `/etc/subuid`
  and `/etc/subgid`): its volumes keep working across recreates and nodes,
  and its config files and `secret-files` belong to the container's user.
  A service keeps the host's ids with `host-user-namespace: <why>`, which
  every service that mounts a volume another one mounts needs
  (`MM_USER_NAMESPACE`). `update.order: start-first` works on Podman too:
  the new container has the service's ids and keeps its volumes. `mm doctor` reports a host
  whose Podman is older than 5.0 (`MM_PODMAN_VERSION`), whose `podman
  compose` does not run Docker Compose (`MM_PODMAN_COMPOSE`), or whose
  engine does not answer (`MM_NODE_ENGINE`), and `mm target nodes --output
  json` lists each node's `engine`, as the terminal UI's Environments view
  does. The firewall chains that keep the mesh between the hosts and `mm
  doctor`'s check of a host's OCI runtimes work on Podman nodes too.
  Projects made before need the new `.config/ansible/bootstrap.yml` from
  `mm init`.
- `jobs.<name>.run` makes a deployment run a job by itself: `before-deploy`
  (after what its service depends on is healthy, before the service is
  started or replaced; a failure stops the deployment with the old version
  running), `after-deploy` (once the deployment converged), or `once` (the
  first time the environment is deployed with it, under `mmd`, recorded on
  every node). `jobs.<name>.environments` lists the environments a job
  belongs to (`environment:` still reads). `mm deploy plan` lists the jobs a
  deployment runs and whether a `once` job is due; a rollback plan lists the
  jobs that ran since the release it returns to. `mmd` runs the schedules a
  deployment reconciled on its host, and moves on the runs nobody waits for.
- Jobs on cluster targets: a job runs on one node, the first node that
  runs its service or the node (or group) `jobs.<name>.placement` names,
  and joins the service's networks there; its schedule runs on that node
  only. `jobs.<name>.user` runs the command as another user. A job a
  deployment runs before it replaces a service stops the deployment when it
  fails, with the job's last output in the error; `mm deploy logs` prints
  the output of the jobs the last deployment ran. A job container a cut-off
  run left behind is removed by the next deployment.
- `mm service restart` and `mm job run` work on remote targets, for a user
  at the `operate` level or higher. A restart waits until the service is
  healthy again; on a cluster target it restarts one node at a time. A job
  runs in a fresh container with what the last deployment gave its service.
  `mm job status`, `mm job history`, and `mm job retry` read and repeat
  runs on the hosts. Every restart and run is in the host's audit log.
- Ingress on cluster targets. A stack's `ingress:` routes work as written:
  the first node (or the nodes under `cluster.ingress`) runs Traefik on ports
  80 and 443 and sends each request to a healthy container of the service on
  its own node, else on any node. The node's `mmd` writes what each route
  sends to from its service map, so a container that starts, fails its
  health check, or leaves changes the routes at once, without a proxy
  restart; the proxy mounts no Docker socket. HTTPS uses Let's Encrypt, or a
  certificate the environment provides with `environments.<env>.tls`
  (two secrets). The deployment plan shows the routes and the nodes.
- Autoscale on cluster targets: `autoscale: {scale: {max: 4}}` on a service
  keeps it between `scale.min` (default: its `replicas`) and `scale.max`
  containers, one added when its average CPU has been above 75% for 30 s and
  one removed when it has been below 40% for 5 m (`metric:` overrides each,
  `type: memory` too). `mmd` measures, decides on the first node of the
  placement that answers, and starts or drains one container at a time; a
  deployment keeps the count. `mm service status` shows the count, the last
  change, and its reason. `replicas: N` on a service runs N containers over
  its placement's nodes. Docker Engine and Podman hosts alike. Validation
  finding `MM_SCALE`.
- `mm service restart` on a cluster target drops no request: each node
  restarts its containers one at a time, and one that another container can
  stand in for leaves first (out of every node's answers and ingress, its
  `drain`), restarts, and is answered again once healthy.
- A container that leaves a cluster node (an update replaces it while the
  service runs on other nodes, its service moves to another node, autoscale
  removes it, or `mmd container leave` takes it out; with `--restart` it
  restarts in place and is answered again once healthy) drops
  out of every node's name answers and ingress first, waits until every node
  confirms, serves what it has for its service's `drain` (default `1m`), and
  then gets its stop signal and `stop_grace_period`. A leave survives an
  `mmd` restart, and `mm service status` shows the container as `leaving`.
  This replaces the eight seconds a `start-first` update waited.
- `mm deploy cancel [OPERATION]` stops what runs on a remote environment, ends
  its child processes on every host, returns the hosts a deployment changed to
  what they ran before, then frees the environment. A job run, and the jobs of
  a cancelled deployment, stop with it and are recorded as cancelled; a
  restart and a push upload stop too, and a container the deployment had begun
  to drain is routed to again. Anyone who may deploy there may cancel; the
  audit log names who started it and who cancelled it. `mm deploy apply`
  prints the operation's ID first and a line every 15 seconds while a step
  runs, and stops with `MM_TIMEOUT` naming the host, the step, and the
  operation when a host shows no progress for `deploy.progress-timeout`
  (default `2m`); a long step whose host is at work, such as a migration or a
  drain, is never stopped by it. `mm deploy status` and the MCP read
  `deployments` list what runs on each host, with who and since when, and `mm
  doctor` reports per host how its `mmd` fares, operations in flight or stuck,
  child processes past their deadline, and services that have not converged.
- `mm key rotate` keeps the device's access: the new key is made beside the
  current one, mm offers both, `mm team join` puts the new key in the entry
  in place of the current one, and `mm key rotate --finish` drops the old key
  once the hosts were synced. A sole admin can rotate.
- `mm service logs` reads a service's containers on every node of a cluster
  target as one stream, in the order the engines recorded the lines. Without
  a service name it reads every service of the environment, and
  `--node NODE` (repeatable) reads only the named nodes. `--follow` picks up
  containers that start later, so a deployment does not end the stream.
  `--timestamps` shows the engine's timestamp of each line. A node that does
  not answer is one line in the stream marked with `!`, and the command
  fails only when no node answers. Lines are ordered by each node's own
  clock, so keep the nodes' clocks synchronised.
- `mm service status` on a remote environment prints the deployment the
  containers were started from (`deployed:` with the commit, time, who
  applied it, the plan, and the build receipt), one line for each node when
  they run different releases. Below the table it says what differs on a
  node from its last deployment: a service deployed there that has no
  container, or a service running there that the deployed configuration
  places on other nodes. JSON has `deployment` and `drift`, also for each
  entry of `nodes`.
- A node that says nothing is waited for until five seconds after the first
  node answered a status or a log read, and is then listed as not answering
  while the other nodes' answers are shown.
- `mm target nodes` prints the cluster's address table under the nodes: each
  network's block and each node's slice of it, and the addresses and routes
  every node already uses. `--output json` has them under `addresses`, and
  the MCP read `nodes` returns the same.
- `MM_ENGINE_API=off`, in the environment of `mm` or of `mmd` on a host,
  makes mm read status and logs through the `docker` command instead of the
  Docker Engine API. The API is used only when the `docker` command would
  talk to the local socket: no `DOCKER_HOST` other than `unix://...` and no
  Docker context other than the default. Podman is asked through its
  command as before.

- A remote host or a cluster can read an environment's secrets from an
  external OpenBao or Vault itself, so their values never pass through the
  machine that deploys. `mmd secrets connect` gives the host its own AppRole
  credential, stored encrypted for root only; `targets.<name>.providers`
  declares where the target's hosts reach the store. A node asks the store
  only for the secrets of its own services, and rotating a secret in the
  store shows as a new version in the next plan.
- File secrets rotate on Docker Compose. Each version of a secret is one
  file, kept for the user mm runs as and named by a keyed digest of the
  store's version. A new version replaces only the services that mount the
  secret, the previous one stays until the deployment has succeeded, and a
  deployment that fails mounts it again. Before, a changed file secret was
  refused when the plan was applied. The first deployment after upgrading
  replaces the services that mount a file secret once.
- `mmd` keeps the last four hours of CPU and memory of the containers on its
  node in memory, sampled every five seconds from cgroup v2 files. No command
  shows them yet.
- `config-files` on a service ships a file of the project, or a template
  rendered from the environment's bindings, to every host that runs the
  service and mounts it read-only. A changed file or binding replaces only
  the services that mount it, the plan says which, and superseded versions
  are removed once the deployment has finished.
- Shared volumes on cluster targets. List a stack's Compose volumes under
  `shared-volumes` and every node that runs a service on one sees the same
  directory: the target's storage node (the first node, or `cluster.storage`)
  keeps the files in `/var/lib/mm/storage` and the other nodes mount them
  over the WireGuard mesh, with no package and nothing to configure. The plan
  shows which node keeps each volume and which nodes mount it, and a
  deployment creates the directory on the storage node before it replaces a
  container. Shared volumes are for files that replicas share, such as
  uploads: they are slower than local disk and do not give the locking
  guarantees a database needs, and the storage node is a single point of failure that mm neither
  replicates nor backs up. A service with `role: database` or `role: queue`
  that mounts one fails validation (`MM_SHARED_VOLUME`) unless it sets
  `allow-shared-volumes: true`. See
  [shared volumes across nodes](docs/remote-daemon.md#shared-volumes-across-nodes).
- `mm target storage status` shows the storage node, each shared volume with
  its size and the nodes that have it mounted, and whether every node reaches
  the storage node; the MCP read `storage`, `mm doctor`, and the terminal UI's
  node list report the same. `mm target storage delete VOLUME` deletes a
  volume's files after showing what it removes, and is the only thing that
  does: taking a volume out of `shared-volumes`, a rollback, and a failed
  deployment keep them.
- A node that starts while the storage node is away starts its other services
  and brings up the ones that mount a shared volume once the storage node
  answers. A deployment that needs a shared volume is refused with
  `MM_STORAGE_UNREACHABLE` before it changes anything when a node cannot
  reach the storage node.
- Containers cannot reach the storage node's NFS port, on their own node or
  through the mesh, and the storage node takes storage traffic from the other
  nodes' mesh addresses only (`mmd storage up`, chains `MM-STORAGE` and
  `MM-STORAGE-IN`). `mm target setup` loads the rules on every node and has
  every node prove that it can mount.
- Changing `cluster.storage` while the old storage node holds files is
  refused by `mm target setup plan` with the steps to move them by hand. A
  target with `cluster.mesh.interface` serves no shared volumes.
- `mm key protect`, `mm key rotate` and `mm key remove`, each a reviewed step
  (`--plan HASH --yes`). `protect` encrypts a plain key made by an earlier
  `mm` in place and keeps its public key, so no host access changes; `rotate`
  replaces a key and its passphrase; `remove` deletes both. `mm key show`
  says how a key is protected, and `mm doctor` reports a plain key
  (`MM_KEY_UNPROTECTED`), a key whose passphrase the vault no longer has
  (`MM_KEY_PASSPHRASE_MISSING`), and a passphrase kept in a file
  (`MM_KEY_PASSPHRASE_IN_FILE`), each with the command that fixes it.
  `MM_KEY_VAULT` chooses the vault. See
  [remote access](docs/remote-access.md#keys-are-encrypted-and-only-mm-uses-them).
- `mm target connections` lists the SSH connections this device keeps open
  to remote hosts, and `mm target connections --close` closes them.
- `MM_TIMING=1` prints how long each phase of a command took (configuration,
  each connection, each request, each engine command), for finding what is
  slow. `mmd` prints the same per request when the variable is in its
  environment. See [CONTRIBUTING.md](CONTRIBUTING.md#keep-it-fast).
- Rolling deploys and rollback across cluster nodes, with no configuration:
  stacks roll out in name order and each stack one node at a time, a node is
  done when its containers are healthy and every other node resolves them, and
  when a node fails, every node the deployment changed returns to what it ran
  before while the nodes it had not reached are never touched. The error names
  the node, the service, and the container's last log lines. Optional
  overrides: `update: {order, parallelism, failure}` on a service
  (`start-first` starts the new container next to the old one and stops the
  old one once the new one is healthy and resolved everywhere), `after:` on a
  stack, and `environments.<env>.deploy.convergence-timeout` (default `10m`),
  which the plan shows.
- `mm deploy rollback` plans and applies a return to what the hosts ran before
  the last deployment, or before a rollout that was left unfinished. The hosts
  plan the earlier commit themselves, so the checkout does not need it.
- `mm deploy status` shows, per host, the last finished deployment, the one
  before, and a rollout that is running or was left unfinished. The MCP read
  `deployments` returns the same, and `mm target nodes`, `mm doctor`
  (`MM_ROLLOUT_UNFINISHED`), and the terminal UI's node list say when a node
  holds one.
- One deployment at a time per environment across a cluster: a second
  `mm deploy apply` while one runs is refused with `MM_DEPLOY_RUNNING`, who
  started the running one, and when.
- A cluster node that reboots runs its services again without a deployment: a
  service whose Compose files set no `restart:` policy gets
  `restart: unless-stopped` on cluster targets, and the plan lists those
  services. The first deployment after an upgrade recreates their containers
  once.
- Deploying to a cluster of Compose hosts. On a target that lists `nodes:`,
  `mm deploy plan` asks every node's `mmd` to plan at the same time and prints
  one plan: a section per node and one plan hash over all of them.
  `mm deploy apply --plan HASH` applies exactly that and reports what each
  node did. A node refuses a plan the hash does not cover.
- Networks that span cluster nodes. On a target that lists nodes, every
  network under `networks:` in `mm.yaml` reaches all nodes that run a service
  on it, with nothing to turn on: two services on different nodes that name
  the same network reach each other by address, and containers that share no
  network cannot, on one node or across nodes. Traffic between nodes stays
  inside the WireGuard mesh and keeps the containers' addresses; nothing
  outside the mesh reaches these networks. Each network takes a /20 of
  `cluster.networks` and each node a /24 of it: 16 nodes per network, room for
  253 containers of a network on each node, and 256 networks in the default
  `10.64.0.0/12`. An address never changes once given, and several projects on
  the same hosts never collide. `mm deploy plan` shows the addresses, the
  routes, and the firewall rules it adds or removes; mm's rules live in the
  iptables chains `MM-CLUSTER` and `MM-CLUSTER-NAT`. `mm-network.service`
  restores them after a reboot and when the engine restarts. `mm service
  status` and `mm doctor` report a node that drifted (`MM_NETWORK_DRIFT`), and
  the next deployment repairs it.
- Service names across cluster nodes. On a network that spans the nodes, a
  container looks up `<service>` and gets the service's healthy containers on
  its own node, or, when its node has none, on every node: a request stays on
  the node when the service runs there, and moves to another node with the
  next lookup when the local container turns unhealthy. `tasks.<service>` is
  every healthy container on every node, the same answer everywhere;
  `<service>.<node>` is one node's container, and a network alias resolves
  like its service's name. Healthy means running and passing the health check, or running
  when there is none: a container that turns unhealthy, and a node that stops
  answering, leave the answers and come back by themselves. Nothing is
  configured. Each node's `mmd` answers its own node's containers on the
  node's address on each network (port 53), learns the other nodes'
  containers over the mesh (TCP port 7953 on the mesh address, open to the
  cluster's nodes only), and passes every other name to the resolvers in the
  node's `/etc/resolv.conf`. The first deployment after upgrading restarts
  the containers of services on these networks once, to give them the node's
  resolver.
- `mm service status` lists what each service name resolves to on each network
  of a cluster target, which nodes answer the name with their own containers
  when the answers differ between nodes, the containers left out of an answer
  because they are not healthy, and what keeps a node from answering names.
  The same is in `--output json` under `names` (each node's answer under
  `lookups`), in a service's details in the terminal UI, and in `mm_read` with
  kind `names`.
- `dns`, `dns_search`, and `dns_opt` in a Compose file fail the plan with
  `MM_COMPOSE_DNS` for a service on a network that spans cluster nodes, where
  they would have no effect.
- A network that an earlier deployment created local to a host moves onto its
  cluster addresses at the next deployment: the plan says the network is
  created again and names the services whose containers restart.
- `mm target setup plan` and `mm deploy plan` refuse a `cluster.mesh.subnet`
  or `cluster.networks` that overlaps an address or a route a node already
  has, such as a provider's private network. The error names the node, the
  route, and the key to set, and offers a range that is free on every node.
  A `cluster.networks` too small for one network fails `mm config validate`
  with how many networks each size holds.
- `internal: true` on a network in `mm.yaml`: its containers reach the members
  of the network and nothing else.
- `mmd network show|up|leave` on a host: the addresses it stored and what
  differs from them, bringing the host to them, and removing mm's firewall
  chains.
- Placing services on cluster nodes. A service's `placement` (a node, a group
  from `cluster.groups`, `all`, or a list) names the nodes that each run one
  container of it; without `placement` a service runs on the first node. Every
  node runs one Compose project per stack with only its own services, and a
  network, volume, secret, or config of the Compose file exists only on the
  nodes that use it. The plan lists the services of each node, and says when a
  `depends_on` was dropped because the dependency runs on another node. A node
  that no service is placed on takes the access change, records the commit,
  and runs nothing.
- Moving a service between nodes in one plan. Change its `placement`: the plan
  says "starts on app-2, then stops on app-1", apply starts the service on its
  new node while it still runs on the old one, and stops it there as the last
  step. Volumes stay on the node they are on. `--nodes` cannot leave out a
  node a service moves to.
- On a cluster target, Compose keys that only Docker Swarm reads
  (`deploy.mode`, `deploy.placement`, `deploy.endpoint_mode`, and
  `deploy.replicas` above 1) fail the plan with `MM_COMPOSE_DEPLOY_KEY` and what
  to write in `mm.yaml` instead. A Compose service without an entry under
  `modules` fails with `MM_PLACEMENT`. An environment with one host reads the
  same Compose file as before.
- A relative bind mount works on every node of a cluster: each node mounts the
  path from its own checkout of the deployed commit.
- `--nodes NODE,...` on `mm deploy`, `mm deploy plan`, and `mm team sync` plans
  for some nodes of a cluster while another cannot be reached. The plan lists
  the nodes it leaves out; they keep what they run.
- `--nodes NODE,...` on `mm target setup plan` and `apply` sets up some nodes
  of a cluster, for example to roll out a new `mm` while one node is down or
  presents an SSH host key that is not trusted yet. Setup does not contact the
  others; the mesh keeps their existing peers, and the storage step is skipped
  when the storage node is left out.
- `mm target nodes` lists every node of the selected environment's target:
  whether it answers and why not, its `mmd` version, its mesh address, and
  the commit and time of its last applied deployment. It exits with status 3
  when a node is not ready and has a JSON form. The terminal UI shows the
  nodes in the Environments view and the MCP server returns them from
  `mm_read` with kind `nodes`.
- The setup plan of a cluster shows the mesh: every node's address, endpoint,
  and public key, and each peer entry it adds (`+`), changes (`~`), removes
  (`-`), or leaves alone (`=`). The plan hash covers it. Adding or removing a
  node changes one entry on every other node; a peer that does not change
  keeps its session. A removed node is named with the command that takes the
  mesh off it (`mmd mesh leave`).
- The WireGuard private key of a node is made on that node and never leaves
  it. It is in no plan, log, or command line; mm reads public keys only.
- `mm target mesh status` shows every node's mesh interface and, per peer, the
  endpoint, the last handshake, and the bytes received and sent, and lists
  what is wrong with what to do: for two nodes without a handshake, the UDP
  port that must be open between their two addresses. It exits with status 3
  when a node does not answer or the mesh has a problem, has a JSON form, and
  needs `view` access. `mm doctor` reports the same problems, the terminal UI
  shows each node's connected peers, and the MCP server returns the report
  from `mm_read` with kind `mesh`.
- `mm target mesh rotate NODE` replaces one node's WireGuard key pair as a
  reviewed plan: the node makes the new key, and every other node replaces
  its one entry for that node. The other nodes keep their keys and sessions.
- The mesh uses one port: UDP 51820 (`cluster.mesh.port`) must be reachable
  between every pair of nodes. Setup says which two hosts when it is not.
- `cluster.mesh.mtu` (default 1420) sets the MTU of the mesh interface and is
  shown in the setup plan. Setup sends one packet of that size between every
  pair of nodes and stops with the setting to lower when the path drops it,
  which is the failure where small requests work and large responses stall.
  Networks between nodes are routed over the mesh without another
  encapsulation.
- `endpoint` on a node of a cluster target: the address the other nodes send
  WireGuard packets to when it is not `host`.
- `mm service status` on a cluster asks every node and labels each row with
  its node; a node that does not answer is listed with the reason.
  `mm service logs`, `mm service exec`, and `mm forward` go to the node that
  runs the service. `mm team sync` plans and applies on every node under one
  hash.
- `mm doctor` asks every node of a cluster environment and reports nodes that
  do not answer, are not set up, or answer under another node's name, and
  nodes that differ in `mmd` version or deployed commit.
- An environment whose target lists `nodes:` uses the `docker-compose` driver
  unless a driver is configured.
- An Environments view in the terminal UI: every configured environment with
  where it runs, its services, and its deployed commit. `enter` switches the
  terminal UI to another environment without leaving it.
- Service actions in the terminal UI. `enter` on a service or one of its
  containers opens a menu: follow logs (with pause, filter, wrap, and
  timestamps), run a command and see its output and exit code, restart, and
  open its variables. Each action also has its own key.
- Editing in the terminal UI: `a` adds, `e` edits, and `D` removes
  environments, jobs, schedules, and configuration values. Each change shows a
  short form and then the exact diff before it writes the file, like
  `mm config`.
- Teammate onboarding in the terminal UI's Access view: `a` adds a teammate
  with their public key and a level per remote environment, `e` changes a
  level, `D` removes them, and `j` requests access for this device. After
  saving, it lists the commit, merge, and deploy steps that make it effective.
- `mm service restart SERVICE` restarts a service's containers on a local
  target. It used to report that the runtime had no restart.
- A Terminal UI page in the documentation.

- Per-environment limits for AI agents. A new `agents:` block in the project
  configuration sets what an agent may do through `mm mcp serve` in each
  environment: `view`, `operate`, or `deploy`, plus `exec` for all or named
  services. The default is `view`, and `mm init` grants `deploy` and `exec` in
  the development environment only. An action outside the grant returns
  `MM_AGENT_HANDOFF` with the exact command for the user to run.
- `mm_access` and `mm_exec` MCP tools. `mm_exec` runs an exact argv in a
  service container, without a shell, TTY, or stdin.
- `mm agent status`, `mm agent trust`, and `mm agent setup`. The block applies
  on a machine only after `mm agent trust`; an edited block grants `view`
  until it is trusted again. `mm agent setup` connects Claude Code, OpenCode,
  Codex, and Cursor to the MCP server, for a project or with `--user`.
- Agent keys: `mm key create --agent` and `mm team join --agent`. A remote
  host gives a key marked `agent: true` its user's access limited by the
  deployed commit's `agents:` block, and shows it as `NAME (agent)`.
- `./install.sh` registers the MCP server with Claude Code and OpenCode when
  they are installed.
- Local HTTPS, the dashboard, and Mailpit on macOS with Docker Desktop. The
  HAProxy forwarder, Compose autostart, and dashboard run as launchd agents of
  your user, so `mm ingress start` and `stop` need no sudo there. Workstation
  setup adds scoped `.test` DNS through `/etc/resolver/test`.
- `mm ingress status` and `mm ingress restart`.
- `mm target ingress configure` accepts `--acme-ca-server` and
  `--acme-ca-certificate` for Let's Encrypt staging or internal ACME servers.
- Remote environments with access declared in the repo. A `users:` key lists
  each user's public keys and a level per environment (`view`, `operate`,
  `deploy`, `admin`, plus `exec`, `forward`, and `secrets` add-ons). Deploys
  and the new `mm team sync` apply it per environment. Hosts run one locked
  `mm` SSH account whose only command is the relay; `mmd` renders its
  `authorized_keys`. `mm deploy plan` sends a commit ID, and the host fetches it
  from the project's pinned origin, enforcing `deploy-from` branch rules. New
  commands: `mm key create|show`, `mm team join|add|remove|show|whoami|sync`,
  `mm config user`, and `mmd access show|block|unblock|render-keys`.
- Remote deploys of images mm builds. `mm build receipt FILE` writes the latest
  published build of the checkout for the environment, and `mm deploy plan
  --receipt FILE` sends it with the commit. The host checks that the receipt is
  a build of that commit for that environment and matches its build settings,
  deploys its digests, and records it as the commit's build for status and
  logs. CI can build once and deploy with a machine key from `MM_SSH_KEY`.
- `modules.<name>.build.credential-only-secrets` marks build secrets that only
  authenticate fetching inputs, such as a Composer token. They are still
  mounted but left out of the build fingerprint, so they no longer disable
  reuse of unchanged images.
- Build receipts promote between environments: a receipt built for one
  environment deploys to another, locally or through `mmd`, when that
  environment's configuration at the receipt's commit builds exactly the same
  units, and the refusal names the first module that differs. Deployment plans
  record the receipt and the environment it was built for in `build_receipt`,
  and `mmd` audits it. Local deploys accept `--receipt` on `deploy plan` and
  `deploy apply`.
- `mm build import FILE [--cache-only]`, a reviewed plan applied with `--plan
  HASH --yes`, records a receipt as the checkout's published build for the
  environment, or with `--cache-only` only seeds reuse so a fresh CI runner
  retags unchanged images instead of rebuilding them.
- `deploy-from` and `initial-deploy-from` accept branch patterns: `*` matches
  within one `/` segment and `?` one character, so `[main, "*.x"]` allows
  release branches such as `26.6.x` and their hotfixes.
- The TUI's Access tab shows whether the environment runs locally or on a
  remote target, this device's key, what the host enforces for you (level,
  add-ons, and the commits it enforces and runs), the team from the users file,
  and saved forwards.
- `mm forward SERVICE[:PORT]` reaches a remote service's port on localhost
  through the mm protocol; the host's SSH forwarding stays disabled. `mm
  forward save|list|remove|up` keeps private shortcuts with pinned local
  ports. Needs the `forward` add-on for the service.
- Remote targets in the repo. A `targets:` key holds each host's public
  connection facts (host, port, SSH host key, daemon identity), so a fresh
  clone reaches every environment without `mm target add`. Clients pin the keys
  on first use and refuse a changed key until `mm target trust NAME`. A private
  target with the same name still takes precedence.
- Remote hosts refuse Compose features that reach the host (privileged
  containers, host namespaces, devices, extra capabilities, and similar) unless
  `host.yaml` allows one feature for one service. A host checks only the
  services the planned environment runs. `mm config validate --for-remote`
  reports them before a push, for the environment `-e` names or else for each
  environment whose target is not `local`; `--host-policy FILE` checks against
  a copy of a host's `host.yaml` and reports what it allows.
- Split configuration files. Any top-level key except `schema-version` can live
  in `.config/mm/<key>.yaml`, which holds just that key's value. `mm config
  split KEY` and `mm config join KEY` move a key through a reviewed plan that
  must leave the configuration's meaning unchanged, `mm config sources` shows
  where each key lives, and `mm config schema KEY` prints one key's schema.
  Errors and config edits name the file that declares the key, and `mm config
  validate` warns when git ignores a split file.
- `mm target setup plan|apply --origin URL --break-glass NAME=KEY` also
  installs the remote access layer: `mm` and `mmd` from one release
  (`--host-binaries`), the `mm` relay account and sshd drop-in, the project's
  enrollment in `/etc/mm/host.yaml` with its pinned origin host key
  (`--origin-host-key`), a read-only deploy key, and the `mmd` service. It
  checks each part and prints the host key, daemon identity, and deploy key.
  The new `mmd access enroll` does the host policy and deploy key part.
- `mmd secrets recipient` and `mmd secrets import`: the host side of pushing an
  environment's age secrets and its git deploy key from the operator's
  machine. Imported secrets keep the operator's revisions, and an installed
  deploy key replaces the generated one and survives later setup runs.
- `mm build` for an environment whose target is this machine builds into the
  local engine, Docker or Podman, under `mm.invalid/PROJECT/MODULE:TAG`, with
  no registry, no login and no push. `mm deploy` and `mm dev up` run these
  images with `pull_policy: never`; remote environments still run published
  builds only.
- `mm dev up` builds first, in the plan it shows, every local image that is
  missing or whose build inputs changed since its last local build, and
  keeps the others.
- A build that cannot run says why and what to run: Docker, Buildx or Podman
  not installed, the engine not running, or a Buildx builder whose `docker`
  driver cannot build several platforms, attach attestations, or write an
  archive (`docker buildx create --use`).
- The `file` secret provider reads a secret from a file on the machine that
  runs mm, such as the credentials file a package manager keeps, each time a
  build or deployment needs it: `storage: {provider: file, location:
  ~/.config/tool/auth.json}`. The content is used as written, newlines
  included. `mm secret status` shows the path, never the value; `set`,
  `rotate` and `generate` refuse it; a relative path, a non-regular file, a
  file over 1 MiB and a path inside mm's home are refused; a host never reads
  it. Each line of a multi-line secret is now redacted from build and
  service output too. See
  [read a secret from a file](docs/secret-providers.md#read-a-secret-from-a-file).

### Fixed

- A container or a job that the engine cannot start fails with
  `MM_RUNTIME_LAUNCH` and says why in mm's words when the cause is one mm
  recognizes: a mount that puts a directory onto a file, an image that is
  missing or cannot be pulled, a network that does not exist, a host port or a
  container name that is taken. It names the service or job and what to
  change, also over MCP. A job run that could not start ends `failed` with
  that message as its output instead of staying `launching`.
- A build whose Buildx builder no longer exists fails with `MM_BUILD`, naming
  the builder and `docker buildx use default`. A local Docker build runs on the
  current Docker context's own builder, so a builder that `docker buildx use`
  selected, or that was removed since, no longer affects it.
- On Docker, a `secret-files` entry's `mode` reaches the container: the file
  has mode `0444` by default, so a service whose image runs as another user
  reads it without `user: "0:0"` or extra capabilities, while its directory
  stays closed to other users of the machine. On a remote host the file also
  belongs to the entry's `uid` and `gid`. On your own machine it belongs to
  you; the plan, and `mm config validate` with the warning
  `MM_SECRET_FILE_OWNER`, name each file whose `uid`, `gid` or `mode` that
  can affect. Entries other than uid 0, gid 0, mode 0444 no longer fail on
  Docker.
- A development mount's `protected` or `excludes` entry that names a file,
  such as `protected: [.env]`, no longer stops every container of the module
  from starting. The deployment plan looks at the source: an empty
  read-only file covers a file, an empty volume still covers a directory,
  and a path that is not in the source is left alone instead of becoming an
  empty directory in the checkout. The plan lists each path and what covers
  it.
- `mm deploy plan` and `mm dev up` no longer stop with `existing route
  ownership has no successful deployment snapshot; recover the target before
  deploying` when an environment's routes on its target are not the ones its
  last finished deployment recorded, such as after an apply cut off after it
  saved its routes. The plan takes those routes back and says so
  (`adopted_route_claims` in JSON); routes of other projects and environments
  still stop it. Refusals about routes name the command to run next. See
  [troubleshooting](docs/help/troubleshooting.md#route-ownership-has-no-successful-deployment-snapshot).
- `mm doctor`'s `MM_HOST_RELEASE_OLDER` said to run `mm target setup plan`
  without `--upgrade`, which setup refuses for hosts on an older release;
  `MM_HOST_RELEASE_NEWER` now says `mm update`.
- State that another release wrote no longer stops an update. A newer `mm`
  or `mmd` reads what an older one stored and moves the state database
  forward; an older one ignores fields a newer one added, and refuses a file
  or database whose format changed with a message that names it and says to
  run `mm update` (or, on a host, `mm target setup plan --upgrade`), without
  changing it. Before, records refused any field they did not know, a newer
  migration said to use a compatible mm, and an `mmd` whose deployment
  record it could not read did not start. See
  [compatibility](docs/reference/compatibility.md#stored-state).
- Every stored schema number starts at 1: `schema-version` in
  `/etc/mm/host.yaml`, `schema_version` in `/etc/mm/mmd.json` and
  `MM_HOME/mcp.json`, and the access policy. Adding a key no longer raises
  one: `mmd.json` and `host.yaml` take keys they do not know with a warning
  that names them (`mcp.json`, which only narrows what agents may do,
  refuses them), and the messages about earlier formats
  (`access.yaml`, version 1 of `mcp.json`) are gone. A host set up by an
  earlier build gets `mmd.json` again from `mm target setup --upgrade`; set
  `schema-version: 1` in its `host.yaml` by hand.
- Job output, `mm service logs` and MCP `mm_logs` are redacted against the
  secrets the environment hands out (its services' variables and secret
  files, registry logins, tunnel token and certificate) and the other
  declared secrets in the stores it uses. Before, they read every declared
  secret, so on a machine with no connection to another environment's secret
  store a local before-deploy job, and with it `mm dev up`, failed.
- A symlink, special file, or `.env` file that the ignore file excludes no
  longer stops a build. One that would be sent is refused with its path in
  the context and the ignore file to exclude it in, in the terminal and over
  MCP.
- `mm config validate` checks every environment's Compose files the way
  `mm deploy` and `mm dev up` compile them and reports what they would
  refuse as `MM_COMPOSE_SOURCE`. Before, such a file passed validation and
  CI and failed first at the deployment.
- A node of a cluster no longer refuses a deployment plan for want of a
  secret that only a service placed on other nodes mounts, such as the
  secret file of a `role: job` service pinned to one node.
- `mm target setup` for a target without `nodes:` reads the release the
  host's `mmd` reports when it installs the remote access layer, and holds it
  to the rules of a cluster node: another build or an older release only with
  `--upgrade` or a `deploy.mm-version` pin, a newer one only with
  `--downgrade`. Before, it installed whatever it was given and ignored both
  flags.
- An environment's `build-variant` no longer makes a module without
  `build.targets` miss its image at deployment.
- On a Podman host, a service whose user is not root can listen on a port
  below 1024, as on Docker Engine: mm sets
  `net.ipv4.ip_unprivileged_port_start=0` on each service with a network
  namespace of its own that does not set it, and the deployment plan lists
  them under `ports:`.
- A remote environment whose first deployment failed can be looked into:
  `mm status`, `mm job history`, `mm deploy logs`, and the terminal UI read
  the hosts' record of the attempt instead of being refused, and a host that
  never began one refuses with `MM_NOT_DEPLOYED`, whose message reaches an
  agent as it is. MCP `mm_read` has the kind `deploy_logs`, what
  `mm deploy logs` prints.
- Requests to a remote environment carried the runtime the command settled
  on: `mm team sync` on a `nodes:` target said podman-compose while its
  deployments said docker-compose. Every request now carries the
  environment's runtime.
- Small findings of the fourth pass on real hosts: a key given twice in
  `host.yaml` is reported at both its lines instead of the first line of the
  file; `mm doctor` reports a node whose `mmd` answered and refused (another
  remote protocol, a host policy it cannot read) as `MM_NODE_REFUSED`, with
  help that points at what it said rather than at `systemctl status mmd`; a
  deployment says `started the new web next to the old` only on nodes where
  it did; an engine that hangs on a cluster node is named after the node,
  not `local`; and a push upload has one ID on every node, so `mm deploy
  cancel` stops all of it, and the client hears `MM_CANCELLED` rather than a
  broken pipe.
- `mm deploy status` ended "No rollout is in progress." when no node had
  answered (a client of another remote protocol, say). It now names the
  nodes that did not answer as unknown, and exits non-zero with
  `MM_UNREACHABLE` when none answered.
- `mm target setup plan` from an older `mm` planned to install its older `mm`
  and `mmd` on nodes that ran a newer release, without a word, cutting off
  every up-to-date client once applied. The plan now names, per node, the
  release it replaces with which, and refuses a node that runs a newer
  release unless `--downgrade`. Nodes report their release from this release
  on, so the refusal protects hosts upgraded past it; an `mm` from before
  this change still sees nothing.
- A job run whose `mm job run` had answered (once its container started)
  was in no host's list: `mm deploy status` did not show it and `mm deploy
  cancel` with its ID said nothing ran. Every host now lists the
  environment's running job runs in `mm deploy status` (text, JSON, the MCP
  read, the terminal UI), and the run's ID stops it.
- An access sync (`mm team sync`, `mmd access sync`) while a deployment
  rolled out made its reviewed plan stale at every node it had not reached
  yet, even when it changed nobody's access, and the rollout failed there.
  A reviewed plan now stays valid unless a grant that lets its deployer in
  changed.
- `mm team join` during a key rotation looked for your entry under `--name`
  or `$USER` only, and otherwise proposed a new user with the old key; a
  `mm key rotate --finish` after it locked the device out. The join now finds
  the entry that lists this device's key (or asks for `--name`), never
  proposes a new user with a key another entry lists, and `--finish` refuses
  while the users file here lists the current key and not the new one
  (`--unlisted` finishes anyway). The rotation's text names root's
  `authorized_keys`, and setup that a host refuses names the key to add.
- Two projects whose ids start with the same 12 hex digits shared their
  networks, name lookups and ingress routes on the hosts they shared: the
  deployment of one could take the ingress proxy off the other's network.
  Setup now refuses to enroll a project whose id starts like an enrolled
  project's, naming both. The names projects already have on running hosts
  stay as they are.
- The terminal UI's review of a deployment to a cluster said `0 routes`
  whatever `ingress:` routes the plan served; it counts the routes of the
  ingress nodes now.
- In the terminal UI, an action key pressed right after it opened, while the
  environment was still being read, was dropped with a note in the footer,
  even when the screen already showed every node's status: `u` brought no
  deploy review. Such a key now waits and goes ahead as soon as the
  environment is read, unless another key is pressed first.
- A deployment that moved a service to another OCI runtime, such as off
  gVisor after `exposed-runtime` left the host's policy, said nothing about
  it in its plan. The plan now names each such service and the runtime it
  ran under.
- A rolling restart, or a deployment, waited its whole window for a container
  that stays paused. It now fails within a few looks with `MM_RUNTIME_PAUSED`,
  naming the container and the command that resumes it.
- Two environments of one project whose names start alike, such as
  `staging` and `staging-eu`, saw each other's networks and service names
  in `mm service status` and `mm target nodes` when they share nodes. Each
  now sees only its own.
- Networks a deployment stopped using stayed on the cluster's nodes with
  their routes and `MM-CLUSTER` rules. The deployment that stops using a
  network now takes it away on every node once the rollout finished there,
  and its plan lists it; another environment's networks stay.
- A deployment whose commit lowers its own deployer's access, such as an
  agent applying a commit that gives agents only `view`, stopped after its
  first step on the first node and held the environment until its lease
  ran out. The deployer now finishes or aborts that rollout; everything
  else follows the new access at once.
- On a cluster, the record of a `once` job names the deployment's cluster
  plan, as its runs do, whichever node started it.
- The terminal UI said "Started app on local with 0 route(s)." after a
  deployment to one remote host; it names the target now.
- A deployment plan that several nodes refuse, such as one for an engine the
  nodes do not run, names every refusing node, not only the first.
- `mm service exec --container ID` on a cluster runs the command on the node
  that has the container. It asked the service's first node, which did not
  find a container of another node.
- An MCP call that mm refuses in its own words, such as `mm_deploy_plan` with
  `rollback` for an environment that runs on this machine, or a change to the
  schedules of a remote environment, now gives the agent mm's message and
  code. The agent used to get a fixed text, which for a valid project sent
  it to `mm init`. Provider output is still never passed on.
- Interrupting `mm job run` of a job on a remote host prints that the run
  goes on there as a note, no longer as an error.
- `mm agent trust --output json` names the review's `plan_hash` and the
  `next` command, as the text output does.
- A second environment could not be set up on a cluster target that fetches
  from git: `mm target setup deploy-key` made it a key of its own, and the
  target names one. It now copies the target's key from another environment
  of the target that holds it. A node that does not run an environment yet
  names the setup command in its refusal. `mm target setup deploy-key` and
  `mm target setup record` no longer need Ansible on this machine.
- After a host moved from Let's Encrypt to a certificate the environment
  provides (`tls:`), the ingress proxy of a cluster served the old Let's
  Encrypt certificate now and then, after each change of its backends. The
  deployment that switches the host now takes the old certificate out of the
  proxy's ACME state and restarts the proxy once.
- A rolling `mm service restart` stopped by `mm deploy cancel` ended with
  "runtime response was invalid"; it now ends `MM_CANCELLED` and names the
  nodes it restarted and the ones it did not reach.
- A cluster deployment failed at its second node with "reviewed remote plan
  has expired" when the steps before it took longer than the plan's ten
  minutes (a long `before-deploy` migration, a long `drain`), and rolled the
  first node back. A node now judges the plan by when it took the hold,
  which every node does when the rollout starts.
- A rollout step stopped by an emergency block (`mmd access block`) kept the
  environment held until the step's own deadline (about 20 minutes for a
  stack step); the lock now lapses a minute later, as after any step.
- `mm deploy apply` printed nothing for 30 seconds while a host's `mmd`
  was frozen: each progress line waited for its look at the host. The line
  now comes every 15 seconds whatever the host answers.
- A rollback on a cluster node stopped the containers the failed step had
  started while the ingress proxy and the names still sent them requests.
  Where another node runs the service, they now leave first (out of the
  names and routes, then the drain of the deployment rolled back to), as on
  a `stop-first` update.
- An agent's `mm_deploy_apply` on a cluster stopped after the MCP request
  bound (`request_timeout_ms`, 15 seconds by default) while the nodes rolled
  the plan out, and left the rollout unfinished on them. Applying a
  deployment through MCP now runs until the rollout ends; every other call
  keeps the bound.
- A host enrolled for push refuses a commit whose stated branches
  `deploy-from` does not allow before the upload, not after it.
- A team sync or deployment that a busy node refused lists that node as not
  changed, with the reason, instead of "changed and left as it is".
- `mm service logs --follow` on a cluster exits with an error naming the node
  and the reason when every node's stream has ended with an error; it used to
  exit 0.
- On a cluster target, a service none of whose containers passes its health
  check is still answered by name and through the ingress, with its running
  containers: such a check is more often wrong than the service. It used to
  get no answer and no backend at all, so one slow engine or strict check
  stopped every request. `mm service status`, `mm deploy status`, MCP and
  the terminal UI say the service is answered while unhealthy, and `mm
  doctor` reports `MM_SERVICE_ANSWERED_UNHEALTHY` naming it. Leaving and
  stopped containers are never answered.
- On a Podman cluster node, the first container of a network there gets its
  first name lookups answered and reaches other nodes from its first
  connection: `mmd` listens on the network's gateway before Podman creates
  the bridge with that container, and gives the bridge its settings before
  answering the container's first lookup. Its first requests by name used
  to fail with "bad address", then "No route to host" for a few seconds.
- `mm service restart` no longer restarts the container of a job that runs
  as the service. That container carries the service's labels, so a restart
  began a running job again from the start, and ran once more a job whose
  container a cut-off run had left behind.
- `mm config validate` refuses a Compose file that sets `environment` or
  `env_file` on a service (`MM_COMPOSE_ENVIRONMENT`). It used to pass, and
  the host refused the commit when it planned the deployment.
- Two `mm` processes that open a new state database at the same moment no
  longer end with one of them reporting `MM_STATE_LEGACY`, as if the
  database came from an earlier mm. The second process read the schema
  while the first was creating it.
- The terminal UI on a remote environment: editing an environment, a
  variable, or the team failed with `no .config/mm.yaml was found from
  /srv/mm/projects/...`, because it looked for the configuration in the
  host's copy of the project; it edits this checkout. The Overview named
  that path as the root. This machine's local HTTPS and shared development
  services are no longer counted as issues of a remote environment or read
  for it, and the Overview shows the nodes that answer (a cluster) or the
  deployed release (one host) where it shows local HTTPS for an environment
  on this machine. A refresh shows each node as it answers and keeps what
  the others said last, instead of waiting for every node; a service whose
  node has not answered yet reads `2/3` with `not answered yet` on that
  node's row, not `2/2`. The Environments view checks only the nodes of the
  environment on screen, not its whole status again.
- `x` on a service of a cluster in the terminal UI picked the first running
  container of the service, on whichever node, and asked the first node of
  the service to run the command in it: it failed when that container was
  on another node. It now runs where `mm service exec` runs it.
- A `start-first` update of a cluster service that also joins a network
  failed after 15 s, waiting for the old container's address on a network
  it was never on.
- Two containers of one cluster network on the same node could not reach
  each other on a host whose kernel filters bridged traffic: mm's
  `MM-CLUSTER` chain dropped it. The next deployment adds one rule per
  network that leaves such traffic to the engine's own rules.
- A rollback puts back only what the deployment changed on a host: a stack
  a step started or replaced, and the services started first for a
  `before-deploy` job. Before, every stack was applied again with the
  environment of the newer configuration, so a stack the deployment had not
  reached yet could restart with new environment values.
- A job run that exits non-zero on its last attempt keeps its output, so
  `mm job status`, `mm deploy logs`, and a failed deployment show why it
  failed. Before, the output of a failed run was dropped.
- No stuck operation can lock an environment. Every step on a host
  has a deadline `mmd` enforces from outside the step; a step whose client
  went away is stopped at once, routes a container it had begun to drain to
  again, and frees the environment; a rollout step whose client was not
  heard from for two minutes (a connection that dropped without SSH
  noticing) is stopped the same way; a one-host deployment is on record
  from its start, so `mm deploy status` says whether it finished, failed
  with its error, or was interrupted, also when its answer never reached
  the client; and a restarted `mmd` marks every request it finds
  running as interrupted. A second deployment of a one-host target is
  refused at once with `MM_DEPLOY_RUNNING` instead of waiting.
- A timeout is reported as `MM_TIMEOUT` and a stopped operation as
  `MM_CANCELLED`, naming the host: no longer `MM_RUNTIME_RESPONSE`
  ("restricted relay disconnected", "operation deadline exceeded"),
  `MM_STATE` (an MCP call that ran out of time), or a missing capability (an
  engine command that hung, "does not support direct_environment_delivery").
  A node that failed while it prepared is reported as not changed, not as
  rolled back.
- A service whose container exits right after it starts, without a health
  check, fails the deployment with `MM_RUNTIME_EXITED`, and one the engine
  keeps restarting (a cluster node's default `restart: unless-stopped`) with
  `MM_RUNTIME_RESTARTING` within a few looks (on Podman also
  `MM_RUNTIME_EXITED`, when a look finds it exited between two restarts),
  instead of reaching the name check after 30 s as "healthy". A container
  without a health check counts as healthy once it has run for five seconds. A
  deployment that does not become healthy names the runtime's code next to the
  service and its last log lines, and on a one-host target fails with that
  code instead of `MM_RUNTIME_RESPONSE`.
- An access sync waits up to half an hour for its turn behind a deployment
  step, saying what it waits for, instead of failing after half a minute.
- A log read of long lines over a slow link could end with
  `frame length must be between 1 and 8388608 bytes` although the lines had
  arrived.
- A service is no longer shown as degraded because of an exited container
  that a newer one replaced, or one that ran once for a job. Such a
  container is in the status JSON with `"history": true`.
- `mmd network show` on a node reports an engine network that is on other
  addresses than the stored ones, as `mm service status` does.
- The message for a `cluster.networks` range with no room for another
  network says where the key is set, as the message for a range a node
  already uses does.

- A second request that changes a host can no longer wedge `mmd`. Every such
  request took a blocking file lock on a thread that also served every
  other request and kept it until the request was done, so an access sync
  or a command that arrived during a deployment could stop the daemon for
  good: nothing was answered, the deployment never finished, a child
  process stayed past its timeout, and systemd still reported the unit as
  active. Requests that change a host now take turns without stopping a
  thread, and one that is still waiting when its time is up fails with the
  new `MM_HOST_BUSY`, which names what is in its way. `mm service exec`
  no longer waits for a deployment.
- `mmd access block` takes effect at once, also during a deployment. It
  used to wait for the running operation; now that operation is stopped
  within two seconds when its key was blocked.
- Engine and git commands that `mmd` starts are ended with everything they
  started when their deadline passes or when the request that needed them
  is gone, and their output is read by a thread of its own. A fetch that
  timed out used to leave `git` running.
- The relay process of a client that went away exits within two seconds. It
  used to wait for the daemon's answer, for ever when the daemon was stuck.
- A connection that a host dropped is no longer explained with sshd's
  remark that the relay account has no home directory.
- A daemon restart in the middle of a rollout step no longer leaves the
  node locked until the step's deadline (half an hour with the default
  convergence window).

- `mm target setup` no longer fails with every task "unreachable" when
  `MM_HOME` is a long path: Ansible's SSH control socket did not fit, so setup
  now connects without connection sharing there and retries a failed connect.
- A failed setup playbook reports the task that failed. With a recent Ansible
  the error showed only deprecation notices.
- Cluster setup leaves `/etc/hosts` and `/etc/mm/node.env` untouched when they
  are already right, and the storage node's NFS server no longer keeps
  `rpc.mountd` or `rpcbind` listening on every address.
- The terminal UI no longer offers actions a remote environment cannot
  perform: stopping no longer asks for confirmation and then fails, and
  `enter` on a variable sets it in the configuration instead of saving a
  local override the host never reads.
- The terminal UI opens on a remote environment whose deployed commit differs
  from your checkout, instead of failing with "configuration changed while the
  TUI snapshot was being built".
- `./install.sh` failed to build since `mm target setup` gained the remote access
  layer: its build snapshot left out the packaging files that `mm` embeds.
- Deployments without local TLS routes no longer require an installed local CA.
- `mm target setup` selects its target and runtime like every other command:
  `--target`, `--runtime`, private preferences, and the environment's settings
  were ignored in favour of the project's top-level target. The plan prints
  the exact `mm target setup apply` command, not a nonexistent
  `mm target bootstrap apply`.
- The HAProxy configuration reused frontend names for backends, which HAProxy
  3.3 and later reject. Installed forwarders are replaced on the next reviewed
  ingress reconciliation.
- `sh install.sh` on macOS, where `sh` is Bash in POSIX mode.
- `mm target setup apply` runs Ansible with a UTF-8 locale. ansible-core 2.14
  and later refused to start under the `C` locale setup used.
- Under `mmd.service`, access changes reach sshd. The unit's read-only system
  sandbox stopped `mmd` from rewriting `/etc/ssh/mm_authorized_keys`, and its
  `UMask=0077` would have made the file unreadable to the relay account.
- On Docker, mm writes files into a volume whose root another user owns with
  mode 0700, such as the volumes of a managed OpenBao or Vault provider or a
  volume Docker fills from an image directory. Installing a managed provider
  failed with `Permission denied`. The helper container now runs as the owner
  of the volume's root with no capabilities, and Docker writes each file with
  its owner and mode.
- A managed OpenBao or Vault provider starts on Docker Compose: its Compose
  file gave the options of its `/tmp` mount as separate `tmpfs` entries,
  which the engine refused.
- A build secret without a usable value said only that it was unavailable.
  It now says whether the secret is unset or invalid and why: an `env`
  secret whose variable holds a newline is `invalid` and points to the
  `file` provider for multi-line values. `mm secret status` gives the same
  reason.

### Security

- A remote host checks a Compose service with what it inherits through
  `extends` from another service of the same file, under the extending
  service's own name, and a host's `allow` entry for the base does not cover
  it. Before, `extends: {service: base}` let a service run a feature such as
  `privileged` or `network_mode: host` that the host never allowed for it,
  also when the base itself does not run in the environment.
- On a cluster node, a container on a network of its own host (the engine's
  default bridge, a Compose network that is not under `networks:`) no longer
  reaches the other nodes through the mesh, where it arrived with its node's
  mesh address and could read their service name records. A node's mesh
  address, and its address on a cluster network, no longer answer a machine
  that routes them at the node's public interface. Two chains do this,
  `MM-MESH` and `MM-MESH-IN`, loaded with the mesh before the container
  engine starts; `mm target mesh status` and `mm doctor` report a node that
  lacks them (`MM_MESH_GUARD`). Run `mm target setup plan` and apply it to
  upgrade the nodes. See
  [what the mesh carries](docs/remote-daemon.md#what-the-mesh-carries).
- The SSH keys `mm` creates (this device's key and the agent key) are
  encrypted with a passphrase `mm` generates and keeps in the system keychain:
  the Secret Service on Linux, the login keychain on macOS, and a file beside
  the key on a machine with neither. `mm` hands the passphrase to `ssh`
  itself, so commands work as before and never prompt; `ssh -i`, `scp` and
  `git` cannot use the key file, and a copy of it authenticates nowhere. The
  agent key's passphrase is released only inside `mm mcp serve`. These vaults
  answer any program running as the user, so a security key remains the
  boundary against one's own account; the docs say what is and is not
  stopped. A connection is still kept open for this device's key. Using an
  encrypted key needs OpenSSH 8.4 on the client.
- `mm target setup deploy-key` generates the git deploy key in memory. It
  used to be written to a temporary file, unencrypted, before it went to the
  secret store.
- rustls 0.23.46, rustls-webpki 0.103.16, and webpki-roots 1.0.11 fix the TLS
  advisories RUSTSEC-2026-0285, -0346, -0348, -0349, -0381, and -0382 in mm's
  HTTPS client.
- The bundled SQLite in the state database is updated through rusqlite 0.41
  (RUSTSEC-2026-0459), and rmcp 3.9 fixes RUSTSEC-2026-0457, -0461, and -0462
  in the MCP server.

## [0.1.0] - 2026-08-25

Not published as a download; builds report 0.1.0 until 0.1.1, the first release.

### Added

- Native tagged-release automation, deterministic platform packages, strict
  stable manifests, checksum-verifying installers, Cloudflare R2 publication,
  and explicit installed-version checks under `mm doctor`.
- Root Cargo workspace, path-aware CI, and an Astro/Starlight documentation site
- Deterministic lockfiles and validation commands for standalone examples
- Fail-closed operation context and selection provenance
- Shared runtime ownership identity and conflict errors
- SHA-256 certificate fingerprints over DER
- Canonical first-party build-node reuse and inferred `FROM` ordering
- Driver-specific process environment allowlists
- MIT license and contributor/security documents
- Domain configuration schema module and CA/trust ports
- Linux mm-owned trust-store copy with ownership-scoped uninstall
- cargo-deny policy and expanded CI gates
- Secret-leak scan of help, schema, validate, and env status
- Scoped automatic HTTPS ingress at `{service}.{project}.test` with auditable
  provenance, certificate references, ownership, and CLI/TUI diagnostics
- Reviewed local development source mounts with framework-aware preservation,
  portable shared intent, owned volumes, and CLI/TUI configuration
- Versioned installation-wide Traefik and Mailpit lifecycle with reviewed
  transactions, trusted `mail.mm.test` HTTPS, private-by-default SMTP,
  ownership diagnostics, recovery, and development mail overlays
- Shared typed Ratatui/headless onboarding workflows, exact reviewed init and
  minimal YAML edit plans, node/clone guidance, generated CLI reference,
  manpage, and shell completions
- Distinct managed and external OpenBao providers with scoped AppRole access,
  exact declared-location policies, KV v2 CAS revisions, pinned TLS, encrypted
  recovery and backups, and reviewed managed lifecycle operations for tested
  OpenBao 2.3 and 2.4 releases
- Distinct managed and external HashiCorp Vault providers with a Vault-specific
  policy and compatibility suite, tested Community 1.18 and 1.19 behavior,
  encrypted Raft recovery/backups, and reviewed managed lifecycle rollback
- A versioned least-privilege MCP stdio server with bounded typed reads,
  redacted logs, shared application use cases, and reviewed mutations

### Changed

- Moved repository governance, Rust tooling, and automation to the monorepo root
- Docker Compose/Swarm advertise only implemented capabilities
- Deployment preflight is capability-based rather than Podman-universal
- Consolidated remaining hardening gaps in `docs/known-issues.md`
- Local development services now default to trusted HTTPS when a single HTTP
  target port can be derived from the canonical Compose model
- `mm init` is now read-only by default and requires its printed plan hash plus
  `--yes` to create project files; new project IDs are deterministic for the
  canonical root and reviewed project name
- Project configuration now requires the strict, explicit schema 1 contract;
  removed experimental shapes and their migration commands are no longer accepted
- OpenBao CLI and Ratatui operations now share provider-neutral application
  plans for setup, diagnostics, lifecycle, backup/restore, and rotation
- The provider-neutral CLI and Ratatui flows now route OpenBao and Vault through
  independent registries

### Security

- Removed private reference material and added a fail-closed public-release content gate
- Unmanaged same-name networks and volumes are no longer treated as owned
- Docker volume-file methods fail instead of returning empty success
- Development bind mounts are rejected for remote/non-development targets and
  sensitive, escaping, ambiguous, or unsupported source paths before mutation
- Generated source binds request shared SELinux relabeling for rootless Podman
  parity across explicitly selected services
- OpenBao bootstrap tokens, AppRole credentials, unseal keys, and snapshot
  recovery records are excluded from plans and logs and encrypted at rest
- Ratatui captures OpenBao bootstrap and rotation input in a separate zeroizing
  masked buffer after review; plaintext never enters TUI state or render snapshots
- Vault bootstrap tokens, AppRole credentials, unseal keys, and snapshot recovery
  records use a separate encrypted registry and are excluded from plans and logs
- MCP requires a private exact-scope policy, rejects model-visible plaintext
  credential fields, exposes no generic execution tool, and audits mutations
  before invoking them
