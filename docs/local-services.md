---
title: Local services
description: Install and operate the shared local ingress, dashboard, and development services.
---

Shared local services give your projects HTTPS, a dashboard, and development
email. One installation serves the local target using your chosen runtime:

- Traefik for local HTTP and HTTPS routes;
- the mm dashboard;
- Mailpit for development email;
- Cap and Valkey only when an access-gate policy requires them, or when a
  reviewed `--gate` plan prepares them.

Run TLS setup first. To install everything in one guided flow:

```sh
mm --runtime podman-compose dev setup --services
```

Docker Compose is also supported. A recorded installation belongs to its exact
target and runtime, so always select the runtime that originally installed it.

The complete topology requires HAProxy and one of these workstations: Linux with
systemd, or macOS with Docker Desktop and the Docker Compose runtime. On macOS
the ingress forwarder, Compose autostart, and dashboard run as launchd agents of
your user; see [Install mm](./start/install.md#privileges-and-binary-ownership).

## Install, check, or repair

Review each plan, then replace `PLAN_HASH` with the hash it prints:

```sh
# Install or update the shared services.
mm --runtime podman-compose dev services plan
mm --runtime podman-compose dev services apply --plan PLAN_HASH --yes

mm --runtime podman-compose dev services status

# Upgrade an older installed stack.
mm --runtime podman-compose dev services plan --operation upgrade
mm --runtime podman-compose dev services upgrade --plan PLAN_HASH --yes

# Repair an interrupted operation or changed resources.
mm --runtime podman-compose dev services plan --operation repair
mm --runtime podman-compose dev services repair --plan PLAN_HASH --yes
```

Repeat all options that affect a plan when applying it. For example, prepare
Cap and Valkey with `--gate` on both `plan` and `apply`. A stale hash, changed
runtime, changed target, or changed prior state is rejected.

Normal removal deletes only matching mm-owned runtime resources and retains
persistent data:

```sh
mm --runtime podman-compose dev services plan --operation remove
mm --runtime podman-compose dev services remove --plan PLAN_HASH --yes
```

To delete retained Mailpit and optional Cap/Valkey data, explicitly review and
repeat `--purge`:

```sh
mm --runtime podman-compose dev services plan --operation remove --purge
mm --runtime podman-compose dev services remove --plan PLAN_HASH --purge --yes
```

Purge is irreversible. Removal also fails if project workloads still use a
managed network; stop those workloads first. `mm` does not disconnect foreign
containers or adopt same-named resources without matching ownership.

## URLs and mail

- Dashboard: `https://mm.test`
- Project dashboard: follow the project link from `https://mm.test`
- Mailpit UI: `https://mail.mm.test`
- Container SMTP: `mailpit:1025` on the managed development network
- Cap widget/challenge origin when installed: `https://cap.mm.test`; only pinned
  assets and site-scoped challenge, redeem, and verification paths are public.
  The root dashboard and administration API are not exposed.

Host SMTP is disabled by default. For a host-native application, choose an
unused loopback port and repeat it during apply:

```sh
mm --runtime podman-compose dev services plan --smtp-port 10252
mm --runtime podman-compose dev services apply --smtp-port 10252 --plan PLAN_HASH --yes
```

The port binds as `127.0.0.1:10252`; it is never published on all interfaces.
Services that accept managed development-mail bindings receive defaults such
as `MM_MAIL_SMTP_URL=smtp://mailpit:1025` without replacing existing values.

```yaml
development-mail:
  enabled: false

modules:
  jobs:
    services:
      worker:
        stack: app
        development-mail: false
```

## Data and conflicts

`mm dev services status` reports health, changed images or routes, certificate
problems, and resource conflicts. If another tool owns a conflicting resource,
resolve that conflict rather than relabeling it as mm-owned.

Mailpit and optional Cap/Valkey service data live in managed volumes and survive
normal removal. Purge removes those declared persistent volumes; access-policy,
user, and session records remain in mm's private state rather than those
volumes.
