---
title: .NET web API example
description: Run ASP.NET Core with an SDK live mount, PostgreSQL, HTTPS, and Mailpit.
---

`examples/dotnet-web-api` is an ASP.NET Core 11 minimal API backed by PostgreSQL 17. Locked NuGet restore and pinned images keep builds deterministic.

## Before you start

Complete [workstation setup](./first-local-app.md#set-up-the-workstation) on a
Linux systemd host with HAProxy. You need Docker Buildx, Docker Compose or
Podman Compose, and a registry you can push to. Replace the example image
repository, commit the change, and use a full checkout with tags.

## Prepare the project

The local environment selects the Dockerfile's `development` target. It runs
`dotnet watch`, source mounts at `/source`, and managed volumes preserve
`/source/bin` and `/source/obj`. Production selects the final ASP.NET runtime
target and direct environment delivery.

```sh
cd examples/dotnet-web-api
mm config validate
mm env review
mm dev mounts plan api
mm config ingress explain api
mm secret init age
mm secret generate --missing
```

Compose defines PostgreSQL and its data volume. mm supplies the API build,
environment values, and managed network settings.

## Build and deploy

After [workstation setup](./first-local-app.md#set-up-the-workstation), replace
the illustrative registry repository and publish from a clean full clone:

```sh
mm build --publish plan
mm build --publish apply --plan BUILD_PLAN_HASH --yes
mm deploy plan
mm deploy apply --plan DEPLOY_PLAN_HASH --yes
mm service status
```

## Verify HTTPS and mail

```sh
curl --resolve api.dotnet-web-api.test:443:127.0.0.1 \
  https://api.dotnet-web-api.test/health
curl --resolve api.dotnet-web-api.test:443:127.0.0.1 \
  https://api.dotnet-web-api.test/api/widgets
curl --resolve api.dotnet-web-api.test:443:127.0.0.1 \
  -H 'content-type: application/json' \
  -d '{"to":"developer@example.test"}' \
  https://api.dotnet-web-api.test/api/mail-preview
mm service logs api --follow
```

Open `https://mail.mm.test` or the installation dashboard at `https://mm.test`.
Follow the project link there. Shared services may need one
services plan/apply after a newly cloned project is registered so the project
dashboard certificate includes its hostname.

Use this example for development. Coordinate PostgreSQL
password rotation with the database. If an application supports a password-file
setting, prefer `secret-files` and runtime-native delivery; this API accurately
uses secret-backed environment delivery because Npgsql expects a value.
