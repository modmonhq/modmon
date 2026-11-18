#!/usr/bin/env bash
# Check the actual pinned file-provider parser, not just generic YAML syntax.
set -euo pipefail
root=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
mkdir -p "$root/.modmon"
work=$(mktemp -d "$root/.modmon/empty-ingress.XXXXXX")
container=
cleanup() {
    [[ -z $container ]] || docker rm -f "$container" >/dev/null 2>&1 || true
    rm -rf "$work"
}
trap cleanup EXIT
cp "$root/core/tests/fixtures/traefik-empty/"*.yaml "$work/"
container=$(docker run -d --cpus=1 --memory=128m --pids-limit=128 \
    -p 127.0.0.1::8080 -v "$work:/fixtures:ro,Z" \
    docker.io/library/traefik@sha256:16acb89c6db341182970d6fdafece31303b0a380a8ed7aa51682e225229bf1d2 \
    --entrypoints.web.address=:8080 --providers.file.directory=/fixtures)
address=$(docker port "$container" 8080)
for _ in {1..30}; do
    response=$(curl --noproxy '*' --silent --max-time 2 --head --header 'Host: proof.invalid' "http://$address/" || true)
    if [[ $response == *'X-Mm-Test-Loaded: yes'* ]]; then
        printf 'Pinned Traefik accepted the empty document and loaded the adjacent route.\n'
        exit 0
    fi
    sleep 0.2
done
docker logs "$container" >&2
printf 'Traefik did not load the route beside the empty configuration.\n' >&2
exit 1
