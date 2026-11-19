---
title: Set up a project
description: Add mm to a Compose project or prepare an existing checkout.
---

[Install mm](./start/install.md) first. For a complete example with workstation
setup, follow [First local app](./tutorials/first-local-app.md).

## New project

From the directory containing your Compose files, preview the generated setup:

```sh
mm init --name example
```

Replace `example` with your project name. mm finds services, build settings,
ports, environment templates, and possible development mounts. Review the
proposed file changes, then use the printed hash to create them:

```sh
mm init --name example --plan PLAN_HASH --yes
```

The review includes a project `.gitignore` update for
`/.config/mm.overrides.yaml`, preserving existing rules. You can create that
optional file later for [local configuration overrides](./reference/configuration.md#local-overrides).

Open `.config/mm.yaml` and check the image repositories, environment bindings,
routes, and mounts. Generated guesses may need editing. See
[where configuration belongs](./concepts/configuration-ownership.md).

```sh
mm config validate
mm env setup
mm secret status
```

Resolve missing values before building. If the project uses Age secrets:

```sh
mm secret init age
mm secret generate --missing
```

`generate --missing` creates values only for the secrets the selected
environment uses that have a configured generator.
For other values, use `mm secret set KEY` and enter the value at the hidden prompt.

## Existing clone

Repositories initialized before local overrides were supported should add and
commit `/.config/mm.overrides.yaml` to their project `.gitignore` once. Existing
projects use the setup flow below; `mm init` is for creating a project configuration.

From the project directory, check what is needed on this workstation:

```sh
mm dev setup project
mm env setup
mm secret status
```

Follow the project's instructions for runtime selection and secret providers.
Set a missing secret without putting its value in shell arguments:

```sh
mm secret set database.password
```

Replace `database.password` with the name reported by `mm secret status`.

## Build and run

`mm dev up` reviews one plan that builds the project's own images into the
local engine, when they are missing or their sources changed, and then starts
the environment:

```sh
mm dev up
mm dev up --plan DEPLOY_PLAN_HASH --yes
mm service status
```

Use the printed hash. A local build needs Docker with Buildx, or Podman, and no
registry; uncommitted changes are fine. See
[build and deploy](./concepts/builds-and-deployments.md).

The project-setup command can also apply a reviewed setup and deployment together:

```sh
mm dev setup project --plan WORKFLOW_HASH --deploy-plan DEPLOY_HASH --yes
```

Use the setup hash from `mm dev setup project` and the deployment hash from
`mm deploy plan`. This does not build missing images: run `mm build` first, or
use `mm dev up`. Regenerate both reviews if either becomes stale.

## Run part of a large project

A large project can keep some modules, services, or jobs off in local
development until a developer needs them. Mark them in `.config/mm.yaml`:

```yaml
modules:
  status:
    path: apps/status
    development: optional
```

A fresh `mm dev up` starts nothing of `status`; its plan says
`off locally (optional)` and names the command that turns it on. To work on it:

```sh
mm dev enable status    # kept for this machine
mm dev up               # starts status with its routes and jobs
mm dev up --with status # or: on for this run only
mm dev disable status   # stops it and removes its routes; its volumes stay
```

`mm dev enable` also turns on the optional units `status` needs through Compose
`depends_on`. The choice is written to the gitignored
`.config/mm.overrides.yaml`, never to the shared file, and mm refuses to create
that file where git does not ignore it. With `--output json`, `mm dev disable`
only changes the file; run `mm dev up` to stop what it turned off. In the
[terminal UI](./terminal-ui.md) press `o` on the Services view to turn the
selected unit on or off, and the [dashboard](./local-dashboard.md) shows the
optional units that are off greyed out. Deployed environments ignore
`development:`. See [optional in development](./reference/configuration.md#optional-in-development).

## Guided terminal

Run `mm` in an interactive terminal and open **Guided** for workstation setup,
new projects, existing clones, and project editing. Scripts should use explicit
subcommands instead.

## Automation

Place `--output json` before the subcommand:

```sh
mm --output json config validate
mm --output json deploy plan
mm --output json deploy apply --plan "$PLAN_HASH" --yes
```

Check the command's exit status as well as its JSON. Use `--yes` for required
confirmation and `--stdin` for secret input:

```sh
printf '%s' "$DATABASE_PASSWORD" | mm secret set database.password --stdin
```

Combined `dev setup --services` is not supported in JSON mode. Split it into
setup and shared-service commands:

```sh
mm --output json dev setup
mm --output json --runtime docker-compose dev services plan
mm --output json --runtime docker-compose dev services apply \
  --plan "$SERVICES_PLAN_HASH" --yes
```

Use the runtime that owns your local services throughout.
