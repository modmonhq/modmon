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

A deployment plan can take back routes. When the routes the environment holds
on its target are not the ones its last finished deployment recorded, such as
after an apply that was cut off after it saved its routes, the plan says
`takes back N routes this environment holds that the last finished deployment
did not record` and replaces them with its own. `mm deploy plan --output json`
and `mm_deploy_plan` list them under `adopted_route_claims`, and the plan hash
covers them. A plan never takes back a route of another project or
environment: an overlapping one stops the plan.

## If apply fails

Deployment attempts to restore the last successful deployment when one is
available. Rollback can also fail, so read the error and check service status
before trying again.

An environment's first deployment has nothing to restore. When one of its
stacks fails, mm leaves what the deployment started as it is, the failed
stack too, so you can read its logs. The error lists those stacks and the
commands to run:

```sh
mm service status --environment production
mm service logs api --environment production
mm deploy plan --environment production
```

The next deployment applies over what is running: a stack that was already
healthy and did not change is not started again. If the first deployment fails
before any stack starts, or after every stack is healthy (while it publishes
routes), mm removes what it created instead. See [recovery steps](../help/troubleshooting.md#interrupted-deployment-or-service-operation).
