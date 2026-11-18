---
title: Run a cluster
description: List your servers under one target, set them up, and deploy a Compose project across them.
---

A cluster is a target that lists several hosts under `nodes:`. `mm` sets every
host up, joins them over a private network, and deploys your Compose project
across them one node at a time. There are no managers, join tokens, or quorum
to look after: a cluster has nodes, services, networks, and volumes.

This guide goes from a node list to a running deployment, then through the
tasks that follow. Each section links to the full description in
[Set up a remote host](../remote-daemon.md), which covers everything `mm` puts
on a node. If you run Docker Swarm today, [Coming from Docker
Swarm](./from-docker-swarm.md) maps its concepts and Compose keys to these.

## What you need

- A project that `mm` already deploys: `.config/mm.yaml` and its Compose files
  in a git repository. To create one, [add mm to a project](../onboarding.md).
- Two or more Debian or Ubuntu hosts with systemd and OpenSSH. Your machine
  must reach each one over SSH, and the hosts must reach each other on UDP
  port 51820.
- This device's mm key (`mm key create`, then `mm key show`) authorized for
  `root` on every host. Setup logs in with that key, never with `~/.ssh` or an
  SSH agent.
- On your machine: `ansible-playbook`, `ssh-keyscan`, and Linux builds of `mm`
  and `mmd` from the same release. Installing `mm` on Linux with
  `--with-daemon` provides both.
- A git origin that every node can fetch the project from, or
  [`delivery: push`](#deploy-without-git-on-the-nodes) on the target so that
  `mm` sends each commit itself. Also a break-glass key pair whose private
  half you keep offline. See the [requirements](../remote-daemon.md#requirements).

## Declare the nodes

Give the target a name and a host for each node, and point an environment at
it:

```yaml
# .config/mm.yaml
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
      - {name: db-1, host: 203.0.113.13}
environments:
  production:
    target: production
```

That is a complete cluster target. The mesh subnet, each node's mesh address,
the WireGuard port, the address range for networks, and the node that serves
shared volumes all have defaults; the optional `cluster:` block exists only to
[change one](../remote-daemon.md#change-the-defaults).

Setup also needs to know where the nodes fetch the project from and which keys
can always get in. Add a `setup:` block to the target as described in
[declare what setup installs](../remote-daemon.md#declare-what-setup-installs),
then check the file:

```sh
mm -e production config validate
```

```text
shop [production]
  config: /srv/code/shop/.config/mm.yaml
  config:      valid
```

A node name that is not a hostname label, a duplicate host, a `placement` that
names an unknown node, and a Compose service with no entry under `modules` are
reported instead, each with the key to change.

## Set up the nodes

Setup is a plan you review, then an apply that runs the same steps on every
node:

```sh
mm -e production target setup deploy-key
mm config split targets
mm -e production target setup plan
mm -e production target setup apply --plan PLAN_HASH --yes
mm -e production target setup record
```

`deploy-key` prints a public key once; add it to the git origin as a read-only
deploy key and set it as `setup.deploy-key.public-key`. `plan` scans every
node's SSH host key and shows what each node gets, including its mesh peers.
`apply` installs Docker Engine with the Compose plugin (or rootful Podman, for
an environment that uses `podman-compose`; see
[run Podman on the hosts](../remote-daemon.md#run-podman-on-the-hosts)) and
`mmd`, brings up the
WireGuard mesh, prepares shared storage, pushes the environment's secrets, and
applies the users file. `record` writes each node's address, host key, and
daemon identity to `.config/mm/targets.yaml`; commit and push that file.
[Run setup](../remote-daemon.md#run-setup) explains each step and what to do
when one node cannot be reached.

Check the result:

```sh
mm -e production target nodes
```

```text
Target production: 3 node(s)
  NODE             ADDRESS                  MESH            STATE        MMD        DEPLOYED
  app-1            203.0.113.11:22          10.90.0.1       ready        0.1.0      nothing yet
  app-2            203.0.113.12:22          10.90.0.2       ready        0.1.0      nothing yet
  db-1             203.0.113.13:22          10.90.0.3       ready        0.1.0      nothing yet

Networks (cluster.networks 10.64.0.0/12)
  No network of this environment has addresses yet; the first deployment gives them out.
```

Every node is `ready`. Below this the command lists the addresses each node
already uses, which the cluster's ranges must not overlap.
`mm -e production target mesh status` shows the WireGuard link between each
pair of nodes and names the port to open when two of them have no handshake;
see [the mesh between the nodes](../remote-daemon.md#the-mesh-between-the-nodes).

## Say where each service runs

Give a service a `placement`: a node name, a group, the word `all`, or a list.

```yaml
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
      - {name: db-1, host: 203.0.113.13}
    cluster:
      groups: {web: [app-1, app-2]}
modules:
  shop:
    path: shop
    services:
      web: {stack: app, placement: web} # app-1 and app-2
      db: {stack: app, placement: db-1} # db-1 only
      worker: {stack: app} # no placement: the first node, app-1
```

A service runs one container on each node its placement names, unless it sets
`replicas` or `autoscale` (see [scale a service](#scale-a-service)). Every service
in a stack's Compose files needs an entry under `modules`, so that `mm` knows
where it runs; `mm config validate` names any that is missing. A named Compose
volume is local to its node, so keep a service that stores data pinned to one
node. [Place services on nodes](../remote-daemon.md#place-services-on-nodes)
covers moving a service and the Compose keys that tie two services to one
host.

## Deploy

Push the commit, then plan and apply it. Every node fetches the commit from
the origin itself, or receives it from `mm` with
[push delivery](#deploy-without-git-on-the-nodes):

```sh
mm -e production deploy plan
mm -e production deploy apply --plan PLAN_HASH --yes
```

The plan opens with what the deployment does across the nodes, then has one
section per node: what each node starts, changes, and stops, and the addresses
each network gets there. One hash covers every section:

```text
Cluster deployment plan 0a4ba0ae34677ecf6507139bc887347d251e8ca7ba19e9f006fc2f016be9f9ab
  target:   production (app-1, app-2, db-1)
  commit:   2e445474cb8dc3608f384f908f60ab7cf47c89e9
  expires:  2026-11-13T17:43:32Z
  networks span the nodes: addresses from 10.64.0.0/12, between nodes through mm0 (MTU 1420)
  service names: each node answers its own containers, on its address on each network (port 53), with a service's healthy containers on the same node, or else on every node
  rollout: app on app-1, then app-2, then db-1
           a stack may take 10m on a node to become healthy (deploy.convergence-timeout); the next node starts once the others resolve its containers
           on failure every node this deployment changed returns to what it ran before
```

Apply replaces one node at a time. A node is done when its containers pass
their health checks and every other node resolves them by name:

```text
==> app-1: preparing
==> app-1: app
==> app-2: preparing
==> app-2: app
==> db-1: preparing
==> db-1: app
==> app-1: finishing
==> app-2: finishing
==> db-1: finishing
Deployment 0a4ba0ae34677ecf6507139bc887347d251e8ca7ba19e9f006fc2f016be9f9ab on production [production]
  app-1            applied      stacks: app
  app-2            applied      stacks: app
  db-1             applied      stacks: app
```

Give each service a health check in its Compose file: without one, a container
counts as healthy as soon as it runs. To replace a container without a gap,
set `update: {order: start-first}` on the service, and the apply says
`app: started the new web next to the old` on each node. To order stacks, set
`after:` on a stack. Both are in [rolling deploys and
rollback](../remote-daemon.md#rolling-deploys-and-rollback).

`mm -e production deploy status` shows what every node runs and any step still
in flight, and `mm -e production deploy cancel` stops a deployment that is
stuck: the nodes it changed return to what they ran before. See [follow a
deployment, and stop one that is
stuck](../remote-daemon.md#follow-a-deployment-and-stop-one-that-is-stuck).

Modules that `mm` builds deploy from a build receipt, because a node never
builds. Follow [deploy a build to a remote
host](../concepts/builds-and-deployments.md#deploy-a-build-to-a-remote-host).

## Roll back

A deployment that fails rolls itself back. When a container fails its health
check, exits, or keeps restarting, the deployment stops, every node it changed
returns to what it ran before, and the nodes it had not reached are never
touched:

```text
==> app-1: preparing
==> app-1: app
==> app-2: preparing
==> app-2: app
==> app-2: rolling back
==> app-1: rolling back
Deployment 85549657b9e8dbab7d79adda5e1cbeb919803cfcab243882043abfbae414ebb4 on production [production]
  app-1            rolled back  runs what it ran before
  app-2            FAILED       service 'web' of stack 'app': its health check fails; rolled back to what it ran before
                   last log lines of the failed container:
                     listening on :3000
                     cannot reach the database at db:5432
  db-1             not started  unchanged
```

To return from a deployment that finished:

```sh
mm -e production deploy rollback
```

It prints a plan for the commit the nodes ran before and applies it after you
confirm. `mm -e production deploy status` shows what every node runs, what it
ran before, and a rollout that was interrupted.

Volumes are not rolled back: the earlier version runs against the data the
later one left. Plan schema changes so that the previous version still works
with them. More in [return to the previous
deployment](../remote-daemon.md#return-to-the-previous-deployment).

## Connect services across nodes

Declare a network in `mm.yaml` and put services on it. It spans every node
that runs one of them:

```yaml
networks:
  backend: {}

modules:
  shop:
    services:
      web: {stack: app, role: http, networks: [backend], placement: web}
      db: {stack: app, role: database, networks: [backend], placement: db-1}
```

`web` on `app-1` reaches the database at `db:5432`, although `db` runs on
`db-1`. The traffic travels inside the mesh, encrypted, and two containers
that share no network cannot reach each other.

| Name       | Resolves to                                                                             |
| ---------- | --------------------------------------------------------------------------------------- |
| `db`       | The healthy containers of `db` on the asking node, or on every node when it has none.   |
| `tasks.db` | Every healthy container of `db`, on every node.                                         |
| `db.db-1`  | The healthy container of `db` on node `db-1`.                                           |

`mm -e production service status` lists what each name resolves to. When no
container of a service passes its health check, the names and the ingress
answer with its running containers all the same, and `mm doctor` reports
`MM_SERVICE_ANSWERED_UNHEALTHY`. A network that only a Compose file declares
stays local to its host.

Limits: a network spans at most 16 nodes, a node runs at most 253 containers
of one network, and the default address range holds 256 networks. Addresses
are IPv4. See [networks across nodes](../remote-daemon.md#networks-across-nodes)
and [service names across nodes](../remote-daemon.md#service-names-across-nodes).

## Share a volume between nodes

Use a shared volume for files that the containers of a service all need on
every node, such as uploads or generated assets. List the Compose volume under
`shared-volumes` on its stack:

```yaml
stacks:
  app:
    files: [compose.yaml]
    shared-volumes: [uploads]
```

**Use a local volume for anything where performance matters: databases,
queues, search indexes, caches.** A shared volume is a network mount. Next to
the storage node, creating a file takes 7 to 9 ms instead of 0.15 ms; on a
node 58 ms away, every file opened and every write flushed waits a round trip,
and a large read runs at 8 MB/s instead of 1 GB/s. It also does not give the
locking guarantees a database needs, and it stops answering while the node
that keeps it is down. Give such a service an ordinary volume and pin it to one
node with `placement`. `mm config validate` refuses a service with
`role: database` or `role: queue` on a shared volume; `allow-shared-volumes:
true` on the service overrides that. The [measurements](../remote-daemon.md#shared-volumes-across-nodes)
are in the remote host guide.

The Compose file stays as it is. One node, the first unless you set
`cluster.storage`, keeps the files under `/var/lib/mm/storage` and serves them
to the other nodes over the mesh with NFS. Nothing is installed on the nodes
that mount it. The deployment plan says which node keeps each volume and which
nodes mount it, and the deployment creates the directory before it replaces a
container.

Check it:

```sh
mm -e production target storage status
```

```text
Target production: app-1 keeps shared volumes in /var/lib/mm/storage, 26.2 GiB free of 29.3 GiB

  app/uploads              12.0 KiB in 3 files; mounted on app-1, app-2

  app-1                    keeps the volumes and serves them on 10.90.0.1
  app-2                    reaches app-1 in under 1 ms
  db-1                     reaches app-1 in 58 ms

Every node reaches the storage node.
```

What to know before you rely on it:

- The storage node is a single point of failure. While it is down, reads and
  writes of shared files on the other nodes wait, and continue when it is
  back. `mm` does not replicate or back up the files: back up
  `/var/lib/mm/storage` on the storage node yourself.
- A node that boots while the storage node is away starts its other services;
  the ones that mount a shared volume start once the storage node answers.
- Taking a volume out of `shared-volumes`, a rollback, and a failed deployment
  never delete files. `mm -e production target storage delete uploads` shows
  what it would remove and deletes it only with the hash it prints.
- A volume that becomes shared starts empty on the storage node. Copy the
  files from the old local volume yourself.
- Changing `cluster.storage` is refused while the old storage node holds
  files, with the steps to move them.
- Shared volumes need the mesh `mm` builds. A target that sets
  `cluster.mesh.interface` serves none.

See [shared volumes across nodes](../remote-daemon.md#shared-volumes-across-nodes)
for where the files are, what happens while the storage node is down, and how
the export is kept private.

## Read logs and status

A service is all of its containers on all of its nodes, and every command
means all of them unless you narrow it:

```sh
mm -e production service status
mm -e production service logs web --follow
mm -e production service logs web --node app-2
mm -e production service logs
```

`service status` asks every node at once and lists each service once, with a
row for each node's container:

```text
  STACK  SERVICE  STATE         NODE   CONTAINER  HEALTH   STATUS      IMAGE
  app    web      degraded 1/2  app-1  web-1      healthy  Up 2 hours  registry.example.com/shop/web:1.4.2
                                app-2  -          -        no answer   -
  app    db       running 1/1   db-1   db-1       healthy  Up 2 hours  postgres:17
  app    worker   running 1/1   app-1  worker-1   running  Up 2 hours  registry.example.com/shop/worker:1.4.2
```

The count under `STATE` is the containers that run and pass their health
check, out of those the service should have. A container whose health check
has not passed yet makes the service read `starting`, as in the middle of a
rollout. A node that does not answer reads `no answer`, and the reason follows
the table:

```text
  node app-2: accepts SSH, but mmd is not running there (the relay found no daemon socket). Start it on the host with `systemctl start mmd`; `journalctl -u mmd` shows why it stopped
```

The other nodes' rows are shown all the same. Between the two, status lists
what each service name resolves to on each network.

`service logs web` merges the lines of every container of `web` into one
stream in time order and starts each line with its node and container:

```text
app-1 web-1 | GET / 200 12ms
app-2 web-1 | GET /assets/app.css 200 2ms
app-1 web-1 | GET /api/projects 200 18ms
```

Without a service name, `service logs` reads every service of the environment
the same way, and `--node` reads only the nodes you name. A node that does not
answer is one line in the stream, marked with `!`, and the command fails only
when no node answered. Lines are ordered by the timestamps the container
engines record, so two nodes whose clocks differ can swap lines that are
closer together than that difference: keep the nodes' clocks synchronised with
NTP or chrony. [Status, logs, and commands](../runtime-observability.md) has
every column and option.

Status and logs take no deployment lock, so they answer in the middle of a
rollout and while a node is down. `mm service exec` runs a command in the
service's container on the first node of its placement; the terminal UI runs
it in the container of the node row you select.

## Harden the containers

One line in `.config/mm.yaml` gives every service of a deployment a read-only
root filesystem, no Linux capabilities, no new privileges, the engine's
default seccomp profile, and a writable `/tmp`:

```yaml
hardening: strict
```

`mm deploy plan` prints what each service will run with, and the plan hash
covers it:

```text
    security: web: read-only root, no capabilities, no new privileges, default seccomp, tmpfs /tmp
```

A service that cannot take one of these defaults leaves it out with a reason,
which the plan prints next to it:

```yaml
modules:
  shop:
    services:
      db:
        stack: app
        hardening:
          except:
            read-only: Postgres writes its socket under /var/run
```

It does not apply to `mm dev` on your machine. Images that start as root and
switch to their own user, such as the official `postgres` and `nginx` images,
need a few capabilities back with `cap_add:` in the Compose file. Each node's
`/etc/mm/host.yaml` can also offer OCI runtimes such as gVisor's `runsc` and
put every service that publishes a port under one of them. Service names,
shared volumes, and secret files work unchanged under a read-only root.
[Hardening](../hardening.md) lists every default and how to choose a runtime.

## Ship configuration files

A file that is not a secret and not part of the image, such as a proxy
configuration or an init script, goes under the service's `config-files`.
Each node that runs the service writes it and mounts it read-only into the
container:

```yaml
modules:
  proxy:
    path: proxy
    services:
      proxy:
        stack: app
        config-files:
          /etc/nginx/conf.d/site.conf: { template: site.conf.tmpl }
```

`file:` ships a file from the module's directory as it is; `template:` replaces
each `{{ KEY }}` with the environment's binding of that key. When the file
changes, the plan says which services are replaced, and only those roll, one
node at a time. A config file is at most 1 MiB and never takes a secret. See
[configuration files](../config-files.md).

## Read secrets on the nodes

Secrets from `mm`'s own store reach the nodes sealed during setup. Secrets
kept in an external OpenBao or Vault can be read by the nodes themselves, so
their values never pass through the machine that runs `mm deploy`. Give the
nodes an AppRole that may only read the environment's secrets, then connect
each node that runs a service using such a secret, once, as root on the node.
The command reads the role ID and the secret ID from stdin, one per line:

```sh
{
  bao read -field=role_id auth/mm-approle/role/mm-host-production/role-id; echo
  bao write -f -field=secret_id auth/mm-approle/role/mm-host-production/secret-id; echo
} | mmd secrets connect --project PROJECT_ID --environment production \
      --provider team-secrets --address https://10.90.0.1:8200 \
      --ca-file /etc/mm/bao-ca.pem
```

It logs in before it keeps anything and stores the credential encrypted, for
root only. A plan then carries the version of every secret each node read,
and a rotated secret replaces only the services that use it. A node asks the
store only for the secrets of its own services, and a node whose services use
none needs no connection. When the nodes reach the store at another address
than the provider's, such as a node's mesh address, declare it on the target:

```yaml
targets:
  production:
    providers:
      team-secrets: {address: https://10.90.0.1:8200}
```

[Read secrets from OpenBao or Vault on the
host](../remote-daemon.md#read-secrets-from-openbao-or-vault-on-the-host) has
the store's policy and role, and what a node does when the store does not
answer.

## Deploy without git on the nodes

When the nodes may not reach your git server, have `mm` send each commit:

```yaml
targets:
  production:
    delivery: push
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
      - {name: db-1, host: 203.0.113.13}
    setup:
      break-glass:
        - {name: ops-recovery, key: ssh-ed25519 AAAA… ops@offline}
```

There is then no `origin` and no deploy key in `setup:`, and the nodes never
run git. `mm deploy plan` sends every node of the plan a bundle of the files
the commit tracks, read from your checkout's git objects, and each node checks
them against the commit before it keeps them. A node that already holds the
commit is sent nothing, so a rollback costs no upload. Images still come from
their registries, so the nodes need a route to those. With push delivery a
node cannot ask the git server which branches hold a commit: `deploy-from` is
checked against the branches your client states, so give `deploy` on such a
target only to CI and to people who may push to those branches. See [deploy
without git on the host](../remote-daemon.md#deploy-without-git-on-the-host)
and [what push cannot guarantee](../remote-daemon.md#what-push-cannot-guarantee).

## Add or remove a node

Add the node to `nodes:`, or delete it, commit, and run setup again:

```sh
mm -e production target setup plan
mm -e production target setup apply --plan PLAN_HASH --yes
mm -e production target setup record
```

The plan shows the one mesh entry that changes on every other node. A new node
runs nothing until a service's `placement` names it and you deploy. Removing a
node drops it from the mesh; setup does not contact a host that is no longer
listed, and the plan says how to take the mesh off it. Move its services to
other nodes and deploy before you remove it. See [add or remove a
node](../remote-daemon.md#add-or-remove-a-node).

## When a node goes down

Nothing is rescheduled. `mm` has no scheduler and no consensus store: the
services placed on a node run on that node and nowhere else.

- The other nodes keep running. Within about ten seconds they stop answering
  service names with the lost node's containers, so a service that also runs
  elsewhere keeps working by name.
- A service that runs only on the lost node is down until the node is back.
  For a service that has to survive a node, place it on more than one.
- When the node boots, the mesh, the firewall rules, and its services come
  back without a deployment.
- While it is down, `mm deploy plan` stops and names the node. To deploy to
  the others, pass `--nodes` with their names; see [when a node does not
  answer](../remote-daemon.md#when-a-node-does-not-answer).
- If the lost node is the storage node, reads and writes of shared volumes on
  the other nodes wait until it returns.

To replace a node for good, remove it from `nodes:`, add its replacement,
change the `placement` of its services, run setup, and deploy. Data in its
local volumes is not moved for you.

## Scale a service

`replicas` runs a count of containers spread over the placement's nodes, and
`autoscale` lets the count follow the load between `scale.min` and
`scale.max`:

```yaml
modules:
  shop:
    services:
      web:
        stack: app
        placement: all
        autoscale:
          scale: { min: 1, max: 4 }
```

A container that leaves first drops out of the names and the ingress, drains,
and then stops, so scaling down drops no request.
[Autoscale a service](../remote-daemon.md#autoscale-a-service) covers the
metrics, the thresholds, and how to watch what the autoscaler decided.

## Route traffic into the cluster

An `ingress:` route on a stack works on a cluster as written for one host:

```yaml
environments:
  production:
    target: production
    domains: {shop: shop.example.com}

stacks:
  app:
    files: [compose.yaml]
    ingress: {domain: shop, service: web}
```

The first node runs the ingress proxy on ports 80 and 443 and sends each
request to a healthy container of `web`, on its own node when one runs there
and on any node otherwise. Point the domain's DNS record at that node and open
the two ports. To run the proxy on more nodes, list them under
`cluster.ingress`, written like a `placement`. With one ingress node, or
traffic that does not enter evenly across them, write
`ingress: {balance: spread}` on the environment to send requests to the
containers on every node; a route to an autoscaled service spreads already.
The plan says where traffic comes in:

```text
  ingress: app-1 takes the traffic on ports 80 and 443 and sends each request to a healthy container on the same node, else on any node; HTTPS uses a certificate from ACME (Let's Encrypt) for each HTTPS host
    https://shop.example.com -> app/web port 8080
```

A container that an update, a rollback, a move, or autoscale takes away first
leaves the routes and the names, finishes the requests it has for the
service's `drain` (`1m` by default), and only then stops. [Route traffic into
the cluster](../remote-daemon.md#route-traffic-into-the-cluster) covers a
certificate you provide, the drain, and the limits.

## When a container stays unhealthy

Each node restarts its own containers whose health check has failed for three
minutes, one at a time and without dropping a request while another container
of the service answers, at most five times an hour; then it leaves the
container out and `mm service status` and `mm doctor` report
`MM_SERVICE_RESTART_LIMIT`. Never during a deployment. `health:` on a service
changes the times or turns it off: see [restart a container that keeps failing
its health check](../remote-daemon.md#restart-a-container-that-keeps-failing-its-health-check).

## Restart a service

```sh
mm -e production service restart web
```

```text
Restarted web on app-1, then app-2, one node at a time, each healthy before the next (2 containers).
```

The service restarts on one node at a time, each container healthy again
before the next. While another container of the service answers, each one
leaves the routes and the names and drains before it restarts, so the restart
drops no request. The image, configuration, and volumes stay as deployed. It
needs the `operate` level in the users file, and each restart is in the
node's audit log. See [restart a service](../runtime-observability.md#restart-a-service).

## Run a job

A job runs on one node: the first node its service is placed on, or the node
its own `placement` names. It joins the service's networks there, so it
reaches every other service by name, wherever it runs:

```yaml
jobs:
  migrate:
    module: shop
    service: web
    command: ["bin/rails", "db:migrate"]
    run: before-deploy
```

```sh
mm -e production job run migrate
```

`run: before-deploy` makes every deployment run it before `web` is started or
replaced on any node, and a failure stops the deployment there; `after-deploy`
runs once the deployment converged, and `once` the first time the environment
is deployed with it. `mm deploy plan` lists the jobs a deployment runs, and
`mm deploy logs` shows their output. See [run a job on a remote
target](../jobs-and-schedules.md#run-a-job-on-a-remote-target) and [run a job
with a deployment](../jobs-and-schedules.md#run-a-job-with-a-deployment).

## Before production

- Go through the [production checklist](./production-checklist.md): each item
  names the test or release file behind it.
- Pin the release with `deploy.mm-version` on the environment, so that nobody
  deploys with another `mm` and every node runs the same `mmd`.
- To upgrade, install the new `mm` and run setup again: it replaces `mm` and
  `mmd` one node at a time without restarting a container. See [upgrade mm
  and your hosts](./upgrade.md).
- Keep your SSH key behind a passphrase: `mm doctor` names a plain key and
  `mm key protect` encrypts it.

## What a cluster does not do

- Nothing is rescheduled when a node goes down: its containers come back when
  the node does, and the other nodes keep answering for services that run on
  them too.
- A named Compose volume is local to its node. Keep a service that stores data
  pinned to one node, or use a shared volume for files where speed does not
  matter.

[Known issues](../reference/known-issues.md) lists the remaining limits.
