---
title: Troubleshooting
description: Diagnose failed plans, runtimes, HTTPS, builds, and interrupted operations.
---

Start with a read-only readiness report from the project directory:

```sh
mm doctor
```

Doctor collects independent problems together. A **blocked** check means a
prerequisite must be resolved before mm can check it; it does not mean the check
passed. Doctor does not initialize or migrate SQLite state, save runtime choices,
or publish generated stack artifacts. `--output json` includes `readiness.checks`,
runtime candidates, and scoped `recovery_commands` as argument arrays.

For an environment on a remote target (`mm doctor -e production`), doctor
fails only for findings about that environment and about what this machine
needs to reach it. What this machine lacks for local development, such as the
local certificate authority or the shared development services, is listed as
a note.

In an interactive terminal, run `mm` for guided setup. It offers local target
registration, available runtime choices, reviewed image builds, local-services
installation, and legacy-state recovery. It rechecks after each action and saves
runtime choices privately for the selected environment. You can inspect project
configuration or exit at any step; completed choices are kept.

Additional diagnostic commands:

```sh
mm config validate
mm doctor
mm target preference show
mm service status
mm --runtime podman-compose dev services status
mm dev tls status
mm doctor --check-updates
```

For automation, put `--output json` before the subcommand. Check the exit status
as well as the JSON: valid JSON can still describe a failure. See
[errors and exit codes](../reference/errors-and-exit-codes.md).

## See what a command printed

mm keeps a record of every command you run in a project on this machine: the
command line, the environment, when it ran and for how long, how it ended, and
what it printed, as plain text without colors or progress drawing. List them,
newest first, and show one by its ID:

```sh
mm history
mm history --outcome failed --since 2d
mm history show 20261115T201530123Z-4f2a
```

`-e production` lists the commands of that environment only. `--output json`
gives the same records with each command's progress events and JSON result.
The records live under `~/.mm/history/`, never in the repository. Values of
flags and `NAME=value` arguments whose name mentions a password, secret, token
or private key are replaced with `[REDACTED]`, and so are private keys in the
output. mm keeps the newest 500 commands, up to 32 MiB, per project, and 256 KiB
of each command's output. Set `MM_NO_HISTORY=1` to leave a command out. A
command run before mm has stored anything on this machine is not recorded:
mm does not create its home just to keep a record.

Agents read the same records through MCP: see
[let an AI agent work on your project](../mcp.md#read-what-commands-printed).

## Plan changed after review

The worktree, configuration, selected environment/runtime/target, secret
revision, registry input, or observed baseline changed. Re-run the exact plan
command with the same flags and apply only its new hash:

```sh
mm build --publish plan
mm build --publish apply --plan NEW_HASH --yes
mm deploy plan
mm deploy apply --plan NEW_HASH --yes
```

Do not reuse the old hash after changing files or selection.

## Runtime missing or unsupported

Run `mm target preference show`, then `mm doctor`. Fix the selected runtime or
choose another with `--runtime`; mm does not silently switch engines.

To remember Docker Compose for this project:

```sh
mm target preference set-runtime docker-compose
```

Use `--environment NAME` on the preference command to limit the choice to one
environment. Explicit command-line selections still take precedence.

## Local target is missing

Register this workstation with:

```sh
mm target local
```

This is idempotent and preserves existing targets. `mm target add` is for SSH
targets, not local workstation registration.

## State uses an unsupported pre-baseline SQLite schema

Stop other mm processes and review recovery:

```sh
mm doctor recover-state
mm doctor recover-state --plan PLAN_HASH --yes
```

Replace `PLAN_HASH` with the displayed hash. Use the same `--mm-home` for review
and apply if you use a custom state directory. The command saves a private
`state-backup-<id>.sqlite3` beside the database, including committed SQLite WAL
data, before resetting the legacy database in a transaction. Changed input
invalidates the review. Current, unknown, or newer schemas cannot be reset this
way.

Resetting loses the active database's private preferences, build records,
operation history, and ownership records. Project files, secret files, the
target catalog, and running containers are preserved. Existing resources may
need ownership reconciliation; resetting state is not a repair for a failed
deployment. Keep the backup until recovery is complete.

## Shared development-services network is missing

Projects using development mail bindings depend on mm's installation-wide
services network. Readiness and deployment apply check that the expected
target-owned network exists before changing runtime resources. Deployment plans
can still be reviewed offline:

```sh
mm dev services plan
mm dev services apply --plan PLAN_HASH
```

Review the plan and replace `PLAN_HASH` with its hash. For a new workstation
that also needs local TLS setup, use `mm --runtime docker-compose dev setup
--services`. To disable injected development mail, set
`development-mail.enabled: false` in `.config/mm.yaml`.

## HTTPS name does not resolve

Check route ownership with `mm config ingress list`. Configure local DNS/hosts for the
exact `.test` name, or isolate DNS from TLS/runtime problems:

```sh
curl --resolve web.my-project.test:443:127.0.0.1 https://web.my-project.test/health
```

If this works, the remaining problem is DNS. From the mm checkout, run
`./install.sh --setup-only` to configure or verify wildcard `.test` resolution.
No per-project `domains` entries or hosts-file entries are needed for automatic
`{service}.{project-name}.test` routing.

If workstation DNS is active but `.test` resolution still fails, inspect
`resolvectl status mm-test-dns`. It should list `169.254.77.53` and only `~test`.
A different server or `~.` indicates another DNS manager overwrote the interface.
Rerun the updated installer with `--setup-only`; its NetworkManager configuration
keeps mm's interface separate from connection dispatcher hooks. The DNS service
answers only `.test`; ordinary DNS continues through the system's existing
upstreams.

On macOS, `scutil --dns` should list a resolver for domain `test` with
nameserver `127.0.77.53`. Check name resolution the way browsers do, with
`dscacheutil -q host -a name web.my-project.test`; `dig` and `nslookup` bypass
`/etc/resolver` and are not a reliable test. If the resolver is listed but names
do not resolve, inspect `sudo launchctl print system/local.mm.dns`.

## Local ingress does not start on macOS

Run `mm ingress status`. The most common causes are:

- **Another program uses port 80 or 443.** `mm ingress start` reports that the
  forwarder did not stay running and leaves it stopped. Find the program with
  `lsof -nP -iTCP:80 -iTCP:443 -sTCP:LISTEN`, stop it, then retry.
- **Docker is not Docker Desktop.** mm reports the active endpoint. Run
  `docker context use desktop-linux`, or unset `DOCKER_HOST`, then retry.
- **The forwarder is disabled in launchd.** Run
  `launchctl enable gui/$(id -u)/local.mm.ingress-forwarder`, then
  `mm ingress start`.

Agent output is in `~/Library/Logs/mm/`: `ingress-forwarder.log` for HAProxy,
`ingress.log` for the Compose replay at login, and `dashboard.log`. After login,
the ingress containers start once Docker Desktop is running; the replay retries
every 15 seconds until then.

## Certificate is untrusted

Run `mm dev tls status`. Linux trust generally needs `sudo`; macOS system trust
needs explicit `--system`. Repair owned trust and key permissions with:

```sh
mm dev tls repair
```

A same-name foreign certificate is not replaced or deleted. Investigate that
collision instead of copying private CA keys or using `curl -k` as a fix.
Stale project leaves require a reviewed deployment reconciliation; stale shared
service leaves require an explicit-runtime `mm dev services plan --operation
repair` and matching `mm dev services repair`.

## Deployment has no matching published build

Confirm the configured image repository is real, the worktree is clean, and
the clone is not shallow:

```sh
git status --short
git rev-parse --is-shallow-repository
git fetch --unshallow --tags  # when the preceding command prints true
```

Then run a reviewed build with `--publish` and re-plan deployment. A build
without `--publish`, a failed build, or one for different source/configuration
does not qualify.

## Configuration schema is rejected

The project file must declare:

```text
schema-version: 1
```

Missing, zero, or other versions are rejected without rewriting the file. Use
the typed `storage` secret declaration shown in the configuration reference;
URI-shaped secret declarations are not part of schema 1.

## Secret is missing or in the wrong place

Inspect metadata without printing values:

```sh
mm secret status
mm env status --no-check-secrets
mm env check
```

For Age, initialize once with `mm secret init age`, then use `mm secret set KEY`
or `mm secret generate --missing`. Check the selected `MM_HOME`; normal private
state is under `~/.mm/identities` and `~/.mm/secrets`, not the repository.

If a stateful database stops authenticating immediately after password
rotation, restore/coordinately set the previous database role password. Its
persistent volume does not re-run image initialization when an mm secret
changes.

## Dashboard has no action buttons

The dashboard is read-only. Use CLI plan/apply commands to change a deployment.
If the dashboard cannot load status, run `mm doctor` and
`mm dev services status` rather than connecting directly to its authenticated
backend port.

## A deleted project is still listed

mm keeps every project folder it ran in on this machine, including scratch
checkouts that were deleted since. `mm doctor` reports them as
`MM_WORKSPACE_STALE`. List them, then review and apply their removal:

```sh
mm workspace list
mm workspace forget --stale
mm workspace forget --stale --plan PLAN_HASH --yes
```

No file is deleted. See
[forget a project that is gone](../local-dashboard.md#forget-a-project-that-is-gone).

## Interrupted deployment or service operation

Deployment apply records a journal and attempts rollback/recovery on the next
apply. Select the same runtime that owned the interrupted operation, run
`mm doctor` and `mm service status`, then make and apply a fresh deployment plan. Do not
delete SQLite state or generated ownership metadata to clear the error.

Recovery restores the previous workloads using the environment configuration
from the newly reviewed plan. Configuration edits after the interruption do not
require restoring an old configuration file. If `mm ingress start` has since
established installation-wide ingress, project recovery leaves it in place.

Shared service operations block further mutation when their journal requires
repair. Use the explicit reviewed repair flow:

```sh
mm --runtime podman-compose dev services plan --operation repair
mm --runtime podman-compose dev services repair --plan PLAN_HASH --yes
mm --runtime podman-compose dev services status
```

Use `docker-compose` throughout if Docker Compose owns the installation.

Repair of an interrupted first installation completes the reviewed installation
in place. It retains shared networks that project containers may already use.
Do not disconnect project endpoints or remove those networks to clear a repair
error. When an earlier successful service release exists, repair can restore
that recorded release instead.

For CA rotation/trust interruption, use `mm dev tls repair`. Recovery commands
remain ownership-checked and fail rather than adopting unknown resources.

## Release installation fails

For the checkout installer, start with
[installation troubleshooting](../start/install.md#if-installation-fails).
The release installer, `https://modmon.dev/install.sh`, downloads from the
releases on GitHub.

`the release has no archive for …` means the release has no archive for your
operating system and architecture: releases cover Linux (glibc) and macOS on
x86-64 and ARM64. Do not rename or substitute another target's archive. A
`download failed` for `releases/latest/download/SHA256SUMS` means no release
is published yet, or GitHub could not be reached; pass `--version` to pick a
published release.

Fix TLS/proxy failures in system trust or proxy configuration. Do not bypass
`https://github.com` or disable certificate validation. Confirm that an
intercepting proxy supports TLS 1.2 or newer.

Resolve permissions by choosing a user-writable install directory, not by piping
the installer through `sudo`.

A checksum mismatch is fail-closed: nothing is installed. Retry only after the
download or proxy issue is understood; never skip checksum verification.

`mm doctor --check-updates` reads `manifest.json` of the
[latest release](https://github.com/modmonhq/modmon/releases/latest) on GitHub
and downloads nothing else. It reports updates as unavailable when GitHub
cannot be reached over HTTPS or the manifest does not validate; restore
connectivity and run it again, or compare `mm --version` with the latest
release by hand. A release candidate is never offered as an update.
