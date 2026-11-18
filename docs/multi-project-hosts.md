---
title: Run multiple projects on one host
description: Enroll several projects on one host and remove one without affecting the others.
---

One mm host can run several trusted projects. Projects share the daemon,
runtime, and host services, while mm tracks their workloads separately by project
UUID. This prevents accidental cross-project changes; it is not isolation for
untrusted tenants.

Start with [remote host setup](./remote-daemon.md) and
[remote access](./remote-access.md). Use Docker Compose or rootless Podman
Compose for this setup.

## Host layout

```text
/etc/mm/mmd.json                         daemon configuration
/etc/mm/host.yaml                        enrolled projects, ceilings, break-glass keys
/etc/ssh/mm_authorized_keys          written by mmd from each project's users file
/var/lib/mm/projects/<uuid>/repo.git     mmd's mirror of the project's origin
/var/lib/mm/projects/<uuid>/access/      reconciled users per environment
/srv/mm/projects/<uuid>/<env>/<sha12>/   one checkout per deployed commit
/var/lib/mm-home/                        target and shared-service state
```

Configuration is root-owned with mode `0600`. Nobody edits checkouts on the
host: `mmd` fetches each commit from the origin pinned for the project and
keeps one immutable directory per commit.

For rootful Docker, use the supplied hardened root system unit. For rootless
Podman, use a system unit running as a dedicated runtime owner with private
state. Do not tie a shared host to a developer's login session.

## Enroll projects

Enroll each project by running `mm target setup` with `--origin` from that
project's checkout, as in [remote host setup](./remote-daemon.md#set-up-the-host).
Setup adds the project to `/etc/mm/host.yaml` and creates its deploy key,
keeping the other projects' entries. Each project is one entry under
`projects:`, keyed by the UUID in its `.config/mm.yaml`:

```yaml
projects:
  019c527b-6242-7b43-bfd4-bd60e27c1de3:
    name: shop
    origin: git@github.com:acme/shop.git
    origin-host-key: ssh-ed25519 AAAA…
    environments: [staging, production]
  019c527b-6242-7b43-bfd4-bd60e27c1de4:
    name: billing
    origin: git@github.com:acme/billing.git
    origin-host-key: ssh-ed25519 AAAA…
    environments: [production]
    ceiling:
      production: { level: deploy }
```

After editing the file by hand, run `mmd access validate`, then restart `mmd`.
A project the host does not list is refused, whatever its users file says.

`mm target setup plan` and its reviewed apply command also install the selected
runtime. Docker setup supports the configured Debian/Ubuntu repository;
rootless Podman requires a working user namespace. Setup installs the selected
engine rather than silently switching to another. Setup installs the remote
access layer only with Docker, because `mmd` runs as root; for rootless Podman,
install it by hand as described above.

## Give developers access

Each project's users file decides who may reach it, per environment; see
[remote access](./remote-access.md). One SSH key can belong to users of several
projects, and each project only sees its own grants. Two projects can use the
same service names without sharing resources. Project dashboard names include a
UUID to avoid name collisions.

## Remove a project

1. Stop and remove its workloads while the project is still enrolled. Check
   that other projects remain healthy.
2. Remove its entry from `/etc/mm/host.yaml`, run `mmd access validate`, and
   restart `mmd`. Its users lose access at once, and `authorized_keys` drops
   keys no other project lists.
3. Archive `/var/lib/mm/projects/<uuid>` and `/srv/mm/projects/<uuid>`
   according to your retention policy. Removing the enrollment does not delete
   them.

Keep shared services, target state, daemon identity, and audit history. Do not
remove a shared provider or data volume merely because one project leaves.

## Recover after a host failure

Retain `/var/lib/mm`, `/var/lib/mm-home`, and engine volumes. Inspect operation
and audit state before making a fresh plan; an interrupted apply may have an
unknown outcome. Restore a corrupt shared database from backup rather than
deleting it to clear an error. Concurrent deployments may wait for the host's
operation lock; this protects shared route and service updates.
