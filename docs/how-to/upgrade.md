---
title: Upgrade mm and your hosts
description: Upgrade mm on your machine and mm and mmd on your hosts, which upgrades setup makes in place, and what it refuses.
---

An upgrade has two halves: `mm` on every machine that deploys, and `mm` (the
relay) and `mmd` on every host. Each release's notes start with the remote
protocol it speaks, such as `Remote protocol: 19`. A client and a host on
different remote protocols refuse to work together, so when the protocol
changes, upgrade the hosts and every machine that deploys to them. When it does
not change, they keep working together and `mm doctor` warns until they run the
same release.

## Before you start

- Install the new release on your machine with `mmd` beside `mm`: run the
  installer with `--with-daemon`, or unpack the release archive, which holds
  both (see [install mm](../start/install.md)). `mm --version` prints the new
  release. Setup installs the `mm` and `mmd` from the directory of the `mm` you
  run; pass `--host-binaries DIRECTORY` to install others.
- Check that no deployment runs: `mm deploy status`. A deployment that runs on
  a node while its `mmd` restarts fails at its current step and rolls back.
- If the environment pins the release with
  [`deploy.mm-version`](../reference/configuration.md#bindings-environments-and-secrets),
  change the pin to the new release in a reviewed commit first. A pinned
  environment refuses plans from any other `mm`, and the pin also lets setup
  install that release without `--upgrade`.

## Upgrade a cluster

From the project, with the environment selected as for setup:

```sh
mm target setup plan --upgrade
mm target setup apply --plan PLAN_HASH --upgrade --yes
mm target nodes
```

The plan has a line for each node that says what setup replaces, such as:

```text
app-1: replaces mm and mmd 0.1.0 (remote protocol 18, build 3f9a1c2e7b4d) with 0.1.1 (remote protocol 19, build 7d0e5a91c2f3)
```

Expected result: `apply` finishes after its `verify` step, and `mm target
nodes` lists every node as answering with the new `mmd` version. Then everyone
else who deploys installs the same release.

Setup works on the nodes over SSH as the setup account, not over the remote
protocol, so it upgrades hosts whose protocol this `mm` no longer speaks.

## Upgrade a single host

For a target without `nodes:`, run setup with the remote access flags you set
the host up with (`--origin URL` or `--delivery push`, and `--break-glass`),
and `--upgrade`:

```sh
mm target setup plan --delivery push \
  --break-glass "ops-recovery=$(cat ops-recovery.pub)" --upgrade
mm target setup apply --plan PLAN_HASH --delivery push \
  --break-glass "ops-recovery=$(cat ops-recovery.pub)" --upgrade --yes
```

The plan reads the release the host's `mmd` reports, over SSH as the target's
account, and treats it as cluster setup treats a node (see below). Its remote
access section says what setup replaces, named after the target:

```text
    release:         production: replaces mm and mmd 0.1.0 (remote protocol 18, build 3f9a1c2e7b4d) with 0.1.1 (remote protocol 19, build 7d0e5a91c2f3)
```

Expected result: `apply` finishes, and `mm doctor` reports the host's `mmd` on
the new release.

## What setup does with each node

Before it plans, setup reads the release each node's `mmd` reports: its
version, its remote protocol, and a digest of the `mmd` file, which tells two
builds of one release apart. One release is newer than another when its remote
protocol is later, or the protocol is the same and its version is later. Every
build before 0.1.1 reports version 0.1.0; the plan tells them apart by protocol
and build.

| The node runs | The plan says | What to pass |
| ------------- | ------------- | ------------ |
| No `mmd` yet | `app-1: installs mm and mmd 0.1.1 (…), in place of the earlier release the node may run` | Nothing |
| An `mmd` that does not report its release: every build before remote protocol 16, including builds without the mesh commands | The same line as above | Nothing |
| An older release, or an older build of remote protocol 16 or later | `app-1: replaces mm and mmd 0.1.0 (remote protocol 18, build …) with 0.1.1 (…)` | `--upgrade`, or a `deploy.mm-version` pin that this release matches |
| Another build of the same release, such as a teammate's own build | `app-1: replaces mm and mmd 0.1.1 (remote protocol 19, build …) with 0.1.1 (remote protocol 19, build …)` | `--upgrade`, or the pin |
| This build | `app-1 runs this release of mm, 0.1.1 (…); setup installs its host binaries again` | Nothing |
| A newer release | Refused, see below | `--downgrade` to install the older release anyway |

Without `--upgrade` (or the pin), a plan that would replace an older release or
another build is refused and names the nodes:

```text
app-1 runs mm 0.1.0 (remote protocol 18, build 3f9a1c2e7b4d), and this mm is 0.1.1 (remote protocol 19, build 7d0e5a91c2f3): setup would replace the mm and mmd the hosts run with this build. Run setup with the mm the hosts run to keep them; pass --upgrade, or pin this release with deploy.mm-version, to install 0.1.1 (remote protocol 19, build 7d0e5a91c2f3)
```

A plan that would replace a newer release is refused even with `--upgrade`:

```text
app-1 runs mm 0.1.2 (remote protocol 19, build …), newer than this mm 0.1.1 (remote protocol 19, build …): setup would install this older mm and mmd there and cut off every client that is up to date. Install mm 0.1.2 or later on this machine and run setup again, or pass --downgrade to install 0.1.1 (remote protocol 19, build …) anyway
```

Setup cannot read the release, and installs this one without a flag and
without a refusal, in three cases:

- A node whose SSH host key the repository does not record yet, such as a new
  node or one you rebuilt and plan with `--trust-new-host-keys`: it is read
  only during apply.
- A target that uses an existing private network (`cluster.mesh.interface`)
  instead of the WireGuard mesh `mmd` runs: no node reports its release, and
  the plan names none.
- The `local` target, which setup runs on this machine.

## What an upgrade changes and keeps

Setup upgrades one node at a time. On each node it replaces `/usr/bin/mm` (the
relay) and `/usr/libexec/mmd` together, rewrites `/etc/mm/mmd.json` and the
`mmd`, `mm-mesh`, and `mm-network` service units, restarts `mmd`, and waits for
it to serve before it moves on. The new relay takes its place only as `mmd`
restarts, so a node answers with the old pair or the new one, never a mix.

It keeps:

- **Containers.** They keep running; the container engine is not restarted.
- **State.** `/var/lib/mm` stays: the daemon identity (nothing needs to be
  recorded again), deployment history, the audit log, the addresses of the
  cluster's networks, and the mesh key.
- **The mesh.** Each node keeps its WireGuard key and its peers; the plan shows
  every existing link as kept. A node that still runs the `wg0` interface of
  an early cluster build moves to `mm0` with the same key, and the plan marks
  it.
- **Shared storage.** The storage node keeps serving the same files. Setup
  never takes a storage node that holds files out of service.
- **Secrets.** Setup pushes the environment's secrets again, sealed to each
  node, at the revisions the plan shows. Their values change only if they
  changed in the secret store.

Name lookups and requests that arrive while `mmd` restarts wait and are
answered by the new `mmd`, when the `mmd` being replaced hands its sockets to
systemd: every build that reports its release does. On a node that runs an
earlier build, requests are refused and lookups go unanswered until the new
`mmd` serves.

Until setup has done the last node, the nodes run two releases. Deploy again
only after every node is upgraded: a node on another remote protocol refuses
the new `mm`. To upgrade some nodes first, pass `--nodes app-1,app-2`; the
others are not contacted and keep their release.

## Clients and hosts on different releases

When the remote protocols differ, the error says which side to upgrade:

```text
this mm speaks remote protocol 18, and the host's mmd 0.1.1 speaks 19. Upgrade mm on this machine to 0.1.1 or newer
this mm speaks remote protocol 19, and the host's mmd 0.1.0 speaks only 18. Upgrade the host: run `mm target setup plan --upgrade` and apply that plan with --upgrade
```

When only the versions differ, `mm doctor` warns with
`MM_HOST_RELEASE_OLDER` or `MM_HOST_RELEASE_NEWER` and names the hosts.

## Upgrades that need more than setup

- **Hosts set up with `access.yaml`.** Builds that gave each developer an
  account on the host kept access in `/etc/mm/access.yaml`, which `mmd` now
  refuses. Move them first: see
  [hosts set up with access.yaml](../remote-daemon.md#hosts-set-up-with-accessyaml).
- **A rebuilt node.** It has a new SSH host key: plan with
  `--trust-new-host-keys` and check the fingerprints the plan prints. To bring
  back its records, see
  [back up and restore a host's records](../remote-daemon.md#back-up-and-restore-a-hosts-records).

## How this is checked

| Statement | Backed by |
| --------- | --------- |
| What the plan says for each kind of node, and what it refuses | `core/tests/upgrade_paths.rs`; `core/tests/mesh.rs`: `setup_refuses_to_downgrade_a_node_and_says_what_it_replaces`, `setup_refuses_another_build_of_the_same_release_unless_asked` |
| A single host is held to the same rules | `core/tests/upgrade_paths.rs`: `a_target_without_nodes_reads_its_host_release_as_a_cluster_reads_each_node` |
| Which side to upgrade when the protocols differ | `core/tests/upgrade_paths.rs`: `a_client_and_a_host_on_different_protocols_are_told_which_side_to_upgrade` |
| No container restarts and no lookup or request fails while `mmd` is replaced | `scripts/live-cluster.sh`, stage "an mmd upgrade, one node at a time, under traffic" |

The live stage replaces `mmd` with another copy of the same build. No test in
the repository upgrades real hosts from one release to another yet: try an
upgrade on a staging cluster before production.
