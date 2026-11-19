---
title: Jobs and schedules
description: Run a command once or on a schedule, then check its history.
---

A job is a named command that finishes, such as a database migration. A schedule
runs a job at specified times. Jobs use an existing service's image, environment,
networks, mounts, and secrets.

Jobs run on Docker Compose and Podman Compose, on this machine and on remote
targets. Each run gets a fresh container of the service's image, never the
service's running container.

## Define a job

Add a job to `.config/mm.yaml`. Use a module and service that already exist:

```yaml
jobs:
  migrate:
    module: app
    service: web
    command: ["php", "artisan", "migrate", "--force"]
    timeout: 10m
    retry: { attempts: 3, backoff: 5s, max-backoff: 5m }
    history-limit: 100
```

Commands are lists of arguments, not shell strings. Check and apply the updated
definitions:

```sh
mm config validate
mm job schedule reconcile
mm job schedule reconcile --plan PLAN_HASH --yes
```

The first reconcile command previews changes; use its hash in the second.
A deployment also updates job and schedule definitions. Removing a definition
retains its run history.

## Run and inspect

```sh
mm job run migrate
mm job history migrate --limit 50
mm job status RUN_UUID
mm job cancel RUN_UUID
mm job retry RUN_UUID
```

Replace `RUN_UUID` with a run ID from the run command or history. Output is
limited in size and redacted against the environment's secrets, as
[service logs are](./runtime-observability.md#secret-values-are-replaced). A job
whose output cannot be redacted, because a secret the environment hands out
cannot be read here, does not start.

## Run a job on a remote target

`mm job run`, `mm job status`, `mm job history`, and `mm job retry` work the
same on a remote target. The host that runs the job's service starts the run;
on a cluster target, that is the first node the service is placed on. The
container gets the Compose files, environment, and secrets the environment's
last deployment gave the service, so deploy before running a job, and deploy
again after changing a value or secret the job reads.

Running a job needs the `operate` level (or higher) in the
[users file](./remote-access.md#levels); it does not need `exec`. Reading runs
needs `view`. Every run is in the host's audit log with who started it.

On a cluster target a job runs on one node: the first node the service is
placed on, or the node `placement` names (a node, or a group whose first node
that runs the service is taken). The node must run the job's service; the job
joins the service's networks there, so it reaches services on other nodes by
name. Its runs, and its schedule's runs, happen on that node only. `user`
runs the command as another user than the service's:

```yaml
jobs:
  migrate:
    module: app
    service: web
    command: ["php", "artisan", "migrate", "--force"]
    placement: app-2
    user: www-data
```

The run's container is detached from `mm`: when `mm` is interrupted while it
waits, the job goes on, and `mm job status RUN_UUID` shows where it is. A run
on a remote host cannot be cancelled with `mm job cancel`; it ends at the job's
`timeout`. The host moves a run nobody waits for on by itself, retrying it by
its `retry` policy.

On hosts that run [Podman](./remote-daemon.md#run-podman-on-the-hosts) a
job's container has its service's user ids, which are the same from one run
to the next: a migration writes the files the run before it wrote, and reads
the service's config files and secret files as the service does.

## Add a schedule

This example runs the job at 02:00 UTC each day. Choose a job that is safe to
repeat on that schedule:

```yaml
schedules:
  nightly:
    job: migrate
    cron: "0 2 * * *"
    timezone: UTC
    enabled: true
    concurrency: forbid
    misfire: run-once
```

Reconcile after editing, then inspect or control the schedule:

```sh
mm job schedule list
mm job schedule next nightly
mm job schedule run-now nightly
mm job schedule history nightly
mm job schedule disable nightly
mm job schedule enable nightly
```

For an environment on this machine, keep `mm job schedule daemon` running to
process due jobs; use a service manager for a persistent setup. On a remote
target, `mmd` does it by itself on each host: a deployment reconciles the
schedules there, and the host that runs a schedule's job starts its runs, so
on a cluster a schedule runs on its job's node only. In the terminal UI's Jobs page, `t` runs one controller
cycle; defining a cron expression does not install an operating-system cron entry.

## Timing and retries

- Cron uses five fields: minute, hour, day of month, month, day of week. When
  both day fields are restricted, either may match.
- Timezones use IANA names such as `America/New_York`. A skipped daylight-saving
  minute does not run; a repeated minute can run twice. Use UTC to avoid this.
- `concurrency: forbid` prevents overlapping runs. `allow` permits them;
  `replace` requests cancellation of existing runs before adding another.
- `misfire: skip` discards missed occurrences; `run-once` runs the latest.
  Bounded catch-up is available in the [configuration reference](./reference/configuration.md#jobs-and-schedules).
- Failed and timed-out runs can retry; cancelled runs do not. Retry delays
  increase up to `max-backoff`.

## Run a job with a deployment

`run` makes every deployment run the job by itself, so migrations and cache
clears no longer live in CI scripts or pasted commands:

```yaml
jobs:
  migrate:
    module: app
    service: web
    user: www-data
    command: ["php", "artisan", "migrate", "--force"]
    timeout: 10m
    run: before-deploy
  clear-cache:
    module: app
    service: web
    command: ["php", "artisan", "cache:clear"]
    run: after-deploy
  restore-content:
    module: app
    service: web
    command: ["php", "artisan", "app:restore", "--no-interaction"]
    timeout: 4h
    run: once
    environments: [integration, uat]
```

- `before-deploy` runs on every deployment, after the services its service
  depends on (`depends_on` in the Compose file, and theirs) are healthy, and
  before its service is started or replaced on any node. Those services start
  first; everything else in the stack waits for the job. If the job fails,
  the deployment stops there: the services that started first go back to
  the previous version, and everything else keeps running untouched.
- `after-deploy` runs on every deployment once it converged. If it fails,
  the deployment stays and `mm deploy` exits non-zero with the job's output.
- `once` starts the first time the environment is deployed with the job,
  once the deployment converged. It runs under `mmd`, detached from `mm`:
  the deployment says it started and how to follow it
  (`mm job status RUN_UUID`). Every node of the target keeps the record, so a
  rebuilt node does not make it due again, and later deployments do not run
  it. To run it again on purpose, run `mm job run restore-content`.

Each job runs in a fresh container of its service's image with the service's
mounts, networks, environment, and secrets, as `user` when it is set; never
in the running container. `mm deploy plan` lists the jobs a deployment will
run, in order, and says whether a `once` job is due:

```text
  job: migrate (before-deploy) on app-1: before web is started or replaced, once what it depends on is healthy; a failure stops the deployment there
  job: clear-cache (after-deploy) on app-1: once the deployment converged
  job: restore-content (once) on app-1: due: starts once the deployment converged and runs on under mmd
```

A rollback (`mm deploy rollback`) never undoes a job; its plan lists the jobs
that ran since the release it returns to. Running a declared job, by
`mm job run` or through MCP, needs the `operate` or `deploy` level, not
`exec`.

## Give jobs a service of their own

A job gets everything its service gets. When a job needs less or more than a
long-running service, such as an admin password the web service must not hold,
give it a service with `role: job`. A deployment never starts a container of
that service: it exists only as the container its jobs run in, with its own
`placement`, networks, environment, config files, and secret files.

```yaml
modules:
  platform:
    services:
      platform-api: { stack: app, role: http, placement: all }
      platform-ops:
        stack: app
        role: job
        placement: app-1
        secret-files:
          db_admin_password: { secret: db.admin-password }
jobs:
  bootstrap:
    module: platform
    service: platform-ops
    run: before-deploy
    command: ["/app/bin/bootstrap"]
```

The Compose file defines `platform-ops` like any other service. While no job
runs, no container holds its secrets. After a secret changes, the next
deployment hands it to the next run and replaces no running container.

`mm deploy plan` lists the service as `job only`, and so does
`mm service status`. It gets no service name on the cluster's networks, no
ingress, and no autoscaling. `mm config validate` warns when no job runs in
it, and refuses `ports`, an ingress route to it, `update.order: start-first`,
`replicas`, `autoscale`, and another service's `depends_on` onto it, each
with what to change.

## Output of a deployment's jobs

A job a deployment runs before it replaces a service writes its output to
the deployment's log. When the job fails, the deployment stops before the
service's stack and the error shows the job's last lines. To read every job
the last deployment ran, node by node:

```sh
mm deploy logs
```

```text
app-2 migrate: deployment 3f2c9a1e5b7d (succeeded, exit 0), run 0193...
app-2 migrate | Migrated: 2026_11_01_000000_create_orders_table
```

With `--output json` it prints the runs, each with its `node`, `state`,
`exit_code`, and `output`. Output is bounded by `output-limit-bytes` and
redacted against the environment's secrets.

It works before the environment's first deployment has finished: when the
first one fails, `mm deploy logs`, `mm job history`, and `mm status` read
the hosts' record of that attempt, so you can see why it failed.

## After an interruption

A job container left behind by a run that was cut off (its client or the
host's `mmd` stopped in the middle) is removed by the next deployment of the
environment, once no recorded run waits for it; nothing needs removing by
hand.

mm inspects an existing job container before relaunching it. If it cannot tell
what happened, the run becomes `unknown` and automatic retries wait for a clear
result. Inspect the run before manually retrying it.

A job can execute more than once after a failure. Make external side effects
safe to repeat, for example by using `MM_JOB_RUN_ID` as a deduplication key.
Jobs also receive `MM_JOB_ATTEMPT` and `MM_JOB_FENCE_TOKEN`.
