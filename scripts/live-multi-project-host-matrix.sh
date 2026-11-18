#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

"$root/scripts/live-multi-project-host.sh" docker-compose
"$root/scripts/live-multi-project-host.sh" podman-compose

printf 'multi-project host matrix complete: docker-compose=pass podman-compose-rootless=pass\n'
