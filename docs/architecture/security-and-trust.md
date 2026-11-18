---
title: Security and trust
description: What mm protects, who can access secrets, and where permissions are configured.
---

## Secret values

Keep secret values in a secret store, not in project YAML. Plans and status
output show names and revisions rather than values. Use hidden CLI prompts or
stdin when entering credentials.

Containers still need access to the secrets they use. A host or container-engine
administrator can inspect those values. Environment delivery and mounted secret
files are not protection against a compromised host. See
[secret delivery](../secret-delivery.md).

## Resources and plans

mm labels the resources it creates and checks those labels before updating or
removing them. It does not adopt an existing resource just because its name
matches. When a plan's inputs change, mm requires a new review.

Missing permissions, unsupported features, or unclear ownership stop the
operation. mm does not silently switch runtimes or fall back to unrestricted
shell access.

## Remote access

Three layers decide every remote request:

| Layer          | Lives in                       | Decides                                                                          |
| -------------- | ------------------------------ | -------------------------------------------------------------------------------- |
| Host policy    | `/etc/mm/host.yaml`, root-only | Which projects and environments run here, ceilings, break-glass keys, exceptions |
| Project access | The repo's `users:` key        | Who may do what in each environment                                              |
| Session        | SSH key → relay → `mmd`        | Identity: the key that authenticated                                             |

Whatever reaches the users file on a deployed commit is trusted, so protect the
branches an environment deploys from (`deploy-from`). The host still caps it:
access never exceeds the host ceiling, and Compose features that would reach the
host (privileged containers, host namespaces, devices, and similar) are refused
unless the host policy allows them for one service.

A project can go further than the host asks: `hardening: strict` runs every
deployed service with a read-only root filesystem and no capabilities, and a
host can put everything that is reachable from outside under an OCI runtime
such as gVisor. See [harden services](../hardening.md).

Developers reach `mmd` through one locked `mm` SSH account that can only start
the relay: no shell, PTY, forwarding, or file transfer, and no engine socket.
`mmd` fetches deployed commits from the origin pinned in the host policy and
never accepts project files from a client. The exception is a project the
host policy marks `delivery: push`: `mmd` then never contacts the git server
and takes each commit as a bundle from a client that may plan, keeping it
only when its files hash to the commit's tree. That host can no longer check
which branch a commit is on; see
[what push cannot guarantee](../remote-daemon.md#what-push-cannot-guarantee)
and [remote access](../remote-access.md).

After a disconnected remote change, inspect status and audit history before
retrying: the operation may have completed even if the response was lost.

## Agent access

AI agents use the [MCP server](../mcp.md). The project's reviewed `agents:`
block sets what an agent may do in each environment; the default is `view`.
An action outside the grant is not performed: the agent receives the command
for the user to run.

- A remote host enforces the block itself. The MCP server connects only with
  a separate agent key, and the host gives that key the user's access limited
  by the `agents:` block of the deployed commit. A local edit cannot widen it.
- On the user's machine the block applies only after the user trusts it with
  `mm agent trust`, so an agent that edits the block gets `view`. `mm` also
  refuses to run directly when a coding tool's environment variables are set.
  Both stop an agent that follows instructions. Neither contains an agent with
  an unrestricted shell, which can call the container engine.
- The SSH keys `mm` creates are encrypted with passphrases it generates and
  keeps in the system keychain (a file beside the key where there is none).
  A key file is useless to `ssh -i`, in a backup, or on another machine, and
  no `mm` command reveals a passphrase; the agent key's is released only
  inside the MCP server. The keychains `mm` supports answer any program
  running as the user, so this is not a boundary against the user's own
  account: a security key is. See
  [remote access](../remote-access.md#what-this-stops-and-what-it-does-not).
- `mm` keeps the SSH connection to a host open for five minutes after a
  user's request, as a socket only that user can reach. It does this for
  this device's key, whose passphrase a program running as the user could
  ask the keychain for anyway, and for a plain key file: never for a
  security key, the agent key, or `MM_SSH_KEY`. The MCP server connects
  anew for every call, and the host authenticates and authorizes every
  request, whichever connection carries it. `mm target connections` lists
  and closes them.
- Changes require a current plan and a short-lived, single-use review token.
  Secret values, port forwards, an interactive shell, and edits to the users
  file are not available through MCP. `mm_exec` output is not redacted.

## Report a vulnerability

Use the repository's private security advisory flow. Include the affected
revision, impact, and reproduction steps without live credentials. See the
[security policy](../project/security.md).
