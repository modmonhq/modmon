#!/usr/bin/env sh
set -eu

docker run --rm --cpus=2 --memory=8g --pids-limit=512 \
    -e CARGO_BUILD_JOBS=1 -v "$PWD:/workspace" -w /workspace \
    mm-rust:1.99.1 sh -c \
    'cargo install cargo-deny --version 0.19.0 --locked && cargo deny check'
