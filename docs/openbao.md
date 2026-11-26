---
title: Use OpenBao
description: Connect an OpenBao server or run one for local development.
---

Use an existing OpenBao server or let mm run a local development instance.
Give the connection a name such as `team-secrets`; secret declarations refer
to that name. See [secret stores](secret-providers.md) for the common workflow.
On a cluster target, mm can also run OpenBao on the nodes for you: see
[keep deploy secrets in a managed store](how-to/managed-secret-store.md).

## External configuration

```yaml
providers:
  team-secrets:
    driver: openbao
    scope: project
    mode: external
    address: https://bao.example.internal:8200
    namespace: engineering # optional
    storage-mount: mm-kv
    automation-auth-mount: mm-approle
    trust-bundle: company-bao-ca.pem
    expected-trust-sha256: sha256:HEX_DIGEST
```

Use an initialized, unsealed OpenBao 2.3.x, 2.4.x or 2.5.x server reachable over HTTPS.
Replace the example address, public CA file, and fingerprint with your server's
details. Setup creates an AppRole (the credential mm uses to access the server)
limited to the configured environment and secret locations. The setup token is
entered through a hidden prompt and is not saved.

```sh
mm secret provider add team-secrets --driver openbao --mode external --scope project --bootstrap
mm secret provider test team-secrets
```

Omit `--bootstrap` when KV v2 and AppRole mounts already exist. For automation,
`--stdin --yes` reads the token after review without placing it in arguments.

mm records who wrote each version in the secret's KV v2 custom metadata
(`mm_set_by` and `mm_set_version`), which `mm secret status` shows as `BY`.
The AppRole policy mm creates allows `update` on each secret's `metadata/`
path for this. A connection set up by mm 0.1.2 or earlier lacks it: its
writes still succeed and show `BY` as `—`. Run the `mm secret provider add`
command above again for the environment to update the policy. If you manage
the policy yourself, grant `["read", "update"]` on `MOUNT/metadata/mm/*`
paths, or leave it out and go without `BY`.

## Managed configuration

```yaml
providers:
  local-secrets:
    driver: openbao
    scope: installation
    mode: managed
    release: "2.4"
    installation: local-secrets # optional, default: the provider's name
    management-port: 18200 # optional, default: 18200
```

Set up [local services](./local-services.md) first. Managed OpenBao supports local
Docker Compose and rootless Podman Compose, not remote lifecycle
operations. This is a development setup. Supported releases are 2.3 and 2.4
(2.5 is for [the store on a cluster](how-to/managed-secret-store.md) only);
the supported upgrade path is 2.3 to 2.4.

```sh
mm secret provider plan local-secrets --operation install
mm secret provider apply local-secrets --operation install --plan PLAN_HASH --yes
mm secret provider status local-secrets
```

The management API is available only on loopback. Normal removal keeps the
server's data volume so the same instance can reuse it later. Purging deletes it.

mm stores the initial root token and unseal key in its private Age-encrypted
store. Back up the Age identity separately from the provider instance; losing
both can make recovery impossible. A production OpenBao deployment needs its
own recovery and unsealing setup.
