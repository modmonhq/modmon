---
title: How projects fit together
description: Projects, services, environments, and the few terms you need to configure mm.
---

An mm project is a repository with a `.config/mm.yaml` file. That file connects
your application code and Compose services to the environments where you run
them: your own machine, one remote host, or a cluster of hosts.

## The main pieces

For a project with an API and a database:

| Term        | Meaning                                  | Example                 |
| ----------- | ---------------------------------------- | ----------------------- |
| Project     | The whole application                    | `shop`                  |
| Module      | A directory of code or configuration     | `services/api`          |
| Service     | A container service declared in Compose  | `api` or `postgres`     |
| Stack       | A set of Compose files deployed together | `compose.yaml`          |
| Environment | Settings for one use of the project      | `local` or `production` |
| Target      | The host or cluster where services run   | Your workstation        |
| Node        | One host of a cluster target             | `app-1`                 |
| Runtime     | The container tool mm uses there         | Docker Compose          |

A module can contain several services. For example, a Laravel module might run
both a web service and a queue worker from the same image.

A target that lists several hosts under `nodes:` is a cluster. Its nodes share
a private network and service names, a deployment rolls through them one at a
time, and a service's `placement` says which nodes run it. Nothing else in the
project changes: see [run a cluster](../how-to/cluster.md).

## Two configuration files

- **Compose** describes how containers run: commands, health checks, ordinary
  volumes, and third-party images such as PostgreSQL.
- **`.config/mm.yaml`** connects those services to image builds, environments,
  secret references, managed networks, and HTTP routes.

See [which settings go where](./configuration-ownership.md) for an example.

## Environment values and secrets

A **binding** connects a name used by an environment template to a value. The
value can be public configuration, such as a database hostname, or a reference
to a **secret**, such as its password. Secret values live in a separate store.

An **ingress route** connects an HTTPS hostname and path to a service. Local
HTTP services can receive automatic addresses such as
`https://api.shop.test`.

## Build, review, run

1. Validate the project with `mm config validate`.
2. Build and publish application images, if the project has its own builds.
3. Run `mm deploy plan` to see the changes.
4. Apply the plan using the hash it prints.

Read [builds and deployments](./builds-and-deployments.md) for the commands.
Jobs, schedules, and access gates are optional; add them when your project
needs [background tasks](../jobs-and-schedules.md) or
[restricted routes](../access-gating.md).
