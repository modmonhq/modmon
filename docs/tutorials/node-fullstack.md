---
title: Node full stack example
description: Run automatic HTTPS, live Node mounts, MongoDB, and shared Mailpit.
---

`examples/node-fullstack` contains a frontend, MongoDB-backed API, and MongoDB
in one stack. It is the smallest complete local-development example.

```text
browser -> frontend -> backend -> MongoDB
                         |
                         +-> shared Mailpit
```

## Before you start

For initial setup, follow [First local app](./first-local-app.md). It covers
workstation requirements, image repositories, secrets, and deployment. This
page adds source mounts, email, and debugging commands.

## Inspect the project

```sh
cd examples/node-fullstack
mm config validate
mm env review
mm dev mounts plan frontend
mm dev mounts plan backend
mm config ingress explain frontend
```

Mounts expose each source tree at `/app`, while separate managed volumes
preserve `node_modules`. Environment values come from the project's bindings
and its encrypted local secret store.

## Build and deploy

Complete [shared workstation setup](./first-local-app.md#set-up-the-workstation),
replace both illustrative image repositories, and use a clean full Git clone.

```sh
mm secret init age
mm secret generate --missing
mm build --publish plan
mm build --publish apply --plan BUILD_PLAN_HASH --yes
mm deploy plan
mm deploy apply --plan DEPLOY_PLAN_HASH --yes
mm service status
```

Automatic HTTPS creates:

- `https://frontend.node-fullstack.test`
- `https://backend.node-fullstack.test`

Use `https://mm.test` for installation status and `https://mail.mm.test` for
Mailpit. Follow project dashboard links from the installation dashboard.
Configure `.test` DNS or use `curl --resolve`; certificates alone do not make
the names resolve.

## Exercise the app

```sh
curl --resolve backend.node-fullstack.test:443:127.0.0.1 \
  https://backend.node-fullstack.test/health
curl --resolve backend.node-fullstack.test:443:127.0.0.1 \
  -H 'content-type: application/json' \
  -d '{"to":"developer@example.test"}' \
  https://backend.node-fullstack.test/api/mail-preview
mm service logs backend --tail 50
mm service exec backend -- node --version
```

Edit either `server.js`; the bind mount exposes the change without replacing
dependencies. A process restart may still be required because this deliberately
small server does not include a file watcher.

MongoDB uses its generated password only while initializing an empty volume.
Coordinate password rotation with the database, and remove project stacks
through a reviewed deployment. Installation service removal is a separate
reviewed operation; `--purge` destroys retained Mailpit/gate data.
