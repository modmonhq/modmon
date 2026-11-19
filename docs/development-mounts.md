---
title: Development mounts
description: Review and enable local source mounts for development containers.
---

Development mounts make your local source files visible inside containers, so
you can edit code without rebuilding the image. Your application still needs a
watcher or restart to load changed code.

## Enable a mount

Replace `app` with your module name. Review the proposed mount, then use its
printed hash to save the configuration:

```sh
mm dev mounts plan app
mm dev mounts configure app --plan PLAN_HASH
mm config stack compile
mm deploy plan
mm deploy apply --plan DEPLOY_PLAN_HASH --yes
```

Mounts are allowed only for `project.development-environment`, on the local
target, with Docker Compose or Podman Compose advertising bind-mount and volume
support. Remote targets do not receive a mount or a copied source fallback.

Check the source directory, container path, services, and preserved directories
in the plan. If discovery chooses the wrong settings, specify them on both
commands:

```sh
mm dev mounts plan app --mount-target /app --service web --preset laravel
mm dev mounts configure app --mount-target /app --service web \
  --preset laravel --plan PLAN_HASH
```

`--mount-target` selects the container directory; the global `--target` still
selects the runtime target, such as `local`.

## Configuration

The persisted source is relative to the module, never an absolute workstation
path. The generated private runtime artifact resolves it to an absolute host
path.

```yaml
schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: mounted-app
  development-environment: local
target: local

stacks:
  app:
    files: [compose.yaml]

modules:
  app:
    path: app
    services:
      web:
        stack: app
    development-mount:
      source: .
      target: /app
      mode: read-write
      preserve:
        - path: /app/vendor
        - path: /app/storage
        - path: /app/bootstrap/cache
      services: [web]
      preset: laravel
environments:
  local:
    bindings: {}
```

Use `mode: read-only` when containers do not need to modify source. Available
presets preserve common container-owned paths in managed named volumes:

| Preset    | Preserved paths                                       |
| --------- | ----------------------------------------------------- |
| `laravel` | `/app/vendor`, `/app/storage`, `/app/bootstrap/cache` |
| `node`    | `/app/node_modules`                                   |
| `dotnet`  | `/app/bin`, `/app/obj`, `/root/.nuget/packages`       |
| `rust`    | `/app/target`, Cargo registry and Git caches          |

Preserved directories are initialized from the image on first use. Existing
volumes are kept, so reinstall dependencies when their lockfiles change. If a
service already mounts application storage through Compose, omit that path from
`preserve` to keep using its existing volume.

For a monorepo with sibling packages, keep the module path at the common build
root and declare additional sources relative to that path:

```yaml
development-mount:
  source: web
  target: /app
  additional:
    - source: packages/commerce
      target: /commerce
  services: [web, worker]
  preserve:
    - path: /app/vendor
    - path: /app/node_modules
  excludes: [.ai]
```

Additional sources use the same services and read/write mode as the primary
source.

Preserved volumes can outlive a changed mount until the next reviewed
deployment removes no-longer-referenced, exactly owned volumes. They may hold
dependencies, build output, or application data; inspect the deployment plan
before cleanup.

## Hide files and directories from containers

`excludes` and `protected` name paths, relative to the primary source, that the
containers must not see, such as a local `.env` file or build output:

```yaml
development-mount:
  source: .
  target: /app
  services: [web]
  excludes: [dist]
  protected: [.env]
```

`mm deploy plan` looks at each path in the source and covers it:

- A directory gets an empty volume, never copied from the image.
- A file gets an empty read-only file that `mm` keeps beside the
  environment's config files, so the container reads an empty `.env`.
- A path that is not in the source is left alone, and nothing is created in
  your checkout. If you create it later, the containers see it until the next
  deployment covers it.

The plan lists each path under its stack, for example
`masked: /app/.env (module app) is a file; an empty read-only file covers it`.
Masked paths are skipped by the source scan because their host contents are
hidden in the container.

## Safety

`mm` resolves the source and symlinks and requires all content to remain inside
both project and module roots. Absolute, drive-qualified, UNC, and `..` sources
are rejected. The safety scan also rejects `.git`, non-example `.env` files,
`.ssh`, private-key files, `secrets` directories, mm private configuration, and
configured sensitive patterns.

For a known false positive, review one exact exception:

```sh
mm dev mounts plan app --safe-exception fixtures/test.key
```

An exception permits that path to enter the container; it does not redact or
protect its contents. Prefer narrowing the mount or removing sensitive data.

On SELinux hosts, generated bind mounts request shared relabeling. This can
change the host source tree's SELinux context, just like a Compose `:z` mount.
Review that effect before mounting security-sensitive or shared trees.

## Disable a mount

Review the current mount, remove its configuration, then deploy the change:

```sh
mm dev mounts plan app
mm dev mounts disable app --plan PLAN_HASH
mm deploy plan
mm deploy apply --plan DEPLOY_PLAN_HASH --yes
```

`mm` provides source visibility and stable container paths only. Hot reload,
watchers, polling, ownership inside the source tree, and framework cache
invalidation remain application concerns.
