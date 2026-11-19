---
title: Set up a remote host
description: Install mmd on a Linux host with mm target setup, so developers reach it through one restricted SSH account.
---

Remote hosts run `mmd`, the mm daemon. Developers connect over SSH to a single
locked `mm` account whose only command is the relay to `mmd`. The daemon checks
each request against the project's users file and runs the operation itself.
Nobody needs a shell, sudo, or the host's Docker socket.

```text
mm → SSH (account mm, relay only) → mmd → container runtime
```

`mmd` fetches each deployed commit from the project's git origin with its own
read-only key, so a deploy runs exactly what is committed.

This is an advanced setup. `mm target setup` installs everything
on the host in one reviewed step; you need root on the host only for that step.

## Requirements

On the host:

- Debian or Ubuntu with systemd and OpenSSH, and an admin account that can
  use `sudo`. Setup installs Docker Engine from Docker's own repository, or
  Podman from the distribution for an environment that uses `podman-compose`
  (see [run Podman on the hosts](#run-podman-on-the-hosts)).
- `/etc/ssh/sshd_config` includes `/etc/ssh/sshd_config.d/*.conf` before its
  own settings. Debian and Ubuntu do this by default.

On your machine:

- `ansible-playbook` (in `/usr/bin` or `/usr/local/bin`) and `ssh-keyscan`.
- The admin account's key: either the target's own key file (`mm target add
  --identity-file`), or an SSH agent (`SSH_AUTH_SOCK`) holding the key.
- `mm` and `mmd` built for the host, from the same release. A Linux `mm`
  installed with `--with-daemon` has `mmd` beside it, and setup uses that pair
  by default. Otherwise, extract the host's Linux release archive and pass its
  directory with `--host-binaries`.
- A break-glass key: an Ed25519 key pair that is admin on every project the
  host runs. Keep the private half offline:

  ```sh
  ssh-keygen -t ed25519 -C ops@offline -f ops-recovery
  ```

## Set up the host

Add the host as a private target that logs in as the admin account. Only setup
uses it; developers connect through the repo's target as `mm`:

```sh
mm target add prod-eu-setup --host prod-eu.example.com --user admin \
  --host-key-fingerprint SHA256:REVIEWED_HOST_KEY_FINGERPRINT
```

Plan the setup for the environment the host runs. `--origin` adds the remote
access layer to the plan:

```sh
mm --environment production --target prod-eu-setup --runtime docker-compose \
  target setup plan \
  --origin git@github.com:acme/shop.git \
  --origin-host-key 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl' \
  --break-glass "ops-recovery=$(cat ops-recovery.pub)"
```

| Flag                | Meaning                                                                                                                       |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `--origin`          | Git URL the host fetches each deployed commit from, with its own read-only key: HTTPS, SSH, or an absolute path on the host.  |
| `--origin-host-key` | The git server's SSH host key, `<type> <base64>`, for SSH origins. Copy it from the git host's published keys; `mmd` pins it. |
| `--break-glass`     | `NAME=PUBLIC_KEY`. Always works, even when a users file locks everyone out. Repeat for more keys.                             |
| `--delivery push`   | Instead of `--origin`: the host fetches nothing and mm sends it each commit. See [deploy without git on the host](#deploy-without-git-on-the-host). |
| `--host-binaries`   | Directory with `mm` and `mmd` for the host. Default: the directory of the `mm` you are running.                               |

The plan lists the `install-access` step, the origin, the break-glass keys,
and the path and digest of each binary it will install. Review it, then run
the apply command it prints. That command repeats the same flags, because the
plan hash covers them:

```sh
mm --environment production --target prod-eu-setup --runtime docker-compose \
  target setup apply --plan PLAN_HASH --origin … --break-glass … --host-binaries …
```

Setup first runs the project's bootstrap playbook, which installs the container
runtime. Then it runs mm's own access playbook, which:

1. Installs `git` and the OpenSSH server.
2. Installs `mmd` at `/usr/libexec/mmd` and puts `mm` beside it as
   `/usr/libexec/mm.next`, and stops if they report different releases.
3. Creates the `mm` account. It has no password, its shell is the relay
   (`/usr/libexec/mm-relay-shell`), and its only group is `mm-relay`. Setup
   stops if the account is in any other group.
4. Writes `/etc/mm/mmd.json` and installs `mmd.service`, and
   `mm-mesh.service` and `mm-network.service`, which stay disabled on a host
   that is not a [cluster node](#the-mesh-between-the-nodes).
5. Enrolls the project in `/etc/mm/host.yaml` with the origin, its pinned host
   key, the environment, and the break-glass keys. Creates the deploy key at
   `/var/lib/mm/projects/PROJECT_ID/deploy-key`, unless a key is already
   there.
6. Installs the sshd drop-in and checks it with `sshd -t` and `sshd -T`: the
   `mm` account must get only the relay, with no TTY or forwarding. If the
   check fails, setup removes a new drop-in and does not reload sshd.
7. Writes `/etc/ssh/mm_authorized_keys`, moves `mm` to `/usr/bin/mm` (the
   relay every SSH session runs), starts `mmd` at once, and checks it is
   running.

Expected result:

```text
Bootstrapped target prod-eu-setup (…)
  …
  verified: remote-access
  mm and mmd: 0.1.0
  host key:   ssh-ed25519 AAAA…
  daemon:     0f3c…
  deploy key: ssh-ed25519 AAAA… mmd-deploy-019c527b-…
```

Add the deploy key to the git host as a read-only deploy key. Keep the host key
and daemon identity for the repo target in the next section.

Run setup again to upgrade `mm` and `mmd`, to add another environment, or to
enroll another project from that project's checkout. Setup keeps the other
projects, ceilings, and exceptions in `host.yaml`, and adds break-glass keys
without replacing existing ones. It rewrites `host.yaml` from its parsed
contents, so comments in that file are lost.

Two projects on the same hosts need ids that differ in their first 12 hex
digits: the names of a project's networks, name lookups, and routes on a host
start with them. Ids `mm init` writes always do. Setup refuses to enroll a
project whose id starts like an enrolled project's, such as an id copied from
another project and edited at its end, and names both projects. Set
`project.id` in that project's `mm.yaml` to a new UUID (`uuidgen` prints one)
and run setup again.

## Adjust the host policy

`/etc/mm/host.yaml` is the host's own limit on every project. It is root-owned
mode `0600`. Projects cannot change it. Setup writes the project's name,
origin, environments, and break-glass keys; add ceilings, the first
`deploy-from` rule, and workload exceptions by hand:

```yaml
schema-version: 2
break-glass: # admin everywhere; keep the private key offline
  - name: ops-recovery
    key: ssh-ed25519 AAAA… ops@offline
projects:
  019c527b-6242-7b43-bfd4-bd60e27c1de3: # the project's id from .config/mm.yaml
    name: shop
    origin: git@github.com:acme/shop.git
    origin-host-key: ssh-ed25519 AAAA… # the git host's key, pinned
    environments: [staging, production] # environments this host runs
    initial-deploy-from:
      production: [main] # until the repo sets deploy-from
    ceiling:
      production: { level: deploy } # the most the repo may grant here
```

- **Break-glass keys** always work, even when a users file locks everyone out.
- **`ceiling`** caps what the repo can grant per environment. Without one, the
  repo may grant up to `admin`.
- **`origin`** is the only place `mmd` fetches code from, with the key at
  `/var/lib/mm/projects/PROJECT_ID/deploy-key`.
- **`allow`** lets one service use one Compose feature that reaches the host,
  in the environments the host runs. See
  [what a deploy may run](./remote-access.md#what-a-deploy-may-run); `mm config
  validate --for-remote --host-policy FILE` checks a project against a copy of
  this file.
- **`runtimes`** and **`exposed-runtime`**, at the top of the file, say which
  OCI runtimes a service may name and which one the host runs everything
  under that is reachable from outside. Without them a service that sets
  `runtime:` is refused. See
  [choose an OCI runtime](./hardening.md#choose-an-oci-runtime).

After editing, check the file and restart the daemon:

```sh
mmd access validate
mmd access render-keys
systemctl restart mmd.service
```

## Connect a client

Commit the host's public connection facts to the repo, so every clone can reach
the environment. Use the host key and daemon identity that setup printed. If
someone else ran setup, get them through a trusted channel; on the host they
are `/etc/ssh/ssh_host_ed25519_key.pub` and `mmd --print-identity`. Name the
target differently from the private setup target, because a private target
with the same name takes precedence:

```yaml
# .config/mm.yaml
targets:
  prod-eu:
    host: prod-eu.example.com
    host-key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA…
    daemon-fingerprint: 0f3c… # 64 characters
environments:
  production:
    target: prod-eu
    deploy-from: [main]
```

These are public keys, not secrets. mm checks both on every connection and
connects as `mm`. The first time a client connects it pins them; if a later
commit changes either, the client refuses until you review the change:

```sh
mm target trust prod-eu          # shows pinned and new keys
mm target trust prod-eu --yes    # accept after checking with the host admin
```

## Set up a cluster

A target can list several hosts under `nodes:` instead of one `host`. A name
and an address for each node are a complete cluster target:

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

`mm config validate` accepts this as written. `mm target setup` then prepares
every node from the declaration: Docker Engine with the Compose plugin, `mmd`,
a private WireGuard [mesh](#the-mesh-between-the-nodes) between the nodes,
shared storage, secrets, and access. The nodes must reach each other on UDP
port 51820, which is the only port the mesh uses. The nodes run Docker Engine
unless the environment uses `podman-compose`: then setup installs Podman
instead, see [run Podman on the hosts](#run-podman-on-the-hosts).

Once the nodes are set up and recorded, you deploy to them as one target: see
[deploy to a cluster](#deploy-to-a-cluster).

[Run a cluster](./how-to/cluster.md) is the short path through the sections
below, and [Coming from Docker Swarm](./how-to/from-docker-swarm.md) maps an
existing project to this target.

### Declare what setup installs

Setup needs a `setup:` block on the target: the git origin every node fetches
the project from, the environment's read-only deploy key, and the break-glass
keys. `mm config validate` does not require it. A target with
`delivery: push` needs only the break-glass keys; see
[set up a host for push](#set-up-a-host-for-push).

```yaml
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
      - {name: db-1, host: 203.0.113.13}
    setup:
      origin: git@git.example.com:acme/shop.git
      origin-host-key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA…
      deploy-key: {secret: origin.deploy-key, public-key: ssh-ed25519 AAAA… mm-deploy}
      break-glass:
        - {name: ops-recovery, key: ssh-ed25519 AAAA… ops@offline}
```

`setup.user` is the account setup logs in as on every node, `root` unless you
set it. Create each node with this device's mm key (`mm key show`) authorized
for that account. mm authenticates with this device's mm key
(`mm key create`), never with `~/.ssh` or your SSH agent. The key is
[encrypted](./remote-access.md#keys-are-encrypted-and-only-mm-uses-them),
and mm opens it itself: while setup applies, an `ssh-agent` of mm's own
holds it for Ansible and ends with the command. In CI, set `MM_SSH_KEY` to
a machine key's private key instead.

Every node needs what a single host needs, and your machine needs the same
tools; see [requirements](#requirements).

A private target with the same name (`mm target add`) takes precedence, for
example to go through a jump host.

### Run setup

From the repo:

```sh
mm -e production target setup deploy-key   # once: prints the public key
gh repo deploy-key add - --title 'mm production' <<<'ssh-ed25519 AAAA… mm-deploy-app-production'
# set setup.deploy-key.public-key to that line and commit
mm config split targets                     # once: record rewrites .config/mm/targets.yaml
mm -e production target setup plan          # scans host keys, shows each node's mesh peers
mm -e production target setup apply --plan HASH --yes
mm -e production target setup record        # writes addresses, host keys, daemon identities
git add .config/mm/targets.yaml && git commit -m "Record production nodes" && git push
```

`apply` runs these steps in order, and each one is safe to repeat:

1. **prepare-nodes**: sets each node's hostname, writes `/etc/mm/node.env`,
   and installs the WireGuard tools.
2. **provision-nodes**: runs the project's `.config/ansible/bootstrap.yml` and
   installs the remote access layer on each node, as single-host setup does.
3. **connect-nodes**: each node's `mmd` makes its WireGuard key and brings up
   the mesh interface `mm0` with the peers the plan shows. Setup then checks
   that every node reaches every other one over the mesh, including with one
   packet as large as the mesh MTU.
4. **serve-storage**: installs an NFSv4.2 server on the storage node. It
   exports `/var/lib/mm/storage` to the nodes' mesh addresses only, and listens
   on the storage node's mesh address only. Every node loads the firewall
   rules that keep containers away from the storage port, and every other
   node proves that it can mount the export. See
   [shared volumes across nodes](#shared-volumes-across-nodes).
5. **push-secrets**: sends the environment's secrets and the deploy key, sealed
   to each node.
6. **sync-access**: runs `mmd access sync` on every node at `HEAD`, or at
   `--commit SHA`. The commit must be pushed to a branch the environment
   deploys from. Each node's `mmd` keeps its own access state.
7. **verify**: checks that host keys, daemon identities, and versions match on
   every node.

The plan binds each node's scanned SSH host key. A key that differs from the
recorded one stops the plan; after rebuilding a node, plan again with
`--trust-new-host-keys` and check the fingerprints it prints. The plan also
binds each secret's revision and every node's mesh peers, so changing a secret
or a WireGuard key after review means planning again.

The plan also says, for each node whose host key the repo records, which
release of `mm` and `mmd` it replaces with this one's, such as `app-1: replaces
mm and mmd 0.1.0 (remote protocol 15) with 0.2.0 (remote protocol 16)`. A node
that runs a newer release than the `mm` you run setup with is refused: setup
would install the older `mm` and `mmd` there, and clients that are up to date
could no longer reach it. Install that release or a later one of `mm` on your
machine and plan again; `--downgrade` installs the older release anyway.

Setup also refuses to replace what a node runs with an older release or with
another build of the same release, such as a teammate's own build, so that
running setup never swaps the `mm` and `mmd` that run your hosts by
accident. Pass `--upgrade` to install this one, or pin the release with
[`deploy.mm-version`](reference/configuration.md#bindings-environments-and-secrets),
which lets setup install exactly that release. The plan names each node's
build, such as `app-1: replaces mm and mmd 0.2.0 (remote protocol 17, build
3f9a1c2e7b4d) with 0.2.0 (remote protocol 17, build 7d0e5a91c2f3)`. Nodes
report their release from this release of `mm` on, and only on a target whose
`cluster.mesh` mm runs (WireGuard).

Expected result: `apply` finishes after `verify`, and `record` fills in each
node's facts:

```yaml
# .config/mm/targets.yaml
production:
  nodes:
    - name: app-1
      host: 203.0.113.11
      private-address: 10.90.0.1
      host-key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA…
      daemon-fingerprint: 0f3c… # 64 characters
    # app-2 and db-1 follow, with 10.90.0.2 and 10.90.0.3
```

A node's mesh address is its `private-address` when it declares one. Otherwise
it is the lowest free address in the mesh subnet, in the order the nodes are
listed: `10.90.0.1` for the first node, `10.90.0.2` for the second. Because
`record` writes the assigned addresses back, later edits to the node list do
not move them.

### Add another environment on the same nodes

Point the environment at the target and run setup for it. A target has one
deploy key for all its environments, because each node keeps one key per
project. The private half is a secret of each environment, so the second
environment needs the key in its own store first:

```sh
mm -e staging target setup deploy-key   # copies the target's key from production
mm -e staging target setup plan
mm -e staging target setup apply --plan HASH --yes
```

`deploy-key` takes the key from an environment of the same target whose
secrets on this machine hold it, preferring the one
`setup.deploy-key.public-key` names, and prints where it came from. The git
host already knows that key, so nothing else changes. It makes a new key only
when no environment of the target holds one. Until setup has run for the
environment, its nodes refuse it with "this host does not run environment
'staging'" and the setup command to run.

### Set up some of the nodes

When one node cannot be reached, or answers with an SSH host key you do not
trust yet, `setup plan` stops and names it. To set up the others now, for
example to roll a new `mm` and `mmd` out to them, pass the nodes to act on to
both commands:

```sh
mm -e production target setup plan --nodes app-1,app-2
mm -e production target setup apply --plan HASH --nodes app-1,app-2 --yes
```

Setup does not contact a node that is left out: it keeps the `mm` version and
the configuration it has. The plan lists the nodes it leaves out and how each
step limits itself:

- **connect-nodes** keeps the WireGuard peer entry each node already has for a
  left-out node, and checks reachability only between the nodes it sets up. A
  left-out node does not learn about changes to the others until a setup run
  includes it.
- **serve-storage** does not run when the storage node is left out; its export
  stays as it is. When the storage node is set up, it still exports to every
  node, and only the nodes set up now load the rules and prove the mount.
- **push-secrets**, **sync-access**, and **verify** run on the selected nodes
  only. `verify` compares versions among them, so a left-out node may run an
  older version until you set it up; `mm doctor` reports the difference.

`mm target setup record` after a subset run updates the nodes that were set up
and leaves the others' recorded facts alone.

### Change the defaults

The optional `cluster:` block overrides defaults. This example writes every
default out and adds one group:

```yaml
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
      - {name: db-1, host: 203.0.113.13}
    cluster:
      mesh: {subnet: 10.90.0.0/24, port: 51820, mtu: 1420}
      networks: 10.64.0.0/12
      storage: app-1
      groups: {web: [app-1, app-2]}
```

| Key              | Default        | Meaning                                                                                                                                      |
| ---------------- | -------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `mesh.subnet`    | `10.90.0.0/24` | Addresses of the WireGuard mesh mm builds between the nodes.                                                                                 |
| `mesh.port`      | `51820`        | UDP port WireGuard listens on, on every node. It must be reachable between the nodes.                                                        |
| `mesh.mtu`       | `1420`         | MTU of the mesh interface. See [choose the MTU](#choose-the-mtu).                                                                            |
| `mesh.interface` | none           | An existing private interface, such as a provider VLAN on `eth1`. Use it instead of the three keys above: mm then creates no WireGuard mesh. |
| `networks`       | `10.64.0.0/12` | Address range for [networks that span nodes](#networks-across-nodes). Each network takes a /20 of it, so the default holds 256 networks.    |
| `storage`        | the first node | The node that keeps [shared volumes](#shared-volumes-across-nodes) and serves them to the others. Changing it while it holds files is refused. |
| `groups`         | none           | Named lists of nodes that a service's [`placement`](./reference/configuration.md#modules-services-and-builds) can refer to.                  |

`mm config validate` checks that:

- node names are lowercase hostname labels, and hosts and private addresses are
  unique;
- private addresses are private and, when mm builds the mesh, inside
  `mesh.subnet`;
- every node declares `private-address` when `mesh.interface` is set, and no
  node sets `endpoint` then;
- `networks` is a private range, a /20 or larger, that does not overlap the
  mesh subnet;
- `storage` and `groups` name declared nodes;
- every `placement` of a service that runs in the environment names a node or
  a group of its target, or is `all`;
- `setup.deploy-key.secret` names a secret declared under `secrets:`.

The [configuration reference](./reference/configuration.md#cluster-targets)
lists every node and `cluster:` key.

### The mesh between the nodes

The nodes talk to each other over a WireGuard interface named `mm0` that `mmd`
owns on every node. You configure nothing for it: each node gets an address
from `10.90.0.0/24`, listens on UDP port 51820, and peers with every other
node at that node's `host`.

- **Open one port.** UDP 51820 (`cluster.mesh.port`) must be reachable between
  every pair of nodes, in both directions. It is the only port the mesh uses.
  Allow it in each host's firewall and in the provider's firewall or security
  group, from the other nodes' addresses.
- **Keys stay on their node.** Each node makes its own WireGuard key pair. The
  private key is a root-only file under `/var/lib/mm/mesh` on that node. It is
  never sent anywhere, and it is not in any plan, log, or command line; mm
  only ever reads public keys.
- **Changes are plans.** `mm target setup plan` shows, per node, the peers it
  adds, changes, removes, or leaves alone, and the plan hash covers them. A
  peer that does not change keeps its session: nothing restarts the interface.
- **It survives a reboot.** `mmd` stores the applied state, and
  `mm-mesh.service` restores the interface at boot before `mmd`, the container
  engine, and the storage server start.

When the nodes should reach each other at another address than the one mm
connects to, such as an address on a network they share, give the node an
`endpoint`:

```yaml
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11, endpoint: 10.0.0.11}
      - {name: app-2, host: 203.0.113.12, endpoint: 10.0.0.12}
      - {name: db-1, host: 203.0.113.13, endpoint: 10.0.0.13}
```

If the nodes already share a private network you want to use as it is, set
`cluster.mesh: {interface: eth1}` instead and give every node its
`private-address` on it. mm then creates no WireGuard mesh.

#### What the mesh carries

Two kinds of traffic belong on the mesh: what the nodes themselves send each
other (service name records, shared storage, anything you serve on a node's
mesh address for the other nodes), and what a container on a
[network that spans the nodes](#networks-across-nodes) sends to that
network's members. Each node keeps everything else off it with two iptables
chains that `mmd` loads together with the interface, at boot before the
container engine starts:

- `MM-MESH`, entered from Docker's `DOCKER-USER` (on a Podman node from the
  mangle table's `FORWARD`, like `MM-CLUSTER`), drops what the node would
  forward out of `mm0` unless it comes from a cluster network's addresses on
  this node and goes to that network. A container on a network of its own
  host (the engine's default bridge, a Compose network that is not under
  `networks:`) therefore reaches no other node through the mesh; without the
  rule it would arrive there with its node's mesh address.
- `MM-MESH-IN`, entered from `INPUT`, takes packets for the node's mesh
  address from the mesh subnet on `mm0` only, and packets for the address the
  node has on a cluster network on that network's bridge only. A machine on
  the public network that routes one of those addresses at the node's public
  interface gets no answer, and a container reaches its own node's mesh
  address no more than another node's.

```sh
iptables -S MM-MESH       # on a node
iptables -S MM-MESH-IN
```

Every rule carries a comment that starts with `mm`. `mm target mesh status`
and `mm doctor` report a node whose chains differ from what its mesh and
networks ask for as `MM_MESH_GUARD`; `systemctl reload mm-mesh` on the node
loads them again. `mmd mesh leave` removes them with the interface.

What this means for you: publish a port on a node's mesh address when the
other nodes should reach it (their requests arrive translated and are let
through), and reach it from the other hosts, not from a container. A
container that has to talk to a service on another node shares a network
under `networks:` with it. A CI runner in a container on a node reaches the
other nodes at their public addresses.

#### Look at the mesh

```sh
mm --environment production target mesh status
```

```text
Target production: mesh between 3 node(s)

  app-1  mm0 up  10.90.0.1  UDP 51820  mtu 1420  key 4kT9wq0aYx1B…
    PEER             ENDPOINT                   HANDSHAKE      RECEIVED       SENT
    app-2            203.0.113.12:51820         34 s ago         18 MiB     21 MiB
    db-1             203.0.113.13:51820         never               0 B      148 B

  app-2  mm0 up  10.90.0.2  UDP 51820  mtu 1420  key Zr81nQe0pL2c…
    PEER             ENDPOINT                   HANDSHAKE      RECEIVED       SENT
    app-1            203.0.113.11:51820         34 s ago         21 MiB     18 MiB
    db-1             203.0.113.13:51820         12 s ago         9304 B     9272 B

  db-1  mm0 up  10.90.0.3  UDP 51820  mtu 1420  key b0Hq7TtW3xNe…
    PEER             ENDPOINT                   HANDSHAKE      RECEIVED       SENT
    app-1            203.0.113.11:51820         never               0 B      148 B
    app-2            203.0.113.12:51820         12 s ago         9272 B     9304 B

Problems:
  error[MM_MESH_NO_HANDSHAKE]: 'app-1' and 'db-1' have no WireGuard handshake (app-1 last heard from it never; db-1 last heard from it never)
    help: UDP port 51820 must be open between 203.0.113.11 (app-1) and 203.0.113.13 (db-1) in both directions: allow it in each host's firewall and in the provider's firewall or security group, then run `mm target mesh status` again
```

Every node answers for itself: its interface, and per peer the endpoint it
sends to, the time since the last handshake, and the bytes received and sent.
Below the nodes the command lists what is wrong and what to do: two nodes
without a handshake and the port to open between them, a node that has no
entry for another (`MM_MESH_PEER_MISSING`) or holds another key than that node
uses (`MM_MESH_KEY_MISMATCH`), and an interface that is down
(`MM_MESH_DOWN`). The command exits with status 3 when a node does not answer
or the mesh has a problem. `--output json` prints the same report as
`{"target": ..., "nodes": [...], "problems": [...]}`. Anyone with `view`
access can run it. `mm doctor` includes the same problems, the terminal UI
shows each node's connected peers in the Environments view, and the MCP server
returns the report from `mm_read` with kind `mesh`.

#### Choose the MTU

`cluster.mesh.mtu` is the largest packet the mesh interface carries. WireGuard
adds up to 80 bytes to each packet, so the default of 1420 fits networks that
carry 1500 bytes between the nodes, which is the usual case. Lower it when the
path between two nodes carries less, for example through a provider tunnel or
PPPoE: use that path's MTU minus 80. Setup tests this for you. After the mesh
comes up, every node sends one packet of exactly the mesh MTU to every other
node, and a path that drops it stops the apply with the two hosts and the
setting to change. Without that check, small requests work and large responses
stall.

The setup plan shows the MTU, and changing it is a planned change to the
interface on every node. No peer is touched and no session is lost.

#### Add or remove a node

Add the node to `nodes:` (or delete it), commit, and run setup again:

```sh
mm -e production target setup plan
mm -e production target setup apply --plan HASH --yes
mm -e production target setup record
```

The plan's mesh section shows the one entry that changes on every node:

```text
Mesh: WireGuard mm0, 10.90.0.0/24 mtu 1420. UDP port 51820 must be reachable between the nodes.
  app-1                10.90.0.1  at 203.0.113.11:51820  key 4kT9wq0aYx1B…
      = app-2: unchanged
      + app-4: 10.90.0.4/32 at 203.0.113.14:51820, key made on app-4 during apply
  app-4                10.90.0.4  at 203.0.113.14:51820  its key is made on the node during apply (new on this node)
      + app-1: 10.90.0.1/32 at 203.0.113.11:51820, key 4kT9wq0aYx1B…
      + app-2: 10.90.0.2/32 at 203.0.113.12:51820, key Zr81nQe0pL2c…
```

A new node's key does not exist until apply, so the plan says where it is made
instead of showing it. A node whose SSH host key the repo already records is
read while planning; a node with a new host key is read only during apply,
after you reviewed that key.

Removing a node drops it as a peer on the nodes that remain. Setup does not
contact a host that is no longer listed, so the plan tells you how to take the
mesh off it: run `sudo /usr/libexec/mmd mesh leave` there, which removes
`mm0`, the stored state, and the node's key.

#### Replace a node's key

```sh
mm -e production target mesh rotate app-2
mm -e production target mesh rotate app-2 --plan HASH --yes
```

The first command prints the plan: `app-2` makes a new key pair, and every
other node replaces the one entry it holds for `app-2`. The second applies
exactly that plan. The other nodes keep their own keys and their sessions with
each other; `app-2`'s links drop for a few seconds while its peers switch.
Every node has to answer, because a node that misses the new key loses its
link to `app-2`. If a rotation is interrupted, run it again: it finishes with
the key the node already made.

#### Hosts set up by an earlier version

Earlier development versions wrote a `wg-quick` configuration for an interface
named `wg0` from the setup scripts. The next `mm target setup` apply moves
such a host to `mm0` by itself: the plan marks the node as replacing `wg0`,
the node keeps its key, so nodes that are not migrated yet still reach it, and
the old configuration and its `wg-quick@wg0` unit are removed.

## Run Podman on the hosts

To run the environment on Podman instead of Docker Engine, set its driver:

```yaml
# .config/mm.yaml
environments:
  production:
    target: production
    runtime: {driver: podman-compose}
```

Then plan and apply setup as for Docker. On each host it installs Podman, its
network stack (netavark, which keeps its rules with nftables), and Docker
Compose as the Compose provider of `podman compose`; it starts Podman's API
socket (`podman.socket`), which that provider and `mmd` talk to, and
`podman-restart.service`, which starts containers with a restart policy after
a reboot (before Podman 5.6 it starts only `restart: always`, so setup adds a
drop-in that includes `unless-stopped`, mm's default on cluster nodes). `mmd` runs as root, so Podman runs rootful there, behind the same
workload guardrails as Docker Engine. On a cluster node setup records the
engine in `/etc/mm/node.env` (`MM_RUNTIME`), and `mmd` uses that engine for
what it does on its own: names, shared storage, its firewall chains,
metrics, the ingress proxy, and containers that leave.

As on Docker Engine, a service whose user is not root may listen on a port
below 1024: Podman leaves `net.ipv4.ip_unprivileged_port_start` at 1024, so
mm sets it to 0 on each service that has a network namespace of its own
(not `network_mode: host`, `container:` or `service:`) and does not set it
under `sysctls`, and the plan lists those services under `ports:`.

Requirements on each host:

- Podman 5.0 or later: Debian 13, or Ubuntu 25.04 or later.
- `.config/ansible/bootstrap.yml` from an mm that knows rootful Podman. A
  project made with an earlier `mm init` has a playbook that refuses it with
  "The bootstrap plan must select rootful Docker Compose or rootless Podman
  Compose"; replace the file with the one a new `mm init` writes.

Every service gets user ids of its own. Its containers' ids 0 to 65535 are
65536 ids of the host that no other service there has: root in a container is
an unprivileged user of the host. The ids are the same in every container of
the service, on every node, and from one deployment to the next, so a volume
keeps working across recreates, rollouts and nodes. Setup reserves them in
`/etc/subuid` and `/etc/subgid`:

```text
mm-services:3221225472:1073676288   # 16383 services of 65536 ids each
containers:2147483647:1073741825    # Podman's own, for any other container
```

`mmd` records which block of the range each service has in
`/var/lib/mm-home/service-user-ids.json`. A service's first choice is a
digest of its project, environment, stack and name, which every node computes
the same way; on a node where another service already has those ids, it gets
the next free block of its own there. To use another range, change the
`mm-services` line in both files on every node before the first deployment,
clear of every other line; setup leaves a line you changed as it is. A host
whose range is full refuses the plan and says so (`MM_USER_NAMESPACE`).

Files mm hands to a container belong to the container's user. A [config
file](./config-files.md) and each `secret-files` entry are given to the host
ids that their `uid` and `gid` are in the service's namespace (root when left
out), with their `mode`: `mode: 0o400` with `uid: "1000"` is readable by user
1000 in the container and by no other service. A secret file is one file per
version of the secret, as on Docker Engine, with a copy for each service that
mounts it, in a directory only the host's root reaches. `uid` and `gid` have
to be from 0 to 65535 (`mm config validate` says otherwise).

A volume that two services mount works only with the host's ids on all of
them: Podman gives a new volume to the first container that mounts it, and
each service's files belong to ids the others do not have. `mm config
validate` refuses such a volume (`MM_USER_NAMESPACE`); use it from one
service, or keep the host's ids for every service that mounts it. A service
that has to keep the host's ids says why, and its containers run in the
host's user namespace:

```yaml
modules:
  shop:
    services:
      web:
        stack: app
        host-user-namespace: shares its uploads with the host's backup job
```

A Compose file that sets `userns_mode` on a service keeps it (`host` needs the
`userns-host` exception in the host policy). A [shared
volume](#shared-volumes-across-nodes) has to reach a service with the same ids
on every node: a node where another service already has that service's first
choice refuses the plan and names that service.

Files written before a service had ids of its own (a volume of a host
deployed by an earlier mm, or of a service that kept the host's ids) keep
their owners. Give them to the service once, as root on the host: with the
service's first id from `podman inspect --format '{{json .HostConfig.IDMappings}}'
CONTAINER` (the second number of `UidMap`) and the volume's directory from
`podman volume inspect --format '{{.Mountpoint}}' VOLUME`, run `chown -R
FIRST:FIRST DIRECTORY` for files of the container's root, or add the
container user's id to `FIRST` for a service that runs as another user.

`mm doctor` asks each host for its engine and reports a host whose Podman is
older than 5.0 (`MM_PODMAN_VERSION`), whose `podman compose` does not run
Docker Compose (`MM_PODMAN_COMPOSE`), or whose engine does not answer
(`MM_NODE_ENGINE`), each with the command that fixes it. `mm target nodes
--output json` shows the engine of each node under `engine`.

A cluster node runs one engine for every environment: it refuses to plan a
deployment of an environment whose driver is not the engine setup installed
there, and names the line to change.

Limits on Podman:

- A service's containers have the user ids 0 to 65535: an image or a
  Compose `user:` that needs a higher id does not run there unless the
  service keeps the host's ids.
- Podman gives a container fewer capabilities than Docker Engine does; it
  has no `NET_RAW`, so `ping` in a container needs `cap_add: [NET_RAW]` in
  the Compose file.
- Podman mounts a shared volume with `mount.nfs`; a container that starts
  while the storage node is away waits up to two minutes for the mount (on
  Docker Engine about half a minute) before `mmd` starts it again later.
- Podman creates a network's bridge with its first container and removes it
  with its last; `mmd` gives a bridge that came back its settings within a
  few seconds, and until then a container reaches only the members on its
  own node.

## Apply the users file for the first time

Nobody but the break-glass key can connect until an environment reconciles
the repo's users file. Push the commit, then reconcile it as root on the host,
without the relay:

```sh
sudo mmd access sync --project PROJECT_ID --environment production \
  --commit 3f2c9a1e5b7d4c6f8a0b2d4e6f8a1c3e5b7d9f0a --dry-run   # review
sudo mmd access sync --project PROJECT_ID --environment production \
  --commit 3f2c9a1e5b7d4c6f8a0b2d4e6f8a1c3e5b7d9f0a
```

`mmd access sync` checks the commit exactly as `mm team sync` does: the host
fetches it from the enrolled origin with the deploy key, it must be on a
branch that `deploy-from` allows, and the host ceilings and Compose
restrictions apply. It then writes the environment's users, rewrites
`authorized_keys`, records `host-root` in the audit log, and prints a JSON
summary:

```json
{
  "project": "019c527b-6242-7b43-bfd4-bd60e27c1de3",
  "environment": "production",
  "commit": "3f2c9a1e5b7d4c6f8a0b2d4e6f8a1c3e5b7d9f0a",
  "previous-commit": null,
  "changed": true,
  "dry-run": false,
  "deploy-from": ["main"],
  "users": [
    { "user": "alice", "machine": false, "access": { "level": "deploy" }, "keys": ["SHA256:…"] }
  ],
  "changes": [{ "user": "alice", "after": { "level": "deploy" }, "keys-added": ["SHA256:…"] }]
}
```

`--dry-run` prints the same summary and writes no access state. Running the
command again at the same commit prints `"changed": false` and changes
nothing. It only moves access forward: it refuses a commit that the commit
already applied is not an ancestor of. Go back with a reviewed `mm team sync`.

On a cluster target you do not run this by hand: the `sync-access` step of
[cluster setup](#run-setup) runs it on every node.

The break-glass key works too:
`MM_SSH_KEY="$(cat ops-recovery)" mm --environment production team sync`.

After that, every `mm deploy apply` and `mm team sync` re-applies the users
file from the commit it uses. See [remote access](./remote-access.md).

Finally, remove personal login accounts and sudo from the host, and store the
break-glass private key offline.

## Use the host

```sh
mm --environment production deploy plan             # the host fetches HEAD's commit
mm --environment production deploy apply --plan PLAN_HASH --yes
mm --environment production service status
mm --environment production service logs api
```

`deploy plan` sends a commit ID. The host fetches it from the origin, checks it
is on an allowed branch, checks it out under `/srv/mm/projects`, and plans from
that checkout. Push the commit before planning. A host that may not reach the
git server takes the files from mm instead; see
[deploy without git on the host](#deploy-without-git-on-the-host).

The first command connects to the host and `mm` keeps that connection open for
five minutes, so the commands after it do not connect again; see
[connections stay open](./remote-access.md#connections-stay-open-for-five-minutes)
for what is kept and how to turn it off.

Remote hosts never build. Modules that mm builds deploy from a build receipt:
publish the images, write the receipt with `mm build receipt`, and pass it to
`deploy plan --receipt`. The host checks that the receipt is a build of the
planned commit, either for this environment or for one whose configuration
builds the same units. See [deploy a build to a remote
host](./concepts/builds-and-deployments.md#deploy-a-build-to-a-remote-host).

### Deploy from CI

Build once in a CI environment, then deploy that build to each environment
with a machine key. Declare a `ci` environment in `.config/mm.yaml` for the
build job. In the build job:

```sh
mm --environment ci build --publish --yes
mm --environment ci build receipt build-receipt.json
```

Keep `build-receipt.json` as a job artifact. In each deploy job, from a
checkout of the same commit, with `MM_SSH_KEY` set to a key the users file
grants `deploy` on the environment:

```sh
PLAN=$(mm --output json --environment production deploy plan --receipt build-receipt.json | jq -r .binding.plan_hash)
mm --environment production deploy apply --plan "$PLAN" --yes
```

The host deploys the receipt's digests when production's configuration at
that commit builds exactly the same units as `ci` did: the same modules,
platforms, image repositories, variants, and build inputs. Otherwise it
refuses and names the first module that differs. A module with a build secret
can only be promoted when every secret is listed under
`credential-only-secrets`, because other secrets can hold different values
per environment. The plan's `plan.build_receipt.environment` and the host's
audit log (`build_receipt_environment`) record the environment the receipt
was built for.

The receipt holds image names, digests, and build settings, but no secret
values.

Values that belong to this release and to no environment, such as the ID of
an OS image the same pipeline built, go to the plan as
[release inputs](./reference/configuration.md#release-facts). Declare them
under `release-inputs:` and give each with the plan:

```sh
PLAN=$(mm --output json --environment production deploy plan --receipt build-receipt.json \
  --release "$CI_COMMIT_TAG" --release-input FLATCAR_BUILD_ID="$FLATCAR_BUILD_ID" | jq -r .binding.plan_hash)
```

Templates read them as `RELEASE.FLATCAR_BUILD_ID`, next to `MM.RELEASE`,
`MM.COMMIT`, and `MM.IMAGE.<module>` for the receipt's images. They are part
of the plan, so `deploy apply --plan` takes nothing more.

To skip rebuilding unchanged modules on a fresh CI runner, import the previous
publication's receipt before building. `mm build --publish` looks for reusable
images only in mm's local state, not in the registry:

```sh
PLAN=$(mm --output json --environment ci build import previous-receipt.json --cache-only | jq -r .plan_hash)
mm --environment ci build import previous-receipt.json --cache-only --plan "$PLAN" --yes
```

Remote exec supports output and cancellation. It currently rejects `--stdin`,
`--tty`, and interactive `shell`.

## Deploy without git on the host

By default each host fetches the commit it deploys from the origin, so it
needs a route to the git server and a deploy key. Where production may not
reach the build tier, have mm send the files instead. Set `delivery: push` on
the target; that is the whole configuration:

```yaml
targets:
  production:
    host: prod-eu.example.com
    host-key: ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA…
    daemon-fingerprint: 0f3c…
    delivery: push
```

The host has to agree: its policy carries `delivery: push` for the project
and no `origin`. Setup writes that (see below). A host whose policy says
`fetch` refuses every bundle, and a host whose policy says `push` never
contacts the git server, whatever a client asks.

The commands do not change:

```sh
mm --environment production deploy plan
mm --environment production deploy apply --plan PLAN_HASH --yes
```

`deploy plan` now builds a bundle of the commit from your checkout, sends it
to the host over the connection it already has, and then asks for the plan.
The plan names the bundle:

```text
Remote deployment plan 5d1c…
  commit:   8a41f0c2…
  bundle:   3fb2… (pushed by mm; the host fetched nothing)
```

A host that already holds the commit is sent nothing, so the next plan of
the same commit, `deploy rollback`, and a redeploy cost no upload.
`mm team sync` delivers the commit the same way.

Images are not part of this. They still come from the registries the build
receipt and the Compose files name, so the host needs a route to those.

### What a bundle carries

A bundle is a tar archive of the files git tracks at the commit, byte for
byte as the commit stores them, with each file's executable bit and each
symbolic link. Next to it mm sends the commit object (author, message,
parents, and the tree's hash), so the host can check the files. Nothing else:

- No file that is not in the commit: nothing untracked, nothing ignored, no
  uncommitted change. mm reads git's object store, not your working tree.
- No `.git` directory and no history.
- No secret values. Secrets reach a host through its own secret store
  (`mm target setup` pushes them sealed), and the build receipt travels with
  the plan request as before; it holds image digests, not values.
- No `.env` file. A commit that tracks a file named `.env` or `.env.NAME` is
  refused before anything is sent, with the path and the command that stops
  tracking it. `.env.example`, `.env.sample`, `.env.template`, `.env.dist`
  and `.env.defaults` are ordinary files.

Because the files are taken as the commit stores them, git attributes that
change a checkout do not apply: no end-of-line conversion, no `export-ignore`,
no smudge filter. Git submodules and Git LFS are refused, as with `fetch`.

A bundle may hold up to 512 MiB of files and 100,000 files. A larger commit
is refused on your machine with its three largest files. The bundle is not
compressed; the upload is about a third larger than the files.

### What the host checks

The host writes the bundle to disk as it arrives and keeps it only when all
three hold:

1. The bytes hash (BLAKE3) to the hash mm stated before it sent them.
2. The commit object hashes (SHA-1) to the commit ID.
3. The files hash to the tree that commit object names, computed the way
   git does.

So a bundle is either exactly the commit it claims to be or it is refused,
and nothing of a refused bundle stays on the host: no checkout, no partial
file. The same holds for an upload that is cut off; run the command again.
The bundle hash is part of the plan hash. `deploy apply` refuses a plan when
the files the host holds for the commit are no longer that bundle.

`/var/lib/mm/audit.jsonl` records each delivery (`"event":
"bundle-delivered"` with `commit`, `bundle_hash`, `bundle_bytes`,
`bundle_files`, who sent it, and the branches they stated) and names the
`bundle_hash` again in the record of every plan made from it.

### What push cannot guarantee

With `fetch`, the host asks the git server itself, so a commit that is not on
an allowed branch of the origin cannot be deployed by anyone. With `push`,
the host has no repository to ask:

| Check                                   | `fetch`                 | `push`                                                    |
| --------------------------------------- | ----------------------- | --------------------------------------------------------- |
| The files are the commit's              | the host fetched them   | the host verifies them against the commit ID              |
| The commit is on the git server         | yes                     | not checked                                               |
| The commit is on a `deploy-from` branch | the host checks         | the host checks the branches the client stated            |
| The users file comes from that branch   | yes                     | as far as the client's statement is true                  |
| `mmd access sync` only moves forward    | yes                     | not checked: the host cannot tell which commit is newer   |

mm states the branches of `origin` that contain the commit in your checkout
(`git branch -r --contains`). With `deploy-from` set, a commit with no such
branch is refused, with the line to run (`git fetch origin`). But a principal
with `deploy` access who changes their client can state any branch for any
commit they can build, including one that grants access in its users file.
With `push`, everyone who may deploy is trusted as much as the allowed
branches are: give `deploy` on such a target to CI and to the people who
could push to those branches anyway.

Sending a commit needs the same access as planning it.

### Set up a host for push

For one host, pass `--delivery push` where a fetching host takes `--origin`:

```sh
mm --environment production --target prod-eu-setup --runtime docker-compose \
  target setup plan --delivery push \
  --break-glass "ops-recovery=$(cat ops-recovery.pub)"
```

Setup enrolls the project without an origin and creates no deploy key; there
is no key to add to the git host. The result prints the target to commit,
with `delivery: push`.

For a cluster, set `delivery: push` on the target and leave `origin`,
`origin-host-key` and `deploy-key` out of `setup:`:

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

`mm target setup plan` and `apply` then skip the deploy key, and the first
access sync sends each node the commit's bundle on the connection setup
already has. `--commit` may be any commit of your checkout; it does not have
to be on the origin.

On a cluster mm sends the bundle to every node of the plan itself, all at
once. Nodes do not pass it on to each other: a node takes project files only
from a principal it authorized. The plan lists who was sent it:

```text
  bundle:   3fb2… (212 files, 1.4 MiB; sent to app-2, db-1)
```

To move a host from `fetch` to `push`, run setup again with the new flags or
the changed target. Setup also removes what fetching left on the host: the
deploy key, the origin's pinned host key, and the clone under
`/var/lib/mm/projects/<id>/`; the plan says so. Remove that deploy key from
the git host as well. To do it by hand, replace the project's `origin` (and
`origin-host-key`) with `delivery: push` in `/etc/mm/host.yaml`, delete those
files, then set `delivery: push` on the target. A commit the host fetched
earlier is sent once more and verified before it is used again.

A pushed commit is held against `deploy-from` as soon as it is offered: a
commit your checkout states is on no allowed branch is refused before
anything is uploaded.

When the target and the host disagree, the error says which side to change:
a client without `delivery: push` gets "commit … was not delivered to this
host", and a client with it on a fetching host gets "this host fetches this
project from its origin and takes no files from a client".

## Deploy to a cluster

On a target that lists `nodes:`, the same commands act on every node. Your
machine coordinates: it talks to each node's `mmd` over the relay, by node
name, and each `mmd` changes only its own host. Nothing has to run between the
nodes for a deploy to work.

```sh
mm --environment production target nodes
mm --environment production deploy plan
mm --environment production deploy apply --plan PLAN_HASH --yes
mm --environment production service status
```

Push the commit first: every node fetches it from the origin itself.

### List the nodes

`mm target nodes` asks every node at once and prints one row per node:

```text
Target production: 3 node(s)
  NODE             ADDRESS                  MESH            STATE        MMD        DEPLOYED
  app-1            203.0.113.11:22          10.90.0.1       ready        0.1.0      3f2c9a1e5b7d at 2026-11-12T18:03:22Z
  app-2            203.0.113.12:22          10.90.0.2       ready        0.1.0      3f2c9a1e5b7d at 2026-11-12T18:03:24Z (runs nothing)
  db-1             203.0.113.13:22          10.90.0.3       unreachable  -          -
      accepts SSH, but mmd is not running there (the relay found no daemon socket). Start it on the host with `systemctl start mmd`; `journalctl -u mmd` shows why it stopped

Networks (cluster.networks 10.64.0.0/12)
  NETWORK           BLOCK          app-1         app-2         db-1
  backend           10.64.0.0/20   10.64.0.0/24  10.64.1.0/24  10.64.2.0/24
  vault (internal)  10.64.16.0/20  -             -             10.64.16.0/24

Addresses the nodes already use (cluster.networks and cluster.mesh.subnet must not overlap them)
  app-1  the address 203.0.113.11 on eth0
  app-2  the address 203.0.113.12 on eth0
         the address 10.70.0.2 on eth1
         the route 10.70.0.0/20 through eth1
  db-1   not known: the node did not answer
```

`STATE` is `ready`, `unreachable`, `not set up` (the repo records no keys for
the node yet), or `wrong node` (the address reaches a host with another node
name). `DEPLOYED` is the commit and time of the node's last applied
deployment. The command exits with status 3 when a node is not ready, and
prints the list either way. `--output json` prints the same facts as
`{"target": ..., "nodes": [...], "addresses": {...}}`. Each node has `name`,
`host`, `port`, `mesh_address`, `state`, `reachable`, `problem`,
`mmd_version`, and `deployed`.

Below the nodes, the command prints the addresses of the
[networks that span the nodes](#networks-across-nodes), for the environment
you named:

- `Networks` has a row for each network that has addresses: its block of
  `cluster.networks`, and under each node's name the slice that node's
  containers get their addresses from. `-` means the node runs no service on
  that network. Before the first deployment the table says that no network
  has addresses yet.
- A line under the table names the blocks that other projects and
  environments hold on the same nodes. A new network never gets one of them.
- When two nodes stored different addresses for a network, the table shows
  what the first of them holds and lists the differences. `mm deploy plan`
  refuses until they agree and says how to put it right.
- `Addresses the nodes already use` lists every address and route a node has
  outside mm. `cluster.networks` and `cluster.mesh.subnet` must not overlap
  them; see [when a node already uses the addresses](#when-a-node-already-uses-the-addresses)
  for how to choose another range.

In JSON these are `addresses.space`, `addresses.networks` (`name`, `key`,
`block`, `internal`, and `nodes` with `node`, `slice`, `network`),
`addresses.held-by-others`, `addresses.differing`, and `addresses.host-ranges`
(`node`, `answered`, `problem`, and `ranges` with `range`, `kind`,
`interface`). A target whose networks stay local to each host prints no
address section.

`mm doctor` on a cluster environment runs the same check and reports each node
that is not ready, nodes that run different `mmd` versions
(`MM_NODE_VERSION_SKEW`), nodes whose last applied commits differ
(`MM_NODE_COMMIT_SKEW`), and a rollout that was left unfinished
(`MM_ROLLOUT_UNFINISHED`). It also reports what
[`mm target mesh status`](#look-at-the-mesh) finds. The terminal UI lists the
nodes in the Environments view, and the MCP server returns the nodes and the
address table from `mm_read` with kind `nodes`.

### Plan and apply

`deploy plan` asks every node to plan the commit at the same time and prints
one plan: a section per node in the order the target lists them, then one plan
hash. Each section shows the node's `mmd` version, the access change, the
services the node runs after the plan, and what the engine starts, changes, or
removes there.

The plan hash covers every section. Each planned node keeps it, and
`deploy apply --plan PLAN_HASH` applies exactly those sections: a node refuses
a plan the hash does not cover, and a plan that changed on any node has a
different hash. Before anything changes, apply checks that every node of the
plan still holds it; if one does not answer or its plan expired, nothing is
applied. Plans expire after ten minutes.

Apply then rolls the plan out one node at a time, prints each step a node
starts and when a node's turn with a stack is over (`healthy`), and then what
each node did:

```text
==> app-1: preparing
==> app-1: app
==> app-1: app: healthy
==> app-2: preparing
==> app-2: app
==> app-2: app: healthy
==> db-1: preparing
==> app-1: finishing
==> app-2: finishing
==> db-1: finishing
Deployment 8a41… on production [production]
  app-1            applied      stacks: app
  app-2            applied      stacks: app
  db-1             applied      runs nothing
```

A node that no service is placed on takes the access change and records the
commit, and its section says it runs nothing.
[Place services on nodes](#place-services-on-nodes) says which node runs what,
and [Rolling deploys and rollback](#rolling-deploys-and-rollback) says what
happens between the nodes and when one of them fails.

In CI, the plan hash of a cluster plan is the top-level `plan_hash`:

```sh
PLAN=$(mm --output json --environment production deploy plan | jq -r .plan_hash)
mm --environment production deploy apply --plan "$PLAN" --yes
```

### Rolling deploys and rollback

A deployment to a cluster needs no configuration to be safe. Stacks roll out
in name order, and each stack one node at a time in the order the target lists
the nodes. A node is done when the stack's containers pass their health checks
(a service without a health check counts as healthy once it runs) and every
other node resolves the new containers by name. Only then does the next node
start.

Before the first node is replaced, every node of the plan stores the
addresses the plan gives the cluster's networks, so a container that starts on
a network a node did not have before is reachable and resolvable from the
nodes whose turn has not come.

The plan says so before anything changes:

```text
  rollout: app on app-1, then app-2, then db-1
           a stack may take 10m on a node to become healthy (deploy.convergence-timeout); the next node starts once the others resolve its containers
           on failure every node this deployment changed returns to what it ran before
```

When a stack does not become healthy on a node, the deployment stops. Every
node it changed returns to what it ran before, newest change first, and the
nodes it had not reached are never touched:

```text
==> app-1: preparing
==> app-1: app
==> app-1: app: healthy
==> app-2: preparing
==> app-2: app
==> app-2: rolling back
==> app-1: rolling back
Deployment 8a41… on production [production]
  app-1            rolled back  runs what it ran before
  app-2            FAILED       service 'web' of stack 'app': its health check fails (MM_RUNTIME_UNHEALTHY); rolled back to what it ran before
                   last log lines of the failed container:
                     listening on :8080
                     cannot reach the database at db:5432
  db-1             not started  unchanged
error[MM_APPLY_STOPPED]: plan 8a41… failed on node 'app-2': service 'web' of stack 'app': its health check fails (MM_RUNTIME_UNHEALTHY). Its last log lines: … Rolled back to what they ran before: app-1, app-2. Not changed: db-1. Fix the cause, then run `mm deploy plan` again
```

The report names the node, the service, and the last twenty lines its
container logged. A node that was not reached keeps its containers as they
were; the network addresses the plan gave out stay reserved. A deployment also fails at once, without waiting for the
window, when a container exits or the engine keeps restarting it. The access
change of the plan is not rolled back: a key the commit removed stays removed
on the nodes the deployment reached.

Three things can be changed, each with a default that works:

```yaml
environments:
  production:
    deploy:
      convergence-timeout: 15m # default 10m

stacks:
  app:
    files: [compose.yaml]
    after: [db] # default: stacks roll out in name order
  db:
    files: [db.yaml]

modules:
  shop:
    services:
      web:
        stack: app
        placement: all
        update:
          order: start-first # default stop-first
          parallelism: 2 # nodes at a time; default 1
          failure: pause # default rollback
```

| Key                               | Default      | Meaning                                                                                                                                                                              |
| --------------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `deploy.convergence-timeout`      | `10m`        | How long a stack may take on a node to become healthy: `90s`, `15m`, `1h`. The plan shows it. A stack that is not healthy in time fails the deployment like any other failure.         |
| `deploy.progress-timeout`         | `2m`         | How long `mm deploy apply` waits without any sign of progress from a host before it stops with `MM_TIMEOUT`, naming the host, the step, and the operation. At least `30s`.            |
| `stacks.<stack>.after`            | none         | Stacks that roll out before this one, on every node, so that a database is healthy before the application that needs it starts.                                                      |
| `update.order`                    | `stop-first` | `start-first` starts the new container next to the old one; see below.                                                                                                               |
| `update.parallelism`              | `1`          | How many nodes replace the service's stack at the same time. A stack takes the smallest value among its services.                                                                    |
| `update.failure`                  | `rollback`   | `pause` stops the deployment where it is when this service fails, so the failed container can be looked at. `mm deploy rollback` then returns the nodes to what they ran before.      |
| `drain`                           | `1m`         | How long an old container keeps serving what it has once no node sends it requests, before it stops; see below.                                                                      |

`deploy.convergence-timeout` applies to every environment, local and remote.
The other keys only matter where there is more than one node or more than one
stack.

#### Replace a container without a gap

With `update.order: start-first`, a node starts the new container of the
service next to the old one and waits until it is healthy. A container that is
still starting is not given out for the service's name or by the ingress, so
requests keep going to the old one. Once every node resolves the new
container, the old one [leaves](#take-a-container-out-without-dropping-a-request):
no node answers with it or sends it requests any more, it finishes what it
has during the service's `drain` (`1m` by default), and then it stops. The
next node starts after that. A new container that never becomes healthy is
removed and the old one keeps serving.

The service is reached by its name on a [network that spans the
nodes](#networks-across-nodes) or through the [ingress](#route-traffic-into-the-cluster);
neither a client that asks again for each request nor one that keeps its
connection sees a failed request. Give the service a health check: without one
a container counts as healthy once it has run for five seconds.

`start-first` needs two containers of the service on one node for a moment, so
`mm config validate` refuses it with `MM_UPDATE_ORDER` for a service that
publishes a fixed host port, sets `container_name`, or runs more than one
container (`scale`). On Podman hosts the two containers have the service's
own user ids, so the new one keeps the old one's volumes; a service whose
Compose file sets `userns_mode` other than `host` is refused there, since
its new container would get other ids. It takes effect when the
service's configuration or image reference changes; a tag that was pushed
again under the same name is replaced the ordinary way.

#### Return to the previous deployment

```sh
mm --environment production deploy rollback
```

Every node remembers the deployment it finished last and the one before.
`deploy rollback` plans a return to what the nodes ran before the last
deployment, prints that plan with its hash, and applies it after you confirm
(`--yes` skips the question). It is a plan like any other: the nodes plan the
earlier commit from their own copy of the repository, with the build receipt
and the [release name and inputs](./reference/configuration.md#release-facts)
that deployment used, so your checkout does not need the commit, and the plan
rolls out and fails the same way a deployment does. Declining leaves you with
the hash for `mm deploy apply --plan`.

Only the nodes that took part in the last deployment return; a node left out
with `--nodes` keeps what it runs. `--nodes` on `deploy rollback` limits it
further. When the nodes ran different commits before, one plan cannot cover
them, and the error lists a `mm deploy plan --commit … --nodes …` command per
commit, with the inputs each group ran with. Volumes are not rolled back: an earlier version runs against the data
the later one left.

#### One deployment at a time, and one that was interrupted

While a deployment runs, the environment is locked on every node of its plan.
A second `mm deploy apply` is refused with `MM_DEPLOY_RUNNING`, who started
the running one, and when. Planning is always allowed.

```sh
mm --environment production deploy status
```

```text
Deployments of production [production]
  app-1
    runs:     commit 3f2c9a1e5b7d  2026-11-12T18:03:22Z  by alice  plan 8a41c0de91f2
    before:   commit 9d1b6a4c2e80  2026-11-10T09:41:07Z  by alice  plan 5be07a3310c4
  app-2
    runs:     commit 9d1b6a4c2e80  2026-11-10T09:41:07Z  by alice  plan 5be07a3310c4
    rollout:  plan 8a41c0de91f2 (commit 3f2c9a1e5b7d) was left UNFINISHED: the node is changed and nobody is working on it; started by alice at 2026-11-12T18:02:55Z
              stacks applied: app; last heard from the client at 2026-11-12T18:03:31Z

A rollout was left unfinished on app-2. `mm deploy rollback` returns the environment to its last finished deployment; `mm deploy plan` plans a new one, and applying it undoes the unfinished rollout first.
```

`deploy status` asks every node what it deployed last, what it ran before,
whether a rollout is in progress or was left unfinished, and what runs on it
right now. Run it after a deployment lost its connection: a node stops the
step it was on as soon as it sees the client go, the lock frees itself a
minute after the client was last heard from, and the output says what to do
next. `mm target nodes`, `mm doctor`
(`MM_ROLLOUT_UNFINISHED`), the terminal UI's node list, and the MCP read
`deployments` report the same. `deploy status` and `deploy rollback` are for
remote environments; on this machine, check out the commit you want and run
`mm deploy`.

Other changes to a host take turns with a deployment's steps: an access sync,
or deleting a shared volume, waits until the step that is running on that
host is done, and then runs. Status, logs, `mm service exec`, and
`mm deploy status` do not wait. While a change waits, its command prints
every 15 seconds what it waits behind. A change that is still waiting when its
own time is nearly up (half an hour for an access sync) fails with
`MM_HOST_BUSY`, which says whose operation is in its way and since when;
nothing was changed, so run it again. A change whose client goes away while
it waits is dropped and never runs.

If `mmd` restarts on a node while a deployment runs there (an upgrade, a
reboot, or [a daemon that stopped answering](#when-mmd-stops-answering)), the
step in progress ends with it. The node gives up the lock at once, and
`deploy status` says why nobody finished the rollout:

```text
    rollout:  plan 8a41c0de91f2 (commit 3f2c9a1e5b7d) was left UNFINISHED: the node is changed and nobody is working on it; started by alice at 2026-11-12T18:02:55Z
              stacks applied: none; last heard from the client at 2026-11-12T18:03:31Z
              failed: mmd on this node restarted at 2026-11-12T18:03:40Z while this rollout ran, which interrupted the step in progress
```

The client that was deploying reports the node as failed and returns the
nodes it had changed to what they ran before. When it cannot, nothing has to
be cleaned up by hand: the next `mm deploy apply` undoes the unfinished
rollout first, and `mm deploy rollback` does only that.

#### Follow a deployment, and stop one that is stuck

`mm deploy apply` prints the operation's ID first, then a line for each step
on each node, and every 15 seconds while a step runs a line that says whether
the host still sees it at work:

```text
==> operation 8a41c0de91f2: deploying to 'production'. `mm deploy status` shows where it is; `mm deploy cancel 8a41c0de91f2` stops it
==> app-1: preparing
==> app-1: app
==> app-1: app, still running after 15s (the host is working on it)
```

The ID is the plan's hash: every step on every node of one deployment shares
it. A host that shows no progress for
[`deploy.progress-timeout`](#rolling-deploys-and-rollback) (two minutes
by default), because its `mmd` does not answer or no longer works on the
step, ends the apply with `MM_TIMEOUT`, which names the host, the step, and the
operation; the nodes the deployment changed return to what they ran before,
as after any failed step. Every step also has a deadline on the host itself:
`mmd` stops a step that passes it, ends its child processes, and frees the
environment, whatever the client does. A step whose client goes away is
stopped at once, and so is one whose client the host has not heard from for
two minutes: while a deployment runs, `mm` tells every node every 15 seconds
that it is still there, and a connection that dropped without SSH noticing
tells it nothing. Before such a step frees the environment, a container it had
begun to take out (during its drain) is routed to again, and one whose stop
had begun is stopped and removed first, so nothing the step began changes the
host under the next deployment's plan.

`mm deploy status` lists what runs on each host now, with who started it and
since when:

```text
    running:  deployment (stack 'app') 8a41c0de91f2 by alice since 2026-11-12T18:02:55Z; at work 2026-11-12T18:04:10Z, stopped at 2026-11-12T18:32:55Z if not done
```

Anyone who may deploy to the environment stops it:

```sh
mm --environment production deploy cancel 8a41c0de91f2
```

```text
  app-1  stopped deployment (stack 'app') 8a41c0de91f2 by alice since 2026-11-12T18:02:55Z; returned to what it ran before plan 8a41c0de91f2
  app-2  plan 8a41c0de91f2 had changed nothing here; the environment is free
  app-3  nothing ran here

`mm deploy status` shows what each host runs now.
```

`mm deploy cancel` asks every host at once. Each stops the operation, ends its
child processes, returns a host the deployment had changed to what it ran
before, and only then frees the environment, so no other deployment begins
there while the host is being returned. Without an ID it stops whatever runs;
the ID can be its first six or more characters. The audit log on each host says
who started the operation and who cancelled it. A host that does not answer is
named and the command fails with its error; the others are cancelled all the
same. An AI agent sees the operation in the MCP read `deployments` and hands
the command to you.

The same goes for the other operations a host lists. A job run (`mm job run`,
whose ID is its run's ID) or a deployment's `before-deploy` or `after-deploy`
job stops: its container is stopped and the run is recorded as cancelled, and
`mm deploy cancel` lists the runs it stopped. A run keeps its place in
`mm deploy status` after the `mm job run` that started it answered, and a run
the schedule started has one too (`running:  run of job 'slow' …`, stopped at
its `timeout`); its ID, as `mm job history` or the status shows it, stops it
the same way. A restart stops waiting for the
service; a container the engine already restarted stays restarted. A commit's
upload to a host with `delivery: push` ends and nothing of it is kept. A
container the deployment had begun to take out, during its drain, is routed to
again when its host returns to what it ran before; one whose stop had begun is
stopped and removed first, and the rollback starts what replaces it. A step
that takes long while its host works on it, such as a ten-minute migration, a
drain, or a large upload, is shown at work the whole time and is never ended by
`deploy.progress-timeout`.

`mm doctor` asks each host how its `mmd` fares and reports, per node, a part
of the daemon that does not answer (`MM_DAEMON_NOT_ANSWERING`), a recent
restart (`MM_DAEMON_RESTARTED`), operations in flight (`MM_OPERATION_IN_FLIGHT`,
or `MM_OPERATION_STUCK` once one is past its deadline or not at work for two
minutes), child processes still there after their deadline
(`MM_CHILD_PAST_DEADLINE`), and services that have not converged
(`MM_SERVICE_NOT_CONVERGED`). It needs read access to the environment, not
root.

#### After a node reboots

A node that reboots runs its services again without a deployment. On a cluster
target mm gives every service whose Compose files set no `restart:` policy
`restart: unless-stopped`, and the plan lists those services. The mesh, the
firewall rules, and the node's name resolver come back before the containers
do. A service that mounts a [shared volume](#shared-volumes-across-nodes)
waits for the storage node when that one is not up yet, and `mmd` starts it
when it is. Set `restart: "no"` on a service that must stay stopped; a service that
exits by itself is reported as a failed deployment, because a service is
expected to keep running (run a one-off task as a job).

### Place services on nodes

Give a service a `placement` in `mm.yaml`. It is a node name, a group from the
target's `cluster.groups`, the word `all`, or a list of those:

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
      metrics: {stack: app, placement: all} # every node
      db: {stack: app, placement: db-1} # db-1 only
      worker: {stack: app} # no placement: the first node, app-1
```

A service runs one container on each node its placement names. Without
`placement` it runs on the first node under `nodes:`. To run another number
of containers, set `replicas`: they are spread over the placement's nodes,
the first nodes getting one more when the count does not divide evenly.

```yaml
      web: {stack: app, placement: all, replicas: 5} # 2, 2 and 1 on three nodes
```

A node whose share is 0 runs none, and its status does not count the service
as missing there. To let the count follow the load, see
[autoscale a service](#autoscale-a-service).

On a cluster target every service of a stack's Compose files needs an entry
under `modules`, so that mm knows where it runs. Without one,
`mm config validate` and `mm deploy plan` fail, and the finding `MM_PLACEMENT`
says which entry to add.

Each node's section of the plan lists what the node runs:

```text
Node app-2  mmd 0.1.0  section 5c1e…
  access:   unchanged (2 user(s))
  services: app/metrics, app/web
  starts here: app/metrics
  depends_on not kept: app/web depends on db, which runs on db-1. An engine cannot wait for a service on another node, so web starts without waiting for it
```

`--output json` has the same facts under each node's `plan.plan.node`:
`services`, `starting`, `leaving`, and `dropped_dependencies`.

A node runs one Compose project per stack, made of the stack's Compose files
with only the services placed on that node. The same holds for what those
files declare at the top level: a network, volume, secret, or config exists
only on the nodes that run a service that uses it. A named volume is local to
its node: two nodes that run the same service each have their own.

Three things follow from a service living on another node:

- `depends_on` between two nodes cannot be honoured by either engine. mm drops
  it from that node's project and the plan says so, as above. Make the service
  tolerate a dependency that is not up yet.
- `links`, `network_mode: service:NAME`, `ipc: service:NAME`, and
  `pid: service:NAME` tie two services to one engine. The plan fails with
  `MM_PLACEMENT` on the node that would run only one of them, until both have
  the same `placement`.

Services on different nodes reach each other over the networks you declare in
`mm.yaml`: see [networks across nodes](#networks-across-nodes).

A relative bind mount, such as `./nginx/site.conf:/etc/nginx/conf.d/site.conf`,
works on every node: each node checks out the deployed commit for itself and
the mount points into that node's own copy. mm refuses a bind mount of a host
path outside the project on every target, so there is no path that could be
missing on one node.

`mm service status` and `mm service logs` cover a service's containers on
every node; see [status, logs, and exec](#status-logs-and-exec). `mm service
exec` and `mm forward` on a service that runs on several nodes reach its
container on the first of them.

#### Move a service

Change the `placement`, commit, push, and plan. One plan moves the service:

```text
Cluster deployment plan 8a41…
  target:   production (app-1, app-2, db-1)
  commit:   3f2c9a1e5b7d…
  expires:  2026-11-12T18:13:22Z
  moves:
    app/worker: starts on app-2, then stops on app-1

Node app-1  mmd 0.1.0  section 91d0…
  access:   unchanged (2 user(s))
  services: app/web
  stops here: app/worker, after it runs on app-2 (last step of the apply)

Node app-2  mmd 0.1.0  section 5c1e…
  access:   unchanged (2 user(s))
  services: app/web, app/worker
  starts here: app/worker
```

Each node's section goes on with what the engine does there. `--output json`
lists the moves under `moves`, each with `stack`, `service`, `from`, `to`, and
`starts_on`.

Apply walks every node first, so the service is running on its new node while
it still runs on the old one. Stopping it on the old node is the last step:

```text
==> app-1
==> app-2
==> db-1
==> app-1: stopping worker (now on app-2)
Deployment 8a41… on production [production]
  app-1            applied      stacks: app; then stopped, now that they run elsewhere: app/worker
  app-2            applied      stacks: app
  db-1             applied      runs nothing
```

Between those two steps the service runs on both nodes. If the apply stops at
a node, nothing is stopped: the service keeps running where it was, and the
next `mm deploy plan` lists it again. A service that publishes a host port can
move between nodes, because each node has its own ports.

Moving a service does not move its data. A named volume stays on the node the
service left, and the service starts with an empty volume on the new node.
Copy the data yourself before you move a service that keeps state, or keep
that service pinned to its node. mm never removes a volume.

`--nodes` cannot leave out a node a service moves to: the plan fails with
`MM_PLACEMENT`, because the service would stop without having started. Add the
node, or leave out the node it moves from as well.

When a node's last service leaves, mm removes the stack's project from that
node, and the node runs nothing.

#### Compose keys that place a service

Compose `deploy` keys that place or multiply a service (`deploy.mode`,
`deploy.replicas` above 1, `deploy.placement`, and `deploy.endpoint_mode`) are
refused on a cluster target: `mm config validate` and `mm deploy plan` fail
with a finding for each key that says what to write.
[Coming from Docker Swarm](./how-to/from-docker-swarm.md#compose-keys) has the
table. The same Compose file keeps working unchanged in an environment with
one host.

### Networks across nodes

A network declared under `networks:` in `mm.yaml` spans every node that runs a
service on it. There is nothing to turn on:

```yaml
networks:
  backend: {}

modules:
  shop:
    services:
      web: {stack: app, role: http, networks: [backend], placement: app-1}
      db: {stack: app, role: database, networks: [backend], placement: db-1}
```

`web` on `app-1` and `db` on `db-1` are on the same network. A container
reaches another by its address on that network, whichever node it runs on.
Two containers that share no network cannot reach each other, on one node or
across nodes. A network that only a Compose file declares stays local to its
host, as does every network of a target that names one host.

The traffic between nodes travels inside the [mesh](#the-mesh-between-the-nodes),
encrypted, and keeps the containers' own addresses. Nothing outside the mesh
can reach an address of these networks: not the public interface, and not
another machine on the provider's network.

Containers keep their way out to the internet, through their node's address.
A network marked `internal: true` has none: its containers reach the members
of the network and nothing else.

```yaml
networks:
  vault: {internal: true}
```

A container finds the others by service name on every node: see
[service names across nodes](#service-names-across-nodes).

#### What the plan shows

Each network gets a /20 block of `cluster.networks`, and each node that runs a
service on it gets a /24 slice of that block, with room for 253 containers of
that network on that node. `mm deploy plan` shows who gets what, the routes
and the firewall rules each node gains or loses, and anything on a node that
differs from the last deployment:

```text
Cluster deployment plan 3f0c…
  networks span the nodes: addresses from 10.64.0.0/12, between nodes through mm0 (MTU 1420)
  service names: each node answers its own containers, on its address on each network (port 53), with a service's healthy containers on the same node, or else on every node

Node app-1  mmd 0.1.0  section 91ab…
  services: app/web
  networks:
    backend: gets 10.64.0.0/24 of 10.64.0.0/20
  routes through the mesh:
    + 10.64.1.0/24 to db-1
  firewall rules:
    + -A MM-CLUSTER -s 10.64.0.0/20 -d 10.64.0.0/20 -i mm0 -o mmn-4a1f0c2b9de -m comment --comment "mm network 5f3a9c2e7b10-production-backend" -j ACCEPT
    …
```

An address never changes once a plan has given it out. Every node stores the
addresses it knows, and each plan reads them from all nodes first, so several
projects on the same hosts never get the same block. `--output json` has the
same facts under `networks`.

A network that an earlier deployment created local to a host has to move onto
its slice. The engine cannot change the addresses of a network in place, so mm
removes the network and creates it again, and the containers on it restart.
The plan says so before anything happens:

```text
  networks:
    backend: created again on 10.64.0.0/24 of 10.64.0.0/20 (was 172.19.0.0/16); restarts web
```

This happens once per network and node, at the first deployment after
upgrading. Plan it for a quiet moment: until every node of a network has
applied, the containers on the nodes still waiting cannot reach the ones that
already moved.

A network that no service of the environment uses any more, because
`mm.yaml` no longer declares it or no service is on it, leaves every node
with the deployment that stops using it: its addresses, its routes, its
firewall rules, and its bridge network. The old containers keep it until the
rollout finished on their node, so they are not cut off while the new ones
start. The plan lists it on each node that held it:

```text
  networks no service uses any more, removed when the rollout finished here: sealed
```

A network stays while a node that is left out of the plan (`--nodes`) holds
part of it, and while a container is still on its bridge, as the old container
of a service that moves to another node is until the rollout stops it; a later
deployment takes it away. Another environment's networks are never touched, also when
its name starts like this one's (`staging` and `staging-eu`).

#### What mm puts on a node

- One bridge network per network, with a bridge interface named `mmn-…`. Its
  subnet is the network's block and containers get addresses from the node's
  slice. The slice's second address is the node's own, and its first and last
  are given to no container, so a container address never ends in `.0` or
  `.255`.
- A route through the mesh to every other node's slices.
- Two iptables chains, `MM-CLUSTER` (entered from Docker's `DOCKER-USER`, or
  on a Podman node from the mangle table's `FORWARD`, which every forwarded
  packet passes before netavark's rules) and `MM-CLUSTER-NAT` (entered from
  `POSTROUTING`). Every rule mm adds is in one of them and carries a comment
  that starts with `mm`.
- For [service names](#service-names-across-nodes): `mmd` listens on port 53
  (UDP and TCP) of the node's own address on each network, and on TCP port
  7953 of the node's mesh address. It keeps one small file per network under
  `/var/lib/mm/networks/resolv`.

`mm-network.service` restores all of it after a reboot and whenever the
container engine restarts, and the firewall chains are loaded before the
engine starts a container. To look at a node, as root on it:

```sh
/usr/libexec/mmd network show
iptables -S MM-CLUSTER            # on a Podman node: iptables -t mangle -S MM-CLUSTER
```

`mmd network show` prints the addresses the node stored, what differs from
them right now, and the addresses and routes the node has outside mm. What
differs is the same list `mm service status` reports for the node, including
an engine network that is on other addresses than the stored ones.

#### When a node already uses the addresses

A host can hold addresses in the default ranges before mm arrives: a
provider's private network, a VPN, or a container network somebody created by
hand. `mm target setup plan` and `mm deploy plan` read every node's addresses
and routes and stop when one overlaps `cluster.mesh.subnet` or
`cluster.networks`:

```text
error[MM_TARGET]: target error: node 'app-2' already has the route 10.70.0.0/20 through eth1, which overlaps cluster.networks 10.64.0.0/12 (the addresses of networks that span the nodes). Set `cluster.networks` under targets.production.cluster to a private range no node uses, such as 10.32.0.0/12, then plan again
```

Set the key the message names and plan again:

```yaml
targets:
  production:
    cluster:
      networks: 10.32.0.0/12
```

The range the message offers is free on every node it could read. A node whose
SSH host key is not recorded yet is read during `mm target setup apply`, which
stops before it changes the mesh on any node. The default route is not
counted, and neither are mm's own mesh interface and bridges.

#### When a node drifts

`mm service status` and `mm doctor` report a node whose rules, routes, bridge
settings, or network addresses differ from what the last deployment stored
(`MM_NETWORK_DRIFT`), with the node and what differs. To repair it:

```sh
mm --environment production deploy plan
mm --environment production deploy apply --plan <hash> --yes
```

The plan lists what it repairs. On the node itself, `systemctl restart
mm-network` restores rules, routes, and bridge settings from what `mmd`
stored; a network on the wrong addresses needs the deployment.

To take mm's rules off a host that is no longer a node, run `/usr/libexec/mmd
network leave` as root on it. It removes the two chains, the rules that enter
them, and the stored addresses, and nothing else.

#### Limits

- The nodes run Docker Engine with its default iptables firewall backend. A
  node whose engine is set to `firewall-backend: nftables` fails the plan and
  the error says how to switch back.
- A network spans at most 16 nodes, and a node runs at most 253 containers of
  one network.
- `cluster.networks` holds one network per /20: 256 with the default /12, 16
  with a /16. When it is full the plan fails, says how many networks the range
  holds, and names another range to set. Networks that already have addresses
  keep them; the new range serves new networks.
- `cluster.networks` and `cluster.mesh.subnet` must be ranges no node uses
  for anything else. See
  [when a node already uses the addresses](#when-a-node-already-uses-the-addresses).
- A node left out of a plan with `--nodes` learns the addresses that plan gave
  out at its own next deployment. Until then its containers cannot reach the
  new slices.
- On a target with `cluster.mesh.interface`, mm does not own the link between
  the nodes, so its networks stay local to each host and no node serves
  shared volumes.
- A container cannot reach another node's own address through the mesh, only
  the members of its networks. That holds for every container on a node, also
  one on a network of that host alone: see
  [what the mesh carries](#what-the-mesh-carries).

### Service names across nodes

A container reaches a service by its name, whichever node runs it. There is
nothing to configure: every service on a
[network that spans the nodes](#networks-across-nodes) gets it.

```yaml
# compose.yaml
services:
  web:
    image: registry.example/shop/web
    environment:
      DATABASE_URL: postgres://db:5432/shop
  db:
    image: postgres:17
    healthcheck:
      test: [CMD, pg_isready]
      interval: 10s
```

`web` connects to `db` wherever `placement` puts the two. On a network they
share, a container can look up a service in three ways:

| Name       | Answer                                                                                                                       | Use it                                                               |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `db`       | The healthy containers of `db` on the node that asks. When that node has none, the healthy containers of `db` on every node. | By default. A request stays on its node when the service runs there. |
| `tasks.db` | Every healthy container of `db`, on every node.                                                                              | To spread requests over all nodes, or to find every member.          |
| `db.db-1`  | The healthy container of `db` on the node `db-1`.                                                                            | To reach one node's container.                                       |

A network alias from the Compose file resolves like the service name: the
containers on the asking node that carry it, or else every node's.

So a `web` container that connects to `cache:6379` uses the `cache` on its
own node when `cache` runs on every node, and the `cache` of another node
when its own is missing or unhealthy. Nothing is configured, and a service
that runs on one node resolves as before.

A container is healthy when it runs and passes its health check; without a
health check, when it runs. A container that starts failing its health check
leaves the answers on its own node at once and on the other nodes within about
two seconds of the engine marking it unhealthy, and returns the same way.
When it was the last healthy container of its service on the node, the next
lookup of the service name on that node gets the other nodes' containers:
there is no moment without an answer. A name whose containers are all
unhealthy resolves to nothing rather than to a container that does not work.
A container always finds itself under its own names, healthy or not, so its
health check may use them.

`tasks.db` and `db.db-1` give the same answer on every node; `db` depends on
the node that asks. Several addresses come in a different order each time;
there are no virtual IPs. Answers may be kept for five seconds, so a client
that keeps one can use it that long after it changed. A container gets answers
only for the networks it is on. Every other name, such as `api.example.com`,
goes to the resolvers and search domains in the node's own `/etc/resolv.conf`.

#### See what a name resolves to

```sh
mm --environment production service status
```

Below the service rows, the status lists every service name per network with
the containers behind it:

```text
  NAME                     NETWORK        RESOLVES TO
  cache                    backend        10.64.0.3 (app-1), 10.64.1.4 (app-2), 10.64.2.4 (db-1); each node resolves to its own
  db                       backend        10.64.2.2 (db-1)
  web                      backend        10.64.0.2 (app-1), 10.64.1.2 (app-2); app-1 and app-2 resolve to their own, db-1 to all
  worker                   backend        10.64.1.3 (app-2); left out: 10.64.2.3 (db-1, unhealthy)
```

The addresses are the service's healthy containers on every node, which is
what `tasks.<service>` resolves to. When the service name does not resolve to
the same on every node, the line ends with which nodes answer with their own
containers and which with all of them. A line without that ending resolves to
the listed addresses everywhere. `left out` names the containers that are not
healthy and so are not in any answer. When none of a service's containers is
healthy, the nodes answer and route (by name and through the ingress) to its
running containers all the same, since a check that fails everywhere at once is
more often the check than the service: the line then ends with `answered while
unhealthy`, `fail-open` is set in `--output json`, `mm deploy status` names
the service, and `mm doctor` reports `MM_SERVICE_ANSWERED_UNHEALTHY`. A
container that is leaving or stopped is never answered. `not known on app-2` after an address
means that node's resolver has not learned of the container, so lookups on
`app-2` miss it. A line that starts with `node` says what keeps a node from
answering, such as a node it has not heard from, and what to run. `mm service
status web` shows one service. `--output json` has the same facts under
`names`, with each node's answer for the service name under `lookups` (`node`,
`addresses`, and `local`, which is `true` when the node answers with its own
containers). The terminal UI shows them in a service's details, and an AI
agent reads them with `mm_read` and kind `names`.

#### How names are answered

Each node's `mmd` answers the lookups of the containers on its own node. It
listens on the node's own address on each network (the first address of the
node's slice, such as `10.64.0.1`), and the containers of the network's
services get an `/etc/resolv.conf` that names that address. `mmd` knows the
containers of its own node by watching the engine. It learns the containers of
the other nodes by asking each node every two seconds, over the mesh, what it
runs. Nothing is stored and no deployment pushes anything: what runs decides
the answers. Because each node answers its own containers, it can prefer the
containers next to them.

When a node has not answered the others for ten seconds, they leave its
containers out of their answers, and `mm service status` says which node is
silent. The containers return when the node answers again.

The port between the nodes answers the cluster's own nodes and nobody else,
and takes no input. A node's resolver answers the containers of that node
only. Neither is bound to a public address.

#### Limits

- `dns`, `dns_search`, and `dns_opt` in a Compose file have no effect on a
  service that is on one of these networks, so the plan fails with
  `MM_COMPOSE_DNS` and names the key. Remove it; to resolve other names
  differently, change `/etc/resolv.conf` on the nodes.
- `/etc/resolv.conf` is read-only in these containers.
- Names on a node are answered by its `mmd`, on a thread of their own: a
  deployment, a slow request, or a request that is stuck on the node does
  not delay a lookup. While `mmd` is stopped on a node, the containers of
  that node resolve no names, and the other nodes leave its containers out
  after ten seconds. An upgrade restarts `mmd` within a second or two,
  systemd starts an `mmd` that failed again after two seconds, and it
  replaces one that [stopped answering](#when-mmd-stops-answering) within
  half a minute.
- A name that is no service's takes as long to fail as the node's own
  resolver takes. On a host where systemd-resolved has LLMNR on, a name
  without a dot that nobody has takes a few seconds.
- A container's own name resolves on its node only, and only when it fits the
  63 characters a DNS label holds; the container names mm generates are
  longer. Use the service name, or `<service>.<node>` for one node's
  container. Compose `links` aliases do not resolve.
- A container started by hand on one of these networks, outside a deployment,
  uses the engine's resolver and finds the containers of its own node only.
- Addresses are IPv4.
- On a target with `cluster.mesh.interface`, and on a target that names one
  host, networks stay local to the host and the engine resolves names as it
  always did.

### Route traffic into the cluster

An `ingress:` route on a stack works on a cluster target as written:

```yaml
environments:
  production:
    target: production
    domains: { shop: shop.example.com }

stacks:
  app:
    files: [compose.yaml]
    ingress: { domain: shop, service: web }
```

The first node of the target runs the ingress proxy (Traefik). It takes ports
80 and 443 and sends each request for `shop.example.com` to a healthy
container of `web`: one on its own node when there is one, otherwise one on any
node. Point the domain's DNS record at that node, and open ports 80 and 443 to
it. A container counts once its health check passes, as for
[service names](#service-names-across-nodes), so give the service a health
check: without one a container counts as soon as it runs.

To run the proxy on other or more nodes, list them under `cluster.ingress`,
written like a `placement`: a node, a group, `all`, or a list. Each of them
serves every route.

```yaml
targets:
  production:
    nodes:
      - { name: app-1, host: 203.0.113.11 }
      - { name: app-2, host: 203.0.113.12 }
      - { name: db-1, host: 203.0.113.13 }
    cluster:
      ingress: [app-1, app-2] # default: the first node
```

The plan says where the traffic comes in and where each route goes:

```text
  ingress: app-1, app-2 take the traffic on ports 80 and 443 and send each request to a healthy container on the same node, else on any node; HTTPS uses a certificate from ACME (Let's Encrypt) for each HTTPS host
    https://shop.example.com -> app/web port 8080
```

#### Spread requests over every node

By default an ingress node sends a request to the service's containers on its
own node, and to the other nodes' only when it runs none (`local-first`). That
keeps a request on one node, and it assumes traffic enters evenly across the
ingress nodes: through a DNS record per ingress node, or a load balancer in
front of them. A node that takes more of the traffic loads its own containers
more while the ones on other nodes idle. With one ingress node, or uneven
traffic, set `balance: spread` and each request goes to the service's healthy
containers on every node:

```yaml
environments:
  production:
    target: production
    domains: { shop: shop.example.com }
    ingress: { balance: spread } # default local-first

stacks:
  app:
    files: [compose.yaml]
    ingress:
      - { domain: shop, service: web } # spread, from the environment
      - { domain: shop, route: /admin, service: admin, balance: local-first }
```

A route without `balance` takes the environment's. Without either, a route to
a service with [autoscale](#autoscale-a-service) is `spread`: the containers
autoscale adds on other nodes would otherwise get no request from an ingress
node that runs one of its own. The plan marks each route that spreads:

```text
    https://shop.example.com -> app/web port 8080, spread over every node
```

Among the containers a route sends to, each request goes to the less busy of
two picked at random: the one with fewer requests in flight. A container that
slows down gets fewer requests well before its health check fails, and a
container on the ingress node itself, which answers sooner, takes a little
more than one on another node. A container that fails its health check leaves
the routes as before. Autoscale decides from the average load of all the
service's containers; spreading moves load between containers and leaves that
average as it is, so it decides as before, and the containers it adds take
requests at once.

The proxy and the routed services meet on a network named `ingress` that mm
adds when `mm.yaml` does not declare it; it spans the nodes like every other
[network](#networks-across-nodes). The proxy mounts no engine socket and runs
with a read-only root, no capabilities, and no new privileges, whatever the
project's `hardening` says, under the engine's default runtime (a host's
`exposed-runtime` does not apply to it), on Docker Engine and Podman nodes
alike. Its routes change when a deployment
finishes on the node; the containers each route sends to change the moment
the node's `mmd` sees a container become healthy, fail its check, start, or
leave, on any node. The proxy picks up each change without a restart and
without dropping a connection; the one exception is a host that moves from
Let's Encrypt to a certificate you provide (see [HTTPS](#https)).

#### Headers, allowlists, and several services on one host

Several routes can share a host and send different paths to different
services. The proxy tries the route with the longer path first, so `/app`
goes ahead of `/`; set `priority` (1 to 1000000) to put a route elsewhere.
Each route can also set response headers and let only some requests through:

```yaml
environments:
  staging:
    target: staging
    domains: { shop: shop.example.com }
    ingress:
      headers: { X-Robots-Tag: noindex } # on every route of staging

stacks:
  app:
    files: [compose.yaml]
    ingress:
      - { domain: shop, service: frontend }
      - { domain: shop, route: /api, service: backend }
      - domain: shop
        route: /.well-known/openid-configuration
        service: backend
      - domain: shop
        route: /hooks
        service: hooks
        allow:
          - { methods: [GET, HEAD], path: "^/hooks/status$" }
          - { methods: [POST], path: "^/hooks/[^/]+/events$" }
```

`/api` and the OpenID configuration go to `backend`, everything else on the
host to `frontend`. Under `/hooks` only the requests `allow` lists reach
`hooks`; the proxy answers 404 to the others and never sends them to
`frontend`. A route's own `headers` are added to the environment's, and win
for the same header. Websocket upgrades pass through the proxy without a
setting. The plan lists a host's routes in the order the proxy tries them,
with what each lets through:

```text
    https://shop.example.com/hooks -> app/hooks port 8080, lets through GET|HEAD ^/hooks/status$, POST ^/hooks/[^/]+/events$ and answers 404 to the rest, sets X-Robots-Tag
    https://shop.example.com/api -> app/backend port 8080, sets X-Robots-Tag
```

#### HTTPS

HTTPS works as on a single host: the proxy asks Let's Encrypt for a
certificate for each HTTPS host, which needs port 80 of the ingress nodes to
be reachable from the internet. Where it cannot be, such as behind
Cloudflare's proxy with an origin certificate, give the environment the
certificate as two secrets:

```yaml
environments:
  production:
    target: production
    domains: { shop: shop.example.com }
    tls: { certificate: tls.certificate, key: tls.key }

secrets:
  tls.certificate:
    storage: { provider: age, location: tls.certificate }
  tls.key:
    storage: { provider: age, location: tls.key }
```

```sh
base64 -w0 origin.pem | mm --environment production secret set tls.certificate --stdin --local
base64 -w0 origin.key | mm --environment production secret set tls.key --stdin --local
mm --environment production target setup plan
```

`--local` keeps the secrets in this machine's store, and setup pushes them to
every node; the next deployment gives them to the proxy. On nodes that are
already set up, leave out `--local` and setup: the command then stages each
secret on every node once a commit that declares it has been planned there
(see [Change an environment's variable](./how-to/change-a-variable.md)). With `tls:` nothing asks Let's Encrypt for that environment.

A host that had a Let's Encrypt certificate before keeps none. The proxy holds
every certificate of its ACME state beside the provided ones and would serve
either, so when the deployment that gives the environment its certificate
finishes on an ingress node, `mmd` takes the old certificate out of the
proxy's ACME state and restarts the proxy once. For that moment the node
takes no new connections; requests already in flight finish first, for up to
10 seconds. Later deployments find nothing to take out and restart nothing.

#### Cloudflare Tunnel

An environment whose hosts keep every port closed takes its routes through a
Cloudflare Tunnel instead: `ingress.type: cloudflared` runs a connector next
to the proxy on every ingress node, writes the connector's configuration from
the routes, and closes ports 80 and 443 while no other environment on the node
uses them. Routes, `headers`, `allow`, `balance`, and draining work as above.
See [Serve a cluster through Cloudflare Tunnel](./how-to/cloudflare-tunnel.md).

```yaml
environments:
  production:
    target: production
    domains: { shop: shop.example.com }
    ingress:
      type: cloudflared
      cloudflared: { token: cloudflare.tunnel-token }
```

#### Take a container out without dropping a request

A container leaves while its service keeps running when an update replaces
it (with `start-first` once the new container is healthy; with the default
`stop-first` before the new one starts, when another node runs the service
too), when a rollback replaces it with the one it ran before (when another
node runs the service too), when its service moves to another node, when
[autoscale](#autoscale-a-service) removes it, or when you take it out by
hand. It leaves in three steps, never shortened by a timeout elsewhere:

1. Every node stops answering its name, and every ingress proxy stops sending
   it new requests. Its node waits until each of the others confirms.
2. It keeps running for its drain time while the requests it has and the
   lookups already answered finish.
3. It gets its stop signal and its `stop_grace_period`, and is removed.

The drain time is the service's `drain`, `1m` by default and at most `15m`.
A rolling update waits for it on each node before the next one starts, so a
service on three nodes takes at least three minutes to update with the
default: set `drain` to what your longest request needs.

```yaml
modules:
  shop:
    services:
      web:
        stack: app
        placement: all
        drain: 30s # default 1m
        update: { order: start-first }
```

To take a container out by hand, run on its node as root:

```sh
/usr/libexec/mmd container leave shop-production-app-web-2 --drain 30s
```

```json
{
  "id": "4f0c…",
  "name": "shop-production-app-web-2",
  "drain-ms": 30000,
  "marked-unix-ms": 1794655200000
}
```

`mmd` does the rest. It keeps each leave in `/var/lib/mm/leaving`, so after a
restart it carries on where it was: the container is never stopped early and
never left behind. Without `--drain` the service's `drain` applies. With
`--restart` the container is not removed: after its drain it gets its stop
signal, starts again, and is answered again once it is healthy, so a restart
drops no request while another container of the service runs. `mm service
restart` restarts a service's containers on a cluster this way, one at a
time (see [restart a service](runtime-observability.md#restart-a-service)).

#### Limits

- Ports 80 and 443 of an ingress node belong to the proxy, for every
  project's environment deployed to the node with public ingress; the proxy
  opens them only while one is. Two environments cannot route the same host
  and path, whatever their ingress type.
- `ingress.middlewares` files, and Traefik files of your own, are not applied
  on a cluster target; limit the bundles to other environments with
  `environments:`, and use a route's `headers` and `allow` instead.
- A plan that leaves nodes out with `--nodes` does not change the routes:
  every ingress node keeps what it serves until a plan covers every node.
- A service whose only container runs on one node has a short gap in a
  `stop-first` update, as on a single host: nothing else can serve it while
  its container is replaced. Give it `update.order: start-first`.
- HTTP and HTTPS only; no TCP or UDP routes.
- The proxy does not watch responses: a container that answers errors quickly
  keeps getting requests until its health check fails. Give the service a
  health check that fails when it does.

### Autoscale a service

A service can grow and shrink with its load. Next to its `placement`:

```yaml
modules:
  shop:
    services:
      web:
        stack: app
        placement: all
        autoscale:
          scale:
            min: 1
            max: 4
          metric:
            type: cpu
            up:
              greater: 75%
              for: 30s
            down:
              less: 40%
              for: 5m
```

Read aloud: keep between 1 and 4 containers; add one when the average CPU of
the service's containers has been above 75% for 30 seconds; remove one when
it has been below 40% for 5 minutes. The percentage is of the container's CPU
limit (`cpus:` in Compose) when it has one and of one core when it has none;
`type: memory` works the same way against the memory limit, or the node's
memory without one.

Only `scale.max` is required. `scale.min` defaults to the service's
`replicas`, which defaults to one container on each placement node, and the
metric defaults to the values above. So this is a whole configuration:

```yaml
      web:
        stack: app
        placement: all
        replicas: 1
        autoscale:
          scale:
            max: 4
```

The service starts at `replicas` when it is set, else at `min`. Nothing else
runs or needs installing: `mmd` on each node measures its own containers
every 5 s, keeps the last four hours in memory, and the nodes tell each other
the sums with the records they already exchange. So the service has to be on
a network of `mm.yaml` (a route under `ingress:` puts it on one), and `4h`
is the longest `for:`.

How the count changes:

- One node decides for the service: the first node of its placement that
  answers. It adds or removes one container at a time. A new container counts
  once it is healthy and every node resolves it; the next change waits until
  it does, and then a rule has to hold for its whole `for:` again.
- New containers go to the placement's nodes with the fewest, the first of
  them in order. A container is removed from the node with the most, the
  newest one first, and leaves without dropping a request: no node sends it
  anything new, it drains for the service's `drain` (`1m` by default), and
  only then gets its stop signal. See
  [take a container out without dropping a request](#take-a-container-out-without-dropping-a-request).
- When the deciding node's `mmd` stops, the next node takes over from the
  count it last heard, after watching one whole `for:` itself. After an
  `mmd` restart the measurements start empty, and a rule waits one whole
  `for:` too; the count itself is kept on each node and does not change.
- If every `mmd` is stopped, the containers keep running and nothing scales.
- A deployment keeps the current count, within the new `min` and `max`: it
  never resets to `min`. While it runs, nothing scales.
- `mm dev` runs one container of the service and does not scale it.

`mm deploy plan` shows the range and the count of the moment on each node:

```text
  autoscale app/web: 1..4 containers over 3 node(s), 2 now (1 here); cpu above 75% adds one, below 40% removes one
```

`mm service status` shows the count, the last change and why, and what the
deciding node sees now. The terminal UI and MCP read the same status, under
each service's `scale` in `--output json`:

```text
  autoscale web: 3 running, 3 wanted (1..4), decided on app-1
    last change: 2 -> 3 at 2026-11-14T12:00:31Z: cpu 82% above 75% for 30s
    cpu over the last 30 s: 81%
    now: cpu 81% above 75% for 10s of 30s
```

Every change is in the deciding node's audit log, `/var/lib/mm/audit.jsonl`,
with `"result":"autoscale"`, the counts, and the reason. While a node starts a
container, `mm deploy status` lists it among the operations in flight
(`autoscale of service 'web'`, by `mmd autoscale`), and `mm deploy cancel`
stops it; it takes the node's turn like a request that changes the host, and
nothing scales while a deployment holds the environment.

#### Limits

- Autoscale and `replicas` need a cluster target. A target of one host fails
  `mm config validate` with `MM_SCALE`; write it as a cluster of one node:
  `nodes: [{name: app-1, host: 203.0.113.11}]`.
- When more containers than nodes can run, a service may not publish a fixed
  host port (two containers on one node would want it): reach it through
  `ingress:` or by name. A fixed `container_name` is refused with autoscale.
- On Podman hosts as on Docker Engine: Docker Compose, which setup installs
  as Podman's Compose provider, starts each added container, and `mmd` reads
  the containers through Podman's API socket.
- CPU and memory only: no request rates or custom metrics, and nodes are not
  added or removed.

### Restart a container that keeps failing its health check

On a cluster target each node restarts its own containers that stay
unhealthy. Nothing is configured for it: a container whose health check has
failed for three minutes is restarted the way
`mm service restart` does it (it leaves the names and the routes, drains while
another container of the service answers, restarts, and is answered again
once healthy), one container of a service at a time on each node. `health:`
next to a service's `placement` changes when, or turns it off:

```yaml
modules:
  shop:
    services:
      web:
        stack: app
        placement: all
        health:
          on-failure: restart # default; none leaves it as it is
          after: 3m # how long it stays unhealthy first; at least 10s
          max-restarts: 5 # within window, then stop and report
          window: 1h
```

The check is the service's `healthcheck:` in its Compose file; a service
without one is never restarted for its health. A node never restarts a
container while a deployment holds the environment (a rollout, a rollback,
or its convergence), while a container of the service leaves (an update's
or autoscale's drain), or one that is leaving itself. It waits until the
deployment is over and looks again.

A container restarted `max-restarts` times within `window` that fails again
is left alone: its node stops restarting it, writes `MM_SERVICE_RESTART_LIMIT`
to its audit log, and keeps it out of the names and the routes while another
container of the service can take its requests, even when its check passes
again for a moment. When no other container of the service can (none is
healthy on any node), the nodes answer it and route to it like any other
running container of a service none of whose containers is healthy: an
answer that may fail is better than none, and a check that fails everywhere
is often the check itself. A deployment that replaces the container, or
`mm service restart`, starts it again from no restart.

Each restart is in the node's audit log (`"result": "health-restart"`, and
`"health-restart-limit"` with the code), in `mm service status`, and in
`mm service logs`, as a line from mm about the node (marked `!`):

```text
  restarted web: 2 times by mm, the last shop-production-app-web-1 on app-2 at 2026-11-16 12:03:04 UTC: unhealthy for 3m
  restart limit web: mm stopped restarting shop-production-app-web-1 on app-3 at 2026-11-16 12:40:10 UTC: unhealthy for 3m after 5 restarts within 1h; it gets no requests while another container of web can take them (MM_SERVICE_RESTART_LIMIT). `mm service logs web` shows why; fix it and deploy, or run `mm service restart web`
```

```text
app-2 ! mm: restarted shop-production-app-web-1 after it was unhealthy for 3m
```

`mm doctor` reports each container past its limit as
`MM_SERVICE_RESTART_LIMIT`. A restart in progress is listed by
`mm deploy status` as an operation of `mmd health`, and `mm deploy cancel`
stops it. A node keeps its restarts in `/var/lib/mm/health`, so an `mmd`
that restarts keeps counting; it starts timing how long a container has been
unhealthy again from when it comes back.

### Shared volumes across nodes

A Compose volume lives on the host that created it. When a service runs on
several nodes and all of its containers need the same files, list the volume
under `shared-volumes`:

```yaml
# compose.yaml
services:
  web:
    image: example/web
    volumes: ["uploads:/var/www/uploads"]
volumes:
  uploads: {}
```

```yaml
# .config/mm.yaml
stacks:
  app:
    files: [compose.yaml]
    shared-volumes: [uploads]

modules:
  shop:
    services:
      web: {stack: app, role: http, placement: all}
```

Deploy, and `web` reads and writes one directory on every node. A file a
container writes on `app-1` is there for the container on `app-2` at once.
There is nothing else to set up: `mm target setup` already made one node the
storage node, and the Compose file stays as it is.

**Use shared volumes for files that replicas share:** uploads, generated
assets, exports. **Do not put a database, a queue, a search index, or
anything else that needs fast or locked disk on one.** Give those an ordinary
volume and pin the service to one node with `placement:`.

- **It is slower than local disk.** Every read and write on a node other
  than the storage node crosses the network. Measured on 2-vCPU hosts with
  NVMe disks, against a storage node in the same data centre and against one
  58 ms away:

  | Operation                                 | Local volume | Shared, same data centre | Shared, 58 ms away |
  | ----------------------------------------- | ------------ | ------------------------ | ------------------ |
  | Write 200 MiB and flush it                | 0.3-1.3 GB/s | 120-170 MB/s             | 34-42 MB/s         |
  | Read 200 MiB that is not cached           | 1.0-2.0 GB/s | 140 MB/s                 | 8 MB/s             |
  | One 4 KiB write, on disk before the next  | 1.4-2.4 ms   | 2.1-2.4 ms               | 62-64 ms           |
  | Create a small file                       | 0.15 ms      | 7-9 ms                   | 185 ms             |
  | Read a small file that is not cached      | 0.4-0.9 ms   | 1 ms                     | 177 ms             |

  Next to the storage node the cost is throughput and every file that is
  created. Further away every operation waits for the round trip, which a
  database pays for each transaction and most programs pay for each file
  they open. On the storage node itself a shared volume is as fast as a
  local one.

- **It does not give the locking guarantees a database needs.** It is a
  network file system (NFSv4.2). A file lock is a request to the storage
  node and holds only while the two nodes can talk, and a file that two
  nodes write or map into memory at the same time is not kept consistent
  between them.
- **The storage node is a single point of failure.** While it is down, every
  read and write of a shared volume on the other nodes waits. mm does not
  replicate shared volumes and does not back them up: copy
  `/var/lib/mm/storage` on the storage node with your own backup tool.

`mm config validate` and the plan refuse a service with `role: database` or
`role: queue` that mounts a shared volume, and say this. If you have a reason
to do it anyway, set `allow-shared-volumes: true` on that service.

#### Where the files are

The storage node is the target's first node, or the one `cluster.storage`
names. It keeps each shared volume as a directory:

```text
/var/lib/mm/storage/<project id>/<environment>/<stack>/<volume>
```

A service on the storage node uses that directory itself. On every other node
the engine mounts it from the storage node when it starts the container. The
plan says which is which:

```text
  shared volumes: app-1 keeps the files and serves them to the other nodes over the mesh. For files that replicas share; a database belongs on an ordinary volume
    app/uploads: kept on app-1 in /var/lib/mm/storage/019c…/production/app/uploads, used there directly, mounted over the mesh on app-2, app-3 (for web)
```

A deployment creates the directory on the storage node before it replaces a
container on any node, so the plan has to include the storage node: with
`--nodes`, name it too. A deployment never removes a directory or a file in
one, and neither does a rollback.

A new directory belongs to root, like a new engine volume, and the engine
copies what the image has at the mount path into it the first time, with its
owner and mode. Containers on every node have to agree on the user and group
IDs they write with.

Listing a volume under `shared-volumes` does not copy what it holds. A volume
that was an ordinary one before keeps its files on the node where they are,
under its old name, and the service starts with the shared directory, which
is empty at first. Copy the files into the directory on the storage node if
you want them there.

#### Look at shared storage

```sh
mm --environment production target storage status
```

```text
Target production: app-1 keeps shared volumes in /var/lib/mm/storage, 21.3 GiB free of 29.4 GiB

  app/uploads              12.4 MiB in 340 files; mounted on app-1, app-2, app-3

  app-1                    keeps the volumes and serves them on 10.90.0.1
  app-2                    reaches app-1 in under 1 ms
  app-3                    reaches app-1 in 59 ms

Every node reaches the storage node.
```

The command asks every node at once. It exits 3 when a node does not answer
or cannot reach the storage node, and says what to check. `--output json`
prints the same; the MCP server reads it as `storage`, and the terminal UI
adds a few words to each node's line. Counting a volume's files stops after
three seconds, and the size then reads "at least".

#### When the storage node is down

The storage node's own containers are down with it. On the other nodes:

- **A container that is running keeps running.** Every read and every write
  of a shared file waits until the storage node is back and then goes on,
  also for a file the node has read before: opening it asks the storage
  node. Nothing is reported written that was not. A request that touches no
  shared file is answered as usual, and a health check that touches none
  keeps passing.
- **A container that is started waits for the storage node.** Its start
  goes on by itself once the storage node is back. On a node that had no
  shared volume mounted, as after a reboot, the engine instead gives up
  after about half a minute and leaves the container down; `mmd` on
  the node, which looks every 20 seconds, starts it once the storage node
  answers. So a node that boots before the storage node starts its other
  services as usual, and the ones with a shared volume follow. A container
  you stopped yourself is left alone.
- **A deployment is refused** within a few seconds, before it changes
  anything, with `MM_STORAGE_UNREACHABLE` and the node that cannot reach the
  storage node.

Measured on three 2-vCPU hosts: the storage node rebooted and answered
again after 22 seconds; containers on the other two nodes that wrote to the
shared volume five times a second paused for 15 to 16 seconds and went on,
and none of them was restarted.

`mm target storage status` shows which node cannot reach the storage node:

```text
error[MM_STORAGE_UNREACHABLE]: node 'app-2' cannot reach shared storage on app-1: 10.90.0.1:2049 did not answer within 3 s
  help: check that app-1 is up and `mm target mesh status` shows a handshake between the two nodes; the storage server listens on TCP port 2049 of app-1's mesh address
```

A health check that reads a shared file fails while it waits, and the other
nodes stop sending that container traffic until the check passes again.

#### Stop sharing a volume, and delete its files

Take the volume out of `shared-volumes` and deploy. The service gets an
ordinary volume on each node again, and the shared directory stays on the
storage node with everything in it. `mm target storage status` lists it as
not in the configuration any more. Put the volume back under
`shared-volumes` and the files are there again.

Deleting the files is a command of its own, and the only thing in mm that
removes shared data:

```sh
mm --environment production target storage delete uploads
mm --environment production target storage delete uploads --stack app --plan PLAN_HASH --yes
```

The first form prints the directory, how much it holds, and the hash; the
second deletes it. It needs `admin` access, and it is refused while a
container on any node still has the volume mounted. `--stack` says which
stack's volume you mean when two stacks have one of the same name.

#### Move storage to another node

`cluster.storage` names the storage node. Changing it while the old storage
node holds files is refused, by `mm target setup plan` and again by the apply,
because mm does not copy shared volumes. The refusal lists the steps: stop
the services that use shared volumes, copy `/var/lib/mm/storage` to the new
node with ownership and permissions, move the old copy aside, then change
`cluster.storage`, run setup, and deploy. When the old node holds no file,
setup moves the role by itself and the plan says so.

#### How it is kept private

The storage node serves NFSv4.2 on its mesh address only and exports to the
nodes' mesh addresses only, so the traffic between nodes is inside the
[mesh](#the-mesh-between-the-nodes) and encrypted. Nothing listens on a
public address, and no port mapper or other helper daemon runs: Docker
Engine mounts with the kernel alone and needs no package; Podman runs
`mount.nfs`, which setup installs (`nfs-common`).

Containers cannot reach the storage port. Every node drops what a container
sends to it through the mesh, and the storage node takes storage traffic from
the other nodes' mesh addresses and from nothing else (chains `MM-STORAGE` and
`MM-STORAGE-IN`, loaded at boot before the engine starts). A container gets
at a shared volume only by having it mounted in the Compose file.

The storage node trusts the other nodes: root on any node can read and write
every shared volume of the cluster.

#### Limits

- Shared volumes need the WireGuard mesh mm creates. On a target with
  `cluster.mesh.interface` no node serves them, and a stack that lists
  `shared-volumes` fails `mm config validate`.
- On a target that names one host, a volume listed under `shared-volumes` is
  an ordinary volume.
- A shared volume is a top-level volume of the stack's Compose files without
  `external` or `name`; mm decides where it is stored.
- One storage node per cluster, and no copy of the files anywhere else.
- A file created on one node can be opened on another at once. What a node
  has cached about a file that changed (its size, its time) can be up to a
  minute old.

### When a node does not answer

A node that cannot be reached fails the plan. The error names the node, the
address, what happened, and what to run:

```text
error[MM_UNREACHABLE]: node 'db-1' (203.0.113.13:22) accepts SSH, but mmd is not
running there (the relay found no daemon socket). Start it on the host with
`systemctl start mmd`; `journalctl -u mmd` shows why it stopped. To go ahead
without db-1, add `--nodes app-1,app-2`: db-1 keeps what it runs now
```

To act on the nodes that answer, name them:

```sh
mm --environment production deploy plan --nodes app-1,app-2
mm --environment production deploy apply --plan PLAN_HASH --yes
```

The plan lists the nodes it leaves out; they keep what they run now. Apply
takes no `--nodes`: the plan hash already fixes the nodes. When the node is
back, plan and apply without `--nodes` to bring it to the same commit.

Every request names the node it is for, and a node refuses a request for
another name with `MM_NODE_MISMATCH`. If two nodes' addresses are mixed up in
the repo, nothing is applied on the wrong host; check `host` for each node
under `nodes:` and run `mm target setup record`.

`mm` and the `mmd` of every node must be the same version: a client and a
node that speak different remote protocol versions do not work together. The
error says which side to upgrade: `mm` on your machine, or the host (run
`mm target setup plan --upgrade` and apply that plan with `--upgrade`).

### When mmd stops answering

`mmd` runs as the systemd unit `mmd.service`. It tells systemd when it is
ready to serve, and from then on that it still serves: every ten seconds it
sends itself a request through its own socket, and it reports itself alive
only when that request was answered and the threads that answer names and
watch over child processes are running. A daemon that stops answering
requests, whatever its process is doing, is therefore killed by systemd
within 30 seconds (`WatchdogSec=30s`) and started again two seconds later.
systemd keeps starting a daemon that fails for as long as it takes.

On the host:

```sh
systemctl status mmd             # "Status: serving requests", or what does not answer
systemctl show mmd -p NRestarts  # how often systemd had to start it again
journalctl -u mmd                # why
```

The journal has a line for each canary that got no answer, and systemd's
`Watchdog timeout` line when it replaced the daemon.

A restart, by the watchdog or by you, is safe at any moment:

- A deployment step that was running ends with the daemon. The node's lock
  is released and the rollout is marked interrupted, as
  [described above](#one-deployment-at-a-time-and-one-that-was-interrupted);
  the next deployment needs no manual step. A deployment to a single host is
  marked the same way, and so is every request that was running.
- The node's containers keep running, and their name lookups are answered.
  `mmd` hands every socket it listens on to systemd (`FileDescriptorStoreMax=`
  in `mmd.service`); a restart keeps them open, and the next `mmd` answers
  the lookups and requests that arrived in between. `systemctl stop mmd`
  closes them: a stopped `mmd` answers nothing. A host whose `mmd.service`
  predates this keeps the sockets from its first restart after setup
  installed the new unit.
- Every engine and git command `mmd` starts has a deadline that `mmd`
  enforces from a thread of its own, and is ended with everything it started
  when the deadline passes or when the request that needed it is gone. A
  command that hangs fails its request; it does not stay behind.
- The relay process of a client that went away exits within two seconds,
  whether or not the daemon answered.

### Status, logs, and exec

A service on a cluster is all of its containers on all of its nodes.

`mm service status` asks every node at once and lists each service once, with
a row for each node's container, the deployment the containers were started
from, and what differs on a node from its last deployment. A node that does
not answer is listed with the reason below the table, and the rows of the
other nodes are still shown. See
[check services](./runtime-observability.md#check-services) for the columns
and the notes, and [what each service name resolves to](#see-what-a-name-resolves-to)
for the lines about names.

`mm service logs` reads the service's containers on every node as one stream
in time order, each line with its node and container:

```sh
mm --environment production service logs web --follow
mm --environment production service logs web --node app-2
mm --environment production service logs
```

`--node` reads only the named node, and without a service name the stream has
every service of the environment. A node that does not answer is one line in
the stream. See [read logs](./runtime-observability.md#read-logs) for the
line format, the order between nodes, and the options.

Status and logs take no deployment lock and wait for no deployment, and every
node answers for itself, so they work during a rollout and while another node
is down.

`mm service exec` goes to the node that runs the service, chosen from this
checkout's configuration. For a service placed on several nodes that is the
first of them.

Each node's `mmd` also measures the containers it runs. Every five seconds it
reads the CPU and memory of each running container from the kernel's cgroup v2
files and keeps the last four hours in memory, about 14 KB for a container.
CPU is a percentage of the container's CPU limit, or of one core when it has
none; memory is a percentage of its memory limit, or of the node's memory.
Nothing is written to disk, so the history starts again when `mmd` restarts.
The [autoscaler](#autoscale-a-service) reads them, and `mm service status`
shows the average it decides on for each autoscaled service; no command lists
the numbers of each container.

### Access on a cluster

Every deploy applies the commit's users file on each node it plans, before
that node's stacks. `mm team sync` does the same without deploying, on every
node under one plan hash, and takes `--nodes` like `deploy plan`:

```sh
mm --environment production team sync
mm --environment production team sync --plan PLAN_HASH --yes
```

`mm target trust` lists the pinned and current keys of each node by name.

## Secrets and the deploy key on the host

To set or change one secret or variable of an environment, you need none of
this: `mm -e ENV secret set KEY` stages it on every host through `mmd`. See
[Change an environment's variable](./how-to/change-a-variable.md).

The host resolves a deployment's age secrets from its own store,
`/var/lib/mm-home/secrets/PROJECT_ID.age`, opened with the age identity in
`/var/lib/mm-home/identities/age.key`. Two root commands copy secrets from
the operator's store into it. They exchange a sealed bundle that mm builds
on the operator's machine; you cannot write one by hand.

Print the host store's age recipient. The command creates the store and the
identity if they are missing, and prints the same recipient every time:

```sh
mmd secrets recipient --project PROJECT_ID --environment integration
```

```json
{ "recipient": "age1…" }
```

Import a bundle sealed to that recipient. It is read from stdin, either as
ASCII-armored or as binary age data, up to 24 MiB:

```sh
mmd secrets import --project PROJECT_ID --environment integration < bundle.age
```

```json
{
  "project": "019c527b-6242-7b43-bfd4-bd60e27c1de3",
  "environment": "integration",
  "secrets": {
    "age://database-password": { "revision": "4f0c…", "changed": true }
  },
  "deploy-key": {
    "public-key": "ssh-ed25519 AAAA… mm-deploy-integration",
    "changed": false
  }
}
```

- The host must already run the environment for the project (`mmd access
enroll`). A bundle for another project or environment is refused.
- Each secret keeps the revision it has in the operator's store, so revisions
  on the host and on the operator's machine compare equal. A secret whose
  revision is already active is left unchanged. Secrets that the bundle does
  not list stay in the host store.
- A bundle can carry the environment's deploy key. The import checks that its
  private half matches its public line, then replaces
  `/var/lib/mm/projects/PROJECT_ID/deploy-key` and `deploy-key.pub`. Later
  setup runs keep that key, so one key on the git host serves every node.
- The output names references and revisions only, never values.

## Read secrets from OpenBao or Vault on the host

A host can read an environment's secrets from an external OpenBao or Vault
itself, so their values never reach the machine that runs `mm deploy`. The
host logs in with its own AppRole (a role ID and a secret ID that the store
issues to a machine), reads each secret's version when it plans, and reads the
values when it applies the plan.

You need an initialized, unsealed OpenBao 2.3 or 2.4, or Vault 1.18 or 1.19,
that the host reaches over HTTPS, with a KV version 2 mount and an AppRole
mount, and the PEM file of the certificate authority that signed the store's
certificate on the host.

Declare the provider and the secrets in the project as for
[any external store](./secret-providers.md#connect-an-existing-server). When
the hosts of a target reach the store somewhere else than the provider's
`address`, for example on loopback because it runs on the host, or on a
[mesh](#the-mesh-between-the-nodes) address, say so on the target:

```yaml
targets:
  production:
    host: 203.0.113.10
    host-key: ssh-ed25519 AAAA…
    daemon-fingerprint: 8f2c…
    providers:
      team-secrets: {address: https://127.0.0.1:8200}
```

In the store, create a policy that reads the environment's secrets and an
AppRole with it. mm keeps a secret at
`MOUNT/data/mm/PROJECT_ID/ENVIRONMENT/LOCATION`. The role's tokens must be
renewable and live between one minute and one hour:

```sh
bao policy write mm-host-production - <<'POLICY'
path "mm-kv/data/mm/PROJECT_ID/production/*"     { capabilities = ["read"] }
path "mm-kv/metadata/mm/PROJECT_ID/production/*" { capabilities = ["read"] }
POLICY
bao write auth/mm-approle/role/mm-host-production \
  token_policies=mm-host-production token_ttl=15m token_max_ttl=30m
```

Then connect the host, as root. The command reads the role ID and the secret
ID from stdin, one per line, so they appear in no argument and no shell
history. Any command that prints the two lines works; this one asks the store:

```sh
{
  bao read -field=role_id auth/mm-approle/role/mm-host-production/role-id; echo
  bao write -f -field=secret_id auth/mm-approle/role/mm-host-production/secret-id; echo
} | mmd secrets connect --project PROJECT_ID --environment production \
      --provider team-secrets --address https://127.0.0.1:8200 \
      --ca-file /etc/mm/bao-ca.pem
```

```json
{
  "project": "019c527b-6242-7b43-bfd4-bd60e27c1de3",
  "provider": "team-secrets",
  "driver": "openbao",
  "address": "https://127.0.0.1:8200",
  "trust-sha256": "sha256:5d1c…",
  "environments": ["production"],
  "authenticated": true
}
```

From now on a plan for this environment carries the version of every secret
the host read, and `mm deploy` delivers the values on the host.

- The host must already run the environment for the project (`mmd access
enroll`).
- The command logs in before it stores anything. A store that does not
  answer, a certificate the CA file did not sign, or a refused login leaves
  the host as it was.
- The connection is kept encrypted in
  `/var/lib/mm-home/secrets/openbao-connections.age` (`vault-connections.age`
  for Vault), with its key in `/var/lib/mm-home/identities`, both readable by
  root only. No command and no request prints the role ID or the secret ID.
- Add `--driver vault` for Vault, `--namespace` for a namespace, and
  `--storage-mount` or `--auth-mount` when the mounts are not `mm-kv` and
  `mm-approle`. They must match the provider in the project.
- A host reads the store only at the address the project declares for the
  environment. When the two differ, the plan is refused and names both ways
  to fix it.
- Run the command again to replace the credential or to add another
  environment. To move to another address or certificate authority, run
  `mmd secrets disconnect --project PROJECT_ID --provider team-secrets`
  first.
- On a [cluster](#set-up-a-cluster), connect every node that runs a service
  which uses one of the provider's secrets. A node asks the store only for
  the secrets of its own services, and a node that uses none needs no
  connection. The store has to answer on an address every such node reaches:
  serve it on a node's mesh address and declare that address on the target.
- A plan made before the host is connected is refused with the command to
  run on the host.

To rotate a secret, write a new version in the store and deploy again. The
new version makes a new plan, and only the services that use the secret are
replaced, whether it reaches them as an environment variable or as a
[mounted file](./secret-delivery.md#file-delivery). The host keeps the
previous version of a file secret until the deployment has succeeded, so a
deployment that fails returns to the file it mounted before; after a
successful one the previous version is removed.

## Recover or upgrade

After a connection drops during a change, inspect service status and the
audit log before retrying. A lost response does not mean the operation
failed. Plans expire after ten minutes and cannot be replayed.

### Upgrade mm and mmd

Install the new release on your machine with `mmd` beside `mm`, then run
setup again. Setup installs both binaries from the directory of the `mm` you
run (or `--host-binaries`), one node at a time:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://modmon.dev/install.sh | sh -s -- --with-daemon
mm target setup plan --upgrade
mm target setup apply --plan PLAN_HASH --upgrade --yes
```

`--upgrade` says you mean to replace the release the nodes run; without it
setup refuses and names the nodes. An environment that pins
`deploy.mm-version` needs no flag. [Upgrade mm and your
hosts](how-to/upgrade.md) lists which installations setup upgrades in place,
what it refuses, and what it cannot check.

On each node, setup replaces `/usr/bin/mm` (the relay) and `/usr/libexec/mmd`
together, restarts `mmd`, and waits for it to serve before it moves on to the
next node. The new relay takes its place only as `mmd` restarts, so a node
answers with the old pair or the new one, never a mix. What the node runs is
not touched:

- Containers keep running; the engine is not restarted.
- Name lookups and requests that arrive during the restart wait in their
  sockets and are answered by the new `mmd` (see
  [when mmd stops answering](#when-mmd-stops-answering)).
- A deployment that is running on the node fails at its current step and
  rolls back, as for any restart. Check `mm deploy status` first and upgrade
  between deployments.
- Setup keeps `/var/lib/mm`, so the daemon identity stays the same and
  nothing needs to be recorded again.

A client and a host on different protocol versions refuse to work together,
and the error says which side to upgrade. Between releases of one protocol,
`mm doctor` warns with `MM_HOST_RELEASE_OLDER` or `MM_HOST_RELEASE_NEWER` and
names the hosts. To keep a team on one release, pin it per environment with
[`deploy.mm-version`](reference/configuration.md#bindings-environments-and-secrets):
plans from another mm, and for hosts that run another `mmd`, are refused.

Keys need nothing: a key made by an earlier mm keeps working, and
`mm doctor` names a plain one (`MM_KEY_UNPROTECTED`); `mm key protect`
encrypts it.

### Back up and restore a host's records

A host keeps records that neither the repository nor the other nodes can
give back: the host policy with your edits, emergency blocks, each
environment's synced access and deployment history, the addresses of the
cluster networks the node holds, and the audit log. Export them as root:

```sh
mmd state export > /root/mm-state-app-1.tar
```

The archive is a tar file with `mm-state.json` first, which names the node,
the `mmd` release, and every file. It never holds a key, a secret value, or a
passphrase: the daemon identity, the mesh and deploy keys, the host's secret
stores, and its connections to OpenBao or Vault stay out, as do checkouts and
the files of shared volumes. Keep the archive like any host backup.

To restore it on the same node, for example after a reinstall, run setup
first so the host has `mmd`, then, as root on the node:

```sh
systemctl stop mmd
mmd state import < /root/mm-state-app-1.tar
systemctl start mmd
```

The import refuses an archive of another node, one with any file an export
never holds, and a host policy that does not load, and changes nothing then.
It leaves out a deployed record whose checkout the host does not have; the
next deployment writes it. Then run `mm target setup record` if setup gave
the node a new daemon identity, and deploy again to start the containers.
Back up the files of shared volumes on the storage node yourself
(`/var/lib/mm/storage`).

### The audit log

Every request, refusal, and change is a line of JSON in
`/var/lib/mm/audit.jsonl`. `mmd access audit --limit N` prints the newest
records. The log needs no care: at 16 MiB it becomes `audit.jsonl.1`, the
older files move up, and the fifth is deleted, so a host keeps about 80 MiB
of history. Copy the files elsewhere if you must keep more. Request records
used against replay are kept for two days.

### A mistake in host.yaml

`mmd` reads `/etc/mm/host.yaml` for every request. If the file does not
load, `mmd` keeps running and keeps answering its containers' names, and it
refuses every request with the file, the line, and what to run, until the
file is fixed:

```sh
mmd access validate
```

No restart is needed after the fix. Until then sshd accepts the keys of the
last policy that loaded.

### Hosts set up with access.yaml

Hosts set up with the earlier per-developer accounts and `access.yaml` must
move to this layout: `mmd` refuses a schema-1 `access.yaml`. Write
`host.yaml`, add each developer to the users file with `mm team join`, sync,
check with `mm team whoami`, and then delete the old `mm-*` accounts.
