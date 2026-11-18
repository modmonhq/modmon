#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf '%s\n' \
        'Usage: sign.sh DIRECTORY KEY_FILE' \
        '' \
        'Signs DIRECTORY/SHA256SUMS with ssh-keygen -Y sign -n mm-release, writing' \
        'SHA256SUMS.sig, and verifies it against packaging/release/allowed_signers.' \
        'KEY_FILE is the private key, or its .pub file when the key is in ssh-agent' \
        'or on a security key.' >&2
    exit 2
}

(($# == 2)) || usage
script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
directory=$1
key=$2

"$script_dir/verify.sh" --checksums-only "$directory"
rm -f "$directory/SHA256SUMS.sig"
ssh-keygen -Y sign -f "$key" -n mm-release "$directory/SHA256SUMS"
"$script_dir/verify.sh" "$directory"
