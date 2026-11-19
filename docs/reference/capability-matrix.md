---
title: Supported features and platforms
description: Compare container runtimes, operating systems, and secret stores.
---

mm runs services with Docker Compose or Podman Compose. Both support the full
local-development workflow.

<!-- Runtime descriptors are checked against code; keep these lines exact.
descriptor:PodmanCompose=one_shot_jobs,stream_logs,exec,runtime_secrets,bind_mounts,direct_environment,render_compose,apply_compose,observe_services,container_health,manage_networks,manage_volumes,mutate_volume_files
descriptor:DockerCompose=one_shot_jobs,stream_logs,exec,runtime_secrets,bind_mounts,direct_environment,render_compose,apply_compose,observe_services,container_health,manage_networks,manage_volumes,mutate_volume_files
-->

## Container runtimes

| Capability                       | Podman Compose | Docker Compose |
| -------------------------------- | -------------- | -------------- |
| Deploy services                  | yes            | yes            |
| Service status                   | yes            | yes            |
| Container health                 | yes            | yes            |
| Managed networks and volumes     | yes            | yes            |
| Update files in managed volumes  | yes            | yes            |
| Direct environment interpolation | yes            | yes            |
| Local bind mounts                | yes            | yes            |
| Runtime secret delivery          | yes            | yes            |
| Follow logs                      | yes            | yes            |
| Execute a command                | yes            | yes            |
| HTTPS routes with ACME           | yes            | yes            |
| One-shot jobs                    | yes            | yes            |

Deployment attempts to restore the last successful deployment if apply fails.
See [build and deploy](../concepts/builds-and-deployments.md) for recovery limits.
SSH access is handled by the [remote daemon](../remote-daemon.md), not by a
direct connection to a remote container engine.

## Local platform support

| Platform      | Trust scope                  | Verification                                              |
| ------------- | ---------------------------- | --------------------------------------------------------- |
| Debian/Ubuntu | system, `sudo`               | CA bundle fingerprint and live CI handshake               |
| Fedora/RHEL   | system, `sudo`               | extracted CA bundle fingerprint; no dedicated live runner |
| macOS         | login keychain by default    | keychain fingerprint and live CI handshake                |

Unknown Linux trust layouts and other operating systems fail closed.

## Release build targets

Each release on [GitHub](https://github.com/modmonhq/modmon/releases) has an
archive for each target below, built by the release workflow; see
[installation](../start/install.md).

| Operating system | x86-64                     | ARM64                       | Archive   |
| ---------------- | -------------------------- | --------------------------- | --------- |
| Linux glibc      | `x86_64-unknown-linux-gnu` | `aarch64-unknown-linux-gnu` | `.tar.gz` |
| macOS            | `x86_64-apple-darwin`      | `aarch64-apple-darwin`      | `.tar.gz` |

The Linux archives use glibc and are built in pinned containers by
`scripts/release/build-linux.sh`; the macOS archives are built on GitHub's
macOS runners. Windows has no release: `mm` does not build for Windows. The
checkout's local `./install.sh` builds a musl Linux binary or a native macOS
binary on x86-64 and ARM64. The installer needs an archive for the exact
operating system and architecture.

## Secret storage

Authentication is private state or process input, never public project
configuration.

| Provider                  | Operations                                                               | Revisions              | Rotation | Authentication             |
| ------------------------- | ------------------------------------------------------------------------ | ---------------------- | -------- | -------------------------- |
| `age`                     | inspect, resolve, create, replace, batch replace, stage, promote, delete | compare-and-swap       | staged   | local X25519 identity      |
| `env`                     | inspect, resolve                                                         | observed process value | none     | named process variable     |
| `file`                    | inspect, resolve                                                         | observed file content  | none     | local file permissions     |
| OpenBao provider instance | inspect, resolve, create, replace, delete                                | compare-and-swap       | replace  | private AppRole credential |
| Vault provider instance   | inspect, resolve, create, replace, delete                                | compare-and-swap       | replace  | private AppRole credential |

Age, environment and file storage accept text up to 1 MiB. OpenBao and Vault accept
safe text up to 512 KiB. Locations are bounded to 1,024 bytes where applicable;
environment locations must be valid variable names, and file locations
absolute paths or paths under `~/`. `mm secret provider list
--output json` returns driver and configured-instance descriptors without
revealing provider credentials.

## Secret delivery

| Driver                 | Runtime        | Delivery              | mm plaintext lifetime                   |
| ---------------------- | -------------- | --------------------- | --------------------------------------- |
| `direct-environment`   | both           | service environment   | no mm file; engines may retain metadata |
| `operation-env-file`   | both           | service environment   | owner-only operation file               |
| `podman-native-secret` | Podman Compose | native mounted secret | Podman-managed store                    |
| `compose-file-secret`  | both           | Compose file source   | file per version until superseded       |

Environment drivers accept text up to 1 MiB and names up to 255 bytes. Runtime
secret drivers use a conservative 500 KiB limit and accept text or binary.
Podman supports numeric UID, GID, and mode. Compose file secrets support
only UID 0, GID 0, mode `0444` on Docker Engine; other requests fail before a
file is written. On a remote host that runs rootful Podman, where every
service has user ids of its own, `compose-file-secret` is the file driver and
gives each consumer's copy the UID, GID, and mode it asks for.

Compose has no native persistent secret store. mm creates a private `0600`
operation file and removes its path after Compose runs; the mounted inode remains
available to the created container, but recreation requires another operation.
Rotation is rejected if rollback material cannot be reconstructed.

One-shots reuse a configured Compose service contract, including its selected
environment and runtime secret mounts. BuildKit secret inputs are a separate,
supported build path: `build.secrets` values are resolved only after review and
passed through private operation files. Compatibility planning rejects
unsupported runtime, workload, material, size, permission, and rollback
combinations before mutation.

## Local services

The shared ingress, dashboard, Mailpit, and optional access-gate components are
installation-wide local services. They require a configured local target,
HAProxy, and either a Linux systemd host with a local Podman Compose or Docker
Compose engine, or macOS with Docker Desktop and Docker Compose. They are not
implemented with other macOS container engines, and are not remote-host or
multi-node service orchestration. Host publication is
loopback-only where offered. Components use pinned images and ownership checks,
and optional services are installed only through a reviewed plan.

Managed OpenBao and Vault support local Docker Compose and rootless Podman
Compose. They do not support managed lifecycle operations on remote targets.
Rootless Podman requires a working user namespace and `podman unshare`.

For automation, [capabilities JSON](/schema/capabilities.json) provides the
generated runtime and secret-provider definitions.

See [Known limitations](./known-issues.md) for user-visible unsupported
combinations.
