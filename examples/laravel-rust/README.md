# Laravel + Rust

```text
browser -> Laravel web -> Rust pricing -> Valkey
              |               |
              +-> PostgreSQL  +-> health
              +-> queue
              +-> shared Mailpit SMTP
```

The `app` and `data` stacks share one mm-managed network. Laravel `web` and
`queue` reuse one image. Laravel's scheduled tasks run through an mm job rather
than a separate scheduler container.

## Prerequisites and setup

Use Linux with systemd and HAProxy, mm, Docker Buildx, and Docker or Podman
Compose. Replace both `ghcr.io/example/...` repositories with ones you can push
to, commit changes, and use a full checkout with tags. See the
[setup walkthrough](../../docs/tutorials/first-local-app.md)
for workstation preparation. Run from this directory:

```sh
mm --runtime docker-compose dev setup --services
mm target preference set-runtime docker-compose
mm config validate
mm secret init age
mm secret generate --missing
mm env review
mm build --publish plan
mm build --publish apply --plan BUILD_PLAN_HASH --yes
mm deploy plan
mm deploy apply --plan DEPLOY_PLAN_HASH --yes
mm job schedule reconcile
mm job schedule reconcile --plan SCHEDULE_PLAN_HASH --yes
```

Keep `mm job schedule daemon` running to process scheduled jobs; use a service
manager for persistent development setups. The Laravel
preset mounts source at `/app` for `web` and `queue`, preserving `/app/vendor`.
Local uses `env-file` delivery; production uses direct delivery.

## Local URLs

- Laravel: `https://web.laravel-rust.test`
- Pricing: `https://pricing.laravel-rust.test`
- Installation dashboard: `https://mm.test`
- Mailpit: `https://mail.mm.test`

Find the project dashboard from `https://mm.test`. Configure DNS or hosts-file
entries for browser access. Replace each plan placeholder with its printed hash.

```sh
curl --resolve web.laravel-rust.test:443:127.0.0.1 \
  https://web.laravel-rust.test/up
curl --resolve web.laravel-rust.test:443:127.0.0.1 \
  -H 'content-type: application/json' \
  -d '{"to":"developer@example.test"}' \
  https://web.laravel-rust.test/api/mail-preview
mm service logs queue --follow
mm job run schedule-run
```

The health request must return HTTP 200, the mail request HTTP 202, and the
manual job must finish successfully in `mm job history schedule-run`.

The `staff-preview` gate declaration demonstrates a versioned gate driver and
production ingress reference. Gate users, password hashes, Cap policy, and
sessions remain private state; create and apply that policy with the reviewed
`mm access` workflow before enabling it for real traffic.

## Optional OpenBao and secret files

The checked-in Age provider is the zero-service local path. To evaluate
OpenBao, declare a provider instance and run `mm secret provider add NAME --driver openbao --mode external`, then test
it, then change only a secret's public storage provider, connection, and
location. Never commit AppRole credentials. Workloads that accept file paths can
declare `secret-files`; Podman uses native secret mounts while Docker
Compose uses a private operation-scoped file. Laravel expects these values in
its environment, so this example does not pretend file delivery is native to the
framework.

## Data and cleanup

Use this example for development. Coordinate PostgreSQL
password changes with the database. Public URLs, modes, and hostnames are values in
`mm.yaml`; the Laravel key and database password values stay in the selected
secret store. Rotating `laravel.app-key` invalidates encrypted sessions and
data. Cleanup is explicit:

```sh
mm dev down --yes
mm dev services plan --operation remove
mm dev services remove --plan SERVICES_PLAN_HASH --yes
```

Use `--purge` in both service commands only when retained Mailpit and access
data can be destroyed.
