#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source_ref=${1:-HEAD}
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
mkdir -p "$temporary/first" "$temporary/second"

"$script_dir/build-release-source.sh" "$temporary/first" "$source_ref" >/dev/null
"$script_dir/build-release-source.sh" "$temporary/second" "$source_ref" >/dev/null
cmp "$temporary/first/mm-source.tar.gz" "$temporary/second/mm-source.tar.gz"
cmp "$temporary/first/mm-source.inventory.txt" "$temporary/second/mm-source.inventory.txt"
cmp "$temporary/first/mm-source.manifest.json" "$temporary/second/mm-source.manifest.json"
cmp "$temporary/first/mm-source.tar.gz.sha256" "$temporary/second/mm-source.tar.gz.sha256"

printf '%s\n' 'Release source archive is deterministic and extraction-safe.'
