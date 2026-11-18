---
title: Build and deploy
description: Publish your application images and deploy the matching build.
---

Builds create container images. Deployments run those images with the selected
environment's configuration. If your project only uses third-party images,
skip the build step.

## Before building

You need Docker Buildx, a builder that supports your configured platforms, and
push access to every configured image repository. Publication also requires a
clean Git worktree and complete history and tags.

```sh
git status --short
git rev-parse --is-shallow-repository
```

Commit your changes. For a shallow clone, fetch the missing history with
`git fetch --unshallow --tags`; otherwise use `git fetch --tags`.
Replace any `ghcr.io/example/...` image names with repositories you own.

## Build and publish

### Local build platforms

To build natively on Apple Silicon while retaining the project's shared platforms,
create `.config/mm.overrides.yaml`:

```yaml
build:
  platforms: [linux/arm64]
```

The list replaces the shared `build.platforms` list. Check the effective platforms
in `mm build --publish plan` before applying it. Removing the override restores
the shared settings; adding, editing, or removing it requires a fresh plan.

New projects receive `/.config/mm.overrides.yaml` in `.gitignore` during reviewed
`mm init`. For an existing project, add and commit that rule first. Verify with
`git check-ignore .config/mm.overrides.yaml` and `git status --short`.
Normal dirty-worktree checks still apply to other changes.

If you used `git update-index --skip-worktree .config/mm.yaml`, first copy your
local settings into the override, then run
`git update-index --no-skip-worktree .config/mm.yaml` and restore only those local
settings in the shared file. Preserve unrelated edits.

For repository-root build contexts, also exclude `.config/mm.overrides.yaml`
through `.dockerignore` (or an existing `.config` rule). Git ignore rules do not
control which files are copied into an image.

### Review and apply

Run from the project directory:

```sh
mm build --publish plan
mm build --publish apply --plan BUILD_PLAN_HASH --yes
```

Replace `BUILD_PLAN_HASH` with the hash printed by the plan command. With
`--publish`, Buildx pushes the images to your registry. Without it, the build
produces a local OCI image archive, which mm cannot use for deployment.

## Deploy the build

```sh
mm deploy plan
mm deploy apply --plan DEPLOY_PLAN_HASH --yes
mm service status
```

Use the hash from the deployment plan. mm selects the latest successful published
build for the project, checkout location, and environment. Each module must have
a result compatible with its image repository, build paths, arguments, secret
IDs, extra-input paths, dependency definitions, platforms, and selected variant.
A changed base-image definition also requires new images for its dependents.
mm pins the deployment to each image's digest, so a later tag change cannot
silently replace the image.

For a named environment, use `--environment production` on both the build and
deployment commands. If a plan changes, generate a fresh one.

### Deploy a build to a remote host

A remote host never builds, and it cannot see this machine's builds. Give it the
build as a receipt: a file with the reviewed build plan and the digests it
published.

```sh
mm --environment production build --publish --yes
mm --environment production build receipt build-receipt.json
mm --environment production deploy plan --receipt build-receipt.json
mm --environment production deploy apply --plan PLAN_HASH --yes
```

`mm build receipt` writes the latest published build of this checkout for the
environment. The receipt must come from a build of the commit you deploy (HEAD,
or `--commit`). The host checks it against that commit's configuration with
the rules above and plans with its digests. When the deployment is applied, the
host records the receipt as that commit's build, so `mm service status` and
`mm service logs` find the same images, and a later plan of the same commit
needs no `--receipt`.

The same works without a remote host: `mm deploy plan --receipt FILE` deploys
a receipt of HEAD to a local target, and `mm deploy apply --plan HASH --receipt
FILE --yes` applies that plan.

### Promote a build to another environment

A receipt built for one environment can deploy to another, so CI can build
once and promote the same digests to integration, UAT, and production:

```sh
mm --environment ci build --publish --yes
mm --environment ci build receipt build-receipt.json
mm --environment production deploy plan --receipt build-receipt.json
mm --environment production deploy apply --plan PLAN_HASH --yes
```

mm plans the target environment's build of the receipt's commit and accepts
the receipt only when it builds exactly the same units: the same modules,
platforms, image repositories, selected variants, build paths, arguments,
extra inputs, dependencies, and file contents. Otherwise it refuses and names
the first module that differs and what differs, such as `build argument
FEATURE`, with the command that builds for the target environment. A module
with build secrets is promoted only when every secret is listed under
[`credential-only-secrets`](../secret-delivery.md#build-secrets): other
secrets can hold a different value in each environment. The deployment plan's
`build_receipt` field records the receipt hash, commit, and the environment it
was built for.

The build context is hashed from the files in the checkout, so files that are
not committed but are not excluded by `.dockerignore` also make the units
differ.

### Import a build

`mm build import` records a receipt in this machine's state without deploying
it. It is a reviewed change like the others:

```sh
mm --environment production build import build-receipt.json
mm --environment production build import build-receipt.json --plan IMPORT_PLAN_HASH --yes
```

The review prints `plan_hash` (in JSON, `.plan_hash`). A normal import follows
the deployment rules above: the receipt must be a build of HEAD, for this
environment or one that builds the same units. It then becomes this checkout's
published build for the environment, so `mm deploy plan`, `mm service status`,
and `mm build receipt` use its digests.

With `--cache-only`, the receipt may be a build of any commit, and mm only
records its images for reuse: a later `mm build --publish` retags an unchanged
module's image instead of rebuilding it. A cache-only import never selects
deployment images. Use it on a fresh CI runner, where `mm build --publish` has
no local record of earlier builds and does not search the registry for them.
Images of modules with build secrets that are not credential-only, or whose
image repository has changed, are not recorded for reuse.

A receipt is not signed. Its hash detects a damaged file, but whoever runs
`deploy plan` chooses which published digests run, within the image
repositories the commit configures. That needs `deploy` access to the
environment, and the host's audit log records the receipt hash and every
digest. See [deploy from CI](../remote-daemon.md#deploy-from-ci).

### Change runtime environment variables

Runtime settings, including environment bindings, service `env` values, domains,
and routes, do not require rebuilding published images. After changing them,
review and apply a fresh deployment plan:

```sh
mm deploy plan
mm deploy apply --plan DEPLOY_PLAN_HASH --yes
```

The deployment uses the existing image digests with the updated runtime
configuration. YAML comments and formatting changes also preserve image
eligibility. New source commits do not automatically publish or select new
images; run the build-and-publish steps when you want to deploy updated code.

Values in `modules.<name>.build.args` are build-time inputs. Changing them requires
a new published build. All reviewed build and deployment plans still require
their exact hashes; configuration edits after review require a fresh plan.

## Build dependencies and variants

This configuration fragment builds a base image before the API and selects the
API Dockerfile's `prod` target for production:

```yaml
build:
  platforms: [linux/amd64, linux/arm64]
  tag-strategy: immutable
modules:
  base:
    path: images/base
    build:
      image: ghcr.io/example/base
  api:
    path: services/api
    build:
      image: ghcr.io/example/api
      depends-on:
        base: BASE_IMAGE
      targets:
        production: prod
environments:
  production:
    build-variant: production
```

An environment builds only the variant it selects. A variant can carry its own
build arguments, merged over the module's, for public values that differ per
environment, such as a site URL a front end compiles in. Both environments
below build the Dockerfile's `production` stage, with different arguments and
tags:

```yaml
modules:
  web:
    path: web
    build:
      image: registry.example.com/web
      args:
        SITE_URL: https://example.com
      targets:
        production: ""
        production-staging:
          target: production # the Dockerfile stage; defaults to the key
          suffix: staging # the tag suffix; defaults to the key
          args:
            SITE_URL: https://staging.example.com
environments:
  production:
    build-variant: production
  staging:
    build-variant: production-staging
```

`mm build plan` lists each variant's arguments, and so do `mm build receipt`
and a remote deployment plan with `--receipt`. A receipt holds the variants
its environment built, so a receipt built for `staging` is refused for
`production`; build for that environment instead. An environment that selects
no variant of a module with several builds all of them, and its receipt can
be promoted to any environment that selects one of them.

Use [BuildKit secrets](../secret-delivery.md) for sensitive build inputs, rather
than public build arguments.

## Recover from a failed deployment

mm attempts to restore the previous successful stacks and routes. An interrupted
deployment is checked again on the next apply. Use the same target and runtime
as the interrupted operation.

Start with `mm doctor` and `mm service status`. Fix the reported problem, then
review a new deployment plan. Rollback is not guaranteed; if it fails, the error
reports both failures. Do not delete private state or change ownership labels
to bypass recovery. See [troubleshooting](../help/troubleshooting.md).
