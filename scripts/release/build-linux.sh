#!/usr/bin/env bash
# shellcheck disable=SC2016
set -euo pipefail

usage() {
    printf '%s\n' \
        'Usage: build-linux.sh TAG [OUTPUT_DIRECTORY]' \
        '       build-linux.sh --check-toolchain' \
        '' \
        'Builds mm, mmd and mm-ingress-helper for x86_64 and aarch64 Linux from the' \
        'tagged commit in Docker, packages them, and writes SHA256SUMS and' \
        'release-notes.md to OUTPUT_DIRECTORY (default: target/dist/TAG).' \
        '--check-toolchain cross-builds a small crate with the same C dependencies' \
        'twice and compares the bytes.' \
        '' \
        'MM_RELEASE_CPUS      CPUs (a whole number) for each build container' \
        'MM_RELEASE_MEMORY    memory limit for each build container, such as 4g' \
        'MM_RELEASE_CARGO_CACHE, MM_RELEASE_NPM_CACHE' \
        '                     Docker volumes that keep downloads between runs' >&2
    exit 2
}

fail() {
    printf 'build-linux: %s\n' "$*" >&2
    exit 1
}

script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
root=$(CDPATH= cd -- "$script_dir/../.." && pwd -P)
dockerfile="$root/packaging/release/linux-builder.Dockerfile"
node_image=node:24-bookworm-slim@sha256:3638d9a6fe4030bd716be989438248074489337ba3275657f93595428be4fc03
targets='x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu'
owner="$(id -u):$(id -g)"
limits=(--pids-limit=2048)
if [[ -n ${MM_RELEASE_CPUS:-} ]]; then
    [[ $MM_RELEASE_CPUS =~ ^[1-9][0-9]*$ ]] || fail "MM_RELEASE_CPUS must be a whole number, not '$MM_RELEASE_CPUS'"
    limits+=(--cpus="$MM_RELEASE_CPUS" -e CARGO_BUILD_JOBS="$MM_RELEASE_CPUS")
fi
[[ -z ${MM_RELEASE_MEMORY:-} ]] || limits+=(--memory="$MM_RELEASE_MEMORY" --memory-swap="$MM_RELEASE_MEMORY")
cargo_cache=${MM_RELEASE_CARGO_CACHE:-mm-release-cargo}
npm_cache=${MM_RELEASE_NPM_CACHE:-mm-release-npm}

builder_image() {
    builder="mm-release-linux:$(sha256sum "$dockerfile" | cut -c1-16)"
    if ! docker image inspect "$builder" >/dev/null 2>&1; then
        printf '==> builder image %s\n' "$builder"
        docker build --platform linux/amd64 --tag "$builder" - <"$dockerfile"
    fi
}

in_builder() {
    local directory=$1 script=$2
    shift 2
    docker run --rm --platform linux/amd64 "${limits[@]}" \
        -e SOURCE_DATE_EPOCH="$epoch" -e CARGO_INCREMENTAL=0 -e OWNER="$owner" -e TARGETS="$targets" "$@" \
        -v "$cargo_cache:/usr/local/cargo/registry" -v "$directory:/build" -w /build "$builder" \
        bash -c 'set -euo pipefail; trap "chown -R \"\$OWNER\" /build" EXIT; '"$script"
}

run_binaries='
run() {
    case "$1" in
        */aarch64-unknown-linux-gnu/*) qemu-aarch64 "$@" ;;
        *) "$@" ;;
    esac
}
glibc() {
    case "$1" in
        */aarch64-unknown-linux-gnu/*) aarch64-linux-gnu-objdump -T "$1" ;;
        *) objdump -T "$1" ;;
    esac | grep -o "GLIBC_[0-9.]*" | sort -uV | tail -n 1
}
'

fresh_directory() {
    rm -rf "$1"
    mkdir -p "$1"
}

check_toolchain() {
    epoch=1
    builder_image
    local work="$root/target/release-work/toolchain" copy target first second
    fresh_directory "$work"
    for copy in first second; do
        mkdir -p "$work/$copy"
        tar --exclude=./target -C "$script_dir/fixtures/cross-check" -cf - . | tar -xf - -C "$work/$copy"
        printf '==> cross-check build (%s)\n' "$copy"
        in_builder "$work/$copy" 'for target in $TARGETS; do cargo build --locked --release --target "$target"; done'
    done
    for target in $targets; do
        first=$(sha256sum <"$work/first/target/$target/release/cross-check" | cut -d ' ' -f 1)
        second=$(sha256sum <"$work/second/target/$target/release/cross-check" | cut -d ' ' -f 1)
        [[ $first == "$second" ]] || fail "$target: two builds of the same source differ ($first, $second)"
        printf '  ok   %s builds the same bytes twice: %s\n' "$target" "$first"
    done
    in_builder "$work/first" "$run_binaries"'
for target in $TARGETS; do
    binary="target/$target/release/cross-check"
    printf "  ok   %s runs: %s (needs %s)\n" "$target" "$(run "$binary")" "$(glibc "$binary")"
done'
    rm -rf "$work"
}

release() {
    local tag=$1 output=${2:-$root/target/dist/$1} identity version commit work
    cd "$root"
    identity=$(node scripts/release/validate-tag.mjs "$tag")
    version=$(node -p 'JSON.parse(process.argv[1]).version' "$identity")
    commit=$(node -p 'JSON.parse(process.argv[1]).commit' "$identity")
    epoch=$(node -p 'JSON.parse(process.argv[1]).source_date_epoch' "$identity")
    mkdir -p "$output"
    output=$(CDPATH= cd -- "$output" && pwd -P)
    [[ -z $(ls -A "$output") ]] || fail "$output is not empty; remove it or pass another directory"
    builder_image
    work="$root/target/release-work/$tag"
    fresh_directory "$work/source"
    git archive --format=tar "$commit" | tar -xf - -C "$work/source"
    rm -rf "$work/source/core/ui/dist"

    printf '==> dashboard assets\n'
    docker run --rm "${limits[@]}" -e OWNER="$owner" \
        -v "$npm_cache:/root/.npm" -v "$work/source:/build" -w /build "$node_image" \
        bash -c 'set -euo pipefail; trap "chown -R \"\$OWNER\" /build" EXIT; npm --prefix core/ui ci --no-audit --no-fund && npm --prefix core/ui run build'

    printf '==> mm, mmd and mm-ingress-helper for %s\n' "$targets"
    in_builder "$work/source" "$run_binaries"'
for target in $TARGETS; do
    cargo build --locked --release --features unstable --package mm --bin mm --bin mmd --bin mm-ingress-helper --target "$target"
done
for target in $TARGETS; do
    for binary in mm mmd mm-ingress-helper; do
        file="target/$target/release/$binary"
        reported=$(run "$file" --version)
        [ "$reported" = "$binary $VERSION" ] || { echo "$file reports \"$reported\", not \"$binary $VERSION\"" >&2; exit 1; }
        printf "  ok   %s %s (needs %s)\n" "$target" "$reported" "$(glibc "$file")"
    done
done' -e VERSION="$version"

    printf '==> packages\n'
    docker run --rm "${limits[@]}" --user "$owner" -e HOME=/tmp -e TAG="$tag" -e TARGETS="$targets" \
        -e SOURCE_DATE_EPOCH="$epoch" -e MM_RELEASE_BINARY_ROOT=/build/target \
        -v "$work/source:/build:ro" -v "$output:/out" -w /build "$node_image" \
        bash -c 'set -euo pipefail; for target in $TARGETS; do bash scripts/release/package-unix.sh "$TAG" "$target" /out; done'

    printf '==> source archive\n'
    "$root/scripts/build-release-source.sh" "$work/source-archive" "$commit" >/dev/null
    mv "$work/source-archive/mm-source.tar.gz" "$output/mm-$tag-source.tar.gz"

    (cd "$work/source" && node scripts/release/release-notes.mjs "$tag") >"$output/release-notes.md"
    (cd "$output" && export LC_ALL=C && sha256sum -- mm-"$tag"-*.tar.gz >SHA256SUMS)
    rm -rf "$work"
    printf '==> %s (%s, SOURCE_DATE_EPOCH=%s)\n' "$output" "$commit" "$epoch"
    cat "$output/SHA256SUMS"
    printf 'Sign it: scripts/release/sign.sh %s KEY_FILE\n' "$output"
}

case "${1:-}" in
    --check-toolchain) (($# == 1)) || usage; check_toolchain ;;
    v*) (($# <= 2)) || usage; release "$@" ;;
    *) usage ;;
esac
