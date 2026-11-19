---
title: Choose a secret store
description: Use local Age secrets, a file another tool keeps, an external OpenBao or Vault server, or a managed development server.
---

For a local project, start with Age: an encrypted store that does not need a
separate server. See [environment values and secrets](./concepts/environments-and-secrets.md).
When a tool already keeps its credentials in a file, read that file with the
`file` provider. Use OpenBao or Vault when your team already has a server, or
when you need to test that integration locally.

## Read a secret from a file

Some tools keep their credentials in a file of their own, such as a package
manager's `auth.json`. The `file` provider reads the secret from that file
when a build or deployment needs it, so the value is not copied into mm or
into an environment variable:

```yaml
secrets:
  registry.auth:
    storage:
      provider: file
      location: ~/.config/tool/auth.json
modules:
  web:
    build:
      secrets: { registry_auth: registry.auth }
```

`location` is an absolute path or one that starts with `~/`, your home
directory. A relative path is refused, because a file in the checkout could
be committed with the project. The value is the file's content exactly as
written, newlines included: UTF-8 text up to 1 MiB. mm reads it each time it
is needed and keeps no copy.

```sh
mm secret status registry.auth
```

```text
example [local]
  KEY                          PROVIDER     STATUS       LOCATION
  registry.auth                file         set          ~/.config/tool/auth.json
```

The secret is `unset` while the file is missing or empty. It is `invalid`,
and `mm secret status` says why, when the path is a directory, a device or a
pipe, holds more than 1 MiB or text that is not UTF-8, or lies inside mm's
own home (`~/.mm`, or the directory `--mm-home` and `MM_HOME` name), which
holds mm's keys. A symbolic link is followed, and its target is checked the
same way. mm never writes the file: `mm secret set`, `rotate` and `generate`
refuse a `file` secret, so edit the file instead.

In CI, write the CI secret to the same path before mm builds. With GitHub
Actions:

```yaml
- name: Write registry credentials
  run: |
    umask 077
    mkdir -p ~/.config/tool
    printf '%s' "$REGISTRY_AUTH" > ~/.config/tool/auth.json
  env:
    REGISTRY_AUTH: ${{ secrets.REGISTRY_AUTH }}
```

A remote host never reads a file a project names. A `file` secret serves
builds made on the machine that runs mm; a deployment whose services need the
value on a host is refused. For those, keep the value in a secret with
`provider: age`, bind it for that environment, and stage it from your
machine with `mm -e ENV secret set NAME --stdin < ~/.config/tool/auth.json`
(see [Change an environment's variable](./how-to/change-a-variable.md)).

## Connect an existing server

Give the connection a name, such as `team-secrets`, in `.config/mm.yaml`:

```yaml
providers:
  team-secrets:
    driver: openbao
    scope: project
    mode: external
    address: https://provider.example.test:8200
    storage-mount: mm-kv
    automation-auth-mount: mm-approle
    trust-bundle: company-ca.pem
    expected-trust-sha256: sha256:HEX_DIGEST
secrets:
  database.password:
    storage:
      provider: team-secrets
      location: database/password
```

Replace the address, trust bundle, and digest with verified server details.
Store the public trust bundle in the project. Never put tokens, unseal keys, or
TLS private keys in this file.

```sh
mm secret provider add team-secrets --driver openbao --mode external --scope project
mm secret provider test team-secrets
mm secret provider status team-secrets
```

Setup asks for credentials through a hidden prompt after showing its plan.
Use the [OpenBao](./openbao.md) or [Vault](./vault.md) guide for server requirements
and bootstrap options.

A remote host does not use this connection. To let a host or a cluster read
the store itself, so that values never pass through your machine, see
[read secrets from OpenBao or Vault on the host](./remote-daemon.md#read-secrets-from-openbao-or-vault-on-the-host).

## Run a local development server

A `managed` provider is a server mm starts and maintains for you. Declare the
instance using the driver guide, then review its installation:

```sh
mm secret provider plan local-secrets --operation install
mm secret provider apply local-secrets --operation install --plan PLAN_HASH --yes
mm secret provider status local-secrets
```

Replace `PLAN_HASH` with the printed hash. These servers require shared local
ingress and a local Docker Compose or rootless Podman Compose runtime. Remote
managed lifecycle operations are unsupported. Rootless Podman also
needs a working user namespace and `podman unshare`.

Managed providers are for development, not production high availability or
automatic unsealing after a disaster. Each instance has separate data volumes
and a loopback-only management port.

## Inspect or remove

```sh
mm secret provider list
mm secret provider show local-secrets
mm secret provider plan local-secrets --operation remove
mm secret provider remove local-secrets --plan REMOVE_PLAN_HASH --yes
```

Removal preserves data by default. Add `--purge` to both removal commands only
when you intend to delete it. Changing a provider's driver or mode requires a
reviewed replacement, not an in-place edit.

See the [CLI reference](./reference/cli.md) for backup and restore options and
the [provider definitions](/schema/provider-descriptors.json) for machine-readable
supported operations.

To change a secret of a remote environment, see
[Change an environment's variable](./how-to/change-a-variable.md).
