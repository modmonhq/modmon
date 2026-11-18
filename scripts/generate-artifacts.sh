#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
root=$(CDPATH= cd -- "$script_dir/.." && pwd -P)
rust_image=${MM_RUST_IMAGE:-mm-rust:1.98.0}
node_image=node:24-bookworm-slim@sha256:3638d9a6fe4030bd716be989438248074489337ba3275657f93595428be4fc03

if ! docker image inspect "$rust_image" >/dev/null 2>&1; then
    printf 'Missing %s; build Dockerfile.cargo before generating artifacts.\n' "$rust_image" >&2
    exit 2
fi

# The mm binary embeds the built dashboard, so build it before running cargo.
docker run --rm --cpus=2 --memory=4g --pids-limit=512 \
    --user "$(id -u):$(id -g)" -e HOME=/tmp \
    -v "$root:/workspace" -w /workspace "$node_image" \
    sh -c 'npm --prefix core/ui ci && npm --prefix core/ui run build'

docker run --rm --cpus=2 --memory=4g --pids-limit=512 \
    -e CARGO_BUILD_JOBS=2 -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
    -v "$root:/workspace" -w /workspace "$rust_image" \
    sh -c 'cargo run --package mm --bin mm --features unstable --locked -- __generate artifacts --directory /workspace && chown -R "$HOST_UID:$HOST_GID" /workspace/site/public/schema /workspace/docs/reference/cli.md /workspace/contrib /workspace/core/ui/src/dashboard.schema.json'

docker run --rm --cpus=2 --memory=4g --pids-limit=512 \
    --user "$(id -u):$(id -g)" -e HOME=/tmp \
    -v "$root:/workspace" -w /workspace "$node_image" \
    sh -c 'npm --prefix core/ui run generate:api'

printf '%s\n' 'Generated CLI, completion, manpage, schema, descriptor, and dashboard API artifacts.'
