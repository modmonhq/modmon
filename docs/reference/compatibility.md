---
title: Compatibility between releases
description: Which releases of mm and mmd work together, what happens when one side is older, what an update does with stored state, and what to run then.
---

From 0.1.1 on, `mm` on your machine and `mmd` on your hosts do not have to run
the same release. Each release speaks a range of remote protocols, from the
oldest to the newest. When `mm` connects to a host, both sides say their range
and use the newest protocol they both speak.

| Release | Oldest remote protocol | Newest remote protocol |
| ------- | ---------------------: | ---------------------: |
| 0.1.1   |                      1 |                      1 |

Each release's notes start with its newest protocol, such as
`Remote protocol: 1`. The number changes only when a release changes the wire
in a way an older peer cannot read. New features keep it: a release adds what
it sends so that an older peer can leave it out.

## What works together

| This machine runs              | The host runs           | Result                                                                                                                            |
| ------------------------------ | ----------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| A newer `mm`                   | `mmd` 0.1.1 or later    | Works. A request the host cannot read, such as one for a feature it does not have yet, is refused by the host and changes nothing. |
| An older `mm`, 0.1.1 or later  | A newer `mmd`           | Works while the host still speaks the protocol of that `mm`. A release that stops speaking an old protocol says so in its notes.   |
| `mm` from before 0.1.1         | `mmd` 0.1.1 or later    | Refused: update `mm` on this machine with the installer.                                                                         |
| `mm` 0.1.1 or later            | `mmd` from before 0.1.1 | Refused: set the host up again with `mm target setup plan --upgrade`.                                                            |

You can upgrade your machine and your hosts in either order. Teammates who
deploy to the same hosts can update `mm` one at a time. While the releases
differ, `mm doctor` warns with `MM_HOST_RELEASE_OLDER` (upgrade the hosts) or
`MM_HOST_RELEASE_NEWER` (update this machine) and names the hosts. To make
everyone deploy with one release, pin it with
[`deploy.mm-version`](configuration.md#bindings-environments-and-secrets).

Each node of a cluster agrees on a protocol with `mm` by itself, so nodes on
different releases keep working. Keep them on one release anyway: `mm doctor`
warns with `MM_NODE_VERSION_SKEW` until they are.

## Builds from before 0.1.1

0.1.1 starts the remote protocol at 1. Earlier builds counted up to 19 and
only worked with their own protocol, so they do not work with 0.1.1 or later
either way. `mm` stops with `MM_UNREACHABLE` and says which side to replace. A
host on 0.1.1 that an older `mm` connects to says:

```text
This mm is from before 0.1.1, and the host's mmd 0.1.1 speaks remote protocol 1. Update mm on this machine to 0.1.1 or newer with the installer, https://modmon.dev/install.sh
```

`mm` 0.1.1 connecting to a host from before 0.1.1 says:

```text
target 'production' (203.0.113.10:22) runs an mm from before 0.1.1, which cannot read the hello of this mm 0.1.1 (remote protocol 1). Upgrade the host: run `mm target setup plan --upgrade` and apply that plan with --upgrade
```

Setup reaches a host over SSH as the setup account and needs no remote
protocol, so `mm target setup plan --upgrade` works on such a host. It orders
releases by version first, so a host that reports 0.1.0 and a protocol up to 19
counts as older than 0.1.1. See [upgrade mm and your hosts](../how-to/upgrade.md).

## When the two sides share no protocol

After 0.1.1, this happens only when a release drops an old protocol. The
message names the release and the protocols of each side, and the command for
the side that is behind: `mm update` on this machine, or
`mm target setup plan --upgrade` and apply that plan with `--upgrade` for the
host.

## When a host is too old for a request

A newer `mm` sends what the command needs. A host whose `mmd` cannot read the
request, such as one for a feature it does not have yet, refuses it whole
with `MM_REMOTE_UPGRADE_REQUIRED` and changes nothing:

```text
mmd 0.1.1 on this host does not understand `<request>` from mm <release>. Upgrade the host: run `mm target setup plan --upgrade` and apply that plan with --upgrade
```

A host never acts on part of a request. When `mm` has to know before it sends
anything, it checks the host's protocol first and stops with the same code:

```text
node 'app-1' (203.0.113.11:22) runs mmd 0.1.1, which does not support <the feature>; upgrade it with `mm target setup plan --upgrade` and apply that plan with --upgrade
```

## Plans

A deployment plan records the protocol of the session that made it. Anyone
whose `mm` shares a protocol with the host can apply it, whichever release
made it. Plans expire after ten minutes, so an upgrade of the host between plan
and apply means planning again.

## Stored state

`mm` keeps state on your machine in its home, `~/.mm` (or `MM_HOME`): the
state database `state.sqlite3`, the target catalog `targets.yaml`, the local
services, the secret stores, pinned host keys, and saved forwards. On a host,
`mmd` keeps its configuration in `/etc/mm` (`mmd.json`, `host.yaml`) and its
records in `/var/lib/mm`: access records, deployments and rollouts, the
mesh, the cluster's networks, ingress routes, and what it restarts and
scales.

### After an update

Every release from 0.1.1 on reads the state an earlier one wrote. `mm update`
and `mm target setup --upgrade` replace the binaries and keep that state:
the newer release fills in what the older one did not record and moves the
state database forward with its migrations the first time it opens it.
Nothing needs to be recreated or reset.

### After a downgrade

A newer release that only adds a key or a field keeps the file's schema
number, and an older release reads the file: it ignores what it does not
know. For the files people read and edit, `/etc/mm/mmd.json` and
`/etc/mm/host.yaml`, it says so:

```text
WARN unknown key `example` in /etc/mm/mmd.json; this mmd ignores it
```

`MM_HOME/mcp.json` is the exception: it only narrows what agents may do, so
`mm` refuses a key it does not read there instead of ignoring it.

A newer release raises a schema number only for a change an older release
cannot read, such as a field whose meaning changed, or a migration of the
state database. The older release then refuses the file and changes
nothing. The message names the file, what it does not know, and what to
run:

```text
state store error: ~/.mm/state.sqlite3 was written by a newer mm (state migration 0002_example); update with `mm update`
state store error: /etc/mm/mmd.json was written by a newer mm (schema 3); upgrade this host: run `mm update` on your machine, then `mm target setup plan --upgrade`
```

Run the command it names, or go back to the newer release.

Plans and build receipts carry a hash of what was reviewed. An older release
never acts on part of one: it refuses one with a field or schema it does not
know, or whose hash no longer matches what it can read.

Every schema number starts at 1 in 0.1.1. Files that builds before 0.1.1
wrote with another number are not read: `mm target setup --upgrade` writes
`mmd.json` again, and a `host.yaml` from such a build needs
`schema-version: 1` by hand.

How mm keeps these promises as it changes is in
[stored state](../contribute/stored-state.md).

## How this is checked

| Statement                                                        | Backed by                                                                                                                                                           |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Both sides use the newest protocol they share, or refuse         | `core/tests/protocol_compatibility.rs`: `client_and_daemon_speak_the_newest_protocol_both_have`, `a_daemon_refuses_a_client_it_has_no_protocol_in_common_with`      |
| A host serves a newer `mm` in the protocol both speak            | `core/tests/protocol_compatibility.rs`: `a_daemon_answers_a_newer_client_in_the_protocol_both_speak`                                                                |
| A host refuses a request it cannot read and acts on none of it   | `core/tests/protocol_compatibility.rs`: `a_daemon_answers_a_request_it_cannot_read_and_acts_on_none_of_it`                                                          |
| `mm` reads what a newer host adds to its answers                 | `core/tests/protocol_compatibility.rs`: `a_client_reads_what_a_newer_daemon_adds_to_its_answers`                                                                    |
| A command that must know first stops before it sends anything    | `core/tests/protocol_compatibility.rs`: `a_request_that_needs_a_newer_protocol_fails_before_it_is_sent`                                                             |
| Every later release reads and writes protocol 1 as 0.1.1 does    | `core/tests/protocol_compatibility.rs`: `frozen_protocol_frames_still_read_and_write_the_same`, with the frames in `core/tests/fixtures/protocol/1/`                 |
| A host from before 0.1.1 is older, whatever protocol it reports  | `core/tests/upgrade_paths.rs`: `a_host_from_before_0_1_1_is_older_whatever_protocol_it_reports`                                                                     |

No test in the repository runs two different releases against each other yet:
the tests above play the other side with frames a newer or an older release
would send.

Contributors changing what crosses the wire: see
[change the remote protocol](../contribute/remote-protocol.md).
