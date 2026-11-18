---
title: First local app
description: Run the Node frontend, API, and MongoDB example with local HTTPS.
---

This walkthrough runs `examples/node-fullstack`: a frontend, API, and MongoDB.
You will build the application images, start the services, and check the API
over HTTPS.

## Before you start

You need:

- [mm installed](../start/install.md).
- A supported Linux systemd machine with HAProxy, Docker Compose, and Docker Buildx.
- A full Git checkout of this repository, including tags.
- Two container repositories you can push to, with Docker already logged in.

This example publishes images even for local deployment. It cannot deploy
first-party images from a local-only build. Use a development machine.

## Set up the workstation

From the repository root:

```sh
cd examples/node-fullstack
mm --runtime docker-compose dev setup --services
mm target preference set-runtime docker-compose
mm doctor
```

Setup creates a local target, installs a local certificate authority, and asks
you to approve the shared HTTPS proxy, dashboard, and Mailpit services. Linux
certificate trust and ports 80/443 can require `sudo`.

The saved runtime preference makes later commands use Docker Compose. If you
prefer Podman Compose, use `podman-compose` in setup and the preference command;
Docker Buildx is still needed for builds.

On an existing checkout, bare `mm` opens the terminal UI and lists what the
environment still needs on its Overview; `enter` runs a step. `mm doctor` ends
with the same list. At this stage, a missing image is expected until the build
step below. Neither command publishes images without a reviewed build.

## Configure the image repositories

Open `.config/mm.yaml` in this example directory. Replace the two
`modules.*.build.image` values with repositories you own:

- `ghcr.io/example/mm-node-fullstack-frontend`
- `ghcr.io/example/mm-node-fullstack-backend`

Commit those changes before building. Publication requires a clean worktree
and complete Git history and tags. Check both:

```sh
git status --short
git rev-parse --is-shallow-repository
```

The first command should print nothing; the second should print `false`. For a
shallow clone, run `git fetch --unshallow --tags`. Otherwise fetch missing tags
with `git fetch --tags`.

## Prepare the environment

```sh
mm config validate
mm secret init age
mm secret generate --missing
mm env check
```

The example declares a generated MongoDB password. Age is the encrypted local
secret store used here. `mm env check` should finish without missing values;
the password is not written into project configuration.

## Build and deploy

Run the plan command first. Review its output, then replace `BUILD_PLAN_HASH`
with the hash it prints:

```sh
mm build --publish plan
mm build --publish apply --plan BUILD_PLAN_HASH --yes
```

After the build succeeds, do the same for deployment:

```sh
mm deploy plan
mm deploy apply --plan DEPLOY_PLAN_HASH --yes
mm service status
```

The frontend, backend, and MongoDB should now be running. Check the reported
health before continuing. If a plan becomes stale, generate a new one rather
than reusing its hash.

## Check the API

This command tests HTTPS without changing your machine's DNS:

```sh
curl --resolve backend.node-fullstack.test:443:127.0.0.1 \
  https://backend.node-fullstack.test/health
```

Expect `{"status":"ok"}`. If it fails, use:

```sh
mm service logs backend --tail 50
mm doctor
```

After installer workstation setup, wildcard `.test` resolution is already
configured. Open `https://frontend.node-fullstack.test`. The frontend and backend
names are generated automatically from their service names and `project.name`;
you do not need to add `domains` entries or edit your hosts file. If you installed
with `--no-setup`, run `./install.sh --setup-only` from the mm checkout first.
See [local HTTPS](../local-https.md) for certificate and DNS troubleshooting.

The [Node example guide](./node-fullstack.md) adds a mail test, source editing,
and dashboard links.

## Clean up

Stop the project's development stacks:

```sh
mm dev down --yes
```

Shared services are separate. If no other projects need them, review removal:

```sh
mm --runtime docker-compose dev services plan --operation remove
mm --runtime docker-compose dev services remove --plan SERVICES_PLAN_HASH --yes
```

Replace `SERVICES_PLAN_HASH` with the removal plan's hash. Retained data is
preserved unless you add `--purge` to both service commands. Purging deletes
Mailpit and access-gate data. Use `mm dev tls uninstall` only when you also want
to remove the installation's local certificate trust.
