---
title: Local HTTPS
description: Set up, trust, rotate, repair, and remove the local certificate authority.
---

mm creates a local certificate authority (CA) so browsers can trust your
development HTTPS sites. Setup installs its public certificate; private keys
stay under `MM_HOME/ca` (normally `~/.mm/ca`).

## Set up certificates

The Linux checkout installer sets up DNS, trusted TLS, and shared ingress by
default. Retry just that setup with `./install.sh --setup-only` from the mm
checkout. Bare `mm` also detects missing local TLS and offers setup before opening
the workspace, including interactive sudo authorization when required.

```sh
mm dev setup
mm dev tls status
```

Linux uses the system trust store and requires preauthorized `sudo`. Supported
layouts are Debian/Ubuntu (`update-ca-certificates`) and Fedora/RHEL
(`update-ca-trust`). macOS uses current-user trust by default. Use
`mm dev setup --system` or `mm dev tls --system ...` on macOS only when
system-wide trust is intentional and privileges are available.

`mm dev setup --no-trust` still generates certificates but intentionally leaves
browsers and system clients unable to trust them. `mm doctor` reports this
degraded mode until setup is rerun without `--no-trust`.

## Routes and name resolution

For an eligible service in `project.development-environment`, local ingress
uses:

```text
https://{service-slug}.{project-slug}.test
```

Shared service routes include `https://mm.test` and `https://mail.mm.test`.
Follow project dashboard links from `https://mm.test`. Explicit ingress
configuration still takes precedence over automatic routing. Inspect the
result with:

```sh
mm config ingress list
mm config ingress explain web
mm --output json config ingress list
```

### Minimal project configuration

**Do not add `domains` entries or explicit ingress routes just to enable local
service names.** For `project.name: store` and services named `web` and `api`, the
automatic URLs are `https://web.store.test` and `https://api.store.test`.

The project slug (sometimes called the project key) is the normalized
`project.name`; there is no separate `project-key` field. Set
`project.development-environment` to the environment you run locally, and set
`default-environment` to that name if it should be selected automatically. The
selected target must use local transport.

Each service must be declared under a module and attached to a Compose stack.
mm infers an unambiguous container HTTP port from Compose `expose`/`ports`; set
the service's `http-port` only when inference is ambiguous. Automatic ingress
and local HTTPS default to enabled. Non-HTTP roles, explicit ingress routes,
and per-service/project opt-outs affect eligibility.

Routing, DNS, and TLS are separate:

- **Routing:** mm generates `{service-slug}.{project-slug}.test` routes from the
  project contract, without per-project domain declarations.
- **DNS:** installer workstation setup resolves wildcard `.test` names to
  `127.0.0.1`; it reuses a working resolver or installs mm's dedicated DNS service.
- **TLS:** setup installs and trusts the local CA. Deployment generates and
  publishes the leaf certificates for the actual project routes.

If you used `--no-setup`, run workstation setup before opening these URLs, or
use a client override to diagnose routing separately from DNS:

```sh
curl --resolve web.file-factory.test:443:127.0.0.1 \
  https://web.file-factory.test/
```

The CLI does not rewrite `/etc/hosts`. The installer manages its own DNS service;
project configuration stays independent of workstation resolver settings.

### Traefik shows its default certificate

`TRAEFIK DEFAULT CERT` means the proxy has no matching loaded certificate for
that hostname. If `https://mm.test` works but a project hostname does not, check
the project's deployment rather than reinstalling the workstation CA.

Workstation setup creates shared-service certificates. Project certificates and
routes are published by a reviewed project deployment. Running containers alone
does not register their routes. From the project directory, prepare a matching
published build and apply a fresh deployment plan:

```sh
mm build --publish
mm deploy plan
mm deploy apply --plan PLAN_HASH
```

Use the same runtime and environment for all three commands. Once the project's
HTTPS route and certificate are published, its HTTP route redirects to HTTPS
automatically. There is no need to add a `domains` entry for the conventional
service hostname. An unknown/unpublished hostname is not redirected to a broken
HTTPS endpoint.

## Rotation and repair

### Temporarily release public ports

To let another development stack use ports 80 and 443, stop only mm's global
public ingress forwarder:

```sh
mm ingress stop
mm ingress start
```

Run the first command to stop forwarding, and the second when you want it back.
Stopping an absent or already stopped forwarder succeeds with “Public ingress
is not running.” and does not request sudo.
`start` is idempotent: it checks the installed forwarder, shared ingress services,
and local TLS. If installation is absent, interrupted, or unhealthy, it prepares
the forwarder prerequisites, sets up TLS when needed, and offers reviewed repair
and installation plans. A healthy running installation is left alone. It reuses
the runtime recorded for the shared services; on first installation, pass
`--runtime docker-compose` or `--runtime podman-compose` if both are available.
JSON and noninteractive use never prompt. For noninteractive installation or
repair, pass `mm ingress start --yes`. This does not configure workstation DNS;
`./install.sh --setup-only` does that. Project containers, volumes, DNS, and CA
trust are retained. These commands control all mm sites on the workstation, not
one site's route. An operation that explicitly reconciles host ingress can start
the forwarder again.

To check forwarding without changing anything, or to reconcile and restart it:

```sh
mm ingress status
mm ingress restart
```

`status` reports forwarding separately from the shared services and local TLS
it depends on, so a deliberately stopped forwarder is not reported as broken.
With `--output json`, `state` is `running` or `stopped`, `forwarder.failed`
marks a forwarder that exited with an error, and `ready` is `true` only when
forwarding is running and the services and TLS are healthy. `restart` follows
the same approval rules as `start`.

How the forwarder runs depends on the workstation:

- **Linux:** HAProxy runs as the `mm-ingress-forwarder.service` system unit.
  `start` and `stop` request sudo only for that fixed operation; the `mm`
  process remains your user. Authorize with `sudo -v` first for noninteractive
  use. To also change boot startup, use
  `sudo systemctl disable --now mm-ingress-forwarder.service` and
  `sudo systemctl enable --now mm-ingress-forwarder.service`.
- **macOS:** HAProxy runs as your user in the `local.mm.ingress-forwarder`
  launchd agent; macOS lets any user listen on ports 80 and 443 on all
  addresses, so `start` and `stop` never use sudo. `stop` unloads the agent but
  keeps it installed, so it starts again at your next login. To keep it off
  across logins, run
  `launchctl disable gui/$(id -u)/local.mm.ingress-forwarder` after
  `mm ingress stop`, and `launchctl enable gui/$(id -u)/local.mm.ingress-forwarder`
  before `mm ingress start`. Forwarder output is in
  `~/Library/Logs/mm/ingress-forwarder.log`.

### Rotate certificates

Leaf certificates are valid for 30 days and renew within seven days of expiry
when deployments or shared services update their routes. To replace the root:

```sh
mm dev tls rotate
# Reapply each local deployment and verify every HTTPS route.
mm deploy plan
mm deploy apply --plan DEPLOY_PLAN_HASH --yes
# Reissue and publish certificates for managed service routes.
mm --runtime podman-compose dev services plan --operation repair
mm --runtime podman-compose dev services repair --plan SERVICES_PLAN_HASH --yes
# Remove the old root only after all routes pass verification.
mm dev tls repair
```

Rotation temporarily trusts both old and new roots because existing ingress
volumes may still serve old-generation leaves. Do not run the final repair
until every local deployment and the shared service stack have been reconciled
and checked. `mm dev tls status` reports a pending rotation during this overlap;
`repair` then removes only the exact owned old root.

Use `docker-compose` in both service commands if Docker Compose owns the
installation.

Use `mm dev tls repair` for missing trust, incorrect key permissions, or an
interrupted rotation. Renew application certificates by applying a fresh
deployment; renew shared-service certificates with the repair flow above.
mm checks certificate ownership and will not replace another tool's certificate.

## Uninstall

```sh
mm dev tls uninstall
```

Uninstall removes exact mm-owned trust entries and local CA/private leaf
material. Remove or reconfigure dependent local services first if you still
need their HTTPS routes. Uninstall does not alter DNS, the hosts file, or
unrelated certificates.
