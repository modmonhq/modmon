---
title: Laravel and Rust example
description: Run image reuse, two stacks, native schedules, a gate, Mailpit, and Rust 1.99.
---

`examples/laravel-rust` separates data and application stacks while joining both
to one mm-managed network. Laravel web and queue reuse one image; Rust pricing
uses its own Rust 1.99 build.

```text
Laravel web -> pricing -> Valkey
     |           |
     +-> queue   +-> health
     +-> PostgreSQL
     +-> shared Mailpit
```

## Before you start

Complete [workstation setup](./first-local-app.md#set-up-the-workstation) on a
Linux systemd host with HAProxy. You need Docker Buildx, a Compose runtime, and
registry access. Replace both example image repositories, commit the changes,
and use a full checkout with tags.

## Inspect and deploy

The Laravel mount preset attaches source at `/app` to web and queue while
preserving `/app/vendor`. PostgreSQL and Valkey remain pinned third-party
Compose services. Local delivery is `env-file`; production is `direct`.

```sh
cd examples/laravel-rust
mm config validate
mm env review
mm dev mounts plan laravel
mm secret init age
mm secret generate --missing
mm build --publish plan
mm build --publish apply --plan BUILD_PLAN_HASH --yes
mm deploy plan
mm deploy apply --plan DEPLOY_PLAN_HASH --yes
```

Replace both illustrative repositories before publication. The build result
contains one Laravel digest reused by `web`, `queue`, and jobs, plus one pricing
digest.

## Use the local platform

- Laravel: `https://web.laravel-rust.test`
- Pricing: `https://pricing.laravel-rust.test`
- Installation: `https://mm.test`
- Mailpit: `https://mail.mm.test`

Follow the project dashboard link from `https://mm.test`. Browser access needs
DNS or hosts-file entries for these names; the curl commands below bypass DNS.

```sh
curl --resolve web.laravel-rust.test:443:127.0.0.1 \
  https://web.laravel-rust.test/api/quotes/example-sku
curl --resolve web.laravel-rust.test:443:127.0.0.1 \
  -H 'content-type: application/json' \
  -d '{"to":"developer@example.test"}' \
  https://web.laravel-rust.test/api/mail-preview
mm service logs queue --tail 50
```

## Run scheduled tasks

There is no scheduler container. `schedule-run` uses the web service's image and settings,
and `laravel-schedule` triggers it every minute:

```sh
mm job schedule reconcile
mm job schedule reconcile --plan SCHEDULE_PLAN_HASH --yes
mm job schedule list
mm job schedule run-now laravel-schedule
```

Keep `mm job schedule daemon` running to process due jobs. Use a service manager
for persistent development setups. Run history survives restarts.

## Gate and provider options

The public `staff-preview` gate names the `password-cap` driver and production
`web` ingress reference. Users, credentials, Cap policy, and sessions are
private state configured through reviewed `mm access` commands. Do not expose a
route until that policy has been reviewed and tested.

Age is the easy local secret provider. An advanced evaluation can create and
test a private external OpenBao connection, then change a secret's public
storage provider/connection/location. AppRole credentials never enter Git.
Runtime-native `secret-files` are appropriate for applications that accept file
paths; Laravel expects these values in its environment, so this example does
not claim otherwise.

Coordinate database password and Laravel app-key rotations with persistent
state. Remove resources through reviewed deploy/service removal, preserving
volumes unless an explicit purge is intended.
