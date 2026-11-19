---
title: Manage remote access
description: Declare who may reach each remote environment in the repo, and apply it on deploy.
---

Access to remote environments lives in the repo, in the `users:` key. Each user
lists their public keys and a level per environment. A change to access is an
ordinary commit, reviewed like any other. The host applies it to an environment
when that environment is deployed or synced.

Complete [remote host setup](./remote-daemon.md) first.

## Levels

| Level     | Allows                                                  |
| --------- | ------------------------------------------------------- |
| `view`    | plan, status, inspect, and logs                         |
| `operate` | `view` plus restart and job run, cancel, and retry      |
| `deploy`  | `operate` plus deploy and `mm team sync`                |
| `admin`   | `deploy` plus exec, forward, secrets, blocks, and audit |

`view` also reads what an environment's hosts hold for each secret and
variable (`mm secret status`): revisions and who changed them, never values.

Add-ons grant more without raising the level: `exec` and `forward` take `true`
or a list of services, and `secrets: true` lets the user set secrets and
variables on the environment's hosts for the next deployment (`mm secret
set`; see [Change an environment's variable](./how-to/change-a-variable.md)).
It is apart from `deploy` because a staged value goes live with anyone's
next deployment and nobody reviews it.
The host's `ceiling` in `host.yaml` caps all of these per environment.

```yaml
users:
  alice:
    keys:
      - ssh-ed25519 AAAAC3Nza… mm:alice@laptop
    access:
      staging: deploy
      production: view
  bob:
    keys:
      - key: sk-ssh-ed25519@openssh.com AAAAGnNr… mm:bob@yubikey
        expires: 2027-01-31
    access:
      "*": operate # every environment
      production: { level: deploy, exec: [api] }
  ci:
    machine: true
    keys: [ssh-ed25519 AAAA… ci@github]
    access: { staging: deploy, production: deploy }
```

Rules:

- User names match `[a-z][a-z0-9-]*`. Keys are `ssh-ed25519` or
  `sk-ssh-ed25519@openssh.com`; RSA and ECDSA are rejected.
- A key belongs to one user. An environment's own entry overrides `*`.
- `mm config validate` checks the users file, and the host checks it again.
- `mm.overrides.yaml` cannot set `users`, `agents`, or `deploy-from`.

To give access its own reviewers, move it to its own file with
`mm config split users` and list `.config/mm/users.yaml` in `CODEOWNERS`.

## Agent keys

An AI agent reaches a remote environment with its own key, never yours. Create
it and add it to your entry:

```sh
mm key create --agent
mm team join --agent                    # reviewed edit of the users file
mm team join --agent --plan PLAN_HASH --yes
```

The entry gains a key marked `agent: true`:

```yaml
users:
  alice:
    keys:
      - ssh-ed25519 AAAAC3Nza… mm:alice@laptop
      - key: ssh-ed25519 AAAAC3Nza… mm-agent:alice@laptop
        agent: true
    access:
      staging: deploy
      production: view
```

The host gives an agent key your access limited by the
[`agents:` block](./mcp.md#set-what-agents-may-do) of the deployed commit, and
by the host's `ceiling`. An agent key never gets `admin` or `forward`; it gets
`exec` only for services that both you and the block allow, and `secrets`
only when both you and the block have it. Without an `agents:` entry for the environment it gets `view`.

- The agent appears as `alice (agent)` in plans, `mm team whoami`, and the
  audit log.
- A user needs a key of their own to have an agent key, and a `machine` user
  cannot have one. When your own keys expire or are removed, your agent's
  access ends too.
- `mmd access block` works on an agent key's fingerprint and leaves your own
  key working.

An agent key is encrypted like your own, and `mm` releases its passphrase
only inside `mm mcp serve`. It cannot be a security key, because an agent
cannot touch one. The host can only tell an agent from you by the key it
presents, so read [what the encryption stops](#what-this-stops-and-what-it-does-not)
before you rely on it.

## Keys are encrypted, and only mm uses them

`mm key create` writes this device's key under `MM_HOME/identities/ssh`,
encrypted with a passphrase that `mm` generates and stores in your system
keychain. Nobody chooses or types the passphrase. `mm` fetches it when it
connects, so every command works as before and none prompts.

The private key file is for `mm` only. Never pass it to `ssh`, `scp`, `git`
or `ssh-add`, and never copy it. On its own it opens nothing:

```text
$ ssh -o BatchMode=yes -i ~/.mm/identities/ssh/id_ed25519 mm@app-1.example.com
mm@app-1.example.com: Permission denied (publickey).
```

`mm key show` prints the public key and how the key is protected:

```text
ssh-ed25519 AAAAC3Nza… mm:alice@laptop
  fingerprint: SHA256:…
  private key: …/.mm/identities/ssh/id_ed25519
  protection:  passphrase in the system keychain
```

Where the passphrase is kept:

| Machine | Vault |
| --- | --- |
| Linux with a desktop session | The Secret Service (GNOME Keyring, KWallet), through `secret-tool`. Install `libsecret-tools` on Debian and Ubuntu; Fedora's `libsecret` includes it. |
| macOS | The login keychain, through `security`. |
| No keychain: a server, a container, an SSH session without a desktop | The file `id_ed25519.passphrase` beside the key, readable by you alone. `mm key create` says so when it happens. |
| CI | None. `MM_SSH_KEY` comes from the CI system's secret store and is not a key `mm` manages. |

Set `MM_KEY_VAULT=keychain` to make a missing keychain an error instead of
falling back to the file, or `MM_KEY_VAULT=file` to never ask a keychain.
A security key (`mm key create --security-key`) has no passphrase: the file
is only a handle, and every connection needs a touch.

### What this stops, and what it does not

It stops:

- **Using the key file directly.** `ssh -i`, `scp` and `git` cannot load it,
  whoever runs them: you, a script, or a coding agent taking a shortcut.
- **A copied key.** The file from a backup, a synced folder, or a stolen disk
  authenticates nowhere. With a keychain, neither does a copy of the whole mm
  home.
- **Getting the passphrase through `mm`.** No command prints or exports it.
  `mm` refuses to run in a coding agent's shell, and the agent key's
  passphrase is released only inside `mm mcp serve`.

It does not stop a program that runs as you and sets out to get the key:

- On Linux the Secret Service answers every program on your session bus while
  you are logged in. On macOS the `security` tool hands the item to whoever
  runs it while the login keychain is unlocked. The passphrase file is
  readable by you. No vault `mm` supports tells one of your programs from
  another.
- Such a program can also run `mm` itself after removing the variables that
  mark a coding agent, and then acts as you.
- While `mm` connects, the passphrase passes to `ssh` through a pipe in a
  directory only you can enter. During `mm target setup apply` on a target
  with `nodes:`, an `ssh-agent` of mm's own holds this device's key until the
  command ends, because Ansible opens those connections.

For a boundary against your own account, use a security key, or run the
agent as another user or in a sandbox that reaches neither your session bus
nor your mm home.

### Protect a key made by an earlier mm

Keys created before `mm` encrypted them are plain files. `mm doctor` reports
each one:

```text
  warning[MM_KEY_UNPROTECTED]: this device's mm key (SHA256:…) is a plain private key: anything running as you can use or copy it
    help: Run `mm key protect`: it encrypts the key in place and keeps its public key, so no host access changes
```

```sh
mm key protect                         # shows what will change
mm key protect --plan PLAN_HASH --yes
```

Expected result: the key file is encrypted, its passphrase is in the vault,
and the public key is the same, so `mm --environment staging team whoami`
answers as before. The same command moves a passphrase from the file into
the keychain on a machine that has one now. It needs OpenSSH 8.4 or newer.

### Replace or remove a key

Replacing a key never locks you out, also when you are the only admin: the
new key is made beside the current one, and mm offers both until you finish.

```sh
mm key rotate                          # or: mm key rotate --agent
mm key rotate --plan PLAN_HASH --yes
mm team join                           # or: mm team join --agent
git commit -am "rotate my key" && git push
mm team sync
mm key rotate --finish                 # then: --finish --plan PLAN_HASH --yes
```

1. `mm key rotate` makes the new key (`id_ed25519.next` beside the current
   one) under a passphrase of its own in the vault. Nothing else changes: from
   now on every connection offers the new key first and the current one
   after it, so a host that lists either accepts this device.
2. `mm team join` puts the new public key in your entry of the users file in
   place of the current one, and says so:

   ```text
   This replaces the previous mm key SHA256:8be0OuPk… of this device in user 'alice'; hosts stop accepting it after the next `mm team sync`.
   ```

   Only that key leaves the entry; the keys of your other devices and any key
   mm did not rotate here stay. The entry is the one that lists this device's
   key, whatever its name. When no entry lists it, pass `--name` with yours.
3. Commit and push the users file and run `mm team sync` in each project.
   The sync authenticates with the current key, which the hosts still list,
   and installs the new one.
4. `mm key rotate --finish` puts the new key in place of the current one and
   deletes the old key and its passphrase. Hosts whose users file does not
   list the new key yet refuse this device from then on, so finish once every
   environment you use was synced. Until you do, `mm doctor` reports
   `MM_KEY_ROTATION_PENDING`. In a project whose users file lists your
   current key and not the new one, `--finish` refuses and names the entry;
   `--unlisted` finishes anyway.

mm's users and `mm team sync` cover only mm's own access. If you also listed
the old key elsewhere, such as in root's `~/.ssh/authorized_keys` on hosts
that `mm target setup` logs in to, put the new key there before you finish.
Setup that a host refuses names the key it logged in with and the line to
add.

A rotation of the agent key works the same way with `--agent`; the tools of
the [MCP server](./mcp.md) keep working throughout. A FIDO2 security key
(`--security-key`) is replaced at once: hosts refuse it until an admin puts
the new public key in the users file and runs `mm team sync`, and a command
says so and stops:

```text
error[MM_UNREACHABLE]: target 'staging' (203.0.113.10:22) refused this device's key. Ask an admin to add the key `mm key show` prints to the users file and run `mm team sync`. In CI, set MM_SSH_KEY to a key the users file lists
```

`mm key remove` (or `mm key remove --agent`) deletes the key and its
passphrase from this device, again as a reviewed step. Remove its public key
from the users file too.

### When the passphrase is gone

A command that needs the key says so and stops:

```text
error[MM_SECRET]: secret provider error: this device's mm key is encrypted and its passphrase is not in the system keychain. If the keychain is locked, unlock it and run the command again. If the entry is gone, the key cannot be recovered: run `mm key rotate`, then `mm key show` and have an admin register the new public key
```

A locked keychain unlocks when you log in to your desktop; over SSH there is
no session to unlock it in. If the entry was deleted or the keychain was
reset, nothing can decrypt the key: rotate it and enrol the new public key
as above. `mm doctor` reports the same condition as
`MM_KEY_PASSPHRASE_MISSING`.

## Connections stay open for five minutes

The first request to a host makes an SSH connection, and `mm` keeps it open in
the background for later requests: the next command, or the next refresh of
the terminal UI, starts on it instead of connecting again. Connecting is most
of what a short read waits for, so everything after the first request answers
much sooner. There is nothing to set up.

- A connection closes by itself five minutes after its last request.
- `mm` closes the connections to an environment's hosts when it applies
  `mm team sync` or a deployment there, because both apply the users file. It
  closes every kept connection when it creates a key, applies
  `mm target setup`, or accepts a target's new keys with `mm target trust`. A
  connection made with a key file that has since been removed or replaced is
  closed the next time `mm` reaches any remote environment.
- The host checks every request, kept connection or not: when your access is
  removed there, your next request is refused.
- `mm key protect`, `mm key rotate` and `mm key remove` close every kept
  connection too.
- `mm target connections` lists the kept connections with their host, key
  and age, and `mm target connections --close` closes them all.
- A closed connection takes no new request. A request already running on it,
  such as another `mm`'s upload to the host, finishes first.
- Set `MM_SSH_REUSE=off` to connect anew for every request. A command run
  with it also closes the connections kept before.

What is kept, and what never is:

| Key | Kept? | Why |
| --- | --- | --- |
| This device's key (`mm key create`) | Yes | `mm` opens the key with a passphrase from a vault that a program running as you can ask too, so the open connection gives such a program nothing more, and less: only the `mm` relay of that host, for five idle minutes, and nothing it can take elsewhere. |
| A private target's `--identity-file` | Without a passphrase | A program running as you can already read the file. A key file under a passphrase of your own is not kept. |
| A security key (`mm key create --security-key`) | No | It asks for a touch per connection. A kept connection would let a program running as you skip the touch. |
| The agent key (`mm key create --agent`) | No | Every call of the MCP server connects by itself, so no call rides on a connection another one opened. |
| `MM_SSH_KEY` | No | The key lives only as long as the command that was given it. |

A kept connection is a socket under `MM_HOME/run/ssh`, a directory only your
user can enter; `mm` keeps none if the directory is anything else. Only `mm`
uses it. A connection that has to authenticate reads the passphrase from the
vault and takes about a dozen round trips to the host; one that starts on a
kept connection takes four and asks no vault. On a deep `MM_HOME` whose socket path would not fit the operating
system's limit, `mm` connects anew every time.

Each kept connection is one background `ssh` process, which ends when the
connection closes. In a container, run `mm` under an init process
(`docker run --init`) so that process is reaped, or set `MM_SSH_REUSE=off`:
a container whose first process does not reap leaves a defunct process
behind for every closed connection. When no further process can be started,
`mm` connects without keeping the connection.

## Join a team

```sh
mm key create                                  # once per device
mm team join --request staging=deploy          # reviewed edit of the users file
mm team join --request staging=deploy --plan PLAN_HASH --yes
```

Commit the change and open a pull request. Once it is merged, an admin deploys
or runs `mm team sync` for each environment. Then check:

```sh
mm --environment staging team whoami
```

Expected result: your name, key fingerprint, level, and the commit whose users
file the environment enforces. `mm key create --security-key` uses a FIDO2
security key instead of a key file.

To add someone else, or a CI key: `mm team add NAME --key 'ssh-ed25519 AAAA…'
--access staging=deploy`, with `--machine` for automation. `mm team show`
lists everyone in the users file.

## Apply access changes

Deploying an environment applies the users file of the deployed commit to that
environment first, then deploys. The plan shows the access change:

```text
  access changes in staging:
    carol: none -> deploy, +1 key(s)
```

A deployment whose commit lowers the access of whoever deploys it, such as an
agent applying a commit whose `agents:` block gives agents only `view`, still
runs to its end: its deployer finishes or aborts that one rollout, and every
other request is decided by the new access at once. Any other change of access
to the environment while it runs, such as a `mm team sync` or a block, ends
that, and the rollout stops where it is.

To change access without deploying, use `mm team sync` (needs `deploy`):

```sh
mm --environment production team sync                       # plans from HEAD
mm --environment production team sync --plan PLAN_HASH --yes
```

Each environment is reconciled on its own: syncing staging never changes
production.

## Control which commits deploy

`deploy-from` limits an environment to commits on certain branches:

```yaml
environments:
  production:
    target: prod-eu
    deploy-from: [main]
```

A commit must be on one of the listed branches of the origin, and a commit
that changes `deploy-from` must pass the list in force.

An entry can also be a pattern: `*` matches any characters within one `/`
segment and `?` matches one character. `"*"` on its own allows any branch.
For release branches with hotfixes:

```yaml
environments:
  production:
    deploy-from: [main, "*.x"] # main, 26.6.x, 26.7.x, …
```

Without `deploy-from`, any commit on the origin may be deployed. Use branch
protection on the git host, because whatever reaches an allowed branch is
trusted.

On a target with `delivery: push` the host has no repository to look the
commit up in. It checks that the files are the commit's, and holds
`deploy-from` against the branches the deploying client states. Everyone with
`deploy` on such a target is therefore trusted like the allowed branches; see
[what push cannot guarantee](./remote-daemon.md#what-push-cannot-guarantee).

## Forward a remote port

To reach a remote database or admin port from your machine, forward it to
`localhost`. You need the `forward` add-on for that service.

```sh
mm --environment production forward postgres                  # localhost:5432
mm --environment production forward postgres:5432 --local 15432
mm --environment production forward postgres redis            # several at once
```

Expected result: `Forwarding 127.0.0.1:5432 -> production/postgres:5432`. Point
your client at the local port; Ctrl-C stops every forward. Without a port, mm
uses the service's `http-port`, or the container side of its Compose `ports` or
`expose`. If the local port is busy, mm picks a free one and prints it.

Each connection travels through the mm protocol to that service's container.
The host never opens SSH forwarding, and mm never connects to host ports or
other machines. Forwards listen on `127.0.0.1` unless you pass `--bind`.

Save forwards you use often. Saved forwards are private to your machine:

```sh
mm --environment production forward save prod-db postgres --local 15432 --auto
mm forward prod-db            # run one by name
mm forward up                 # start every forward saved with --auto
mm forward list
mm forward remove prod-db
```

Forwarding needs a rootful Docker or Podman host for now; rootless runtimes
report that the container is not reachable.

## What a deploy may run

Trusting the repo means trusting it inside the host's limits. A remote host
refuses Compose features that would reach the host itself: `privileged`,
capabilities beyond Docker's defaults, devices, host namespaces (`pid`, `ipc`,
`uts`, `userns_mode`, `cgroup`, `network_mode: host`), `cgroup_parent`,
unconfined `security_opt`, non-`net.*` sysctls, bind or device `driver_opts`
volumes, external volumes and networks, `configs`/`secrets` files outside the
project, and `build:` (remote hosts never build).

A host checks only the services the environment it plans runs: the ones
`mm deploy plan` renders. A service whose `environments:` leaves the
environment out is not checked there, and neither is a stack left with no
service. A service that only `local` runs, such as a developer tool with
`network_mode: host`, needs no exception on any host.

Check before you push:

```sh
mm config validate --for-remote                  # every environment whose target is not local
mm -e staging config validate --for-remote       # one environment
```

Expected result: `config: valid`, or one `MM_REMOTE_GUARDRAIL` error per
feature, naming the environment, the `<stack>/<service>`, and the Compose key.

Only a host admin can make an exception, for one feature of one service, in
`/etc/mm/host.yaml`:

```yaml
projects:
  019c527b-6242-7b43-bfd4-bd60e27c1de3:
    allow:
      - service: monitoring/node-exporter # <stack>/<service>
        features: [pid-host]
```

To check a project against the policy its hosts have, for example in CI,
keep a copy of their `host.yaml` and pass it:

```sh
mm config validate --for-remote --host-policy ops/staging-host.yaml
```

With `--host-policy`, mm reads the file as `mmd` does and checks the
environments it runs for the project, or the one `-e` names. A feature its
`allow` list grants is reported as `MM_REMOTE_GUARDRAIL_ALLOWED` and does not
fail the command; anything else fails as before. A policy that does not
enroll the project is refused with the project's id.

A host also decides which OCI runtimes a service's `runtime:` may name, and
can run every service that is reachable from outside under one of them. See
[choose an OCI runtime](./hardening.md#choose-an-oci-runtime).

### Run a service on the host's network

A VPN that manages the host's interfaces, or a DNS server that answers on
the host's own address, needs `network_mode: host` in its Compose
definition:

```yaml
# compose.yaml
services:
  vpn:
    image: ghcr.io/acme/vpn:1.4
    network_mode: host
    cap_add: [NET_ADMIN]
```

On a remote host or a cluster node, `mmd` plans it only when the host policy
allows `network-host` for that service, and `cap-add` for capabilities
beyond Docker's defaults. A host admin adds it on every host that runs the
service:

```yaml
projects:
  019c527b-6242-7b43-bfd4-bd60e27c1de3:
    allow:
      - service: edge/vpn # <stack>/<service>
        features: [network-host, cap-add]
```

A host without that entry refuses the plan, and the message names the
service and the feature to allow. `mm config validate --for-remote` reports
the same before you push.

To run the service on your own machine too, for example in `mm dev up`, say
why in `mm.yaml`. The plan shows the reason, on your machine and on the
hosts:

```yaml
modules:
  edge:
    services:
      vpn:
        stack: edge
        host-network: owns the tunnel interface
```

Without it, `mm config validate`, `mm deploy`, and `mm dev up` refuse an
environment that runs on your machine, and the message names the Compose
file, the service, and the key to add. What else such a service may not do
is listed under
[`host-network`](./reference/configuration.md#modules-services-and-builds).

On a cluster the service is on none of the
[networks across the nodes](./remote-daemon.md#networks-across-nodes): other
services do not reach it by name, and it resolves names as its node does, so
service names do not resolve from it. Reach it on the node's address, and
have it reach other services on the ports they publish. It runs one container
on each node of its placement; two on one node would want the same ports.

## Remove a developer

Remove the user (`mm team remove NAME`), merge, and sync or deploy each
environment. Their key stops working there at once, including in sshd.

For immediate effect before the repo catches up, a host admin can block a key:

```sh
mmd access block --project PROJECT_ID --environment production SHA256:FINGERPRINT
```

On a cluster target, run it on every node: a block applies only to the host
where you set it. A block takes effect at once, also while that key's
deployment or command is running on the host: within two seconds `mmd` stops
the operation where it is and refuses it with `MM_SECURITY`. A deployment
stopped this way leaves the host partly changed; `mm deploy status` shows
it, and the next deployment by someone who still has access puts it right.
That deployment can begin about a minute after the block, once the stopped
rollout's lock has lapsed, or at once after `mm deploy cancel`.
The block lifts by itself once a sync no longer lists the
key, or with `mmd access unblock`. `mmd access show` lists users, blocks, and
deployed commits for every environment.

## Emergency access

Break-glass keys in `host.yaml` are admin on every enrolled project. Use one
with `MM_SSH_KEY` to repair a users file that locks everyone out, then put the
private key back offline.
