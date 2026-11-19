---
title: Change the remote protocol
description: The rules for changing what mm and mmd send each other, so that every release keeps working with hosts and clients on 0.1.1 and later.
---

`mm` and `mmd` talk over the remote protocol: JSON frames, each with the
protocol `version` it belongs to. Hosts that nobody upgrades keep running the
`mmd` they have, and teammates update `mm` at different times, so a change to
what crosses the wire has to work with releases that are already out. Follow
these rules for any change to the types in `core/src/domain/remote.rs`,
`core/src/domain/remote/`, or any type a request or an answer carries.

## How the two sides agree

- `PROTOCOL_VERSION` in `core/src/domain/remote.rs` is the newest protocol a
  build speaks, `MINIMUM_PROTOCOL_VERSION` the oldest. Protocol 1 is the 0.1.1
  baseline.
- The client's hello says both (`version` and `minimum`) and its release
  (`client_version`). The daemon picks the newest protocol both speak and
  answers with it as the `version` of its hello, or refuses with
  `MM_REMOTE_UPGRADE_REQUIRED` and the side to upgrade. A hello without
  `minimum` comes from a build before 0.1.1 and is refused.
- Every later frame of the session, both ways and through the relay, has the
  agreed version. The one exception is a read the client sends together with
  its hello, before it knows the answer: it has the version the hello offered.
- The daemon reads a hello leniently, so a newer client's additions do no
  harm, and reads a request strictly: an unknown method or field fails the
  whole request. The daemon then answers `MM_REMOTE_UPGRADE_REQUIRED` when
  the client is newer, by protocol or by release, and the reason otherwise,
  and acts on none of it.
- A client reads answers leniently: it leaves out the keys it does not know.

## Rules

1. A change that an older peer can read keeps the protocol number. Adding an
   optional field, a method, or a result for a new method is such a change.
2. A new field is `#[serde(default, skip_serializing_if = ...)]`. A peer that
   does not know it never sees it unless it is used, and a newer peer reading
   an older one gets the default.
3. Answer types ignore keys they do not know: no `deny_unknown_fields` on a
   type only a daemon sends. Types that are also hashed, reviewed or stored
   (plans and their bindings, seals, configuration, stored records) stay
   exact; a new field there is only filled when it is used.
4. Requests stay strict: every object a request carries rejects unknown
   fields (`#[serde(deny_unknown_fields)]`), so a daemon never applies a
   mutation from a request it read only in part. An older daemon refuses a
   request with a new method or a new field whole, with
   `MM_REMOTE_UPGRADE_REQUIRED`, which tells the user to upgrade the host. A
   build receipt is the one exception: its hash covers all of it.
5. A new value of an enum in an answer, or a new kind of answer, reaches only
   a client that can read it: send it only in reply to something only such a
   client sends, such as a new method or a field it set.
6. Never remove or rename a field, variant, method, result, or stream item,
   and never change what an existing one means. That is a breaking wire
   change. It raises `PROTOCOL_VERSION`, the code keeps speaking the old
   protocol for sessions that agreed on it (`RemoteRequestContext::protocol`
   on the daemon, the connection's protocol on the client), and a request
   whose meaning changed names the new protocol in `RemoteMethod::protocol`
   so that it is never sent to a host that agreed on an older one.
7. When `mm` must know before it sends anything that the host can take a
   request, which is rare, it checks with `require_protocol` against the
   session's protocol, which then has to be one that guarantees it.
8. A new field of the hello goes inside `hello`, never beside it, and holds
   nothing a daemon may ignore.
9. `MINIMUM_PROTOCOL_VERSION` rises only with a breaking release, whose notes
   say which releases stop working with it and what their users run. The
   fixtures of the protocols it drops are deleted with it.

## Frozen fixtures

`core/tests/fixtures/protocol/<N>/` holds one frame of each kind, method,
result, and stream item of protocol N, with every field filled, as the build
that added it wrote it. A fixture is added once and never edited:

```sh
cargo test --test protocol_compatibility -- --ignored write_protocol_fixtures
```

It writes only the files that are missing, under the current
`PROTOCOL_VERSION`. Run it after adding a frame, method, result, or stream
item, or after raising the protocol, review the new files, and commit them
with the change.

`cargo test --test protocol_compatibility` then checks that the build reads
and writes every frozen frame of every protocol it speaks exactly as frozen,
that the current protocol has a fixture for each frame, method, result, and
stream item the build has, that every object a request carries is read
strictly, and how a daemon answers a newer and an older client. A change that
breaks a fixture breaks the peers that still speak that protocol: change the
code, not the fixture.
