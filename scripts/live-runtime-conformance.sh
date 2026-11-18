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

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
fixture="$root/core/tests/fixtures/runtime-conformance"
network=mm-runtime-conformance-network
volume=mm-runtime-conformance-volume
foreign=mm-runtime-conformance-foreign-collision
target_id=319c527b-6242-7b43-bfd4-bd60e27c1de3
override=$(mktemp "${TMPDIR:-/tmp}/mm-runtime-conformance-failure.XXXXXX.yaml")
volume_source=$(mktemp "${TMPDIR:-/tmp}/mm-runtime-conformance-volume.XXXXXX")
podman_service_pid=
podman_socket_started=false

if [ "$runtime" = podman-compose ]; then
  engine=podman
  compose='podman compose'
else
  engine=docker
  compose='docker compose'
fi

owned_label() {
  "$engine" "$1" inspect --format "{{ index .Labels \"$2\" }}" "$3" 2>/dev/null
}

require_owned() {
  kind=$1
  name=$2
  resource=$3
  [ "$(owned_label "$kind" io.mm.target-id "$name")" = "$target_id" ] &&
    [ "$(owned_label "$kind" io.mm.resource "$name")" = "$resource" ] &&
    [ "$(owned_label "$kind" io.mm.resource-kind "$name")" = "$kind" ] &&
    [ "$(owned_label "$kind" io.mm.generation "$name")" = 1 ]
}

remove_stack() {
  name=$1
  file=$2
  $compose --file "$fixture/$file.compose.yaml" --project-name "$name" down --remove-orphans >/dev/null 2>&1 || true
}

cleanup() {
  remove_stack mm-runtime-conformance-app app
  remove_stack mm-runtime-conformance-data data
  if "$engine" network inspect "$foreign" >/dev/null 2>&1; then
    "$engine" network rm "$foreign" >/dev/null 2>&1 || true
  fi
  count=0
  while "$engine" network inspect "$network" >/dev/null 2>&1 ||
    "$engine" volume inspect "$volume" >/dev/null 2>&1; do
    if "$engine" volume inspect "$volume" >/dev/null 2>&1 && require_owned volume "$volume" conformance-data; then
      "$engine" volume rm "$volume" >/dev/null 2>&1 || true
    fi
    if "$engine" network inspect "$network" >/dev/null 2>&1 && require_owned network "$network" conformance-network; then
      "$engine" network rm "$network" >/dev/null 2>&1 || true
    fi
    count=$((count + 1))
    [ "$count" -lt 30 ] || break
    sleep 1
  done
  rm -f "$override" "$volume_source"
  if [ -n "$podman_service_pid" ]; then
    kill "$podman_service_pid" >/dev/null 2>&1 || true
    wait "$podman_service_pid" 2>/dev/null || true
  elif [ "$podman_socket_started" = true ]; then
    systemctl --user stop podman.socket podman.service >/dev/null 2>&1 || true
  fi
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

section() {
  printf '\n== %s ==\n' "$1"
}

create_resources() {
  if "$engine" network inspect "$network" >/dev/null 2>&1; then
    require_owned network "$network" conformance-network
  else
    set --
    if [ -n "${MM_CONFORMANCE_SUBNET:-}" ]; then
      set -- --subnet "$MM_CONFORMANCE_SUBNET"
    fi
    "$engine" network create "$@" \
      --label io.mm.generation=1 \
      --label io.mm.resource=conformance-network \
      --label io.mm.resource-kind=network \
      --label "io.mm.target-id=$target_id" "$network" >/dev/null
  fi
  if "$engine" volume inspect "$volume" >/dev/null 2>&1; then
    require_owned volume "$volume" conformance-data
  else
    "$engine" volume create \
      --label io.mm.generation=1 \
      --label io.mm.resource=conformance-data \
      --label io.mm.resource-kind=volume \
      --label "io.mm.target-id=$target_id" "$volume" >/dev/null
  fi
}

deploy_stack() {
  name=$1
  file=$2
  $compose --file "$fixture/$file.compose.yaml" --project-name "$name" up -d --no-build --remove-orphans
  containers=$($engine ps --all --quiet --filter "label=com.docker.compose.project=$name")
  # shellcheck disable=SC2086
  [ -z "$containers" ] || "$engine" update --pids-limit 64 $containers >/dev/null
}

observe_limits() {
  app=$($engine ps --all --quiet --filter label=com.docker.compose.project=mm-runtime-conformance-app)
  data=$($engine ps --all --quiet --filter label=com.docker.compose.project=mm-runtime-conformance-data)
  # shellcheck disable=SC2086
  "$engine" inspect --format '{{.Name}} cpus={{.HostConfig.NanoCpus}} memory={{.HostConfig.Memory}} pids={{.HostConfig.PidsLimit}}' $app $data
}

verify_revision() {
  expected=$1
  container=$($engine ps --quiet \
    --filter label=com.docker.compose.project=mm-runtime-conformance-app \
    --filter label=com.docker.compose.service=api)
  actual=$($engine exec "$container" cat /tmp/revision)
  [ "$actual" = "$expected" ]
  printf 'revision=%s\n' "$actual"
}

observe() {
  $compose --file "$fixture/app.compose.yaml" --project-name mm-runtime-conformance-app ps --all --format json
  $compose --file "$fixture/data.compose.yaml" --project-name mm-runtime-conformance-data ps --all --format json
}

runtime_observability() {
  container=$($engine ps --quiet \
    --filter label=com.docker.compose.project=mm-runtime-conformance-app \
    --filter label=com.docker.compose.service=api)
  if [ "$runtime" = podman-compose ]; then
    logs=$($compose --file "$fixture/app.compose.yaml" --project-name mm-runtime-conformance-app \
      logs --no-color --tail 20 api 2>&1)
  else
    logs=$($compose --file "$fixture/app.compose.yaml" --project-name mm-runtime-conformance-app \
      logs --no-color --no-log-prefix --tail 20 api 2>&1)
  fi
  [ -n "$container" ]
  printf '%s\n' "$logs" | grep -q 'mm-observe-stdout'
  printf '%s\n' "$logs" | grep -q 'mm-observe-stderr'

  literal='a b;$(touch /tmp/mm-observe-injected)'
  actual=$($engine exec "$container" printf '%s|%s' left "$literal")
  [ "$actual" = "left|$literal" ]
  $engine exec "$container" test ! -e /tmp/mm-observe-injected

  set +e
  $engine exec "$container" sh -c 'exit 17'
  exit_code=$?
  set -e
  [ "$exit_code" -eq 17 ]

  if $engine exec mm-runtime-conformance-container-does-not-exist true >/dev/null 2>&1; then
    printf '%s\n' 'missing-container exec unexpectedly succeeded' >&2
    return 1
  fi

  set +e
  if [ "$runtime" = podman-compose ]; then
    timeout -k 2 2 $compose --file "$fixture/app.compose.yaml" --project-name mm-runtime-conformance-app \
      logs --no-color --follow --tail 0 api >/dev/null 2>&1
  else
    timeout -k 2 2 $compose --file "$fixture/app.compose.yaml" --project-name mm-runtime-conformance-app \
      logs --no-color --no-log-prefix --follow --tail 0 api >/dev/null 2>&1
  fi
  follow_code=$?
  set -e
  [ "$follow_code" -eq 124 ] || [ "$follow_code" -eq 137 ]
  printf '%s\n' "logs=stdout+stderr exec=argv-safe exit=$exit_code follow-cancel=$follow_code"
}

wait_running() {
  count=0
  while :; do
    app=$($engine ps --filter label=com.docker.compose.project=mm-runtime-conformance-app --format '{{.Status}}')
    data=$($engine ps --filter label=com.docker.compose.project=mm-runtime-conformance-data --format '{{.Status}}')
    statuses=$(printf '%s\n%s\n' "$app" "$data")
    running=$(printf '%s\n' "$statuses" | grep -c '^Up' || true)
    unhealthy=$(printf '%s\n' "$statuses" | grep -c -E 'unhealthy|starting' || true)
    [ "$running" -eq 2 ] && [ "$unhealthy" -eq 0 ] && return 0
    count=$((count + 1))
    [ "$count" -lt 60 ] || return 1
    sleep 1
  done
}

section versions
if [ "$runtime" = podman-compose ]; then
  podman version
  podman info --format 'rootless={{.Host.Security.Rootless}} socket={{.Host.RemoteSocket.Path}}'
  podman compose version
else
  docker version
  docker info --format 'rootless={{json .SecurityOptions}}'
  docker compose version
fi

section preflight
create_resources
if "$engine" network inspect "$foreign" >/dev/null 2>&1; then
  "$engine" network rm "$foreign" >/dev/null
fi
if [ -n "${MM_CONFORMANCE_COLLISION_SUBNET:-}" ]; then
  "$engine" network create --subnet "$MM_CONFORMANCE_COLLISION_SUBNET" "$foreign" >/dev/null
else
  "$engine" network create "$foreign" >/dev/null
fi
if require_owned network "$foreign" collision-test; then
  printf '%s\n' 'foreign collision was incorrectly accepted' >&2
  exit 1
fi
printf '%s\n' 'MM_OWNERSHIP_CONFLICT: foreign network rejected before mutation'
"$engine" network rm "$foreign" >/dev/null

section volume-delivery
if [ "$runtime" = podman-compose ]; then
  printf '%s' 'atomic-volume-delivery' >"$volume_source"
  mountpoint=$(podman volume inspect --format '{{.Mountpoint}}' "$volume")
  podman unshare install --mode 0600 --no-target-directory -- \
    "$volume_source" "$mountpoint/conformance"
  printf '%s\n' 'Podman atomic volume-file delivery staged'
else
  printf '%s\n' 'MM_UNSUPPORTED: atomic volume-file delivery rejected before mutation'
fi

section render
export MM_CONFORMANCE_REVISION=initial
$compose --file "$fixture/app.compose.yaml" --project-name mm-runtime-conformance-app config
$compose --file "$fixture/data.compose.yaml" --project-name mm-runtime-conformance-data config

section initial-deploy
deploy_stack mm-runtime-conformance-app app
deploy_stack mm-runtime-conformance-data data
wait_running
observe
verify_revision initial
if [ "$runtime" = podman-compose ]; then
  container=$(podman ps --quiet \
    --filter label=com.docker.compose.project=mm-runtime-conformance-app \
    --filter label=com.docker.compose.service=api)
  [ "$(podman exec "$container" cat /data/conformance)" = atomic-volume-delivery ]
  printf '%s\n' 'volume-delivery=atomic-volume-delivery'
fi
section resource-limits
observe_limits

section runtime-observability
runtime_observability

section update
export MM_CONFORMANCE_REVISION=updated
deploy_stack mm-runtime-conformance-app app
deploy_stack mm-runtime-conformance-data data
wait_running
observe
verify_revision updated

cat >"$override" <<'YAML'
services:
  api:
    image: localhost/mm-runtime-conformance-image-that-does-not-exist:failure
    pull_policy: never
YAML

section induced-failure
if $compose --file "$fixture/app.compose.yaml" --file "$override" --project-name mm-runtime-conformance-app up -d --no-build; then
  printf '%s\n' 'failure injection unexpectedly succeeded' >&2
  exit 1
fi

section rollback-recovery
deploy_stack mm-runtime-conformance-app app
deploy_stack mm-runtime-conformance-data data
wait_running
observe
verify_revision updated

section cleanup
cleanup
trap - EXIT HUP INT TERM
if "$engine" network inspect "$network" >/dev/null 2>&1 ||
  "$engine" volume inspect "$volume" >/dev/null 2>&1; then
  printf '%s\n' 'owned resources remain after cleanup' >&2
  exit 1
fi
printf '%s\n' 'cleanup complete: stacks, services, network, volume, and failure fixture removed'
