---
title: Getting started
description: Choose a setup path and run your first mm commands.
---

[Install mm](./start/install.md) on Linux or macOS with the one-line installer,
or build it from a checkout. Then choose the path that matches what you are
working on.

| Your goal                             | Follow this guide                                 |
| ------------------------------------- | ------------------------------------------------- |
| Try mm with a complete example        | [First local app](./tutorials/first-local-app.md) |
| Set up a project that already uses mm | [Existing clone](./onboarding.md#existing-clone)  |
| Add mm to your Compose project        | [New project](./onboarding.md#new-project)        |
| Deploy to several servers             | [Run a cluster](./how-to/cluster.md)              |

## What you need

- **To run containers:** Docker Compose or Podman Compose.
- **To build your own images:** Docker Buildx and a registry you can push to.
- **For local HTTPS, the dashboard, and Mailpit:** HAProxy with Linux systemd,
  or macOS with Docker Desktop.
- **For browser access to local sites:** DNS or hosts-file entries for the
  generated `.test` names. mm creates certificates but does not configure DNS.

Check [supported features](./reference/capability-matrix.md) for platform and
runtime differences.

## In an existing project

Run these from the directory containing `.config/mm.yaml`:

```sh
mm config validate
mm doctor
mm env check
```

These check configuration, workstation requirements, and missing environment
values. They do not deploy your app. Follow any reported setup steps before
creating a deployment plan.

## Review before applying

Most changes use two commands: one to show the plan and one to apply it.

```sh
mm deploy plan
mm deploy apply --plan PLAN_HASH --yes
```

Replace `PLAN_HASH` with the hash printed by the plan command. If inputs change,
create and review a fresh plan. See [plans and apply](./concepts/plans-and-apply.md)
for details.

For a guided interface, run `mm` in an interactive terminal: see the
[terminal UI](./terminal-ui.md). Use explicit subcommands in scripts.
