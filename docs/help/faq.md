---
title: FAQ
description: Short answers about mm selection, plans, secrets, dashboards, and runtimes.
---

## Why did my reviewed plan hash stop matching?

Something used by the plan changed: configuration, build inputs, target,
runtime, secrets, or deployed state. Run the plan command again, review its
output, and apply the new hash.

## Will mm try another runtime if mine is unavailable?

No. Use `mm target preference show` and `mm doctor` to check the selection.
Fix that runtime or choose another explicitly with `--runtime`. See
[target selection](../concepts/targets-and-runtimes.md) for the precedence rules.

## Why do `.test` sites not resolve?

`.test` is reserved, not a built-in wildcard DNS zone. `mm` does not edit DNS
or the hosts file. Configure a local wildcard resolver, add exact host entries,
or test with `curl --resolve HOST:443:127.0.0.1`.

## Why does local trust need privileges?

Debian/Ubuntu and Fedora/RHEL use system trust and normally require `sudo`.
macOS login-keychain trust is user-scoped by default; `--system` explicitly
requests privileged system trust. `--no-trust`
creates certificates but leaves clients warning until trust is repaired.

## Where are Age secrets?

Normally under `~/.mm`: the private identity is in `identities/`, and encrypted
project documents are in `secrets/`. `MM_HOME` or `--mm-home` changes that root.
The `location` in `.config/mm.yaml` identifies the secret in that store; it is
not the password or a plaintext file path.

## Why does deploy say a published build is missing?

First-party images must come from a successful matching build made with
`--publish`; a local OCI archive does not satisfy deployment. Publication also
requires a clean worktree and complete Git history/tags. Replace example
registry names, commit changes, fetch full history and tags, then review and
apply a new published build before replanning deployment.

## Which project schema version is supported?

Use `schema-version: 1` in `.config/mm.yaml`. Missing or other versions are
rejected.

## Can the web dashboard change my deployment?

No. It shows status, routes, services, and recent operations. Use the CLI or
terminal UI for changes. Open `https://mm.test` and follow the project links.

## Are validation overlays deployed?

No. Example `compose.validation.yaml` files only add placeholder first-party
images for `docker compose config`. Their `example.invalid` references are not
runnable; mm generates separate reviewed overlays with published image digests,
bindings, networks, ingress, and ownership metadata.

## Can I rotate a database password safely?

Not by rotating only the mm secret when a persistent database volume already
exists. PostgreSQL and MongoDB initialization passwords are applied only to an
empty data directory. Coordinate the in-database credential update with secret
rotation, or recreate the volume only when data loss is intended.
