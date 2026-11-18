---
title: Choose a secret store
description: Use local Age secrets, an external OpenBao or Vault server, or a managed development server.
---

For a local project, start with Age: an encrypted store that does not need a
separate server. See [environment values and secrets](./concepts/environments-and-secrets.md).
Use OpenBao or Vault when your team already has a server, or when you need to
test that integration locally.

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
