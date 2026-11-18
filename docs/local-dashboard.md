---
title: Local dashboard
description: View project health, routes, and recent operations in your browser.
---

The dashboard shows service health, routes, resources, and recent operations.
It is read-only: use the CLI or terminal UI to deploy or change configuration.
In the development environment it also lists the
[optional units](./onboarding.md#run-part-of-a-large-project) that are off on
this machine, greyed out, with the `mm dev enable` command for each.

## Open the dashboard

Install the [shared local services](./local-services.md), then open
`https://mm.test`. Choose a registered project to see its dashboard.
Use the links shown by mm rather than guessing a project hostname.

The full dashboard setup requires Linux with systemd and HAProxy, plus Docker
Compose or Podman Compose. Configure DNS or hosts-file entries for its `.test`
names; mm does not add them automatically.

## Check a problem

```sh
mm --runtime docker-compose dev services status
mm dev tls status
mm doctor
```

Use `podman-compose` instead if it owns your shared services. If a project was
just registered, refresh the shared services to add its dashboard certificate:

```sh
mm --runtime docker-compose dev services plan
mm --runtime docker-compose dev services apply --plan PLAN_HASH --yes
```

Replace `PLAN_HASH` with the printed hash. See
[troubleshooting](./help/troubleshooting.md) for DNS, certificate, and service
repair steps.

## Access and privacy

The dashboard omits secret values, credentials, raw command output, and private
filesystem paths. Anyone who can reach the trusted local routes can view the
overview; it is not a per-user authorization system for shared workstations.

Normal setup starts the dashboard automatically. `mm dev dashboard serve` is
for development and diagnostics. Its backend defaults to `0.0.0.0:42831` and
requires mm's private proxy credential; do not expose or proxy it separately.

API consumers can use the version-1 [dashboard schema](/schema/dashboard.schema.json).
The read-only endpoints are `/api/v1/dashboard`, `/api/v1/events`, and
`/api/v1/schema`. Events tell clients when to refresh their snapshot.
