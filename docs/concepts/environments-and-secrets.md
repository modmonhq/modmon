---
title: Environment values and secrets
description: Configure values for local and production environments without committing passwords.
---

An environment is a named group of settings, such as `local` or `production`.
Choose one with `--environment` or `MM_ENVIRONMENT`; otherwise mm uses the
project's `default-environment`.

## Connect a variable to a value

A module's `.env.example` can map application variables to logical names:

```dotenv
DB_HOST=localhost # mm:from=DB.HOST required
DB_PASSWORD= # mm:from=DB.PASSWORD required
APP_URL= # mm:from=APP.URL required
```

Add bindings in `.config/mm.yaml`. This fragment uses a public hostname, a
secret reference, and a URL built from an environment's domain:

```yaml
default-environment: local
bindings:
  DB.HOST:
    value: postgres
  DB.PASSWORD:
    secret: database.password
environments:
  local:
    domains:
      web: api.example.test
    bindings:
      APP.URL:
        cast: url
        value-ref: domains.web
```

Environment bindings override project-wide bindings with the same name.
`cast: url` adds the route's HTTP/HTTPS scheme, defaulting to HTTPS.

## Tell a container which release it runs

A template can read what mm knows about the deployment itself: the commit,
the release name, and the images of the build it deploys.

```dotenv
APP_RELEASE=dev # mm:from=MM.RELEASE
APP_COMMIT=dev # mm:from=MM.COMMIT
```

`mm deploy plan --release v26.6.0` names the release; without it,
`MM.RELEASE` is `sha-` and the commit's first 12 characters. Values that CI
supplies per deployment are declared under `release-inputs:` and given with
`--release-input NAME=VALUE`; a plan needs those that the environment's
templates read. The
[release facts](../reference/configuration.md#release-facts) reference lists
every key. `mm deploy rollback` returns to the name and inputs of the
deployment it goes back to, and `mm deploy redeploy` keeps the ones the
hosts run. Nothing is deployed by `mm dev`, so there these variables take
their template defaults.

## Add a secret

Declare where the value is stored, without putting the password in YAML:

```yaml
secrets:
  database.password:
    storage:
      provider: age
      location: local/database-password
    generate:
      preset: password
      length: 40
```

Then initialize Age, the encrypted local store, and generate the missing value:

```sh
mm secret init age
mm secret generate --missing
mm secret status database.password
mm env check
```

`generate --missing` creates the generated secrets the selected environment
uses and leaves existing values alone. Those are the secrets its bindings name,
its services mount with `secret-files`, and its registries, TLS certificate,
tunnel and deploy key read. Run it once per environment, for example
`mm -e staging secret generate --missing`. `--all` creates every secret with a
`generate:` block, whichever environment uses it.

When this machine has no connection to a secret's provider, such as a team
OpenBao you have not added with `mm secret provider add`, or the provider is
read-only, such as `env` or `file`, the secret is listed as skipped with the
reason and the others are still created. The command
succeeds. Naming one secret, as in `mm secret generate database.password`,
fails instead.

`mm secret status` shows a secret that is not stored as `unset`, or as
`default` when every variable that reads it in the selected environment has a
non-empty default in its `.env.example`, as in
`REDIS_PASSWORD=devpass # mm:from=CACHE.PASSWORD`. A `default` secret is still
missing from the store: `mm env status` shows those variables as `defaulted`,
`generate --missing` still creates it, and deploys use the template default
until you store a value. An empty default (`KEY=`), or a secret also mounted
with `secret-files` or read by a registry, TLS certificate or tunnel, stays
`unset`. In JSON output, `status` stays `unset` and `defaulted` is `true`. For
an environment on remote hosts, `mm secret status` shows the same as
`live: default`.

To enter an existing password instead, use `mm secret set database.password`.
The prompt hides your input. Other stores are described in
[secret providers](../secret-providers.md).

## See when a secret was set and who set it

mm never shows a secret's value, but `mm secret status` shows when its current
revision was written and who wrote it:

```text
minimal [local]
  KEY                PROVIDER  STATUS  SET                      BY    LOCATION
  database.password  age       set     3 days ago (2026-11-21)  alex  local/database-password
  stripe.key         env       set     —                        —     STRIPE_KEY
```

- `SET` is the time the provider reports for the current revision. `env` and
  `file` secrets have none and show `—`.
- `BY` is who mm recorded as the writer when `mm secret set`, `generate` or
  `rotate` wrote the revision: your team user (the `users:` entry that lists
  this device's mm key) when there is exactly one, else your login name. A
  write by an agent through `mm mcp` reads `agent:NAME (USER)`, for example
  `agent:claude (alex)`. A revision written outside mm or by mm 0.1.2 or
  earlier shows `—`; mm never guesses. mm 0.1.2 reads an Age store this
  release wrote, but when it writes to the store it drops every recorded
  writer, so all its secrets show `—` until they are set again. Long names are cut in the table.
- A host's copy of an environment's secrets, which `mm target setup` imports,
  keeps the original writer and time. A value staged on hosts names the user
  the host authenticated.
- `--output json` adds `set_at_unix_ms` and `set_by` to a secret only when they
  are known.

`BY` is a hint for finding who to ask, not a tamper-proof audit trail: anyone
who can write the store can write any name. A rotation that is only staged
keeps showing the active revision's time and writer until it is promoted.

## Set a workstation-only value

```sh
mm env set DB.HOST localhost
mm env status
mm env check
```

This stores a private override, not a project-file edit. Only keys used by a
template can be overridden. Secret-backed keys must use `mm secret set` instead.

When several sources provide a variable, the first available value wins:

1. Service `env` in `.config/mm.yaml`.
2. A secret binding.
3. A private workstation override.
4. An environment or project public binding.
5. The template default.

An unresolved required value is an error. An optional unset secret may use the
template default; a provider error does not silently fall back to a public value.

## How values reach containers

```sh
mm env delivery show
mm env review
```

These show the effective settings without printing secret values. `direct`
delivery passes values through the runtime process environment; `env-file`
uses temporary private files. Container administrators may still inspect
environment values. If your app supports reading secrets from files, see
[secret delivery](../secret-delivery.md).
