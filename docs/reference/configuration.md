---
title: Configuration
description: Schema version 1 project configuration for mm.
---

mm discovers the shared project file, `.config/mm.yaml`, starting at `--root`
(or the current directory) and searching its parents. Any top-level key except
`schema-version` can instead live in its own file under `.config/mm/` (see
[split a large configuration](#split-a-large-configuration)). If present,
`.config/mm.overrides.yaml` beside that file supplies checkout-local settings.

The current schema is **version 1**:

```text
schema-version: 1
```

A key mm does not read is ignored with a warning that names the file and the
key (see [keys mm does not know](#keys-mm-does-not-know)). YAML must be valid UTF-8,
each configuration file must be a regular file no larger than 1 MiB (4 MiB across
all of them), and neither they nor configured project paths may contain symlink
components. On Unix, files mm
edits must be owned by the current user and not writable by group or others;
project directories have the equivalent ownership and write restrictions.

Paths in configuration are project-relative unless a field explicitly says
otherwise. Absolute paths and `..` traversal are rejected. Compose files and
module directories must exist. Build contexts and Dockerfiles are relative to
their module; `build.extra-inputs` are relative to the project root. A
development mount `target` is instead an absolute POSIX container path.

## Local overrides

Create `.config/mm.overrides.yaml` with just the settings to change locally:

```yaml
build:
  platforms: [linux/arm64]
```

The override uses the same field names as the shared file. Maps merge recursively;
scalars and lists replace the shared value; omitted keys inherit it. An empty
map leaves inherited keys intact. `null` is an explicit value: it clears nullable
fields, such as `default-environment`, and is invalid for non-nullable fields.
Use `{}` for an empty override file. The shared configuration must remain complete
and valid on its own; defaults are applied when deserializing the merged result.
Both sources use schema version 1, and the override need not repeat `schema-version`.

All project commands use the effective configuration, including validation,
builds, deployment and setup inspection. Existing CLI and private target/runtime
preference precedence still applies. Paths keep their usual project-relative
meaning. An invalid override is an error, rather than a fallback to shared values.
Changing either source requires a new review for configuration-dependent plans.
Config-editing commands write the shared file that declares the key, either
`.config/mm.yaml` or its split file; a local override still takes precedence
over an edited shared setting.

`mm init` includes `/.config/mm.overrides.yaml` in the project `.gitignore` as
part of its reviewed setup. Existing projects should add and commit this rule
once. Keep the override untracked. See [platforms on one machine](../concepts/builds-and-deployments.md#platforms-on-one-machine).

## Split a large configuration

When one key grows large, or needs its own reviewers, move it to its own file.
`mm config split` reviews the move first, like other config edits:

```sh
mm config split stacks                        # prints the plan and its hash
mm config split stacks --plan HASH --yes      # applies exactly that plan
mm config sources                             # shows where each key lives
```

Expected result: `.config/mm/stacks.yaml` holds the value of `stacks`, with its
comments, and `.config/mm.yaml` no longer has a `stacks:` key. The plan is
refused unless the project means exactly the same thing before and after the
move. `mm config join stacks` moves the key back.

A split file holds only the key's value, without the key itself:

```yaml
# .config/mm/stacks.yaml
app:
  files: [compose.yaml]
```

You can also create or edit these files by hand. The rules:

- `.config/mm.yaml` is still required. mm finds the project by it, and it keeps
  `schema-version`. Every other top-level key can be split, including `project`.
- Each key lives in exactly one place. A key in both `.config/mm.yaml` and its
  split file is an error that names both; nothing is merged.
- File names are the exact key plus `.yaml`, such as
  `.config/mm/environment-delivery.yaml`. A `.yaml` file named after a key mm
  does not read is ignored with a warning, as that key would be in
  `.config/mm.yaml`. Any other file, a `.yml` extension, a subdirectory, or a
  symlink in `.config/mm/` is an error. Hidden files and names ending in `~` are
  skipped, so editor swap files don't break loading.
- An empty split file is an error. Write `{}` for an empty map, or delete the
  file.
- Paths inside a split file are relative to the project root, as in
  `.config/mm.yaml`. Errors name the split file and the line.
- `.config/mm.overrides.yaml` overlays the combined result, as before.
- Commit split files. Remote environments load configuration from a commit, so
  `mm config validate` warns (`MM_CONFIG_SOURCE_IGNORED`) when git ignores one.
- Splitting changes the configuration fingerprint, so plans reviewed before the
  move must be reviewed again. Projects that split nothing keep their existing
  fingerprint.

To give a key its own reviewers, list its file in `CODEOWNERS`:

```text
/.config/mm/environments.yaml  @acme/platform
```

Editors can validate a split file with its own schema. `mm config schema KEY`
prints it, and the docs publish one per key at `/schema/mm/KEY.schema.json`,
for example [`/schema/mm/stacks.schema.json`](/schema/mm/stacks.schema.json).

## Top-level fields

New to the file? Start with [where configuration belongs](../concepts/configuration-ownership.md)
or a runnable example. The sections below are a field reference; YAML fragments
show the relevant settings and are not complete projects unless noted.

| Field                  | Required | Shape and default                                                                                             |
| ---------------------- | -------- | ------------------------------------------------------------------------------------------------------------- |
| `schema-version`       | yes      | Integer `1`. Missing, zero, and every other value are errors.                                                 |
| `project`              | yes      | `{id, name, development-environment?}`. `id` is a UUID.                                                       |
| `default-environment`  | no       | Environment selected when neither `--environment` nor `MM_ENVIRONMENT` is set.                                |
| `target`               | yes      | Default target name.                                                                                          |
| `runtime`              | no       | `{driver?}`; driver is `podman-compose` or `docker-compose`.                                                  |
| `build`                | no       | `{platforms, tag-strategy}`; defaults to `linux/amd64`, `linux/arm64`, and `auto`. `immutable` is also valid. |
| `networks`             | no       | Map of network names to `{}` or `{internal: true}`. Stacks and modules attach names from this catalog.        |
| `stacks`               | yes      | Map of Compose stack definitions.                                                                             |
| `modules`              | no       | Map of source modules and their services.                                                                     |
| `secrets`              | no       | Map of logical secret declarations; never put secret material in this file.                                   |
| `bindings`             | no       | Project-wide environment bindings.                                                                            |
| `environment-delivery` | no       | `{default?, development-default?}` using `direct` or `env-file`.                                              |
| `environments`         | yes      | Map of environment overlays; at least one is required.                                                        |
| `ingress`              | no       | Project automatic-local and middleware policy.                                                                |
| `development-mounts`   | no       | `{sensitive-patterns: [...]}`.                                                                                |
| `development-mail`     | no       | `{enabled?, bindings?}`.                                                                                      |
| `jobs`                 | no       | Map of named one-shot command declarations.                                                                   |
| `schedules`            | no       | Map of durable five-field cron triggers referring to jobs.                                                    |
| `gates`                | no       | Map of declarative provider and ingress references.                                                           |
| `users`                | no       | Who may reach each remote environment. See [remote access](../remote-access.md).                              |
| `agents`               | no       | What an AI agent may do, per environment or `*`. Default `view`. See [agents](../mcp.md).                     |
| `targets`              | no       | Remote targets: `{host, port?, host-key, daemon-fingerprint, providers?}` or `{nodes, cluster?, setup?, providers?}`, each with an optional `delivery`. See the [remote daemon guide](../remote-daemon.md#connect-a-client) and [cluster targets](#cluster-targets). |
| `hardening`            | no       | `strict`: every deployed service gets a read-only root, no capabilities, no new privileges, the engine's seccomp profile, and a tmpfs at `/tmp`. See [harden services](../hardening.md). |
| `registries`           | no       | Image registry logins by host: `{username, password: {secret}}`. See [prepare a deployment environment](../how-to/deploy-production.md#pull-from-a-private-registry). |
| `release-inputs`       | no       | Values each deployment is given with `mm deploy plan --release-input NAME=VALUE`: `{NAME: {pattern?}}`. See [release facts](#release-facts). |

Names used for projects, stacks, modules, services, networks, jobs, schedules,
and gates contain only ASCII letters, digits, `-`, and `_`. More restrictive
engine-specific checks also apply where relevant.

## Stacks and ingress

```yaml
networks:
  frontend: {}
  vault: { internal: true } # no outbound access

stacks:
  app:
    project-name: myapp-{environment}-{stack} # optional; {project-id} also works
    files: [compose.yaml, compose.local.yaml]
    networks: [frontend]
    shared-volumes: [uploads] # cluster targets: one directory on every node
    after: [db] # stacks that roll out before this one
    ingress:
      - domain: app
        route: / # default
        service: web # optional when unambiguous
        port: 8080 # optional
        https: true # default
        environments: [local, staging] # empty means all
        balance: spread # cluster targets: default the environment's ingress.balance
      - domain: app
        route: /hooks
        service: hooks
        priority: 5000 # default: the longer path goes first
        headers: { Cache-Control: no-store } # cluster targets
        allow: # cluster targets: every other request gets 404
          - { methods: [GET, HEAD], path: "^/hooks/status$" }
          - { methods: [POST], path: "^/hooks/[^/]+/events$" }

ingress:
  automatic-local: true
  middlewares:
    redirects:
      file: .config/traefik/redirects.yaml
      environments: [local]
```

`ingress` under a stack accepts either one route mapping or a list. Domain keys
resolve through the selected environment's `domains`. Route paths start with
`/`, contain no traversal or repeated slash, and have no trailing slash except
for `/`. Middleware files are bounded, project-relative regular files; the
currently supported document shape is `http.middlewares` with
`replacePathRegex` definitions.

`balance` says, on a [cluster target](#cluster-targets), which of the
service's healthy containers an ingress node sends the route's requests to:
`local-first` sends them to the containers on the node that took the request,
and to other nodes' only when it runs none; `spread` sends them to the
containers on every node. Without it the route takes the environment's
`ingress.balance`, and without that a route to a service with `autoscale` is
`spread` and every other route `local-first`. See
[spread requests over every node](../remote-daemon.md#spread-requests-over-every-node).

`priority` orders the routes of one host, from 1 to 1000000: the proxy tries
the route with the higher priority first. Without it the route with the longer
path goes first, so `/api` goes ahead of `/`.

`headers` and `allow` apply on a cluster target, with either
[ingress type](#bindings-environments-and-secrets). `headers` sets response
headers, on top of the environment's `ingress.headers`; an empty value removes
the header. `allow` lists the only requests that reach the service: each entry
has `methods` (in capitals, any when left out) and a `path` regular expression
the request path matches (any when left out; anchor it with `^` and `$`). The
proxy answers 404 to every other request for the route's host and path. `GET`
does not include `HEAD`: list both. A plan for a target of one host refuses a
route with either key, and local development leaves both out. See
[headers, allowlists, and several services on one host](../remote-daemon.md#headers-allowlists-and-several-services-on-one-host).

`ingress.middlewares` files do not apply on a cluster target: a cluster plan
that would apply one is refused. Limit the bundle to other environments with
`environments:`, and use `headers` and `allow` on clusters.

Each entry of `networks` is an engine network that mm creates and that every
stack, module, or service naming it joins. `internal: true` gives its
containers no outbound access: they reach the members of the network and
nothing else. On a [cluster target](#cluster-targets) every network of this
catalog spans the nodes that run a service on it; see
[networks across nodes](../remote-daemon.md#networks-across-nodes). On a
single host `internal` takes effect when the network is created: to change it
later, stop the services on the network and remove the network with the engine
first.

`shared-volumes` lists top-level Compose volumes of that stack that are one
directory on every node of a [cluster target](#cluster-targets): the target's
storage node keeps the files and the other nodes mount them over the mesh.
Use it for files that replicas share, such as uploads. It is slower than
local disk and does not give the locking guarantees a database needs, so a
service with `role: database` or `role: queue` that mounts one fails
validation with `MM_SHARED_VOLUME` unless it sets
`allow-shared-volumes: true`; give such a service an ordinary volume and pin
it with `placement:`. The volume must not set `external` or `name` in the
Compose file. On a target that names one host a shared volume is an ordinary
volume. See
[shared volumes across nodes](../remote-daemon.md#shared-volumes-across-nodes).

`after` lists stacks that a deployment rolls out before this one. Without it,
stacks roll out in name order. A name that is not a stack, a stack that lists
itself, and stacks that wait for each other fail `mm config validate` with
`MM_STACK_ORDER`.

## Modules, services, and builds

```yaml
modules:
  api:
    path: apps/api
    networks: [frontend]
    env:
      template: .env.example
    build:
      enabled: true # default
      context: . # optional
      dockerfile: Dockerfile # optional
      image: ghcr.io/acme/api # required when enabled
      default-variant: production
      targets: { production: "", debug: debug }
      args: { APP_MODE: production }
      secrets: { npm_token: npm.token }
      credential-only-secrets: [npm_token] # optional
      depends-on: { base: MM_BASE_IMAGE }
      extra-inputs: [package-lock.json]
    services:
      web:
        stack: app
        role: http
        http-port: 8080
        automatic-local-ingress: true
        local-https: true
        development-mail: true
        networks: [frontend]
        placement: web # cluster targets: a node, a group, `all`, or a list
        replicas: 2 # cluster targets: containers over the placement's nodes
        autoscale: # cluster targets; only scale.max is required
          scale: { min: 1, max: 4 } # min defaults to replicas
          metric: # defaults shown
            type: cpu # or memory
            up: { greater: 75%, for: 30s }
            down: { less: 40%, for: 5m }
        allow-shared-volumes: false # true lets a database or queue mount a shared volume
        host-user-namespace: shares its files with a host user # Podman hosts: keep the host's user ids, and why
        host-network: owns the tunnel interface # this machine: lets `network_mode: host` run, and why
        drain: 1m # cluster targets: how long a leaving container keeps serving
        health: # cluster targets; defaults shown
          on-failure: restart # or none
          after: 3m # unhealthy this long, then restarted
          max-restarts: 5 # within window, then left alone and reported
          window: 1h
        update: # optional; every key has a default
          order: stop-first # or start-first
          parallelism: 1 # nodes at a time
          failure: rollback # or pause
        env:
          APP.DEBUG: false
        secret-files:
          database-password:
            secret: database.password
            uid: "0" # defaults shown
            gid: "0"
            mode: 292 # YAML integer for 0444
            environments: [production] # optional; every environment of the service by default
        config-files:
          /etc/app/settings.toml: # the file's path in the container
            template: settings.toml.tmpl # or `file:` for a file shipped as it is
            uid: "101" # optional, the container's ids; without them root (yours on your own machine)
            gid: "101"
            mode: 0o440 # default 0444
            environments: [staging] # optional, as on secret-files
```

A build sends `context` without what the Dockerfile's own ignore file
(`<dockerfile>.dockerignore`) or, without one, the context's `.dockerignore`
excludes; see
[Choose the files a build sends](../concepts/builds-and-deployments.md#choose-the-files-a-build-sends).

`build.image` is the repository a published build pushes to. A build for an
environment on this machine leaves it alone and loads the image into the local
engine as `mm.invalid/PROJECT/MODULE:TAG` instead; see
[build on this machine](../concepts/builds-and-deployments.md#build-on-this-machine).

Each entry of `build.targets` is a build variant that an environment's
`build-variant` selects (else `default-variant`, else the only target). The
string form names the image tag suffix and builds the Dockerfile stage of the
same name; `""` adds no suffix. The mapping form sets `target` (the Dockerfile
stage) and `suffix`, both defaulting to the key, and `args`:

```yaml
build:
  image: registry.example.com/web
  args: { SITE_URL: https://example.com }
  targets:
    production: ""
    production-staging:
      target: production
      suffix: staging
      args: { SITE_URL: https://staging.example.com }
```

A variant's `args` merge over the module's `args` and follow the same rules:
`VERSION` is reserved, and names that look like secrets fail with
`MM_BUILD_SECRET_ARG_FORBIDDEN`. An environment builds only the variant it
selects, so two environments build two images from one stage. A build receipt
deploys to another environment only when it holds the variant that environment
selects, built with the same arguments.

Service roles are `http`, `database`, `queue`, `worker`, `scheduler`, and
`job`. Infrastructure and background roles do not receive automatic ingress.
A deployment starts no container of a service with `role: job`: it is the
container its [jobs](../jobs-and-schedules.md#give-jobs-a-service-of-their-own)
run in, and `mm service status` lists it as `job only`.

`environments` on a `secret-files` or `config-files` entry names the
environments in which the file is mounted, with the same meaning as on a
service. Elsewhere the file is not mounted, its secret is not required, and
the plan does not list it.

A `secret-files` entry's `mode` applies on every runtime. Its `uid` and `gid`
apply on remote hosts and with Podman; on Docker on your own machine the file
belongs to you, and the plan says so where that can matter. See
[file delivery](../secret-delivery.md#file-delivery).

`placement` says which nodes of a [cluster target](#cluster-targets) run a
service: a node name (`app-1`), a group from the target's `cluster.groups`
(`web`), the word `all`, or a list of those. The service runs one container on
each node it names. Without `placement` it runs on the first node under
`nodes:`. When the environment's target lists `nodes:`, `mm config validate`
resolves each name against it and fails with `MM_PLACEMENT` when a name is
neither a node nor a group of that target, and when a service of a stack's
Compose files has no entry under `modules`. A target with one host has nothing
to choose from, so placement is ignored there and the same service definition
works locally and on a cluster. See
[place services on nodes](../remote-daemon.md#place-services-on-nodes).

`config-files` ships files with a service: each key is a path in the
container, and the entry names either a `file` of the module or a `template`
in which every `{{ KEY }}` is replaced by the environment's binding of that
key. The file is mounted read-only on every host that runs the service, and a
changed file replaces only the services that mount it. A template cannot use
a binding that is a secret; `mm config validate` fails with `MM_CONFIG_FILE`
and names `secret-files` instead. See
[ship config files with a service](../config-files.md).

`replicas` is how many containers the service runs on a cluster target when
nothing scales it, spread over its placement's nodes, the first nodes getting
one more when the count does not divide evenly. Without it the service runs
one container on each placement node.

`autoscale` lets the count follow the load, between `scale.min` (default:
`replicas`) and `scale.max`, the one required key. A container is added when
the average of the service's containers has been above `up.greater` for
`up.for` (default `75%` for `30s`), and removed when it has been below
`down.less` for `down.for` (default `40%` for `5m`). The percentage is of the
container's CPU limit when it has one and of one core when it has none
(`type: memory`: of its memory limit, or of the node's memory). `for` is at
most `4h`. `mm config validate` fails with `MM_SCALE` and names the key for a
`min` above `max`, a `replicas` outside the range, a `down.less` that is not
below `up.greater`, a longer `for`, a service on no network of `mm.yaml`,
and for `replicas` or `autoscale` on a target of one host. `mm dev` ignores
both and runs one container. See
[autoscale a service](../remote-daemon.md#autoscale-a-service).

`health` says what the node does with a container of the service whose
health check (its Compose `healthcheck:`) keeps failing, on a cluster
target. Without it the node restarts a container that has been unhealthy for
`after` (`3m`, at least `10s`) as `mm service restart` does, one at a time;
past `max-restarts` (`5`) restarts within `window` (`1h`) it leaves the
container alone, keeps it out of the names and routes while another
container of the service can take its requests, and reports
`MM_SERVICE_RESTART_LIMIT`. `on-failure: none` turns it off. A value it
cannot use fails `mm config validate` with `MM_HEALTH`, as does a `health`
that restarts on a target of one host. See
[restart a container that keeps failing its health check](../remote-daemon.md#restart-a-container-that-keeps-failing-its-health-check).

`allow-shared-volumes: true` lets a service with `role: database` or
`role: queue` mount a volume listed under a stack's `shared-volumes`, which
mm refuses otherwise: a shared volume is a network mount, slower than local
disk and without the locking guarantees such a service needs. Leave it out
unless you measured that your case works.

`hardening: {except: {<default>: "<reason>"}}` leaves one default of the
project's [`hardening: strict`](../hardening.md#when-a-service-needs-more)
out for this service. The defaults are `read-only`, `cap-drop`,
`no-new-privileges`, `seccomp`, and `tmpfs`; the value is the reason, which
the plan prints. A value that is empty fails validation with
`MM_HARDENING_REASON`.

`host-user-namespace` keeps the service's containers in the host's user
namespace on a remote host that runs Podman, where every other service has
65536 user ids of its own, the same on every node and in every deployment.
Its value says why and may not be empty. Every service that mounts a volume
another service mounts too needs it (`MM_USER_NAMESPACE`). On Docker Engine and on your own machine it changes
nothing. See [run Podman on the hosts](../remote-daemon.md#run-podman-on-the-hosts).

`host-network` lets the service's Compose definition say
`network_mode: host`, which puts its containers in the network namespace of
the machine they run on, as a VPN or DNS server needs. Its value says why,
the plan shows it, and it may not be empty (`MM_HOST_NETWORK`). It is
required in an environment that runs on your own machine; there a service
with `network_mode: host` and no `host-network` fails `mm config validate`,
`mm deploy`, and `mm dev up` with a message that names the Compose file, the
service, and this key. On a remote host or a cluster its host policy decides
instead, and the key is optional: see
[run a service on the host's network](../remote-access.md#run-a-service-on-the-hosts-network).
Wherever it runs, such a service:

- writes `network_mode: host` itself, on the service: not from a variable,
  and not through a YAML merge key;
- has no ingress route, gets no automatic local route, and no development
  mail variables;
- joins no network: neither `networks` in its Compose definition nor a
  network of `mm.yaml` under the stack, its module, or the service;
- publishes no `ports`: it listens on the host's ports itself;
- keeps the host's user namespace (`userns_mode: host`), unless its Compose
  definition sets `userns_mode`;
- cannot use `update.order: start-first`, because two containers cannot hold
  the same ports.

`cap_add` works as for any service: on your own machine nothing refuses
`cap_add: [NET_ADMIN]`.

`drain` says how long a container of the service that leaves a cluster node
(an update replaced it, the service moved, autoscale removed it, or
`mmd container leave` took it out)
keeps serving the requests it already has, once no node sends it new ones:
`30s`, `2m`, at most `15m`, default `1m`. Then it gets its stop signal and its
`stop_grace_period`. See
[take a container out without dropping a request](../remote-daemon.md#take-a-container-out-without-dropping-a-request).

`update` says how a deployment replaces the service's containers. `order` is
`stop-first` (the engine stops the old container, then starts the new one) or
`start-first` (the new container starts next to the old one, which stops once
the new one is healthy and resolved by every node). `parallelism` is how many
nodes of a cluster target replace the service's stack at the same time.
`failure` is `rollback` (every node the deployment changed returns to what it
ran before) or `pause` (the deployment stops where it is). `start-first` is
refused with `MM_UPDATE_ORDER` for a service that publishes a fixed host port,
runs on the host's network, sets `container_name`, runs more than one
container, or on Podman sets its own `userns_mode` other than `host`. See
[rolling deploys and rollback](../remote-daemon.md#rolling-deploys-and-rollback).

Set `environments` on a service to run it only in the named environments. Omitted
or empty lists mean all environments; every name must be configured:

```yaml
modules:
  api:
    services:
      scheduler:
        stack: app
        role: scheduler
        environments: [integration, uat, production]
```

For other environments, mm omits that service from the generated Compose files,
environment/secret delivery, managed networks and automatic ingress. Build and
image declarations remain module-level; a deployment needs published images only
for modules with active services. Deployment and stack-compilation plans list
excluded services. Applying a deployment removes previously deployed excluded
services; a stack with no remaining services is removed as a whole.

Active services cannot depend on excluded ones through `depends_on`, `links`,
`volumes_from`, or `service:` namespace sharing (`network_mode`, `ipc`, `pid`).
Optional `depends_on` entries with `required: false` are removed. Dependency
checks apply to each source Compose file; remove conflicting references from
earlier files as well as later overrides. Explicit ingress routes must allowlist
compatible environments. Jobs and schedules targeting an excluded service are
inactive in that environment.

### Optional in development

`development` on a module, a service, or a job says whether it runs in the
project's `development-environment`:

```yaml
modules:
  status:
    path: apps/status
    development: optional # on (default) | optional | off
    services:
      status-web: { stack: app }
      status-worker: { stack: app, development: on } # its own value wins
jobs:
  report:
    module: api
    service: api
    development: optional
    command: [bin/report]
```

- `on` runs it, as without the key.
- `optional` leaves it out until a developer turns it on for their machine
  with `mm dev enable`, or for one run with `mm dev up --with`.
- `off` leaves it out, the same as leaving the development environment out of
  its `environments`.

Every other environment ignores the key. A service takes its module's value
unless it sets its own; a job of a service that is off is off too. A unit that
is off is excluded like a service outside its `environments` (above): its
routes, jobs, schedules, development mounts, and bindings go with it. The plan
lists it as `off locally (optional)` with the `mm dev enable` command that
turns it on. The preserved volumes of a development mount stay while its module
is off.

Turning a unit on also turns on the optional services it cannot start without
(Compose `depends_on`, `links`, `volumes_from`, or a `service:` mode). A
service that runs and needs an optional one that is off fails validation with
`MM_DEVELOPMENT_DEPENDENCY`, which names both and the command that fixes it.

The units a machine runs are kept in `.config/mm.overrides.yaml`, which `mm
dev enable` and `mm dev disable` edit; only that file may set `development:` at
its top level:

```yaml
development:
  enabled:
    - status # a module, service, `module/service`, or job
```

A name there that matches nothing is the warning `MM_DEVELOPMENT_UNIT`. See
[run part of a large project](../onboarding.md#run-part-of-a-large-project).

Build `args` are public values; names that look secret are rejected and belong
under `build.secrets`.

`credential-only-secrets` lists keys of `build.secrets` that only authenticate
fetching inputs, such as a package registry token. They are still mounted, but
they are left out of the build fingerprint, so they do not prevent reusing an
unchanged image or promoting a build to another environment. Any other build
secret disables both. See [build secrets](../secret-delivery.md#build-secrets).

## Jobs and schedules

```yaml
jobs:
  migrate:
    module: api
    service: web
    command: ["php", "artisan", "migrate", "--force"]
    environments: [production] # optional; every environment by default
    run: before-deploy # optional: before-deploy, after-deploy, or once
    timeout: 10m
    retry: { attempts: 3, backoff: 5s, max-backoff: 5m }
    output-limit-bytes: 65536
    history-limit: 100
    placement: app-2 # cluster targets: a node or group that runs the service
    user: "1000:1000" # optional; the service's own user otherwise

schedules:
  framework:
    job: migrate
    cron: "0 2 * * *"
    timezone: America/New_York
    enabled: true
    starts-at: 2026-01-01T00:00:00Z
    ends-at: 2027-01-01T00:00:00Z
    jitter: 30s
    concurrency: forbid # allow, forbid, replace
    misfire: run-once # skip or {bounded-catch-up: {maximum: 10}}
    retry: { attempts: 2, backoff: 10s, max-backoff: 1m } # optional override
    timeout: 5m # optional override
```

Durations use one compact suffix: `ms`, `s`, `m`, `h`, or `d`. Commands are
argument vectors and never shell strings. A job reuses the named service's
image, environment, networks, mounts, and secret delivery contract. On a
cluster target a job runs on one node: the first node its `placement` selects
that runs the service (a node name, or a group), or else the first node that
runs the service. `mm config validate` refuses a placement where the service
does not run. `run` makes a deployment run the job by itself: `before-deploy`
after what its service depends on is healthy and before the service is
started or replaced, `after-deploy` once the deployment converged, `once` the
first time the environment is deployed with it. `environment: NAME` is read as
`environments: [NAME]`. `mm.overrides.yaml` cannot set `run`, `user`, or
`environments`. See
[Jobs and durable schedules](../jobs-and-schedules.md) for timing, recovery,
and operational semantics.

A module can also contain `development-mount`:

```yaml
development-mount:
  source: .
  target: /workspace
  mode: read-write # or read-only
  services: [web]
  preset: node # laravel, node, dotnet, or rust
  preserve: [{ path: /workspace/node_modules }]
  excludes: [dist] # a directory: an empty volume covers it
  protected: [.env] # a file: an empty read-only file covers it
  safe-exceptions: []
```

`excludes` and `protected` are paths relative to `source`. A deployment plan
covers each one by what the source has there, and leaves a path that is not
in the source alone; see
[Hide files and directories from containers](../development-mounts.md#hide-files-and-directories-from-containers).

## Bindings, environments, and secrets

Binding keys use dot-separated uppercase names such as `DB.HOST`. A binding is
exactly one of these untagged shapes:

```yaml
bindings:
  APP.DEBUG: { value: false }
  DB.PASSWORD: { secret: database.password }
  APP.URL: { cast: url, value-ref: domains.app }

environments:
  local:
    domains: { app: app.example.test }
    bindings:
      APP.DEBUG: { value: true }
    runtime: { driver: podman-compose }
    target: local
    build-variant: debug
    environment-delivery: env-file
    automatic-local-ingress: true
    deploy:
      convergence-timeout: 10m # default; how long a stack may take to become healthy
      progress-timeout: 2m # default; how long a deployment waits for a sign of progress from a host
      pull-timeout: 20m # default; how long a host may take to pull the images of a stack or a job
      failure-log-lines: 200 # default; how much of a failed container's log a host keeps
  production:
    target: production
    tls: { certificate: tls.certificate, key: tls.key } # optional
    ingress:
      balance: spread # cluster targets: local-first (default) | spread
      headers: { X-Robots-Tag: noindex } # cluster targets: on every route
```

`tls` gives an environment's HTTPS routes a certificate you provide, in place
of the one the ingress requests from Let's Encrypt: two declared secrets, each
holding a PEM file encoded as base64 on one line. Use it where ACME's
challenge cannot reach the ingress, such as behind Cloudflare's proxy with an
origin certificate. A name that `secrets:` does not declare fails validation
with `MM_ENVIRONMENT_TLS_INVALID`. See
[HTTPS](../remote-daemon.md#https).

`ingress.balance` is the `balance` of each of the environment's routes that
does not set its own, and `ingress.headers` are response headers every route
of the environment sets on a cluster target; a route's own `headers` win (see
[stacks and ingress](#stacks-and-ingress)).

`ingress.type` says where a cluster target takes the environment's requests:
`public` (the default) on ports 80 and 443 of the ingress nodes, `cloudflared`
through a Cloudflare Tunnel, with ports 80 and 443 closed. A tunnel names the
declared secret that holds its token:

```yaml
environments:
  staging:
    target: production
    ingress:
      type: cloudflared
      cloudflared:
        token: cloudflare.tunnel-token # a secret declared under secrets:
        image: docker.io/cloudflare/cloudflared@sha256:d9ff527dc19799e24d3689058161fb01e752a24be1823577bae54c9c2489275f # optional
```

`type: cloudflared` without `cloudflared.token` fails to load and names the
`mm secret set` command; `cloudflared:` without `type: cloudflared` fails too.
A token that `secrets:` does not declare fails validation with
`MM_TUNNEL_TOKEN_MISSING`, and so does a plan while the secret has no value on
an ingress node. `cloudflared.image` replaces the connector's image; the
default is the cloudflared release mm pins by digest, shown above. Cloudflare
ends TLS, so `tls:` with `type: cloudflared` fails validation
(`MM_ENVIRONMENT_TLS_INVALID`). Local development ignores `type`, and a target
of one host refuses `cloudflared`. See
[Serve a cluster through Cloudflare Tunnel](../how-to/cloudflare-tunnel.md).

`deploy.convergence-timeout` is how long a deployment waits for a stack to
become healthy on a host before it counts as failed and is rolled back: a
number with `s`, `m`, or `h`, at least `1s`. The default is `10m`, and
`mm deploy plan` shows the value in use. A stack whose container exits, or
whose health check fails, fails at once without waiting for it. A container
without a health check counts as healthy once it has run for five seconds.

`deploy.progress-timeout` is how long `mm deploy apply` (and `mm team sync`)
on a remote environment waits without any sign of progress from a host: the
host does not answer, or its `mmd` no longer works on the step. The command
then stops with `MM_TIMEOUT`, naming the host, the step, and the operation,
and the host stops the step when it sees the client go. At least `30s`; the
default is `2m`. While a step runs, the command prints a line every 15
seconds. A step that takes long while its host is at work, such as a
`before-deploy` job, a container's `drain`, or a large `delivery: push`
upload, shows progress the whole time and is not stopped by this timeout.

`deploy.pull-timeout` is how long a host may take to pull the images a stack
or a job lacks, before any of its containers starts or stops: at least `1s`,
the default is `20m`. The pull is a step of its own, with the registry logins
of `registries`, so `compose up` and `compose run` find every image in place
and keep their own shorter limit. A pull past the limit stops the deployment
with `MM_TIMEOUT` and names this key. An image that `pull_policy: always` asks
for is pulled each time; one with a `build` section, one mm built, or one with
`pull_policy: never` or `build` is not pulled.

`deploy.failure-log-lines` is how many last lines of a failed container's log
a host keeps when a stack does not become healthy: 1 to 1000, and the default
is `200`. The deployment's report and `mm deploy status` show them, also after
a rollback removed the container, until the host's next deployment. A host
keeps at most 64 KiB of them, so very long lines leave fewer. A `before-deploy`
job that fails keeps the same number of its last output lines in the report;
`mm deploy logs` shows its whole output. Outside 1 to 1000 it fails validation
with `MM_DEPLOY_FAILURE_LOG_LINES`.

`deploy.mm-version` pins the release of mm that may plan deployments to the
environment and of `mmd` on its hosts. It is written as Cargo writes a
version requirement: `"0.9"` is any 0.9 release, `"=0.9.3"` exactly that
one, `">=0.9.2, <0.11"` a range. Not set, any mm whose protocol the hosts
speak may deploy.

```yaml
environments:
  production:
    target: production
    deploy:
      mm-version: "0.9"
```

With a pin, `mm deploy plan`, `mm deploy rollback`, and `mm target setup plan`
refuse an mm outside it and name the installer option that fits
(`--version vMAJOR.MINOR.PATCH`); a deployment plan also asks each host which
`mmd` it runs and refuses a host outside it, naming `mm target setup` as the
fix. `mm doctor` reports both as `MM_VERSION_PIN`. The pin is read from the
configuration of the checkout you plan from, so changing it is a reviewed
commit like any other.

### Release facts

A deployment knows the release it runs. Env templates read these keys with
`mm:from=`, config-file templates with `{{ KEY }}`, and a binding can give one
another name with `value-ref`:

| Key                         | Value                                                                                          |
| --------------------------- | ---------------------------------------------------------------------------------------------- |
| `MM.COMMIT`                 | The full commit SHA being deployed.                                                            |
| `MM.COMMIT_SHORT`           | Its first 12 characters.                                                                       |
| `MM.RELEASE`                | The name given with `mm deploy plan --release NAME`; otherwise `sha-` and the short commit.    |
| `MM.IMAGE.<module>`         | The module's image as `repository@sha256:...`, from the build the plan deploys.                |
| `MM.IMAGE_DIGEST.<module>`  | That image's digest alone.                                                                     |
| `RELEASE.<NAME>`            | The value of an input `release-inputs:` declares.                                              |

```yaml
release-inputs:
  FLATCAR_BUILD_ID: { pattern: "^[A-Za-z0-9._-]+$" }
  NODE_IMAGE: { pattern: "^[a-z0-9./-]+@sha256:[0-9a-f]{64}$" }

bindings:
  APP.VERSION: { value-ref: MM.RELEASE }
```

```sh
mm -e production deploy plan --receipt build-receipt.json \
  --release v26.6.0 \
  --release-input FLATCAR_BUILD_ID=4757.2.3-12 \
  --release-input NODE_IMAGE=registry.example.com/platform@sha256:...
```

Each input that a template of the planned environment reads has to be
given, and nothing undeclared may be: the plan fails with `MM_RELEASE_INPUT`
and names the inputs missing with the template line that reads each, the one
not declared, or the value that does not match its `pattern`. An input that
only another environment's templates read may be left out, so `mm deploy
plan` of a development environment needs none of the values CI supplies for
staging. A pattern is a regular
expression that the whole value must match; without one, any single line of
text is accepted. Input names are one upper-case word, such as `BUILD_ID`.

The release is part of the plan: the plan prints its name and inputs, and
another value is another plan hash. The hosts record it with the deployment,
`mm deploy status` shows it for each node, and `mm deploy rollback` plans the
earlier deployment again with the name and inputs it ran with.

Release values are never secret: they appear in the plan, in `mm deploy
status`, and in the hosts' audit log. Keep secrets in `secrets:`.

Keys that start with `MM.` belong to mm, and a binding named `RELEASE.<NAME>`
for a declared input is refused; `mm config validate` reports both with
`MM_ENV_KEY_INVALID`. `mm config validate`, `mm env status`, and `mm dev`
deploy no release, so there these keys have no value yet: an env template
uses its default, and a config-file template renders them empty. When a
deployment is planned, a key whose fact the release does not have fails the
plan with `MM_RELEASE_INPUT`. `MM.IMAGE.<module>` comes from the build
receipt given with `--receipt` or, on your own machine, from the environment's
last build there: a published image, or the local tag of an image built into
the engine.

Environment bindings override project bindings. `direct` passes
operation-scoped interpolation values to the runtime; `env-file` uses a private,
operation-scoped file. With a declared development environment, the built-in
default is `env-file` for that environment and `direct` elsewhere. Explicit
environment, development, and general policy values override those defaults.

```yaml
providers:
  staging-secrets:
    driver: openbao
    scope: project
    mode: external
    address: https://bao.example.test:8200
    storage-mount: mm-kv
    automation-auth-mount: mm-approle
    trust-bundle: company-ca.pem
    expected-trust-sha256: sha256:HEX_DIGEST

secrets:
  database.password:
    storage:
      provider: staging-secrets # age, env, file, or a declared provider instance
      location: projects/example/db
    input: { prompt: Database password }
    rotation:
      strategy: manual # manual, replace-and-redeploy, coordinated
      interval-days: 90

  app.key:
    storage: { provider: age, location: app.key }
    generate: { preset: bytes, length: 32, encoding: base64-url }
```

A provider with `mode: managed` and `scope: environment` is the store mm runs
on the nodes of each cluster environment (`driver: openbao`, `release: "2.4"`
or `"2.5"`,
and optionally `nodes`: `all`, the default when the target has 3 or 5 nodes,
or a list of 3 or 5 node names). In an environment on your machine it is the
local development instance, whose `installation` and `management-port`
default to the provider's name and `18200`. See
[keep deploy secrets in a managed store](../how-to/managed-secret-store.md).

`provider: env` reads the environment variable `location` names, and
`provider: file` reads the file at `location`, an absolute path or one that
starts with `~/`. Both are read-only. See
[read a secret from a file](../secret-providers.md#read-a-secret-from-a-file).

Generator presets are `password` (length 32), `token` (48), `bytes` (32 and
`base64-url`), `laravel-app-key`, and `uuid`. Length overrides must be 16–4096.
`mm secret generate --missing` creates the generated secrets the selected
environment uses, and `--all` creates every one.
Other byte encodings are `base64` and `hex`. Provider instance names are stable
and independent of driver/mode. Credentials and recovery material remain
private mm state, not project configuration.

## Development mail, jobs, schedules, and gates

```yaml
development-mail:
  enabled: true
  bindings:
    MAIL_HOST: smtp-host
    MAIL_PORT: smtp-port
    MAIL_URL: smtp-url
    MAIL_UI: ui-url

jobs:
  migrate:
    module: api
    service: web
    command: [php, artisan, migrate, --force]
    environment: staging # optional

schedules:
  nightly-migrate:
    job: migrate
    cron: "0 2 * * *"
    enabled: true # default

gates:
  admin:
    provider: password-cap
    ingress: [admin]
```

Jobs require an existing module/service and a non-empty argument vector.
Schedules require an existing job and exactly five cron fields. A gate
declaration names a gate driver such as `password-cap` and optional ingress
references. Operational policies, users, sessions, and credentials are private
state rather than project configuration. The `--secret-provider` accepted by
`mm access add` separately selects storage for access secrets; it is not this driver
field.

## Cluster targets

A target that lists `nodes:` is a cluster. A name and a host for each node are
enough:

```yaml
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
      - {name: db-1, host: 203.0.113.13}
```

[Set up a cluster](../remote-daemon.md#set-up-a-cluster) has the commands, the
setup steps, and what works on a cluster target in this release.

Each node accepts:

| Key                  | Required | Meaning                                                                                                                                            |
| -------------------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `name`               | yes      | The node's name, a lowercase hostname label. Setup makes it the node's hostname.                                                                   |
| `host`               | yes      | The address or DNS name mm connects to.                                                                                                            |
| `port`               | no       | SSH port. Default `22`.                                                                                                                            |
| `endpoint`           | no       | The address or DNS name the other nodes send WireGuard packets to, without a port. Default: `host`. Not allowed with `cluster.mesh.interface`.     |
| `private-address`    | no       | The node's address on the mesh. Default: the lowest free address in the mesh subnet, in declaration order. Required with `cluster.mesh.interface`. |
| `host-key`           | no       | The node's SSH host key, `<type> <base64>`. `mm target setup record` writes it.                                                                    |
| `daemon-fingerprint` | no       | The identity of the node's `mmd`. `mm target setup record` writes it.                                                                              |

With the default mesh, the first node gets `10.90.0.1`, the second `10.90.0.2`,
and so on. `mm target setup record` writes each assigned `private-address`
back with `host-key` and `daemon-fingerprint`, so later edits to the node list
do not move addresses.

The optional `cluster:` block overrides defaults:

```yaml
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
      - {name: db-1, host: 203.0.113.13}
    cluster:
      mesh: {subnet: 10.90.0.0/24, port: 51820, mtu: 1420} # or {interface: eth1}
      networks: 10.64.0.0/12
      storage: app-1
      groups: {web: [app-1, app-2]}
      ingress: [app-1, app-2]
```

| Key        | Default                                          | Shape and meaning                                                                                                                                                                                                                                                                                                                       |
| ---------- | ------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mesh`     | `{subnet: 10.90.0.0/24, port: 51820, mtu: 1420}` | `{subnet?, port?, mtu?}`: mm builds a WireGuard mesh between the nodes. Or `{interface}`: an existing private interface, such as a provider VLAN; mm then creates no WireGuard mesh, and every node must declare `private-address`.                                                                                                     |
| `networks` | `10.64.0.0/12`                                   | CIDR that [networks spanning the nodes](../remote-daemon.md#networks-across-nodes) get their addresses from. A private range, /20 or larger, that overlaps neither the mesh subnet nor an address or route a node already has. Each network takes a /20 (16 nodes, room for 253 containers on each), so the default holds 256 networks. |
| `storage`  | the first node                                   | Name of the node that keeps [shared volumes](../remote-daemon.md#shared-volumes-across-nodes) in `/var/lib/mm/storage` and serves them to the other nodes over the mesh. Changing it while that node holds files is refused with the steps to move them.                                                                                 |
| `groups`   | none                                             | `{group: [node, ...]}`. Group names that `placement` can refer to.                                                                                                                                                                                                                                                                      |
| `ingress`  | the first node                                   | The nodes that run the [ingress proxy](../remote-daemon.md#route-traffic-into-the-cluster) and take ports 80 and 443 for every `ingress:` route, written like a `placement`: a node, a group, `all`, or a list.                                                                                                                       |

`mesh.port` is the UDP port every node listens on; it must be reachable
between the nodes and is the only port the mesh uses. `mesh.mtu` is the MTU of
the mesh interface, between 1280 and 9000: the MTU of the network between the
nodes minus 80. [The mesh between the nodes](../remote-daemon.md#the-mesh-between-the-nodes)
explains both, and how to look at the mesh.

`setup:` on the target is `{user?, origin, origin-host-key?, deploy-key?,
break-glass?}`. `mm target setup` needs it; `mm config validate` does not.

`delivery` on a target (either form) is `fetch` or `push`; the default is
`fetch`, where each host fetches a commit from its origin. With `push`, mm
sends the commit's tracked files and the hosts never contact the git server;
`setup.origin`, `setup.origin-host-key` and `setup.deploy-key` are then
refused, because nothing fetches. See
[deploy without git on the host](../remote-daemon.md#deploy-without-git-on-the-host).

`providers:` on any remote target, with one host or with nodes, is
`{PROVIDER: {address}}`: where the target's hosts reach an external provider
declared under the top-level `providers:`, when that is not the provider's own
`address`. The address is `https://HOST` or `https://HOST:PORT`. See
[read secrets from OpenBao or Vault on the host](../remote-daemon.md#read-secrets-from-openbao-or-vault-on-the-host).

## Schema and validation

`mm config schema` prints the generated JSON Schema without loading a project or
probing a container engine:

```sh
mm config schema > mm.schema.json
mm config validate
```

The same generated schema is published as
[`/schema/mm.schema.json`](/schema/mm.schema.json), with one schema per
top-level key for split files under `/schema/mm/`. Core tests compare that file
byte-for-byte with `mm config schema`, while the hand-written sections above explain
cross-resource behavior that JSON Schema alone cannot express.

`./scripts/generate-artifacts.sh` is the only checked-in generation path. The
schema carries its source and generation command and must not be manually edited.

Schema 1 is the complete current project contract. Missing or different schema
versions are rejected rather than guessed or rewritten. A release that adds a
key keeps schema 1: only a change an older mm would read wrongly raises it.

### Keys mm does not know

mm ignores a key it does not read, and every command that reads the
configuration prints one warning for it on stderr, naming the file and the key:

```text
Warning: unknown key `modules.web.build.sufix` in .config/mm.yaml; this mm ignores it (did you mean `suffix`?)
```

A key that a later release added is ignored the same way, so a project that
uses it still plans and deploys with an older `mm`, or on hosts whose `mmd` is
older. A misspelled key gets the same warning, with the key it most likely
meant. The rule covers `.config/mm.yaml`, split files, the local overrides,
and middleware files.

- `mm config validate` lists each key as a warning, `MM_CONFIG_UNKNOWN_KEY`,
  and still succeeds. `mm config validate --strict` fails with exit code 2
  instead: run it in CI so a typo never reaches a deployment.
- A plan's JSON (`mm deploy plan --output json`, and `mm_deploy_plan` over
  MCP) lists the keys under `unknown_config_keys`, each with `file`, `key`, and
  `suggestion`. They are not part of the plan's hash.
- A remote plan carries the keys each host's `mmd` ignores, since the host reads
  the configuration from the commit it deploys. `mm deploy plan` prints them
  with the nodes that ignore them, such as
  ``Warning: unknown key `targets.production.newer` in .config/mm.yaml; mmd on app-1, app-2 ignores it``.
  Upgrade those hosts with [`mm target setup plan --upgrade`](../how-to/upgrade.md).
- Editors still flag unknown keys: the published JSON Schema allows only the
  keys it lists.

`mm config validate` resolves the selected environment and performs cross-resource,
path, file, ingress, build, environment, and secret-reference checks without
inspecting containers. It also checks every environment's Compose files the
way `mm deploy` and `mm dev up` compile them, and reports what they would
refuse with `MM_COMPOSE_SOURCE`, naming the environments. It knows targets by
name only, as `--for-remote` does: an environment on the target `local` runs
on this machine, and every other one on a host that decides by its own
policy.
