---
title: Harden services and choose their runtime
description: One key gives every deployed service a read-only root, no capabilities, and the engine's seccomp profile; the host says which OCI runtimes it offers.
---

Add one line to `.config/mm.yaml` and every service of a deployment runs with
a hardened set of container options:

```yaml
hardening: strict
```

Nothing else is needed. Run `mm deploy plan` and each stack lists what every
service will run with:

```text
  stack:  app (5b1c…)
    security: web: read-only root, no capabilities, no new privileges, default seccomp, tmpfs /tmp
    security: db: writable root, capabilities CHOWN, SETGID, SETUID only, no new privileges, default seccomp, tmpfs /tmp
    security: db: not read-only (the root filesystem stays writable): Postgres writes its socket under /var/run
```

The lines are part of what the plan hash covers, so a reviewer approves them
with the rest of the plan.

## What strict sets

| Default             | What mm adds to the service             | What it means in the container                                                              |
| ------------------- | --------------------------------------- | ------------------------------------------------------------------------------------------- |
| `read-only`         | `read_only: true`                       | The image's filesystem cannot be written. Volumes, bind mounts, and tmpfs stay writable.    |
| `cap-drop`          | `cap_drop: [ALL]`                       | No Linux capabilities, not even as root: no `chown`, no switching user, no raw sockets.     |
| `no-new-privileges` | `security_opt: [no-new-privileges:true]` | A setuid binary such as `sudo` or `ping` does not gain privileges.                          |
| `seccomp`           | nothing                                 | The engine's default seccomp profile stays. A Compose file that names another is refused.   |
| `tmpfs`             | `tmpfs: [/tmp]`                         | `/tmp` is a memory-backed scratch directory, writable, emptied when the container restarts. |

mm adds these in the Compose overlay it generates, after your Compose files,
so you do not edit those. What a Compose file already says is kept: its own
`cap_drop`, `tmpfs` entries, and `security_opt` lines stay next to mm's.

## Where it applies

`hardening` applies to every deployment: `mm deploy` to a remote host, to a
cluster, or to this machine. It does not apply to `mm dev`, that is, to the
project's `development-environment` on this machine. There your code is
mounted into the containers and development images install packages and write
caches wherever their tools like, so a read-only root would break the work
without protecting anything. To try the hardened set before it reaches a
host, deploy an environment other than the development one to this machine.

## When a service needs more

Most images run unchanged. The ones that do not usually fail at start, with
`Read-only file system` or `Operation not permitted` in `mm service logs`.
Fix the cause in this order:

1. **It writes to a path.** Give the path a volume, or add a `tmpfs:` line
   for it in the Compose file. Both stay writable under a read-only root.
   Many images document a setting for a read-only root.
2. **It needs a capability.** Images that start as root and switch to their
   own user (the official `postgres`, `nginx`, `redis`, and `mysql` images
   do) need a few. List exactly those in the Compose file; mm still drops
   the rest and the plan shows what is kept:

   ```yaml
   services:
     db:
       image: postgres:17
       cap_add: [CHOWN, DAC_OVERRIDE, FOWNER, SETGID, SETUID]
   ```

   Better still, run the image as its own user (`user: postgres`), which
   needs none.
3. **It cannot take one of the defaults.** Leave that default out for that
   service, and say why:

   ```yaml
   modules:
     shop:
       path: shop
       services:
         db:
           stack: app
           hardening:
             except:
               read-only: Postgres writes its socket under /var/run
   ```

   The keys under `except` are the five defaults: `read-only`, `cap-drop`,
   `no-new-privileges`, `seccomp`, and `tmpfs`. Each value is the reason,
   which the plan prints next to the service. An exception without a reason
   fails `mm config validate` with `MM_HARDENING_REASON`.

Things to know:

- A service has to be listed under a module in `mm.yaml` to take an
  exception. A service that only its Compose file names gets the full set.
- `tmpfs` at `/tmp` is mounted the way the engine mounts any tmpfs, without
  permission to execute files. A service that runs programs from `/tmp`, or
  wants a size limit, writes its own line (`tmpfs: /tmp:exec,size=64m`) and
  mm then adds none. mm also adds none when the service mounts a volume at
  `/tmp`.
- A health check runs inside the container with the same options. One that
  writes a file needs a writable path, and one that uses `ping` needs
  `cap_add: [NET_RAW]`.
- Service names, shared volumes, and secret files work unchanged: the
  resolver file, volumes, and files under `/run/secrets` are mounts, which a
  read-only root does not affect. A new shared volume still starts with the
  image's files and their owners, because the engine copies them, not the
  container.
- Without capabilities, root in a container cannot change the owner of files
  on a volume. An image whose start script runs `chown` on its data
  directory needs `cap_add: [CHOWN]`, on a shared volume as on a local one.
- `privileged: true` and `security_opt: no-new-privileges:false` in a Compose
  file contradict the set, so the plan is refused and says which exception
  to add.

## Choose an OCI runtime

A Compose service can name the runtime its container runs under, such as
gVisor's `runsc`:

```yaml
services:
  web:
    image: ghcr.io/acme/web
    runtime: runsc
```

On this machine mm passes `runtime:` to the engine as it is. A remote host
only accepts runtimes that its policy offers. The host's admin installs the
runtime, registers it with the engine, and lists it in `/etc/mm/host.yaml`:

```yaml
runtimes: [runsc]
```

A plan that names a runtime the host does not offer fails and says what the
host offers:

```text
error[MM_SECURITY]: security policy rejected the operation: service 'app/web'
sets `runtime: kata`, which this host does not offer. It offers runsc. Use
one of those or remove `runtime:` from the service; to offer kata, the
host's admin installs it and adds it to `runtimes:` in /etc/mm/host.yaml
```

A service without `runtime:` runs under the engine's default and needs
nothing from the host. On a cluster target every node has its own policy, so
offer the runtime on each node that runs the service.

### Put what faces the outside under a runtime

A host can run every service that is reachable from outside it under one of
its runtimes, without any change to the projects it runs:

```yaml
runtimes: [runsc]
exposed-runtime: runsc
```

A service is reachable from outside when its Compose file publishes a port
(`ports:`), uses the host's network, or when a route of `mm.yaml` sends
ingress traffic to it. mm sets the runtime for those services on that host
and the plan says so:

```text
    security: web: runtime runsc (this host runs what publishes a port or takes ingress traffic under it)
```

A service that names another runtime itself is refused there. Services that
only talk to other services keep the engine's default.

When a service's runtime changes against the last deployment, for example
because `exposed-runtime` was taken out of the host's policy, the plan says
so, since its containers are created again:

```text
  security: web: the engine's default runtime (was runtime runsc); its containers are created again
``` The ingress proxy mm
runs on the ingress nodes of a cluster target is not a service of the
project: it always runs with a read-only root, no capabilities, and no new
privileges, under the engine's default runtime.

### Check a host's runtimes

`mm doctor` asks every host of a remote environment to run a container under
each runtime it offers:

```sh
mm -e production doctor
```

A runtime the engine does not know (Docker Engine's list of runtimes, or
the runtime Podman resolves the name to), or one that fails to start a
container, is reported as `MM_RUNTIME_NOT_WORKING` with what the engine said
and what to do on the host. The check runs the `hello-world` image, which the host pulls
the first time. A host that offers no runtime is not asked to run anything,
and a host that could not check at all is the warning
`MM_RUNTIME_NOT_CHECKED`.

After editing `/etc/mm/host.yaml`, check it on the host:

```sh
mmd access validate
```

## Limits

- mm does not install a runtime. Install it on the host and register it with
  the engine: for gVisor on Docker Engine, `runsc install`, then
  `systemctl reload docker`; Podman finds a runtime by name at the paths
  `[engine.runtimes]` in `/etc/containers/containers.conf` lists.
- Containers of different services that share a volume must agree on user
  ids: a shared volume is exported without mapping root, and a runtime that
  maps user ids differently changes who owns new files.
