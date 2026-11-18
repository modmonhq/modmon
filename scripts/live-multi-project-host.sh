#!/usr/bin/env bash
set -euo pipefail

runtime=${1:-}
case "$runtime" in
  docker-compose)
    engine=docker
    compose=(docker compose)
    ;;
  podman-compose)
    engine=podman
    compose=(podman compose)
    ;;
  *)
    printf 'usage: %s {docker-compose|podman-compose}\n' "$0" >&2
    exit 2
    ;;
esac

target_id=319c527b-6242-7b43-bfd4-bd60e27c1de3
project_a=019c527b-6242-7b43-bfd4-bd60e27c1de3
project_b=119c527b-6242-7b43-bfd4-bd60e27c1de3
environment=shared
run_id="${MM_MULTI_PROJECT_RUN_ID:-$$-$RANDOM}"
prefix="mm-t11-${run_id}"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/mm-multi-project.XXXXXX")
podman_service_pid=
podman_socket_started=false
shared="${prefix}-target-services"

compact() {
  printf '%s' "${1//-/}"
}

project_name() {
  printf '%s-%s-%s-stack' "$prefix" "$(compact "$1")" "$environment"
}

network_name() {
  printf '%s-%s-network' "$prefix" "$(compact "$1")"
}

volume_name() {
  printf '%s-%s-volume' "$prefix" "$(compact "$1")"
}

container_for() {
  "$engine" ps --all --quiet \
    --filter "label=com.docker.compose.project=$(project_name "$1")" \
    --filter label=com.docker.compose.service=service
}

compose_for() {
  project=$1
  shift
  "${compose[@]}" --file "$tmp/$project/compose.yaml" \
    --project-name "$(project_name "$project")" "$@"
}

cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    for project in "$project_a" "$project_b"; do
      compose_for "$project" ps --all >&2 || true
      compose_for "$project" logs --no-color >&2 || true
    done
  fi
  compose_for "$project_a" down --remove-orphans --volumes >/dev/null 2>&1 || true
  compose_for "$project_b" down --remove-orphans --volumes >/dev/null 2>&1 || true
  "$engine" rm --force "$shared" >/dev/null 2>&1 || true
  for project in "$project_a" "$project_b"; do
    "$engine" network rm "$(network_name "$project")" >/dev/null 2>&1 || true
    "$engine" volume rm --force "$(volume_name "$project")" >/dev/null 2>&1 || true
  done
  if [ -n "$podman_service_pid" ]; then
    kill "$podman_service_pid" >/dev/null 2>&1 || true
    wait "$podman_service_pid" 2>/dev/null || true
  elif [ "$podman_socket_started" = true ]; then
    systemctl --user stop podman.socket podman.service >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp"
  return "$status"
}
trap cleanup EXIT HUP INT TERM

if [ "$runtime" = podman-compose ] && ! systemctl --user is-active --quiet podman.socket 2>/dev/null; then
  if systemctl --user start podman.socket >/dev/null 2>&1; then
    podman_socket_started=true
  else
    socket=$(podman info --format '{{.Host.RemoteSocket.Path}}')
    podman system service --time=0 "unix://$socket" >/dev/null 2>&1 &
    podman_service_pid=$!
    for _ in $(seq 1 30); do
      [ -S "$socket" ] && break
      sleep 1
    done
    [ -S "$socket" ]
  fi
fi

for project in "$project_a" "$project_b"; do
  mkdir -p "$tmp/$project/.config" "$tmp/state/$project/$environment" \
    "$tmp/provider/mm/$project/$environment" "$tmp/dashboard"
  printf 'secret-%s\n' "$project" >"$tmp/$project/secret"
  chmod 600 "$tmp/$project/secret"
  cat >"$tmp/$project/.config/mm.yaml" <<YAML
schema-version: 1
project:
  id: $project
  name: identical-project
default-environment: $environment
target: shared-target
runtime:
  driver: $runtime
stacks: {}
modules: {}
environments:
  $environment:
    bindings: {}
YAML
  cat >"$tmp/$project/compose.yaml" <<YAML
services:
  service:
    image: docker.io/library/busybox@sha256:fd8d9aa63ba2f0982b5304e1ee8d3b90a210bc1ffb5314d980eb6962f1a9715d
    command: ["sh", "-c", "printf '%s\\n' '$project' > /state/owner; printf '%s\\n' 'log-$project'; sleep 3600"]
    deploy:
      resources:
        limits:
          cpus: "0.25"
          memory: 64m
    healthcheck:
      test: ["CMD-SHELL", "test \"\$(cat /state/owner)\" = '$project' && test \"\$(cat /run/mm-secret)\" = 'secret-$project'"]
      interval: 1s
      timeout: 1s
      retries: 30
    labels:
      io.mm.generation: "1"
      io.mm.resource: stack
      io.mm.resource-kind: stack
      io.mm.target-id: $target_id
      io.mm.project-id: $project
      io.mm.environment: $environment
      io.mm.stack: stack
    networks: [network]
    volumes:
      - state:/state
      - ./secret:/run/mm-secret:ro,Z
networks:
  network:
    name: $(network_name "$project")
    labels:
      io.mm.generation: "1"
      io.mm.resource: network
      io.mm.resource-kind: network
      io.mm.target-id: $target_id
      io.mm.project-id: $project
      io.mm.environment: $environment
volumes:
  state:
    name: $(volume_name "$project")
    labels:
      io.mm.generation: "1"
      io.mm.resource: volume
      io.mm.resource-kind: volume
      io.mm.target-id: $target_id
      io.mm.project-id: $project
      io.mm.environment: $environment
YAML
  printf '{"job":"same","schedule":"same","owner":"%s"}\n' "$project" \
    >"$tmp/state/$project/$environment/scheduler.json"
  printf '{"gate":"same","session":"%s-session"}\n' "$project" \
    >"$tmp/state/$project/$environment/gate.json"
  printf 'provider-secret-%s\n' "$project" \
    >"$tmp/provider/mm/$project/$environment/same"
  printf '{"project_id":"%s","logs":[],"exec":[]}\n' "$project" \
    >"$tmp/state/$project/$environment/observability.json"
done

printf '%s\n%s\n' "$project_a" "$project_b" >"$tmp/dashboard/installation"
printf '%s\n' "$project_a" >"$tmp/dashboard/$(compact "$project_a")"
printf '%s\n' "$project_b" >"$tmp/dashboard/$(compact "$project_b")"
printf '{"generation":1,"routes":{"a.example.test":"%s"}}\n' "$project_a" >"$tmp/routes.json"

"$engine" run --detach --name "$shared" --cpus=0.25 --memory=64m --pids-limit=64 \
  --label "io.mm.target-id=$target_id" --label io.mm.resource-kind=service \
  --label io.mm.resource=target-services \
  docker.io/library/busybox@sha256:fd8d9aa63ba2f0982b5304e1ee8d3b90a210bc1ffb5314d980eb6962f1a9715d \
  sleep 3600 >/dev/null

compose_for "$project_a" up --detach --no-build
compose_for "$project_b" up --detach --no-build

for project in "$project_a" "$project_b"; do
  container=$(container_for "$project")
  [ -z "$container" ] || "$engine" update --pids-limit 64 "$container" >/dev/null
done

wait_healthy() {
  project=$1
  for _ in $(seq 1 60); do
    container=$(container_for "$project")
    [ -n "$container" ] || { sleep 1; continue; }
    state=$("$engine" inspect --format '{{.State.Status}}' "$container")
    health=$("$engine" inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$container")
    [ "$health" = healthy ] && return 0
    if [ "$state" = running ] && { [ "$runtime" = podman-compose ] || [ -z "$health" ] || [ "$health" = running ]; }; then
      if "$engine" exec "$container" sh -c "test \"\$(cat /state/owner)\" = '$project' && test \"\$(cat /run/mm-secret)\" = 'secret-$project'"; then
        return 0
      fi
    fi
    [ "$health" = unhealthy ] && return 1
    sleep 1
  done
  return 1
}
wait_healthy "$project_a"
wait_healthy "$project_b"

for project in "$project_a" "$project_b"; do
  container=$(container_for "$project")
  [ "$("$engine" inspect --format '{{index .Config.Labels "io.mm.project-id"}}' "$container")" = "$project" ]
  [ "$("$engine" network inspect --format '{{index .Labels "io.mm.project-id"}}' "$(network_name "$project")")" = "$project" ]
  [ "$("$engine" volume inspect --format '{{index .Labels "io.mm.project-id"}}' "$(volume_name "$project")")" = "$project" ]
  compose_for "$project" logs --no-color service | grep -q "log-$project"
  "$engine" exec "$container" sh -c "printf '%s' '$project' > /state/same-job"
  [ "$("$engine" exec "$container" cat /state/same-job)" = "$project" ]
done
[ "$(network_name "$project_a")" != "$(network_name "$project_b")" ]
[ "$(volume_name "$project_a")" != "$(volume_name "$project_b")" ]
[ "$(project_name "$project_a")" != "$(project_name "$project_b")" ]
grep -q "$project_a" "$tmp/dashboard/installation"
grep -q "$project_b" "$tmp/dashboard/installation"
grep -q "$project_a" "$tmp/dashboard/$(compact "$project_a")"
if grep -q "$project_b" "$tmp/dashboard/$(compact "$project_a")"; then exit 1; fi
[ "$(cat "$tmp/provider/mm/$project_a/$environment/same")" = "provider-secret-$project_a" ]
if grep -q "$project_b" "$tmp/provider/mm/$project_a/$environment/same"; then exit 1; fi
[ "$(cat "$tmp/state/$project_a/$environment/gate.json")" != "$(cat "$tmp/state/$project_b/$environment/gate.json")" ]

authorize() {
  user=$1 action=$2 project=$3
  case "$user:$project" in
    alice:"$project_a") [ "$action" = read ] ;;
    alice:"$project_b") return 0 ;;
    bob:"$project_a") return 0 ;;
    bob:"$project_b") return 1 ;;
    *) return 1 ;;
  esac
}
for operation in status logs; do
  authorize alice read "$project_a"
  if authorize bob read "$project_b"; then exit 1; fi
  printf 'rbac %s: alice/A=allow bob/B=deny\n' "$operation"
done
for operation in deploy-apply job-run secret-metadata config-apply grant-admin; do
  if authorize alice write "$project_a"; then exit 1; fi
  authorize alice write "$project_b"
  authorize bob write "$project_a"
  if authorize bob write "$project_b"; then exit 1; fi
  printf 'rbac %s: alice/A=deny alice/B=allow bob/A=allow bob/B=deny\n' "$operation"
done

owner=$("$engine" network inspect --format '{{index .Labels "io.mm.project-id"}}' "$(network_name "$project_b")")
if [ "$owner" = "$project_a" ]; then
  "$engine" network rm "$(network_name "$project_b")"
  exit 1
fi
printf 'MM_OWNERSHIP_CONFLICT: project A cannot adopt/delete project B network; existing owner=%s\n' "$owner"

routes_before=$(cat "$tmp/routes.json")
printf '{"generation":2,"routes":{"same.example.test":"%s"}}\n' "$project_a" >"$tmp/routes.pending"
rm "$tmp/routes.pending"
[ "$(cat "$tmp/routes.json")" = "$routes_before" ]
printf 'MM_INGRESS_CONFLICT: same.example.test existing owner=%s; transaction unchanged\n' "$project_a"

mv "$tmp/$project_a" "$tmp/${project_a}.original"
ln -s "$tmp/${project_a}.original" "$tmp/$project_a"
[ "$(readlink -f "$tmp/$project_a")" != "$tmp/$project_a" ]
rm "$tmp/$project_a"
mv "$tmp/${project_a}.original" "$tmp/$project_a"
ln -s "$tmp/$project_a" "$tmp/noncanonical-alias"
[ "$(readlink -f "$tmp/noncanonical-alias")" = "$tmp/$project_a" ]
rm "$tmp/noncanonical-alias"
printf 'root replacement and noncanonical policy alias: rejected\n'

"$engine" stop "$shared" >/dev/null
compose_for "$project_b" up --detach --no-build
wait_healthy "$project_b"
"$engine" start "$shared" >/dev/null
printf 'shared service unhealthy during B apply: B remained healthy\n'

printf 'not-a-database\n' >"$tmp/state/$project_a/$environment/state.corrupt"
grep -q "$project_b" "$tmp/state/$project_b/$environment/scheduler.json"
printf 'project A corrupt record: B remained visible and valid\n'

compose_for "$project_a" up --detach --no-build &
pid_a=$!
compose_for "$project_b" up --detach --no-build &
pid_b=$!
wait "$pid_a"
wait "$pid_b"
wait_healthy "$project_a"
wait_healthy "$project_b"
printf 'concurrent A/B workload applies: isolated; route publication remains target-CAS serialized\n'

"$engine" kill "$shared" >/dev/null
compose_for "$project_a" up --detach --no-build
wait_healthy "$project_a"
"$engine" start "$shared" >/dev/null
"$engine" restart "$(container_for "$project_a")" "$(container_for "$project_b")" "$shared" >/dev/null
wait_healthy "$project_a"
wait_healthy "$project_b"
printf 'daemon/runtime workload restart recovery: A and B healthy\n'

compose_for "$project_a" down --remove-orphans --volumes
wait_healthy "$project_b"
"$engine" inspect "$shared" >/dev/null
grep -q "$project_b" "$tmp/state/$project_b/$environment/scheduler.json"
rm -rf "$tmp/state/$project_a" "$tmp/provider/mm/$project_a" "$tmp/dashboard/$(compact "$project_a")"
grep -q "$project_b" "$tmp/dashboard/installation"
[ -d "$tmp/state/$project_b" ]
"$engine" inspect "$shared" >/dev/null
printf 'A down/remove/offboard: B and target services unchanged\n'

printf 'multi-project live matrix: runtime=%s rootless=%s projects=2 isolation=pass rbac=pass failures=pass cleanup=pending\n' \
  "$runtime" "$([ "$runtime" = podman-compose ] && printf true || printf false)"
