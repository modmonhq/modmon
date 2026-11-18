#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
verify="$script_dir/verify.sh"
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
release="$temporary/release"
mkdir -p "$release"
archive() {
    local top="mm-v1.2.3-$1" stage="$temporary/stage"
    shift
    rm -rf "$stage"
    mkdir -p "$stage/$top"
    for binary in "$@"; do printf '#!/bin/sh\n' >"$stage/$top/$binary"; done
    tar -czf "$release/$top.tar.gz" -C "$stage" "$top"
}
archive x86_64-unknown-linux-gnu mm mmd mm-ingress-helper
archive aarch64-unknown-linux-gnu mm mmd mm-ingress-helper
printf 'source archive\n' >"$release/mm-v1.2.3-source.tar.gz"
(cd "$release" && sha256sum -- mm-v1.2.3-*.tar.gz >SHA256SUMS)

expect_refusal() {
    local expected=$1 output
    shift
    if output=$("$@" 2>&1); then
        printf 'expected a refusal with "%s", got success:\n%s\n' "$expected" "$output" >&2
        exit 1
    fi
    if [[ $output != *"$expected"* ]]; then
        printf 'expected "%s" in:\n%s\n' "$expected" "$output" >&2
        exit 1
    fi
}

"$verify" --checksums-only "$release" >/dev/null

complete="$temporary/complete"
mkdir -p "$complete"
cp "$release"/* "$complete/"
archive aarch64-unknown-linux-gnu mm mmd
(cd "$release" && sha256sum -- mm-v1.2.3-*.tar.gz >SHA256SUMS)
expect_refusal 'mm-v1.2.3-aarch64-unknown-linux-gnu.tar.gz does not hold mm-ingress-helper' "$verify" --checksums-only "$release"
cp "$complete"/* "$release/"
"$verify" --checksums-only "$release" >/dev/null

expect_refusal 'lists no signing key yet' "$verify" "$release" "$script_dir/../../packaging/release/allowed_signers"

allowed="$temporary/allowed_signers"
printf '%s\n' '# a comment' 'release@example.invalid namespaces="mm-release" ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkPDuJzgp2r/scZMqe+noBIAdbQcrLlOG3w6WBNqR+w' >"$allowed"
expect_refusal 'SHA256SUMS.sig is missing; sign with: scripts/release/sign.sh' "$verify" "$release" "$allowed"

printf '%s\n' '-----BEGIN SSH SIGNATURE-----' 'U1NIU0lHAAAAAQ==' '-----END SSH SIGNATURE-----' >"$release/SHA256SUMS.sig"
expect_refusal "SHA256SUMS.sig was not made by a key in $allowed" "$verify" "$release" "$allowed"

printf 'tampered\n' >>"$release/mm-v1.2.3-aarch64-unknown-linux-gnu.tar.gz"
expect_refusal 'mm-v1.2.3-aarch64-unknown-linux-gnu.tar.gz: FAILED' "$verify" --checksums-only "$release"

rm "$release/mm-v1.2.3-x86_64-unknown-linux-gnu.tar.gz"
expect_refusal 'mm-v1.2.3-x86_64-unknown-linux-gnu.tar.gz: FAILED open or read' "$verify" --checksums-only "$release"

expect_refusal 'has no SHA256SUMS' "$verify" --checksums-only "$temporary"
expect_refusal 'Usage: sign.sh DIRECTORY KEY_FILE' "$script_dir/sign.sh" "$release"

printf '%s\n' 'Release verification refused a Linux archive without the ingress helper, a missing or unlisted signature, a changed or missing file, and a missing SHA256SUMS.'
