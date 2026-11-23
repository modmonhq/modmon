---
title: Prepare a deployment environment
description: Configure domains, images, and a remote target for a production-like environment.
---

Use this workflow to set up a production-like environment. Try it on a test
host first.

Keep the same project configuration and Compose files. Add the environment's
domains, target, and build variant:

```yaml
environment-delivery:
  default: direct
  development-default: env-file

environments:
  local:
    build-variant: development
  production:
    target: production
    environment-delivery: direct
    build-variant: production
    domains:
      api: api.example.com
```

Provision the target and restricted operator identity before deploying.
`mm target setup` installs the runtime, and with `--origin` and
`--break-glass` it also installs `mmd` and the restricted SSH account. See
[Remote daemon operations](../remote-daemon.md) and [Remote access and
RBAC](../remote-access.md); do not copy private target credentials into project
configuration.

```sh
mm --target production target setup plan
mm --target production target setup apply --plan TARGET_PLAN_HASH --yes
mm --environment production env review
mm --environment production build --publish plan
mm --environment production build --publish apply --plan BUILD_PLAN_HASH --yes
mm --environment production build receipt build-receipt.json
mm --environment production deploy plan --receipt build-receipt.json
mm --environment production deploy apply --plan DEPLOY_PLAN_HASH --yes
mm --environment production service status
```

Replace each hash with the one printed by its plan command. The remote host
never builds: it deploys the digests in the build receipt, which must be a
build of the commit you deploy. A receipt CI built for another environment
works too when production builds the same units; see [promote a build to
another
environment](../concepts/builds-and-deployments.md#promote-a-build-to-another-environment).
Before applying, check the target, images,
secrets, routes, and backups. If the configuration
or target changes after review, create a new plan.

Use `mm service logs` to diagnose the remote services. Remote exec needs an
explicit permission grant and has no stdin, TTY, or interactive shell support.
Do not assume remote jobs or schedules are available merely because local
Compose supports them. See [remote access](../remote-access.md).

## Pull from a private registry

When the images are private, declare the registry and a secret holding a pull
token. Set the secret in each environment that deploys; the same name holds a
different token per environment:

```yaml
secrets:
  ghcr.read-token:
    storage: {provider: age, location: local/ghcr-read-token}
    input: {prompt: "GHCR token (classic PAT, read:packages)"}

registries:
  ghcr.io:
    username: example-bot
    password: {secret: ghcr.read-token}
```

```sh
mm -e production secret set ghcr.read-token
```

Each deploy reads the token from the deploying host's store and gives it to
Docker, Compose, or Podman in a private configuration that is removed when the
command ends; nothing is written to `~/.docker`. On a cluster target,
`mm target setup apply` pushes the token to every node with the environment's
other secrets.

When the token is missing, `mm` says where to create one. For GHCR that is a
classic personal access token with only the `read:packages` scope, because the
GitHub container registry accepts no other kind:
`https://github.com/settings/tokens/new?scopes=read:packages`. Choose "No
expiration", or the longest your organization allows, and authorize the token
for single sign-on if the organization requires it. Prefer a machine account,
so the token does not depend on one person's access.

## Large images and slow registries

Before a host starts or replaces a stack's containers, or launches a job, it
pulls the images it does not have yet, with the registry logins above. This
step has its own limit, 20 minutes by default, so starting the containers
keeps its short one. While a remote host pulls, the line `mm deploy apply`
prints every 15 seconds names the image, and so does `mm deploy status`.

When a pull needs longer, raise the limit for the environment:

```yaml
environments:
  production:
    deploy:
      pull-timeout: 45m
```

A pull that runs past the limit stops the deployment with `MM_TIMEOUT` before
anything of the stack was stopped or started. The layers that arrived stay on
the host, so the next `mm deploy apply` carries on from them.

On a single host, `mm deploy apply` waits at most one hour for the whole
deployment, pulls included. If several stacks each pull for a long time, the
command can stop with `MM_TIMEOUT` after an hour even though no pull passed
its own limit. Run `mm deploy status` to see where the host got to, then run
`mm deploy apply` again: the images that arrived stay, so the next deployment
has less to pull.

The local services that `mm dev setup` and `mm dev services` start (ingress,
Mailpit and the others) do not take this step: Compose pulls their images
itself, under its own limit.
