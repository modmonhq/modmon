#!/usr/bin/env bash
set -euo pipefail

tag=${1:?usage: package-unix.sh TAG TARGET OUTPUT_DIRECTORY}
target=${2:?usage: package-unix.sh TAG TARGET OUTPUT_DIRECTORY}
output=${3:?usage: package-unix.sh TAG TARGET OUTPUT_DIRECTORY}
binary_root=${MM_RELEASE_BINARY_ROOT:-target}
[[ $tag =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-rc\.(0|[1-9][0-9]*))?$ ]] || { printf 'invalid release tag: %s\n' "$tag" >&2; exit 2; }
case "$target" in
    x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|x86_64-apple-darwin|aarch64-apple-darwin) ;;
    *) printf 'unsupported Unix release target: %s\n' "$target" >&2; exit 2 ;;
esac
version=${tag#v}
epoch=${SOURCE_DATE_EPOCH:?SOURCE_DATE_EPOCH is required}
archive="mm-$tag-$target.tar.gz"
top="mm-$tag-$target"
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
stage="$temporary/$top"
mkdir -p "$stage/completions" "$stage/man"
binaries=(mm mmd)
[[ $target != *-unknown-linux-gnu ]] || binaries+=(mm-ingress-helper)
for binary in "${binaries[@]}"; do
    install -m 0755 "$binary_root/$target/release/$binary" "$stage/$binary"
done
install -m 0644 LICENSE README.md "$stage/"
install -m 0644 contrib/completions/mm.* "$stage/completions/"
install -m 0644 contrib/man/mm.1 "$stage/man/mm.1"
mkdir -p "$output"
tar_command=tar
command -v gtar >/dev/null 2>&1 && tar_command=gtar
LC_ALL=C "$tar_command" --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
    --format=posix --pax-option=delete=atime,delete=ctime -C "$temporary" -cf - "$top" | gzip -n >"$output/$archive"
node ./scripts/release/package-metadata.mjs "$output/$archive" "$target" tar.gz "${binaries[@]}" >"$output/$target.metadata.json"
printf 'packaged %s (%s)\n' "$archive" "$version"
