#!/usr/bin/env bash
# Print the peak memory of compiling the mm crate's unit tests.
#
# The crate is large, and the compiler holds all of it in memory at once: the
# unit-test build (the library with its `#[cfg(test)]` modules, what
# `cargo test --lib` compiles) is the largest single step of the Rust checks.
# They run in a container with one CPU and 4 GB, and a build that needs more
# is killed. Run this after adding a lot of code or tests to see how much room
# is left. New tests that need only the public API belong in `core/tests/`,
# where each file group is compiled on its own.
#
# Usage: scripts/test-build-memory.sh [--library] [--domain] [CARGO_OPTION...]
#
#   --library      Measure the library alone (`cargo build --lib`) instead: the
#                  part of the peak that no test moved out of the crate lowers.
#                  Everything that links the library is rebuilt afterwards.
#   --domain       Measure the `mm-domain` crate (core/src/domain, compiled
#                  on its own) instead of `mm`.
#   CARGO_OPTION   Passed to cargo before its subcommand, for example
#                  --config 'profile.dev.package.mm.codegen-units=16'.
#
# The build runs without debug info for the two crates, with 256 codegen units and
# without incremental compilation: the settings that need the least memory.
# Debug info and fewer, larger codegen units both raise the peak.
#
# Environment:
#   MM_BUILD_MEMORY        Memory limit of the container (default: 4g).
#   MM_BUILD_DOCKER_ARGS   More `docker run` arguments, separated by spaces,
#                          such as a volume that caches the crate registry.
#
# Two numbers are printed, both the container's anonymous memory, read once a
# second from its control group (file cache is left out: the kernel drops it
# before it kills anything):
#
#   peak    The most the build held. This is what the limit kills. It falls in
#           the first seconds of code generation and depends on which codegen
#           units are in work together: runs of one tree differ by 50 MiB or so.
#   before code generation
#           What the compiler held when it wrote its first object file: the
#           whole analysed crate. Runs of one tree differ by about 20 MiB, so
#           compare this number between two commits.
#
# Needs the `mm-rust:1.99.1` image (see CONTRIBUTING.md) and control groups v2.
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
image=mm-rust:1.99.1
limit=${MM_BUILD_MEMORY:-4g}

target='test'
package='mm'
while [[ ${1:-} == --library || ${1:-} == --domain ]]; do
    if [[ $1 == --library ]]; then target='library'; else package='mm-domain'; fi
    shift
done
if [[ ${1:-} == -h || ${1:-} == --help ]]; then
    sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed -e '$d' -e 's/^# \{0,1\}//'
    exit 0
fi

if ! docker image inspect "$image" >/dev/null 2>&1; then
    printf 'Missing image %s. Build it with: docker build -f Dockerfile.cargo -t %s .\n' \
        "$image" "$image" >&2
    exit 1
fi

docker_args=()
if [[ -n ${MM_BUILD_DOCKER_ARGS:-} ]]; then
    read -r -a docker_args <<<"$MM_BUILD_DOCKER_ARGS"
fi

# Runs in the container. Cargo skips a build whose inputs did not change, so
# the record that the target is up to date is removed first.
# shellcheck disable=SC2016
measure='
set -u
target=$1
package=$2
shift 2
crate=$(echo "$package" | tr - _)
stat=/sys/fs/cgroup/memory.stat
if [ ! -r "$stat" ]; then
    echo "This needs control groups v2: $stat is not readable in the container." >&2
    exit 1
fi
case $target in
    test) record=test-lib-$crate; set -- "$@" test --lib --no-run ;;
    library) record=lib-$crate; set -- "$@" build --lib ;;
esac
rm -rf target/debug/.fingerprint/"$package"-*/"$record" target/debug/.fingerprint/"$package"-*/"$record".json

log=$(mktemp)
marker=$(mktemp)
samples=$(mktemp)
echo "0 0" > "$samples"
(
    peak=0
    before=0
    anon=0
    while :; do
        while read -r key value; do
            if [ "$key" = anon ]; then anon=$value; fi
        done < "$stat"
        if [ "$anon" -gt "$peak" ]; then peak=$anon; fi
        # An object file newer than the marker is one of this build.
        if [ "$before" -eq 0 ] &&
            [ -n "$(find target/debug/deps -name "$crate-*.rcgu.o" -newer "$marker" | head -n 1)" ]; then
            before=$anon
        fi
        echo "$peak $before" > "$samples"
        sleep 1
    done
) &
sampler=$!

started=$(date +%s)
cargo "$@" --package "$package" --all-features --locked > "$log" 2>&1
status=$?
seconds=$(( $(date +%s) - started ))
kill "$sampler" 2>/dev/null
wait "$sampler" 2>/dev/null

read -r peak before < "$samples" || { peak=0; before=0; }
peak=$(( peak / 1048576 ))
before=$(( before / 1048576 ))
limit=$(cat /sys/fs/cgroup/memory.max)
killed=0
while read -r key value; do
    if [ "$key" = oom_kill ]; then killed=$value; fi
done < /sys/fs/cgroup/memory.events

if [ "$limit" = max ]; then
    of=""
else
    limit=$(( limit / 1048576 ))
    of=" of $limit MiB"
fi
if [ "$killed" -gt 0 ]; then
    echo "$package $target build: killed at the memory limit after $seconds s (peak $peak MiB$of)"
    exit 1
fi
if [ "$status" -ne 0 ]; then
    tail -n 40 "$log" >&2
    echo "$package $target build: cargo failed after $seconds s (peak $peak MiB$of)" >&2
    exit "$status"
fi
if ! grep -q "Compiling $package " "$log"; then
    echo "$package $target build: cargo found nothing to compile, so there is nothing to measure." >&2
    exit 1
fi
if [ -n "$of" ]; then
    of="$of, $(( limit - peak )) MiB free"
fi
echo "$package $target build: peak $peak MiB$of; $before MiB before code generation; $seconds s"
'

exec docker run --rm --cpus=1 --memory="$limit" --memory-swap="$limit" --pids-limit=512 \
    -e CARGO_BUILD_JOBS=1 -e CARGO_INCREMENTAL=0 \
    ${docker_args[@]+"${docker_args[@]}"} \
    -v "$root:/workspace" -w /workspace "$image" \
    sh -c "$measure" measure "$target" "$package" \
    --config 'profile.dev.package.mm.debug=false' \
    --config 'profile.dev.package.mm.codegen-units=256' \
    --config 'profile.dev.package.mm-domain.debug=false' \
    --config 'profile.dev.package.mm-domain.codegen-units=256' \
    "$@"
