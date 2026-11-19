---
title: Compatibility between releases
description: Which releases of mm and mmd work together, what happens when one side is older, and what to run then.
---

From 0.1.1 on, `mm` on your machine and `mmd` on your hosts do not have to run
the same release. Each release speaks a range of remote protocols, from the
oldest to the newest. When `mm` connects to a host, both sides say their range
and use the newest protocol they both speak.

| Release | Oldest remote protocol | Newest remote protocol |
| ------- | ---------------------: | ---------------------: |
| 0.1.1   |                     19 |                     19 |

Each release's notes start with its newest protocol, such as
`Remote protocol: 19`.

## What works together

| This machine runs     | The host runs           | Result                                                                                                                       |
| --------------------- | ----------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| A newer `mm`          | `mmd` 0.1.1 or later    | Works: `mm` speaks the host's protocol. A command that needs something the host does not have stops before it sends anything. |
| An older `mm`, 0.1.1 or later | A newer `mmd`   | Works while the host still speaks the protocol of that `mm`. A release that stops speaking an old protocol says so in its notes. |
| `mm` from before 0.1.1 | `mmd` 0.1.1 or later   | Refused: update `mm` on this machine.                                                                                        |
| `mm` 0.1.1 or later   | `mmd` from before 0.1.1 | Refused: upgrade the host.                                                                                                   |

You can upgrade your machine and your hosts in either order. Teammates who
deploy to the same hosts can update `mm` one at a time. While the releases
differ, `mm doctor` warns with `MM_HOST_RELEASE_OLDER` (upgrade the hosts) or
`MM_HOST_RELEASE_NEWER` (update this machine) and names the hosts. To make
everyone deploy with one release, pin it with
[`deploy.mm-version`](configuration.md#bindings-environments-and-secrets).

Each node of a cluster agrees on a protocol with `mm` by itself, so nodes on
different releases keep working. Keep them on one release anyway: `mm doctor`
warns with `MM_NODE_VERSION_SKEW` until they are.

## When the two sides share no protocol

`mm` stops with `MM_UNREACHABLE`. The message names the release and the
protocols of each side, and the side to upgrade. A host on 0.1.1 that an older
`mm` connects to says:

```text
This mm speaks remote protocol 18, and the host's mmd 0.1.1 speaks 19. Update mm on this machine to 0.1.1 or newer: run `mm update`, or the installer, https://modmon.dev/install.sh
```

`mm` 0.1.1 connecting to a host from before 0.1.1 says:

```text
target 'production' (203.0.113.10:22) runs an mm from before 0.1.1, which cannot read the hello of this mm 0.1.1 (remote protocol 19). Upgrade the host: run `mm target setup plan --upgrade` and apply that plan with --upgrade
```

Setup reaches a host over SSH as the setup account, not over the remote
protocol, so `mm target setup plan --upgrade` works on a host this `mm` cannot
talk to. See [upgrade mm and your hosts](../how-to/upgrade.md).

## When a command needs a newer host

A command that needs a feature the host's `mmd` does not have yet stops with
`MM_REMOTE_UPGRADE_REQUIRED` before it sends anything:

```text
node 'app-1' (203.0.113.11:22) runs mmd 0.1.1, which does not support <the feature>; upgrade it with `mm target setup plan --upgrade` and apply that plan with --upgrade
```

A host that gets a request it cannot read, such as one from a newer `mm` that
did not check first, answers with the same code and does nothing:

```text
mmd 0.1.1 on this host does not understand `<request>` from mm <release>. Upgrade the host: run `mm target setup plan --upgrade` and apply that plan with --upgrade
```

## Plans

A deployment plan records the protocol of the session that made it. Anyone
whose `mm` shares a protocol with the host can apply it, whichever release
made it. Plans expire after ten minutes, so an upgrade of the host between plan
and apply means planning again.

## How this is checked

| Statement                                                        | Backed by                                                                                                                                                           |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Both sides use the newest protocol they share, or refuse         | `core/tests/protocol_compatibility.rs`: `client_and_daemon_speak_the_newest_protocol_both_have`, `a_daemon_refuses_a_client_it_has_no_protocol_in_common_with`      |
| A host serves a newer `mm` in the protocol both speak            | `core/tests/protocol_compatibility.rs`: `a_daemon_answers_a_newer_client_in_the_protocol_both_speak`                                                                |
| A host answers a request it cannot read and acts on none of it   | `core/tests/protocol_compatibility.rs`: `a_daemon_answers_a_request_it_cannot_read_and_acts_on_none_of_it`                                                          |
| `mm` reads what a newer host adds to its answers                 | `core/tests/protocol_compatibility.rs`: `a_client_reads_what_a_newer_daemon_adds_to_its_answers`                                                                    |
| A command that needs a newer host stops before it sends anything | `core/tests/protocol_compatibility.rs`: `a_request_that_needs_a_newer_protocol_fails_before_it_is_sent`                                                             |
| Every later release reads and writes protocol 19 as 0.1.1 does   | `core/tests/protocol_compatibility.rs`: `frozen_protocol_frames_still_read_and_write_the_same`, with the frames in `core/tests/fixtures/protocol/19/`                |

No test in the repository runs two different releases against each other yet:
the tests above play the other side with frames a newer or an older release
would send.

Contributors changing what crosses the wire: see
[change the remote protocol](../contribute/remote-protocol.md).
