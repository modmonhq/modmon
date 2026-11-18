#!/usr/bin/env sh
set -eu

fail() {
    printf '%s\n' "Rust pin check failed: $*" >&2
    exit 1
}

expect_line() {
    file=$1
    line=$2
    grep -Fqx "$line" "$file" || fail "$file does not contain: $line"
}

expect_text() {
    file=$1
    text=$2
    grep -Fq "$text" "$file" || fail "$file does not contain: $text"
}

expect_line rust-toolchain.toml 'channel = "1.98.0"'
expect_line rust-toolchain.toml 'components = ["clippy", "rustfmt"]'
expect_line core/Cargo.toml 'rust-version = "1.98"'
expect_line core/domain/Cargo.toml 'rust-version = "1.98"'
expect_line examples/laravel-rust/services/pricing/Cargo.toml 'rust-version = "1.98"'
expect_line Dockerfile.cargo 'FROM rust:1.98-bookworm@sha256:e70e2eec3d495fd5c8e0be74adda86507dfac7f51a724fbf9813ff59b2b247c7'
expect_line Dockerfile.cargo 'ENV RUSTUP_TOOLCHAIN=1.98.0'
expect_text .github/workflows/rust.yml 'rust-version: "1.98.0"'
expect_text .github/workflows/examples.yml 'RUSTUP_TOOLCHAIN=1.98.0'
expect_text README.md 'Rust 1.98.0'
expect_text CONTRIBUTING.md 'Rust 1.98.0'

action_lines=$(git grep -nE '^[[:space:]]*-[[:space:]]+uses:' -- '.github/workflows/*.yml' || true)
invalid_actions=$(printf '%s\n' "$action_lines" | grep -Ev 'uses: [^[:space:]]+@[0-9a-f]{40}[[:space:]]+# [^[:space:]]+' || true)
[ -z "$invalid_actions" ] || fail "GitHub Actions must use commit SHAs with release comments:\n$invalid_actions"

printf '%s\n' 'Rust package, toolchain, container, CI action, and documentation pins agree.'
