---
title: Environment variables
description: Public mm controls, runtime inheritance, and provider inputs.
---

These variables configure mm itself. Application variables belong in project
bindings and templates; see [environment values](../concepts/environments-and-secrets.md).

## Stable mm controls

| Variable               | Behavior                                                                                                        |
| ---------------------- | --------------------------------------------------------------------------------------------------------------- |
| `MM_ENVIRONMENT`       | Selects the environment. `--environment` overrides it; otherwise the project uses `default-environment`.        |
| `MM_HOME`              | Overrides mm's local state, target catalog, identities, and private connection root. Equivalent to `--mm-home`. |
| `MM_AGE_IDENTITY_FILE` | Overrides the age identity path; the default is the age identity beneath `MM_HOME`.                             |
| `MM_LOG_FORMAT`        | `json` selects JSON tracing on stderr; other values use text tracing.                                           |
| `MM_KEY_VAULT`         | Where the passphrase of a new or newly protected key goes: `keychain` (fail without one) or `file` (never ask a keychain). Unset, mm uses the system keychain when it answers and a file beside the key otherwise. See [remote access](../remote-access.md#keys-are-encrypted-and-only-mm-uses-them). |
| `MM_SSH_KEY`           | A private key for CI, used instead of this device's key. See [remote hosts](../remote-daemon.md).               |
| `MM_SSH_REUSE`         | `off` connects to remote hosts anew for every request and closes the [connections kept before](../remote-access.md#connections-stay-open-for-five-minutes). |
| `MM_ENGINE_API`        | `off` makes mm read status and logs through the `docker` command instead of the Docker Engine API on the engine's local socket. Set it for `mm` on your machine, or in the environment of `mmd` on a host. See [runtime and remote support](../runtime-observability.md#runtime-and-remote-support). |
| `MM_TIMING`            | `1` prints how long each phase of a command took, on stderr. For finding what is slow; the format may change.   |
| `MM_PLAIN`             | `1` prints plain lines instead of drawing progress in the terminal: one line per finished step with its time. mm does this on its own when output is not a terminal, and when `CI` or `NO_COLOR` is set or `TERM` is `dumb`. |
| `MM_NO_HISTORY`        | `1` leaves the command out of the project's [command history](../help/troubleshooting.md#see-what-a-command-printed). |
| `RUST_LOG`             | Standard tracing filter consumed by mm, for example `mm=debug`. The default filter is `warn`.                   |

Command-line `--root`, `--runtime`, `--target`, and `--output` do not have
environment-variable aliases.

## Runtime variables inherited by child engines

Runtime subprocesses start from an explicit allowlist, not the complete mm
environment. All supported runtimes may receive:

`PATH`, `HOME`, `USER`, `LOGNAME`, `LANG`, `LC_ALL`, `LC_CTYPE`, `TMPDIR`,
`TMP`, `TEMP`, `XDG_RUNTIME_DIR`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`,
`XDG_CACHE_HOME`, `DBUS_SESSION_BUS_ADDRESS`, and `SSH_AUTH_SOCK`.

Podman Compose may additionally receive:

`CONTAINER_HOST`, `CONTAINER_CONNECTION`, `REGISTRY_AUTH_FILE`,
`CONTAINERS_CONF`, `CONTAINERS_STORAGE_CONF`,
`CONTAINERS_REGISTRIES_CONF`, `CONTAINERS_POLICY`, and
`PODMAN_COMPOSE_PROVIDER`.

Docker Compose may additionally receive:

`DOCKER_HOST`, `DOCKER_CONTEXT`, `DOCKER_TLS_VERIFY`, `DOCKER_CERT_PATH`, and
`DOCKER_CONFIG`.

Unset variables are omitted. Values compiled from project bindings are added
separately for the operation and are redacted from command records.

## Provider variables

The `env` secret-storage provider reads the variable named by each declaration:

```yaml
secrets:
  ci.token:
    storage:
      provider: env
      location: CI_TOKEN
```

Here `CI_TOKEN` is a provider input, not an mm control and not automatically
forwarded to container engines. Locations must be valid environment names and
cannot collide with mm's runtime-reserved interpolation names.

OpenBao and Vault credentials are stored in named private connections. Their
conventional client variables such as `VAULT_ADDR` and `VAULT_TOKEN` are not a
public configuration mechanism for mm.

## Workload variables

Project bindings and module environment templates define workload variables.
The generated `MM_ENV_*` placeholders used during runtime interpolation and the
default development-mail variables (`MM_MAIL_SMTP_HOST`,
`MM_MAIL_SMTP_PORT`, `MM_MAIL_SMTP_URL`, `MM_MAIL_UI_URL`) belong to generated
workload delivery, not to mm process configuration.

## Internal and test variables

Variables used only by source tests, live-conformance harnesses, or internal
service wiring are not public API. This includes `MM_TEST_*`, `MM_LIVE_*`,
`MM_CAP_INTERNAL_ENDPOINT`, `MM_CAP_ADMIN_ENDPOINT`, and similarly scoped
fixture variables. They may change or disappear without compatibility notice.
