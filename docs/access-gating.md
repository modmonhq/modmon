---
title: Protect a local route
description: Require a password and a Cap challenge before visitors reach a local service.
---

An access gate protects an mm-managed HTTP route. The `password-cap` provider
requires both a valid password and a self-hosted Cap challenge. Gate users are
separate from developers who can deploy or administer the host.

Set up [shared local services](./local-services.md) first. This guide uses
Docker Compose; use `podman-compose` throughout if it owns your installation.

## Prepare the challenge service

```sh
mm --runtime docker-compose dev services plan --gate
mm --runtime docker-compose dev services apply --gate --plan SERVICE_PLAN_HASH --yes
```

Use the printed hash. `--gate` installs Cap and its Valkey data store. After
the first gate policy exists, ordinary service plans retain them automatically.
The public Cap route exposes challenge endpoints, not its administration UI.

## Create a gate and user

Age is the default store for gate secrets. Initialize it, then preview and
create a policy named `internal`:

```sh
mm secret init age
mm access add internal
mm access add internal --plan GATE_PLAN_HASH --yes
```

Sessions last seven days by default. Use `--session-days` on both commands for
a different duration between 1 and 30 days.

Add a user and enter their password at the hidden prompt:

```sh
mm access user add internal alice
mm access user add internal alice --plan USER_PLAN_HASH --yes
```

For automation, add `--stdin` and provide one password line on standard input.
Passwords are not accepted as command-line arguments.

## Attach a route

Get the target UUID from `mm target list` and the router name from the `routes`
section of `mm --output json deploy plan`. Replace `ROUTER` and `TARGET_UUID`:

```sh
mm access attach internal ROUTER --target-id TARGET_UUID
mm access attach internal ROUTER --target-id TARGET_UUID --plan ATTACH_PLAN_HASH --yes
```

Visit the route and verify that login is required before the application loads.
Later deployments preserve the attachment. Detach the gate before removing
the router or changing its host, path, or TLS settings.

## Manage access

```sh
mm access show internal
mm access user list internal
mm access session list internal
mm access user disable internal alice
mm access user disable internal alice --plan DISABLE_PLAN_HASH --yes
```

Disabling, removing, or resetting a user revokes their active sessions. Password
reset follows the same preview/apply pattern:

```sh
mm access user password-reset internal alice
mm access user password-reset internal alice --plan RESET_PLAN_HASH --yes
```

Use `mm access session revoke` to revoke a specific session and
`mm access session rotate-key` to rotate session encryption keys. Each command
requires its own reviewed plan to apply. See the [CLI reference](./reference/cli.md).

## Remove a gate

Detach all routes first, then remove the policy:

```sh
mm access detach internal ROUTER --target-id TARGET_UUID
mm access detach internal ROUTER --target-id TARGET_UUID --plan DETACH_PLAN_HASH --yes
mm access remove internal
mm access remove internal --plan REMOVE_PLAN_HASH --yes
```

This removes the policy's users, sessions, and stored gate secrets. Review and
apply a normal shared-service plan afterward. If no policies remain, that plan
removes the unneeded Cap containers while retaining data unless purged.

If Cap, its secret store, or the policy store is unavailable, authentication is
denied rather than allowing visitors through.
