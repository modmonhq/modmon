# .NET web API

```text
client -> ASP.NET Core API:8080 -> PostgreSQL
                    |
                    +-> shared Mailpit SMTP
```

The API creates a small `widgets` table and exposes health, widget, and mail
preview endpoints. NuGet uses committed locked-mode restore metadata.

## Prerequisites and setup

Use Linux with systemd and HAProxy, mm, Docker Buildx, and Docker or Podman
Compose. Replace `ghcr.io/example/mm-dotnet-web-api` with a repository you can
push to. Commit changes and use a full checkout with tags. See the
[setup walkthrough](../../docs/tutorials/first-local-app.md)
if this is your first example. Run from this directory:

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
```

Local development selects the Dockerfile's SDK `development` target, mounts
source at `/source`, and preserves `/source/bin` and `/source/obj`. Production
selects the final ASP.NET runtime image. Local environment delivery is
`env-file`; production is explicitly `direct`.

## Local URLs

- API: `https://api.dotnet-web-api.test`
- Installation dashboard: `https://mm.test`
- Mailpit: `https://mail.mm.test`

Find the project dashboard from `https://mm.test`. Configure DNS or hosts-file
entries for browser access. Replace each plan placeholder with its printed hash.

```sh
curl --resolve api.dotnet-web-api.test:443:127.0.0.1 \
  https://api.dotnet-web-api.test/health
curl --resolve api.dotnet-web-api.test:443:127.0.0.1 \
  -H 'content-type: application/json' \
  -d '{"to":"developer@example.test"}' \
  https://api.dotnet-web-api.test/api/mail-preview
```

The health request must return HTTP 200 with `"status":"ok"`; the mail request
must return HTTP 202 before the message is visible in Mailpit.

Use `mm service logs api --follow`, `mm service exec api -- dotnet --info`, and `mm service status` for
logs, command execution, and service health.

## Data and cleanup

Use this example for development.
PostgreSQL consumes its initial password only for an empty volume, so rotate it
as a coordinated database operation. The app currently expects an environment
password; applications that support `...__FILE` should use a `secret-files`
declaration and the runtime-native delivery described in the docs.

Public ASP.NET environment names and database coordinates are normal bindings
in `mm.yaml`; only the database password declaration is public, and its value
stays in the Age store. Cleanup is explicit:

```sh
mm dev down --yes
mm dev services plan --operation remove
mm dev services remove --plan SERVICES_PLAN_HASH --yes
```

Use `--purge` in both service commands only when retained Mailpit and access
data can be lost.
