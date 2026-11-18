---
title: Where configuration belongs
description: Decide what goes in Compose, mm.yaml, and private workstation state.
---

## Project settings

Keep the project file at `.config/mm.yaml` and commit it. mm searches the current
directory and its parents for that exact path. It requires `schema-version: 1`
and rejects unknown keys, so typos are reported instead of ignored.

```sh
mm config validate
```

To select a project from elsewhere, use `mm --root PATH config validate`.
Paths inside the configuration must stay inside the project; symlink escapes
are not allowed.

A large project can move any top-level key into its own committed file, for
example `stacks:` into `.config/mm/stacks.yaml`, so it can have its own
reviewers. `mm config split stacks` reviews the move. See
[split a large configuration](../reference/configuration.md#split-a-large-configuration).

## Compose or mm.yaml?

| Setting                                                           | Put it in         |
| ----------------------------------------------------------------- | ----------------- |
| Container command, health check, restart policy, ordinary volumes | Compose           |
| Third-party image, such as PostgreSQL                             | Compose           |
| Image repository for code mm builds                               | `.config/mm.yaml` |
| Environment templates, bindings, and secret references            | `.config/mm.yaml` |
| Managed networks and their service connections                    | `.config/mm.yaml` |
| Domains and HTTP routes                                           | `.config/mm.yaml` |

For example, Compose describes how an API runs:

```yaml
# compose.yaml
services:
  api:
    command: ["./app", "serve"]
    healthcheck:
      test: ["CMD", "./app", "health"]
    volumes:
      - api-data:/var/lib/api
volumes:
  api-data: {}
```

This fragment of `.config/mm.yaml` connects it to a build and managed network:

```yaml
networks:
  backend: {}
stacks:
  app:
    files: [compose.yaml]
    networks: [backend]
modules:
  api:
    path: services/api
    build:
      image: ghcr.io/example/api
    services:
      api:
        stack: app
        networks: [backend]
```

mm supplies the image, environment, and managed network settings when deploying.
Do not duplicate them in source Compose. Source Compose cannot declare
`environment`, `env_file`, mm-managed networks, or reserved mm labels.
Nested Compose includes are also unsupported.

## Private workstation settings

For per-checkout project customizations, use `.config/mm.overrides.yaml` beside
the shared configuration. For example, set `build.platforms: [linux/arm64]` on
an Apple Silicon workstation. The override is a partial configuration: mappings
merge by key and lists replace their shared counterparts. See the
[configuration reference](../reference/configuration.md#local-overrides).

`mm init` reviews and installs the corresponding project `.gitignore` rule.
Existing repositories should commit that rule once; each developer's override
then remains untracked. Shared config editors continue to edit `.config/mm.yaml`,
or the split file that declares the key.

Targets, runtime preferences, credentials, encrypted secrets, certificates, and
operation history normally live under `~/.mm`. They are not project files and
must not be committed.

Use `MM_HOME` or `--mm-home` for a separate private state directory:

```sh
export MM_HOME="$HOME/.mm-work"
mm target list
mm target preference show
```

The project can refer to a target or secret by name without containing its
connection credentials. See [environments and secrets](./environments-and-secrets.md).
