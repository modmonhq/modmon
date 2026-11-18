#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
root=$(CDPATH= cd -- "$script_dir/.." && pwd -P)
output_directory=${1:-/tmp/mm-release-source}
source_ref=${2:-HEAD}
mkdir -p "$output_directory"
output_directory=$(CDPATH= cd -- "$output_directory" && pwd -P)

"$script_dir/check-public-content.sh"

commit=$(git -C "$root" rev-parse "$source_ref^{commit}")
source_date_epoch=$(git -C "$root" show -s --format=%ct "$commit")
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
stage="$temporary/stage"
extracted="$temporary/extracted"
mkdir -p "$stage" "$extracted"

git -C "$root" archive --format=tar "$commit" | tar -xf - -C "$stage"
# Release source rebuilds dashboard assets from core/ui; source archives never
# carry dist output, even if a local build left one behind.
rm -rf "$stage/core/ui/dist"

required=(
    LICENSE README.md SECURITY.md CONTRIBUTING.md CODE_OF_CONDUCT.md
    Cargo.lock site/package-lock.json core/ui/package-lock.json packages/ui/package.json
    docs/index.md site/public/schema/mm.schema.json
    docs/reference/cli.md site/public/schema/dashboard.schema.json contrib/man/mm.1
    packaging/release/manifest.schema.json scripts/release/build-linux.sh packaging/release/install.sh
    contrib/completions/mm.bash examples/README.md packaging/mmd.example.json
)
for path in "${required[@]}"; do
    if [[ ! -f "$stage/$path" ]]; then
        printf 'Release source inventory is missing required file: %s\n' "$path" >&2
        exit 1
    fi
done

prohibited=$(find "$stage" -mindepth 1 \( \
    -name .git -o -name .modmon -o -name node_modules -o -name dist -o \
    -name .nuxt -o -name .output -o \
    -name coverage -o -name test-results -o -name playwright-report -o \
    -name 'mutants.out*' -o -name .mm -o -name tasks -o -name '.task*' -o \
    -name '.env' -o \( -name '.env.*' ! -name '.env.example' \) -o \
    -name credentials.json -o -name secrets.json -o -iname '*agent-report*' \
\) -print -quit)
if [[ -n $prohibited ]]; then
    printf 'Release source inventory contains prohibited path: %s\n' "${prohibited#"$stage/"}" >&2
    exit 1
fi
for build_output in "$stage/target" "$stage/examples/laravel-rust/services/pricing/target"; do
    if [[ -e $build_output ]]; then
        printf 'Release source inventory contains Rust build output: %s\n' "${build_output#"$stage/"}" >&2
        exit 1
    fi
done

"$script_dir/check-public-content.sh" --source-tree "$stage"

archive="$output_directory/mm-source.tar.gz"
inventory="$output_directory/mm-source.inventory.txt"
manifest="$output_directory/mm-source.manifest.json"
checksum="$output_directory/mm-source.tar.gz.sha256"
LC_ALL=C tar --sort=name --mtime="@$source_date_epoch" --owner=0 --group=0 \
    --numeric-owner --format=posix --pax-option=delete=atime,delete=ctime \
    -C "$stage" -cf - . | gzip -n >"$archive"
node "$script_dir/release-manifest.mjs" create "$stage" "$commit" \
    "$source_date_epoch" "$archive" "$inventory" "$manifest" >"$checksum"

tar -xzf "$archive" -C "$extracted"
node "$script_dir/release-manifest.mjs" compare "$stage" "$extracted"
"$script_dir/check-public-content.sh" --source-tree "$extracted"

printf 'Release source archive: %s\n' "$archive"
printf 'Release source commit: %s\n' "$commit"
printf 'Release source SHA-256: %s\n' "$(cut -d ' ' -f 1 "$checksum")"
