---
title: Use HashiCorp Vault
description: Connect a Vault server or run one for local development.
---

Use an existing Vault server or let mm run a local development instance. Give
the connection a name such as `team-secrets`; secret declarations refer to that
name. See [secret stores](secret-providers.md) for the common workflow.

## License and server support

mm and its integration code use mm's MIT license. HashiCorp Vault and its
container images are separate products under HashiCorp's terms, including the
Business Source License for supported releases. mm does not redistribute or
relicense Vault.

The driver uses Community KV v2, AppRole, Raft storage, health/seal APIs, and
Raft snapshots. An external instance sends an Enterprise namespace only when
explicitly configured; this does not imply any other Enterprise capability.

## External configuration

```yaml
providers:
  team-secrets:
    driver: vault
    scope: project
    mode: external
    address: https://vault.example.internal:8200
    namespace: engineering # optional Enterprise namespace
    storage-mount: mm-kv
    automation-auth-mount: mm-approle
    trust-bundle: company-vault-ca.pem
    expected-trust-sha256: sha256:HEX_DIGEST
```

Use an initialized, unsealed Vault Community 1.18.x or 1.19.x server reachable
over HTTPS. Replace the example address, public CA file, and fingerprint with
your server's details. Setup configures KV v2 secret storage and AppRole
authentication. The setup token is entered through a hidden prompt and is not
saved. Add `--bootstrap` when those mounts need to be created.

```sh
mm secret provider add team-secrets --driver vault --mode external --scope project --bootstrap
mm secret provider test team-secrets
```

## Managed configuration

```yaml
providers:
  local-secrets:
    driver: vault
    scope: installation
    mode: managed
    installation: local-secrets
    release: "1.19"
    management-port: 18210
```

Set up [local services](./local-services.md) first. Managed Vault supports local
Docker Compose and rootless Podman Compose, not remote lifecycle
operations. This is a development setup. Supported releases are 1.18 and 1.19;
the supported upgrade path is 1.18 to 1.19.

```sh
mm secret provider plan local-secrets --operation install
mm secret provider apply local-secrets --operation install --plan PLAN_HASH --yes
mm secret provider status local-secrets
```

The management API is available only on loopback. Normal removal keeps the
server's data volume so the same instance can reuse it later. Purging deletes it.

mm stores the initial root token and unseal key in its private Age-encrypted
store. Keep a separate backup of the Age identity. Production Vault needs its
own recovery setup and license review.
