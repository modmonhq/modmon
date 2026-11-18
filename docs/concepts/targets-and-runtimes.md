---
title: Choose a target and runtime
description: Choose where your project runs and which container engine mm uses.
---

A **target** is a machine where you run services. A **runtime** is the container
tool mm uses on that machine: Docker Compose or Podman Compose.

## Use a local runtime

```sh
mm target list
mm target preference set-runtime docker-compose
mm target preference show
mm doctor
```

Use `podman-compose` if that is your chosen engine. Preferences are private
workstation settings; they do not change the repository.

To override the choice for one command:

```sh
mm --environment local --runtime podman-compose --target local deploy plan
```

If the selected runtime is missing or unsupported, mm reports an error rather
than switching to another engine.

## Set environment-specific preferences

```sh
mm target preference set-runtime docker-compose --environment local
mm target preference set-target production --environment production
mm target preference show
```

The target must already exist. To register a remote host, follow
[remote daemon setup](../remote-daemon.md#connect-a-client); it requires verified
SSH host and daemon identities, not just a hostname.

## Which setting wins?

Environment selection uses `--environment`/`MM_ENVIRONMENT`, then
`default-environment`. Target and runtime selection each use this order:

1. Command-line flag.
2. Private preference for the selected environment.
3. Private preference for the project.
4. Environment configuration.
5. Project configuration.

If no runtime is selected there, mm uses the target's default runtime.
The target must offer that runtime.

## Runtime differences

Docker Compose and Podman Compose support local source mounts, jobs, container
health, logs, and exec. They differ in how secret files reach a service; see
[secret delivery](../secret-delivery.md).

A target that lists several hosts under `nodes:` is a cluster: the hosts share
a private network, service names, and rolling deploys, and both runtimes stay
plain Compose on each node. [Run a cluster](../how-to/cluster.md) goes from the
node list to a deployment, and
[deploy to a cluster](../remote-daemon.md#deploy-to-a-cluster) shows one plan
applied across the nodes.

See the [support table](../reference/capability-matrix.md) for details.
Remote operations go through `mmd` over SSH; mm does not connect directly to
remote Docker or Podman sockets.
