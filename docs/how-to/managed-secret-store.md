---
title: Keep deploy secrets in a managed store
description: Let mm run OpenBao on your cluster's nodes, initialise and unseal it, and read deploy secrets from it with versions and an audit log.
---

A cluster environment can keep its deploy secrets in an OpenBao store that
`mm` runs for it. `mm target setup` installs one OpenBao member on each node,
outside every Compose stack, joins them into one Raft cluster over the mesh,
initialises the store, and gives each node's `mmd` what it needs to unseal its
member and read the environment's values. You never unseal anything by hand.

Compared with keeping deploy secrets in the age store that setup pushes, the
store gives each value versions, an audit log of every read and write, and a
policy that limits each node to one environment's values. Your applications
still get values as environment variables or files at deploy time; they never
talk to the store.

## What you need

- A [cluster target](./cluster.md) with 3 or 5 nodes on mm's WireGuard mesh,
  set up with `mm target setup`. With another number of nodes, name 3 or 5 of
  them as the store's members (below).
- Nodes that can download from `github.com` during setup, or the release
  archive placed on each node first (see [Nodes without internet
  access](#nodes-without-internet-access)).
- `mm` and `mmd` of this release on your machine and the nodes.

## Declare the store

Add a provider with `mode: managed` and `scope: environment`, and point
secrets at it:

```yaml
providers:
  deploy-secrets:
    driver: openbao
    mode: managed
    scope: environment # one store per environment
    release: "2.4"

secrets:
  db_password:
    storage: { provider: deploy-secrets, location: db_password }
```

That is the whole configuration. Every environment whose target lists
`nodes:` gets its own store on that target's nodes. In an environment that
runs on your machine the same provider is the [local development
instance](../openbao.md#managed-configuration), so `mm dev` keeps working.

Optional keys:

| Key               | Default                                 | Meaning                                                          |
| ----------------- | --------------------------------------- | ---------------------------------------------------------------- |
| `nodes`           | `all`, when the target has 3 or 5 nodes | The members, as a list of node names: 3 or 5 of them.            |
| `installation`    | the provider's name                     | Local development only: the name of the local instance.          |
| `management-port` | `18200`                                 | Local development only: the loopback port of the local instance. |

`mm config validate` refuses a store on a target with another number of nodes,
a release mm does not install, and a second provider with `scope: environment`,
each with `MM_SECRET_STORE_INVALID` and the key to change.

## Set it up

```sh
mm -e production target setup plan
mm -e production target setup apply --plan PLAN_HASH --yes
```

The plan has a `secret-store` step and says, for each member, whether setup
installs OpenBao, keeps it, or upgrades it, and whether this apply initialises
the store. On the first apply setup:

1. downloads the pinned OpenBao release on each member and checks its SHA-256
   against the digest built into mm before anything is installed;
2. writes the member's configuration and its systemd unit,
   `mm-openbao-ENVIRONMENT-PROJECT.service`, and starts it. The member runs as
   the `mm-openbao` account, may write only its own data directory, and
   listens on loopback and on the node's mesh address, nowhere else;
3. gives each member a TLS certificate for the key it made itself, signed by a
   certificate authority setup creates for the environment. The authority's
   key stays in your machine's sealed store and never reaches a node;
4. initialises the store on the first member, seals its unseal key and
   `mmd`'s logins to each node's own store, keeps a recovery copy sealed to
   you, and revokes the root token;
5. unseals every other member, which joins the Raft cluster, and checks that
   every member is a voter and one of them leads.

Each member is changed and back in service before setup touches the next one.
Running setup again changes nothing that is already in place.

After a node reboots, or after every node reboots at once, each node's `mmd`
unseals its own member as soon as it answers. No one has to be there.

## Use it

The commands are the ones you use with the age store:

```sh
mm -e production secret set db_password          # staged on every node
mm -e production secret generate --missing       # production's, generated here, staged on every node
mm -e production secret rotate db_password
mm -e production secret status
mm -e production deploy plan                     # binds the staged revision
mm -e production deploy redeploy                 # the running commit with the new values
```

A value you set is staged on every node and is not live yet. The next
deployment delivers it, and once that deployment succeeded each node writes it
to the store as a new version. The version keeps the revision the plan bound,
so the containers that use it are not replaced a second time. A deployment
that fails or is rolled back leaves the value staged. `mm secret discard` drops
it.

Who may do what follows the users file: `secrets: true` to set values,
`deploy` to plan with them. Coding tools set values through the MCP server's
stage tool and see their status, never a value.

`mmd` logs in with a role whose policy reads and writes only this
environment's path, and uses each short-lived token for one operation. A
second role, kept the same way, reads the Raft membership.

## What runs where

| On                  | Path                                                  | What                                                   |
| ------------------- | ----------------------------------------------------- | ------------------------------------------------------ |
| Each member         | `/usr/libexec/mm-openbao/VERSION/bao`                 | The pinned release                                     |
| Each member         | `/etc/mm-openbao/ENVIRONMENT-PROJECT/`                | Configuration, certificate, the node's own key, record |
| Each member         | `/var/lib/mm-openbao/ENVIRONMENT-PROJECT/`            | Raft data, and the audit log `audit.log` mmd rotates   |
| Each node           | `/var/lib/mm-home/secrets/openbao-connections.age`    | Unseal key and `mmd`'s logins, sealed for root         |
| Your machine        | `~/.mm/secrets/PROJECT.stores.age`                    | The authority and the recovery copy, sealed to you     |

The store's API answers on port 8200 on loopback and on the node's mesh
address; Raft uses port 8201 on the mesh address. mm's mesh rules drop
anything a container sends to a node's mesh address, so no container on any
network reaches either port.

The audit device writes every request to `audit.log` with values hashed, never
in the clear.

## Move secrets from the age store

An environment whose deploy secrets are in the age store moves them in two
steps. First declare the store and run setup, as above. Then copy the values:

```sh
mm -e production secret migrate --from age --to deploy-secrets
```

A node reads every secret the deployed commit keeps in the age store, as it
is live, and writes each into the store at the same location, as a version
that carries the revision it has now. The command lists each secret and
where it went; it changes nothing the running containers use, and it can run
again: a secret the store holds already is left alone.

Then point the secrets at the store in `.config/mm.yaml`, keeping their
locations, and deploy:

```yaml
secrets:
  db_password:
    storage: { provider: deploy-secrets, location: db_password }
```

```sh
mm -e production deploy plan
```

Each secret keeps its revision, so no container is replaced. The copies in
the age stores stay where they are and are no longer read.

## Check the store

```sh
mm -e production secret provider status deploy-secrets
```

prints each member as its node reports it: its OpenBao release, whether it is
unsealed, and whether it leads or votes in the Raft cluster, followed by what
needs attention. It exits with 3 when a member does not answer, is sealed,
or the store is degraded; differing releases only warn. `--output json`
prints the same report, and an agent reads it
with `mm_read` kind `secret_store`. `mm target nodes` lists the same members
under the nodes, and `mm doctor` reports the same findings:

| Code                           | Meaning                                                                       |
| ------------------------------ | ----------------------------------------------------------------------------- |
| `MM_SECRET_STORE_SEALED`       | A member is sealed. Its `mmd` unseals it once it holds the keys.              |
| `MM_SECRET_STORE_DEGRADED`     | A member does not run or answer, is not a voter, or no member leads.          |
| `MM_SECRET_STORE_VERSION_SKEW` | Members run different OpenBao releases, as during an upgrade that stopped.    |

`mm target setup plan` and its apply bring a member back: they give a node
the keys it lacks and start what does not run.

The audit device writes every request to the member's `audit.log`, with
values hashed. `mmd` rotates it at 16 MiB as it rotates its own audit log, and
prints it next to `mmd access audit` on the node:

```sh
mmd openbao audit --project PROJECT_ID --environment production --limit 20
```

To list every secret the store keeps for the environment, with each version
and when it was written but never a value, run on a member:

```sh
mmd openbao versions --project PROJECT_ID --environment production
```

## Keep the recovery copy safe

The recovery copy holds the store's unseal key, `mmd`'s logins and the
environment's certificate authority. Setup keeps it on the machine that ran
the first apply, sealed to that machine's age identity. Export it to a safe
place:

```sh
mm -e production secret provider recovery export deploy-secrets --to production-store.age
```

To let another operator run setup for this environment, seal it to their age
recipient instead: the `age1…` line `age-keygen -y ~/.mm/identities/age.key`
prints on their machine, once their project's age store exists
(`mm secret init age`):

```sh
mm -e production secret provider recovery export deploy-secrets \
  --recipient age1... --to for-robin.age
# on their machine
mm -e production secret provider recovery import deploy-secrets for-robin.age
```

Setup needs the copy only to give a node its keys again, for example after the
node was reinstalled. Without it on the machine that runs setup, that setup
stops and says how to import it.

## Take a snapshot

```sh
mm -e production secret provider snapshot deploy-secrets --to production-store.snap
```

A member takes a snapshot of the store's Raft data, every value with its
versions and every login, and `mm` writes it with the keys it opens with,
sealed with age to this machine's identity. Taking and restoring a snapshot
needs `admin` in the users file. Keep snapshots where you keep the recovery
copy. `mm` writes them to a file you name; it does not upload them anywhere
or take them on a schedule: run the command from your own scheduler and copy
the file away.

## Restore a snapshot onto fresh nodes

This is also the drill to run before you rely on snapshots. On fresh nodes,
or on the same nodes after their store was removed:

1. Set the nodes up as usual. Setup installs the members and initialises an
   empty store:

   ```sh
   mm -e production target setup plan
   mm -e production target setup apply --plan PLAN_HASH --yes
   ```

2. Review what the restore puts back, then run the command it prints:

   ```sh
   mm -e production secret provider restore deploy-secrets --file production-store.snap
   mm -e production secret provider restore deploy-secrets --file production-store.snap --plan HASH --yes
   ```

   The first member takes the snapshot in place of everything the new store
   holds. Every node then gets the keys the snapshot opens with, sealed to
   its own `mmd`, and unseals its member. The recovery copy on your machine
   becomes the snapshot's.

3. Check that every member serves and every value came back:

   ```sh
   mm -e production target nodes
   mm -e production secret status
   mm -e production deploy plan
   ```

   `secret status` shows each value with the revision it had when the
   snapshot was taken, so the plan replaces no container.

## Upgrade the store

Change the release and run setup:

```yaml
providers:
  deploy-secrets:
    release: "2.5"
```

```sh
mm -e production target setup plan
mm -e production target setup apply --plan PLAN_HASH --yes
```

The plan says which members it upgrades. Setup installs the new release next
to the old one, then restarts one member at a time on it, the leader last:
each is unsealed again by its `mmd` and votes in the Raft cluster before
setup touches the next. A member that does not come back stops the upgrade
with the others on the release they run; `mm doctor` reports
`MM_SECRET_STORE_VERSION_SKEW` until a setup finishes it.

## Replace a node

Reinstall the node, then run setup for it:

```sh
mm -e production target setup plan --trust-new-host-keys
mm -e production target setup apply --plan PLAN_HASH --yes
```

The plan reads what each member holds. A member whose store data is gone
while the store's Raft configuration still lists it shows as `rejoins`, with
the reason. On apply, setup installs the member, removes its old entry from
the Raft configuration through a member that serves, gives it its keys from
the recovery copy on your machine, and waits until it votes again before it
turns to the next node. A member that is only down, with its data intact, is
never removed: setup starts it and its `mmd` unseals it.

The other members keep serving meanwhile: with 3 members the store keeps
working while one is gone. With two of three gone it has lost its quorum, and
no member can remove an entry; restore a snapshot instead.

## Nodes without internet access

Setup downloads `bao_VERSION_Linux_x86_64.tar.gz` (or `_arm64`) from the
OpenBao releases on GitHub on each member. To install without that, copy the
archive to `/var/lib/mm-openbao/archives/` on each node before setup; setup
uses it when its SHA-256 is the pinned one and refuses it otherwise.

## Limits

- One managed store per environment, and one environment's store per node: a
  second environment of the same project on the same nodes would need the
  same ports. Deploy it to another target.
- A node that is not a member reads the store at the first member's mesh
  address.
- The releases mm installs on clusters are OpenBao 2.4 (2.4.4) and 2.5
  (2.5.5); the supported upgrade is 2.4 to 2.5.
