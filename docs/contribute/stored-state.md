---
title: Change stored state
description: The rules for changing a file or table that one release writes and another reads, and the fixtures that check them.
---

Every release from 0.1.1 on must read the state an earlier release wrote, and
an older release must refuse what it cannot read with a message that names
the newer release's fix. Users see the result in
[compatibility](../reference/compatibility.md#stored-state). Follow these
rules when you change a type that `mm` or `mmd` writes to disk.

## Add a field

Adding a key or a field is not a breaking change: the schema number stays.

- Give it `#[serde(default)]`, or make it an `Option`, so files without it
  still read.
- Do not put `#[serde(deny_unknown_fields)]` on a stored type: an older
  release ignores the field. A reader of a file people read or edit
  (`mmd.json`, `host.yaml`) warns about each key it does not know, with
  `domain::stored_state::unknown_keys`. `mcp.json` refuses one: it only
  narrows what agents may do, and ignoring a key could widen it. Types shared with project
  configuration, such as `EnvironmentAccess` and `AllowRule`, follow that
  configuration's rules, and `MeshState` stays strict so that it refuses a
  private key.
- An older release drops the field when it rewrites the file. If the field
  must survive that, or an older release would act wrongly without it, such
  as grant more than the file allows, it is a breaking change.
- In a record with a hash (plans, seals, build receipts), add the field to
  the hash, and skip it when it holds its default
  (`skip_serializing_if`) so that records written before it keep their
  hash. An older release then refuses a record that sets it instead of
  acting on part of it.

## Make a breaking change

Only a change an older release cannot read raises the file's version field
(`schema`, `schema_version`, `version`, or `format`); every one starts at 1
in 0.1.1. Read every earlier version forward, or refuse it with the command
that writes it again, such as `mm target setup plan --upgrade` for a host
file. Readers call `domain::stored_state::check_schema` before they parse the
rest, so an older release refuses a newer file with `written by a newer mm
(schema N)` and the command that updates it. A type without a version field gets one, read as
1 when it is missing, the first time its format changes; the releases before
it cannot refuse the new format, so that first change must stay one they can
read.

## Change the state database

Append a migration to `MIGRATIONS` in `core/src/adapters/state/sqlite.rs`
with a new name, such as `0002_add_release_notes`, and its SQL in
`core/migrations/`. Never edit a released migration. A migration runs in the
same transaction that records it, the first time a newer `mm` opens the
database, and an older `mm` refuses a database with a migration it does not
know before it changes anything.

A new value in a text column that an older release parses, such as an
operation kind, and a new shape of a JSON column need a migration too, even
an empty one, so that an older release refuses the database instead of
failing on one row.

## Keep the fixtures

`core/tests/fixtures/state/0.1.1/` holds every stored format as 0.1.1 wrote
it, and `core/tests/stored_state.rs` reads each with the current code. Never
change these files: when a test fails on one, make the reader handle it.
When a release is cut, freeze its own fixtures:

```sh
cargo test --all-features --test stored_state -- --ignored write_fixtures
```

It writes `core/tests/fixtures/state/<version>/` and refuses to overwrite one
that exists.
