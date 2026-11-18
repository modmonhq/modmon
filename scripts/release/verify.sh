#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf '%s\n' \
        'Usage: verify.sh [--checksums-only] DIRECTORY [ALLOWED_SIGNERS]' \
        '' \
        'Checks every file SHA256SUMS in DIRECTORY lists, that each Linux archive' \
        'holds mm, mmd and mm-ingress-helper, then that SHA256SUMS.sig' \
        'is a signature over SHA256SUMS in the mm-release namespace by a key in' \
        'ALLOWED_SIGNERS (default: packaging/release/allowed_signers).' >&2
    exit 2
}

fail() {
    printf 'verify: %s\n' "$*" >&2
    exit 1
}

script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
signature=1
if [[ ${1:-} == --checksums-only ]]; then
    signature=0
    shift
fi
(($# == 1 || $# == 2)) || usage
directory=$1
allowed=${2:-$script_dir/../../packaging/release/allowed_signers}

[[ -s $directory/SHA256SUMS ]] || fail "$directory has no SHA256SUMS"
(cd "$directory" && sha256sum --check --strict SHA256SUMS) || fail "a file in $directory does not match SHA256SUMS"
while read -r _ name; do
    name=${name#\*}
    [[ $name == *-unknown-linux-gnu.tar.gz ]] || continue
    listing=$(tar -tzf "$directory/$name") || fail "$name is not a readable archive"
    for binary in mm mmd mm-ingress-helper; do
        grep -qx "${name%.tar.gz}/$binary" <<<"$listing" || fail "$name does not hold $binary"
    done
done <"$directory/SHA256SUMS"
((signature)) || exit 0

grep -Eq '^[^#[:space:]]' "$allowed" || fail "$allowed lists no signing key yet; add the release key's line to it"
[[ -s $directory/SHA256SUMS.sig ]] || fail "$directory/SHA256SUMS.sig is missing; sign with: scripts/release/sign.sh $directory KEY_FILE"
principal=$(ssh-keygen -Y find-principals -f "$allowed" -s "$directory/SHA256SUMS.sig" 2>/dev/null | head -n 1) ||
    fail "SHA256SUMS.sig was not made by a key in $allowed"
[[ -n $principal ]] || fail "SHA256SUMS.sig was not made by a key in $allowed"
ssh-keygen -Y verify -f "$allowed" -I "$principal" -n mm-release -s "$directory/SHA256SUMS.sig" <"$directory/SHA256SUMS" ||
    fail "SHA256SUMS.sig does not verify SHA256SUMS for $principal in the mm-release namespace"
