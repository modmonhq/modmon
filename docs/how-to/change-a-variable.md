---
title: Change an environment's variable
description: Change a value in the repo, or set a secret or variable on a remote environment's hosts for its next deployment, without setup or root.
---

An environment's services get their values from two places: the repo's
`.config/mm.yaml`, which everyone reviews, and the hosts' own store, which
holds the secrets and the values that differ per environment. This guide
covers both, and when you still need `mm target setup`.

## Values fixed in the repo

A value under `bindings:` (`value:` or `cast:`), or under a service's `env:`,
is part of the configuration. Change it like any other line:

```yaml
environments:
  uat:
    bindings:
      APP.MODE:
        value: staging
```

Commit the change, open a pull request, and deploy the merged commit as you
always do (`mm -e uat deploy plan`, then `apply`). `mm` refuses to set such a
value on the hosts and tells you to change it in the repo.

## Secrets and per-environment variables on the hosts

A secret declared under `secrets:` with the `age` provider, and a variable a
template uses (`# mm:from=KEY`) that `.config/mm.yaml` gives no value, can be
set on a remote environment's hosts directly. You need no root, no setup and
no Ansible; `mm` sends the value through the same connection as `mm deploy`.

```sh
mm -e uat secret set storage.dev-bucket --stdin < bucket-key.txt
mm -e uat secret set web.app-key          # asks for the value, hidden
mm -e uat env set APP.FEATURE_FLAGS beta,search
```

What happens:

- `mm` asks each host for its store's age recipient, seals the value to it,
  and sends it. The value is never in plain text in transit, on disk, in the
  audit log or in any output.
- Every host stages the new revision, or none does: a host that cannot be
  reached or refuses stops the change, and the hosts that had taken it put
  back what they held. The error names the host.
- Running services keep their current values. The change goes live with
  the next deployment.
- Only a key the deployed commit declares can be set. To add a secret,
  declare it under `secrets:`, deploy that commit, then set it.

Expected result:

```text
Staged web.app-key revision 5f1c9e2a on app-1, app-2, db-1
Running services keep their values until the next deployment: `mm -e uat deploy plan` lists it, `mm -e uat deploy redeploy` makes it live
```

To remove a value at the next deployment, stage its removal; to drop a
staged change before it is deployed, discard it:

```sh
mm -e uat secret unset web.app-key
mm -e uat secret discard web.app-key
mm -e uat env discard APP.FEATURE_FLAGS
```

A secret kept in OpenBao or Vault is not staged on the hosts: write a new
version in the store and redeploy. To fill this machine's own store instead,
the one `mm target setup` pushes, pass `--local` to `secret set`, `unset` or
`status`.

A new secret can be set once a commit that declares it reached the hosts:
`mm deploy plan` for that commit refuses while the secret is unset, names the
command that sets it, and leaves the commit on the hosts so that the command
works.

### Who may set values

Setting a value needs `secrets: true` for the environment in the users file;
`admin` includes it. `deploy` alone does not, because a staged value goes
live with anyone's next deployment and nobody reviews a value:

```yaml
users:
  alice:
    keys: [ssh-ed25519 AAAAC3Nza… mm:alice@laptop]
    access:
      uat: { level: deploy, secrets: true }
```

Anyone with `view` sees what the hosts hold. Every stage, removal and
discard is written to each host's audit log with who did it, the key and the
revision, never the value. An AI agent sets values only when the `agents:`
block also says `secrets: true` for the environment (see
[MCP](../mcp.md#set-secrets-and-variables)).

### See what the hosts hold

```sh
mm -e uat secret status
```

```text
Secrets and variables of uat on uat
  web.app-key          secret   app-1   live: 3f9a1c2e by alice   staged: ~ 5f1c9e2a by bob
  web.app-key          secret   app-2   live: 3f9a1c2e by alice   staged: ~ 5f1c9e2a by bob
  storage.dev-bucket   secret   app-1   live: unset               staged: + 8d0e4b71 by bob
  APP.FEATURE_FLAGS    variable app-1   live: 9a7c3d10 by alice   staged: -
```

Each line is one key on one host: the live revision and who changed it, and
what is staged. `live: default` marks a secret with no live value whose
services all fall back to a non-empty default in their `.env.example`, judged
by the templates in your checkout; `live: unset` means at least one of them has
no default. JSON output marks such a row `"defaulted": true`. The terminal UI's Variables view shows the same, and lets you
set (`enter`), stage a removal (`x`) or discard (`X`) the selected key.

## Make staged changes live

`mm deploy plan` lists what it makes live, per service:

```text
~ web.app-key         secret, revision changed, staged by bob, for web
+ storage.dev-bucket  secret, staged by bob, for worker
```

The plan binds the staged revisions: a change staged after you reviewed the
plan makes it stale, and you plan again. A host that would deploy without a
required value refuses the plan and names the key and the command that sets
it.

To make staged changes live without a new commit, redeploy the commit the
environment runs. It is an ordinary reviewed deployment, rolling and
drained, and needs `deploy` access:

```sh
mm -e uat deploy redeploy
```

A redeploy keeps the release the hosts run, its name and its
[release inputs](../concepts/environments-and-secrets.md#tell-a-container-which-release-it-runs);
it refuses `--release` and `--release-input`. To change them, plan a
deployment with them instead.

Re-running your CI deploy job for the same commit does the same. In GitHub
Actions, for example:

```yaml
on: { workflow_dispatch: {} }
jobs:
  redeploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: mm -e uat deploy redeploy --yes
        env:
          MM_SSH_KEY: ${{ secrets.MM_SSH_KEY }}
```

## When setup is still needed

`mm target setup` builds hosts: it installs the engine, `mm` and `mmd`, the
mesh, and pushes the secrets of your local store. Setting a value never needs
it. Setup also refuses to replace the `mm` and `mmd` the hosts run with
another build or an older release unless you pass `--upgrade` or pin the
release with `deploy.mm-version`, so a teammate on another build cannot swap
what runs the hosts by accident. See
[Upgrade mm and mmd](../remote-daemon.md#upgrade-mm-and-mmd).
