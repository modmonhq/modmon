---
title: mm documentation
description: Turn a list of servers into one cluster that runs your Compose project.
---

`mm` is a Docker Swarm replacement. It turns a list of servers into one
cluster that runs your Docker Compose project, with a private network between
the nodes, service names that resolve on every node, rolling deploys that roll
back, HTTPS routes into every node, autoscaling, shared volumes, and one merged
log. The same configuration runs the
project on your own machine, with container builds, environments, secrets,
and local HTTPS.

Keep your Compose files for container settings; use `.config/mm.yaml` for the
configuration shared across environments.

The complete local
setup needs Linux, systemd, HAProxy, and Docker Compose or Podman Compose.
[Check platform support](./reference/capability-matrix.md) before starting.

## Start here

- **Try mm:** [install it](./start/install.md), then
  [run the Node example](./tutorials/first-local-app.md).
- **Use an existing project:** follow [project setup](./onboarding.md#existing-clone).
- **Add mm to a project:** follow [new project setup](./onboarding.md#new-project).
- **Deploy to several servers:** [run a cluster](./how-to/cluster.md), or start
  from [Coming from Docker Swarm](./how-to/from-docker-swarm.md).
- **Fix a problem:** start with [troubleshooting](./help/troubleshooting.md).

## Run a cluster

- [Run a cluster](./how-to/cluster.md): declare the nodes, set them up, deploy,
  and roll back.
- [Share a volume between nodes](./how-to/cluster.md#share-a-volume-between-nodes),
  and when to use a local volume instead.
- [Read logs and status across nodes](./how-to/cluster.md#read-logs-and-status)
- [Scale a service](./how-to/cluster.md#scale-a-service) with `replicas` or
  with the load.
- [Route traffic into the cluster](./how-to/cluster.md#route-traffic-into-the-cluster)
  with `ingress:` routes.
- [Restart a service](./how-to/cluster.md#restart-a-service) one node at a time,
  and [run a job](./how-to/cluster.md#run-a-job) with a deployment.
- [Harden the containers](./how-to/cluster.md#harden-the-containers) with
  `hardening: strict`.
- [Ship configuration files](./how-to/cluster.md#ship-configuration-files)
  with a service.
- [Read secrets from OpenBao or Vault on the nodes](./how-to/cluster.md#read-secrets-on-the-nodes)
- [Deploy without git on the nodes](./how-to/cluster.md#deploy-without-git-on-the-nodes)
- [Before production](./how-to/cluster.md#before-production): the production
  checklist, the release pin, and upgrades.
- [Upgrade mm and your hosts](./how-to/upgrade.md): which installations setup
  upgrades in place, and what it refuses.
- [Coming from Docker Swarm](./how-to/from-docker-swarm.md)

## Everyday tasks

- [Build and deploy images](./concepts/builds-and-deployments.md)
- [Set environment variables and secrets](./concepts/environments-and-secrets.md)
- [Use local HTTPS](./local-https.md)
- [Mount source code for development](./development-mounts.md)
- [Read logs and run commands](./runtime-observability.md)
- [Run jobs](./jobs-and-schedules.md)

Use the CLI for commands, or run `mm` in a terminal for a guided interface.
The [web dashboard](./local-dashboard.md) shows status; changes are made through
the CLI or terminal interface.

For individual settings and commands, use the
[configuration reference](./reference/configuration.md) and
[CLI reference](./reference/cli.md).
