---
title: Plans and apply
description: Preview a change, approve it, and handle an out-of-date plan.
---

A plan shows what mm will change before it makes the change. It prints a hash:
the value you pass back to confirm which plan you reviewed.

## Review and apply a deployment

```sh
mm deploy plan
mm deploy apply --plan PLAN_HASH --yes
```

Replace `PLAN_HASH` with the hash from the first command. Review the selected
environment, target, images, and resources before applying.

Use the same flags for both commands. For example:

```sh
mm --environment production --target prod deploy plan
mm --environment production --target prod deploy apply --plan PLAN_HASH --yes
```

Builds and shared service changes follow the same pattern, but each operation
has its own plan. A build hash cannot approve a deployment.

## When a plan changes

mm checks the inputs again before applying. A plan can become stale if you:

- Edit project configuration, Compose files, or build inputs.
- Change the environment, target, runtime, or build options.
- Update a secret used by the operation.
- Apply another deployment or change resources on the target.

If you see `deployment plan changed`, run the original plan command again,
review the new result, and use its new hash. Do not edit stored plans.

## What a plan includes

Plans include resource changes and secret references, but not secret values.
An unsupported feature, missing required value, or conflicting resource stops
the operation. mm will not take over another tool's resource merely because
its name matches.

## If apply fails

Deployment attempts to restore the last successful deployment when one is
available. Rollback can also fail, so read the error and check service status
before trying again. See [recovery steps](../help/troubleshooting.md#interrupted-deployment-or-service-operation).
