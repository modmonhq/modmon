# `mm` core

This package contains the Rust binary and library for `mm`. It is one member of
the repository's virtual Cargo workspace.

Run package and workspace commands from the repository root:

```sh
docker run --rm --cpus=2 --memory=4g --pids-limit=512 \
  -e CARGO_BUILD_JOBS=2 -e RUST_TEST_THREADS=2 \
  -v "$PWD:/workspace" -w /workspace mm-rust:1.99.1 \
  cargo test --workspace --all-targets --all-features --locked
```

The supported library API is the set of types re-exported from the crate root:
`Application`, its request and response types, and `MmError`. Internal domain,
application, adapter, and presentation modules are available only through the
`unstable` feature and have no compatibility guarantee. The binary target uses
those internal modules, so source builds select that feature explicitly:

```sh
docker run --rm --cpus=2 --memory=4g --pids-limit=512 \
  -e CARGO_BUILD_JOBS=2 -v "$PWD:/workspace" -w /workspace \
  mm-rust:1.99.1 cargo run -p mm --features unstable -- --help
```

This boundary changes Rust source visibility only. It does not change project
configuration, persisted state, CLI output, or plan serialization.

Project documentation, current capability claims, and architecture decisions
are maintained in the root [documentation](../docs/index.md).
See the [capability matrix](../docs/reference/capability-matrix.md)
before relying on a runtime feature.
