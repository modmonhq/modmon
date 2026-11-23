---
title: Pass secrets to your app
description: Choose environment variables or mounted files for application secrets.
---

Choose where a secret is stored with its provider declaration. Separately,
choose how your app reads it: as an environment variable or a mounted file.
You can change delivery without moving to a different secret store.

## When a secret store does not answer

A deployment plan reads the current version of every secret it uses. When a
store that holds one of them cannot answer (unreachable, sealed, not
initialized, or it refuses the login), `mm deploy plan` and `mm deploy apply`
stop with `MM_SECRET_STORE_UNAVAILABLE` and plan nothing. This holds for a
remote host, a cluster node, and a deployment to this machine. The message
names the provider and what failed, never a value or a credential. Check the
store with `mm secret status -e ENVIRONMENT`; on a remote host, check that the
store answers from the host and run `mmd secrets connect` there again if its
login was refused. Then plan again.

`mm dev` is the exception, so that a stopped development store does not block
local work: it still plans the development environment on this machine, and
its environment status reports the store as `MM_SECRET_PROVIDER_UNAVAILABLE`.
A secret it needs from that store cannot be read until the store runs again.

## Environment delivery

Secret-backed environment bindings use the environment's selected delivery
mode along with non-secret values:

```yaml
environment-delivery:
  default: direct
  development-default: env-file

environments:
  production:
    environment-delivery: direct
```

- `direct` passes values through the runtime process environment.
- `env-file` writes private temporary environment files for the operation.

When a development environment is declared, it defaults to `env-file` and other
environments default to `direct`. Inspect the actual setting:

```sh
mm env delivery show
mm env review
mm env delivery set production direct
```

Environment variables are visible to the process and may be exposed by
application diagnostics or privileged runtime inspection. Use mounted files
when the application supports them and narrower exposure matters.

## File delivery

Declare file consumers under the service. The map key is the file name under
the runtime's secret directory, not a host path:

```yaml
schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: secret-files
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  api:
    path: api
    services:
      api:
        stack: app
        secret-files:
          database-password:
            secret: database.password
            uid: "0"
            gid: "0"
            mode: 292 # decimal form of 0444
secrets:
  database.password:
    storage:
      provider: age
      location: local/database-password
environments:
  local:
    bindings: {}
```

`environments: [staging]` on an entry mounts the file only in those
environments; elsewhere the secret needs no value.

The deployment plan shows secret names, versions, permissions, and consuming
services, without printing values. If a secret changes after review, make a
new plan.

Runtime behavior differs:

| Runtime                          | File delivery                         | Important constraint                               |
| -------------------------------- | ------------------------------------- | -------------------------------------------------- |
| Podman Compose, your own machine | Versioned native Podman secret        | Native store; UID/GID/mode supported               |
| Podman on a remote host          | File per version, the consumer's copy | No native store; UID/GID/mode supported            |
| Docker Compose on a remote host  | File per version, a copy per owner    | No native store; UID/GID/mode supported            |
| Docker Compose, your own machine | File per version, a copy per mode     | No native store; mode supported, the file is yours |

Podman receives material on standard input. Rotation creates a new
owned revision, updates workloads, and retains the previous revision for
rollback. Inspection and cleanup use labels and metadata and do not read secret
content.

Docker Compose has no secret store, so mm keeps one file per version of a
secret under its own home on the machine that runs the containers
(`run/runtime-secrets-files`, `/var/lib/mm-home/run/runtime-secrets-files` on
a remote host), in directories that are `0700` and belong to the user mm runs
as: no other user of the machine can reach the files. A file's name is a keyed
digest of the store's version, made with a key that never leaves the machine;
it says nothing about the value.

Docker bind-mounts the file into the container and ignores the owner and mode
a Compose secret asks for, so mm gives the file the entry's `mode` itself, in
a copy for each mode and owner the services ask for. With the default `0444`
every user in the container can read the file, whatever user the image runs
as, without `user: "0:0"` or extra capabilities. On a remote host, where `mmd`
runs as root, the copy also belongs to the entry's `uid` and `gid`, which are
the container's ids on Docker Engine. A Docker daemon with `userns-remap`
(`docker info` lists `name=userns` under Security Options) gives containers
other ids than the host's: there the plan refuses an entry with an owner
other than root or a mode not every user can read (`MM_USER_NAMESPACE`). Use
`uid` 0, `gid` 0 and a mode such as the default `0444`, or turn off
`userns-remap` in `/etc/docker/daemon.json`. On your own machine mm runs as
you, and only root can give a file to other ids: the file is yours, with the
entry's `mode`. When an entry names other ids and that can matter, such as
`uid: "999"` or a mode that lets no other user read the file, the plan says so
for that service and file, and so does `mm config validate` when the
environment runs on Docker on this machine (`MM_SECRET_FILE_OWNER`).

On a remote host that runs [Podman](./remote-daemon.md#run-podman-on-the-hosts),
whose Compose provider is Docker Compose too, mm keeps the same files, and
each service that mounts a secret gets its own copy: given to the host ids
that the entry's `uid` and `gid` are in the service's user namespace, with the
entry's `mode`, in the same root-only directories. A rotation replaces only
the services that mount the secret there as well. A plain
`userns_mode: auto` gets the service mm's own ids too. A service whose
Compose file picks its own user namespace, such as `auto:size=1024`, gets
its user ids from Podman when its container is created, too late for mm to
give the copy to them: its entries have to use `uid` 0, `gid` 0 and a mode
every user can read, and the plan refuses others (`MM_USER_NAMESPACE`) until
you remove `userns_mode` from the service or set it to plain `auto`.

Rotating a secret gives the services that mount it another file, so only they
are replaced. The previous version stays until the deployment has succeeded,
and a deployment that fails mounts it again. Versions that no deployment uses
any more are removed after a successful deployment.

Check metadata without revealing values:

```sh
mm secret status
mm env review
mm deploy plan --output json
```

These views omit credentials and secret values. Host and container-engine
administrators can still access secrets used by workloads.

## Build secrets

For sensitive build inputs, use `modules.<name>.build.secrets` to map a BuildKit
secret ID to a declared mm secret. Do not use public build arguments for tokens.
mm passes build secrets through private files after plan review. See the
[build configuration reference](./reference/configuration.md#modules-services-and-builds).

A build secret can change the image, and its value can differ between
environments, so mm never reuses or promotes a build that mounts one. When a
secret only authenticates fetching inputs, such as a Composer or npm token for
private packages, list it under `credential-only-secrets`:

```yaml
modules:
  web:
    build:
      secrets: { composer_auth: composer.auth }
      credential-only-secrets: [composer_auth]
```

The secret is still mounted, but it is left out of the build fingerprint, so the
same inputs reuse the same image whoever builds them and however often the token
rotates. Only list a secret here when the image does not depend on its value.

To change a secret of a remote environment, see
[Change an environment's variable](./how-to/change-a-variable.md).
