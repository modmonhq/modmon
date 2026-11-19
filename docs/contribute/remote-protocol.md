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
  build speaks, `MINIMUM_PROTOCOL_VERSION` the oldest.
- The client's hello says both (`version` and `minimum`) and its release
  (`client_version`). The daemon picks the newest protocol both speak and
  answers with it as the `version` of its hello, or refuses with
  `MM_REMOTE_UPGRADE_REQUIRED` and the side to upgrade.
- Every later frame of the session, both ways and through the relay, has the
  agreed version. The one exception is a read the client sends together with
  its hello, before it knows the answer: it has the version the hello offered.
- The daemon reads a hello leniently, so a newer client's additions do no
  harm, and reads a request strictly: an unknown method or field fails the
  whole request, which the daemon then answers with
  `MM_REMOTE_UPGRADE_REQUIRED` and does not act on.

## Rules

1. Never remove or rename a field, variant, method, result, or stream item
   while `MINIMUM_PROTOCOL_VERSION` is at or below the protocol that has it,
   and never change what an existing one means. Add a new one instead.
2. A new field is `#[serde(default, skip_serializing_if = ...)]`. A peer that
   does not know it never sees it unless it is used, and a newer peer reading
   an older one gets the default.
3. A new method, request field, result, stream item, or frame, or a new value
   of an enum anywhere on the wire, belongs to a new protocol: raise
   `PROTOCOL_VERSION` by one, say what it added in its doc comment, and write
   its fixtures (see below).
4. The client checks before it sends. `RemoteMethod::protocol` returns the
   protocol a request needs, by method and by the fields it uses; a request
   goes only to a host that agreed on at least that protocol, else `mm`
   refuses it before sending and names the host and the upgrade. A request
   that needs more than `MINIMUM_PROTOCOL_VERSION` is never sent with the
   hello. For anything else, use `require_protocol`.
5. The daemon answers in the agreed protocol. `RemoteRequestContext::protocol`
   holds it. An answer never holds a result, stream item, or enum value the
   agreed protocol does not have, and never a new field of a type the client
   reads strictly: plans and their bindings, seals, configuration, and stored
   records. Answer types only a daemon sends, such as `NodeFacts`, are read
   leniently by every client from 0.1.1 on, so a new field there needs no
   check.
6. Every object a request carries rejects unknown fields
   (`#[serde(deny_unknown_fields)]`), so a daemon never acts on a request it
   read only in part. A build receipt is the exception: its hash covers all
   of it.
7. A new field of the hello goes inside `hello`, never beside it, and holds
   nothing a daemon may ignore. Whatever a daemon must not ignore is a
   protocol number.
8. Raising `MINIMUM_PROTOCOL_VERSION` drops every client and host that speaks
   only older protocols. It is a breaking release: the release notes say which
   releases stop working with it and what their users run, and the fixtures of
   the protocols it drops are deleted with it.

## Frozen fixtures

`core/tests/fixtures/protocol/<N>/` holds one frame of each kind, method,
result, and stream item of protocol N, with every field filled, as the release
that introduced N wrote it. They are written once and never edited:

```sh
cargo test --test protocol_compatibility -- --ignored write_protocol_fixtures
```

It writes only the files that are missing, under the current
`PROTOCOL_VERSION`. Run it after raising the protocol, review the new files,
and commit them with the change.

`cargo test --test protocol_compatibility` then checks that the build reads
and writes every frame of every protocol it speaks exactly as frozen, that the
current protocol has a fixture for each frame, method, result, and stream item
the build has, that every object a request carries is read strictly, and how a
daemon answers a newer and an older client. A change that breaks a fixture
breaks the peers that still speak that protocol: change the code, not the
fixture.
