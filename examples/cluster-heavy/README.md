# Cluster heavy

A deployment load for timing cluster deploys: ten stacks in three layers of
`after:`, six services each. Every service has a health check with
`interval: 30s`; the first three services of each stack run on every node,
the other three on the target's first node, and the sixth of each stack
publishes a host port. Stack `api` has a `before-deploy` job.

It needs a cluster target named `cluster` with three nodes (see [Run a
cluster](../../docs/how-to/cluster.md)) and a branch `main` the nodes deploy
from. It is a load, not an application: every container is `nginx`.

## Time a deploy

From this directory, with the target set up and the project committed and
pushed:

```sh
mm -e production deploy plan
mm -e production deploy apply --plan HASH --yes
MM_BIN=$(command -v mm) ../../scripts/live-timing.sh --environment production --deploy
```

`--deploy` changes the image tag of one service in `api.yaml`, `auth.yaml`
and `search.yaml`, commits, pushes, and times `mm deploy apply`; then it
deploys again with nothing changed. It fails when the first apply takes
longer than `MM_TIMING_DEPLOY_BUDGET_MS` (default 60000) or the second longer
than `MM_TIMING_UNCHANGED_BUDGET_MS` (default 15000). `MM_TIMING=1` on the
same command prints where the time goes.
