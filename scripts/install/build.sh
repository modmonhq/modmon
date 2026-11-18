#!/usr/bin/env bash
# Runs against an isolated source snapshot, on the host or in the Linux builder.
set -euo pipefail
target=${1:?target required}
with_daemon=${2:?daemon selection required}
profile=${MM_BUILD_PROFILE:-local}
flags=(--profile "$profile")
directory=$profile
if [[ $profile == release ]]; then flags=(--release); fi
case $target in
    *-unknown-linux-musl)
        host=$(rustc -vV | sed -n 's/^host: //p')
        linker=$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld
        test -x "$linker"
        target_key=$(printf '%s' "$target" | tr '[:lower:]-' '[:upper:]_')
        export "CARGO_TARGET_${target_key}_LINKER=$linker"
        export "CARGO_TARGET_${target_key}_RUSTFLAGS=-C link-arg=--threads=1"
        ;;
esac
npm --prefix core/ui ci --no-audit --no-fund
npm --prefix core/ui run build
test -s core/ui/dist/index.html
test -s core/ui/dist/gate/cap_wasm_bg.wasm
# rust-embed reads assets at compile time; Cargo does not track every asset.
touch core/src/lib.rs core/src/main.rs
bins=(--bin mm --bin mm-ingress-helper)
if [[ $with_daemon == 1 ]]; then bins+=(--bin mmd); fi
cargo build --package mm "${bins[@]}" --features unstable "${flags[@]}" --locked --target "$target"
{
    rustc --version
    cargo --version
    printf 'node %s\n' "$(node --version)"
    printf 'npm %s\n' "$(npm --version)"
} > "${CARGO_TARGET_DIR:?}/$target/$directory/build-toolchains.txt"
