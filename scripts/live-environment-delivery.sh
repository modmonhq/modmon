#!/bin/sh
set -eu

runtime=${1:-}
case "$runtime" in
  docker-compose|podman-compose) ;;
  *)
    printf '%s\n' "usage: $0 {docker-compose|podman-compose}" >&2
    exit 2
    ;;
esac

operation="$(date +%s)$$"
placeholder="MM_ENVIRONMENT_DELIVERY_${operation}"
value="MM_DIRECT_ENVIRONMENT_${operation}_VALUE"
project="mm-environment-delivery-${runtime}-${operation}"
root=$(mktemp -d "${TMPDIR:-/tmp}/mm-environment-delivery.XXXXXX")
overlay="$root/direct.compose.yaml"
podman_service_pid=
podman_socket_started=false
chmod 700 "$root"

if [ "$runtime" = podman-compose ]; then
  engine=podman
  compose='podman compose'
else
  engine=docker
  compose='docker compose'
fi

cleanup() {
  env "$placeholder=$value" $compose --file "$overlay" --project-name "$project" down --remove-orphans >/dev/null 2>&1 || true
  if [ -n "$podman_service_pid" ]; then
    kill "$podman_service_pid" >/dev/null 2>&1 || true
    wait "$podman_service_pid" 2>/dev/null || true
  elif [ "$podman_socket_started" = true ]; then
    systemctl --user stop podman.socket podman.service >/dev/null 2>&1 || true
  fi
  rm -rf "$root"
}
trap cleanup EXIT HUP INT TERM

if [ "$runtime" = podman-compose ] && ! systemctl --user is-active --quiet podman.socket 2>/dev/null; then
  if systemctl --user start podman.socket >/dev/null 2>&1; then
    podman_socket_started=true
  else
    socket=$(podman info --format '{{.Host.RemoteSocket.Path}}')
    podman system service --time=0 "unix://$socket" >/dev/null 2>&1 &
    podman_service_pid=$!
    count=0
    while [ ! -S "$socket" ]; do
      count=$((count + 1))
      [ "$count" -lt 30 ] || exit 1
      sleep 1
    done
  fi
fi

cat >"$overlay" <<EOF
services:
  probe:
    image: docker.io/library/busybox@sha256:fd8d9aa63ba2f0982b5304e1ee8d3b90a210bc1ffb5314d980eb6962f1a9715d
    command: ["sh", "-c", "test -n \"\$\$MM_DELIVERED_ENVIRONMENT_VALUE\"; sleep 300"]
    environment:
      MM_DELIVERED_ENVIRONMENT_VALUE: \${$placeholder?mm direct environment value unavailable}
    healthcheck:
      test: ["CMD-SHELL", "test -n \"\$\$MM_DELIVERED_ENVIRONMENT_VALUE\""]
      interval: 1s
      timeout: 1s
      retries: 20
    deploy:
      resources:
        limits:
          cpus: "0.25"
          memory: 64m
          pids: 32
EOF
chmod 600 "$overlay"

if LC_ALL=C grep -F "$value" "$overlay" >/dev/null; then
  printf '%s\n' 'plaintext value reached the placeholder overlay' >&2
  exit 1
fi
if find "$root" -type f \( -name '*.env' -o -name '*.secrets.env' -o -name 'manifest.json' \) -print -quit | grep -q .; then
  printf '%s\n' 'direct delivery created a prohibited environment artifact' >&2
  exit 1
fi

env "$placeholder=$value" $compose --file "$overlay" --project-name "$project" up -d --no-build --remove-orphans >/dev/null
container=$($engine ps --all --quiet --filter "label=com.docker.compose.project=$project")
[ -n "$container" ]
"$engine" update --pids-limit 32 "$container" >/dev/null
metadata=$($engine inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "$container")

case "$metadata" in
  *"MM_DELIVERED_ENVIRONMENT_VALUE=$value"*) ;;
  *)
    printf '%s\n' 'runtime metadata did not receive the process-environment value' >&2
    exit 1
    ;;
esac

if find "$root" -type f \( -name '*.env' -o -name '*.secrets.env' -o -name 'manifest.json' \) -print -quit | grep -q .; then
  printf '%s\n' 'direct delivery created a prohibited artifact during execution' >&2
  exit 1
fi

printf 'runtime=%s mode=direct placeholder=%s files=placeholder-only metadata-exposure=confirmed cleanup=armed\n' \
  "$runtime" "$placeholder"
