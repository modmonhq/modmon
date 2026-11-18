#!/bin/sh
set -eu

runtime=${1:-}
case "$runtime" in
  podman|docker-compose) ;;
  *)
    printf '%s\n' "usage: $0 {podman|docker-compose}" >&2
    exit 2
    ;;
esac

operation="$(date +%s)$$"
root=$(mktemp -d "${TMPDIR:-/tmp}/mm-secret-delivery.XXXXXX")
project="mm-secret-delivery-${runtime}-${operation}"
v1="${project}-v1"
v2="${project}-v2"
collision="${project}-collision"
cas="${project}-cas"
sentinel="MM_SECRET_DELIVERY_${runtime}_${operation}_PLAINTEXT"
next="${sentinel}_ROTATED"
digest1=$(printf '%s' "$sentinel" | sha256sum | cut -d ' ' -f 1)
digest2=$(printf '%s' "$next" | sha256sum | cut -d ' ' -f 1)
container=
chmod 700 "$root"

cleanup() {
  if [ "$runtime" = podman ]; then
    [ -z "$container" ] || podman rm --force "$container" >/dev/null 2>&1 || true
    podman secret rm "$v1" "$v2" "$collision" "$cas" >/dev/null 2>&1 || true
  else
    containers=$(docker ps --all --quiet --filter "label=com.docker.compose.project=$project" 2>/dev/null || true)
    # shellcheck disable=SC2086
    [ -z "$containers" ] || docker rm --force $containers >/dev/null 2>&1 || true
    docker compose --file "$root/compose.yaml" --project-name "$project" down --remove-orphans >/dev/null 2>&1 || true
  fi
  rm -rf "$root"
}
trap cleanup EXIT HUP INT TERM

assert_absent() {
  path=$1
  if [ -e "$path" ] && LC_ALL=C grep -F "$sentinel" "$path" >/dev/null 2>&1; then
    printf 'plaintext sentinel found in %s\n' "$path" >&2
    exit 1
  fi
}

if [ "$runtime" = podman ]; then
  printf '%s' foreign | podman secret create "$collision" - >/dev/null
  if [ "$(podman secret inspect --format '{{index .Spec.Labels "io.mm.project-id"}}' "$collision")" = secret-delivery-test ]; then
    printf '%s\n' 'foreign collision incorrectly passed ownership' >&2
    exit 1
  fi
  printf '%s\n' 'ownership-collision=rejected-before-mutation'
  podman secret rm "$collision" >/dev/null

  left=0
  right=0
  (printf '%s' "$sentinel" | podman secret create --label io.mm.project-id=secret-delivery-test "$cas" - >/dev/null 2>&1) &
  left_pid=$!
  (printf '%s' "$sentinel" | podman secret create --label io.mm.project-id=secret-delivery-test "$cas" - >/dev/null 2>&1) &
  right_pid=$!
  wait "$left_pid" || left=$?
  wait "$right_pid" || right=$?
  if { [ "$left" -eq 0 ] && [ "$right" -ne 0 ]; } || { [ "$left" -ne 0 ] && [ "$right" -eq 0 ]; }; then
    :
  else
    exit 1
  fi
  podman secret rm "$cas" >/dev/null
  printf '%s\n' 'concurrent-create=single-winner'

  printf '%s' "$sentinel" | podman secret create \
    --label io.mm.project-id=secret-delivery-test \
    --label io.mm.logical-secret=database.password \
    --label io.mm.source-revision-digest=v1 "$v1" - >/dev/null
  podman secret inspect --format '{{.Spec.Name}} labels={{json .Spec.Labels}}' "$v1"
  container=$(podman run --detach --rm --name "${project}-probe" \
    --network none --cpus 0.25 --memory 64m --pids-limit 32 --read-only \
    --secret "source=$v1,target=probe,uid=0,gid=0,mode=0400" \
    docker.io/library/busybox@sha256:fd8d9aa63ba2f0982b5304e1ee8d3b90a210bc1ffb5314d980eb6962f1a9715d \
    sleep 300)
  actual=$(podman exec "$container" sha256sum /run/secrets/probe | cut -d ' ' -f 1)
  [ "$actual" = "$digest1" ]
  [ "$(podman exec "$container" stat -c '%a:%u:%g' /run/secrets/probe)" = 400:0:0 ]
  podman inspect "$container" >"$root/engine-state.json"
  podman logs "$container" >"$root/container.log" 2>&1
  podman secret inspect "$v1" >"$root/secret-metadata.json"
  ps -eo args >"$root/processes.txt"
  assert_absent "$root/engine-state.json"
  assert_absent "$root/container.log"
  assert_absent "$root/secret-metadata.json"
  assert_absent "$root/processes.txt"
  podman rm --force "$container" >/dev/null
  container=

  printf '%s' "$next" | podman secret create \
    --label io.mm.project-id=secret-delivery-test \
    --label io.mm.logical-secret=database.password \
    --label io.mm.source-revision-digest=v2 "$v2" - >/dev/null
  for revision in "$v2:$digest2" "$v1:$digest1"; do
    name=${revision%%:*}
    expected=${revision#*:}
    actual=$(podman run --rm --network none --cpus 0.25 --memory 64m --pids-limit 32 \
      --secret "source=$name,target=probe,uid=0,gid=0,mode=0400" \
      docker.io/library/busybox@sha256:fd8d9aa63ba2f0982b5304e1ee8d3b90a210bc1ffb5314d980eb6962f1a9715d \
      sha256sum /run/secrets/probe | cut -d ' ' -f 1)
    [ "$actual" = "$expected" ]
  done
  printf '%s\n' 'rotation=versioned rollback=verified permissions=0400:0:0'
  podman secret rm "$v1" "$v2" >/dev/null

else
  secret_file="$root/operation.secret"
  printf '%s' "$sentinel" >"$secret_file"
  chmod 600 "$secret_file"
  cat >"$root/compose.yaml" <<EOF
services:
  probe:
    image: docker.io/library/busybox@sha256:fd8d9aa63ba2f0982b5304e1ee8d3b90a210bc1ffb5314d980eb6962f1a9715d
    command: ["sh", "-c", "sha256sum /run/secrets/probe | cut -d ' ' -f 1 > /tmp/digest; sleep 300"]
    secrets:
      - source: probe_secret
        target: probe
    healthcheck:
      test: ["CMD-SHELL", "test -s /tmp/digest"]
      interval: 1s
      timeout: 1s
      retries: 20
    deploy:
      resources:
        limits:
          cpus: "0.25"
          memory: 64m
          pids: 32
secrets:
  probe_secret:
    file: $secret_file
EOF
  chmod 600 "$root/compose.yaml"
  assert_absent "$root/compose.yaml"
  docker compose --file "$root/compose.yaml" --project-name "$project" up -d --no-build >/dev/null
  container=$(docker ps --quiet --filter "label=com.docker.compose.project=$project")
  [ -n "$container" ]
  docker inspect "$container" >"$root/engine-state.json"
  docker logs "$container" >"$root/container.log" 2>&1
  ps -eo args >"$root/processes.txt"
  assert_absent "$root/engine-state.json"
  assert_absent "$root/container.log"
  assert_absent "$root/processes.txt"
  [ "$(docker exec "$container" cat /tmp/digest)" = "$digest1" ]
  [ "$(stat -c '%a' "$secret_file")" = 600 ]
  rm -f "$secret_file"
  [ ! -e "$secret_file" ]
  [ "$(docker exec "$container" cat /tmp/digest)" = "$digest1" ]
  printf '%s\n' 'file-mode=0600 operation-cleanup=verified mounted-inode-lifetime=container'
  docker rm --force "$container" >/dev/null
  container=

  failed="$root/failed.secret"
  printf '%s' "$next" >"$failed"
  chmod 600 "$failed"
  cat >"$root/failure.yaml" <<EOF
services:
  probe:
    image: localhost/mm-secret-delivery-image-does-not-exist:failure
    pull_policy: never
secrets:
  probe_secret:
    file: $failed
EOF
  chmod 600 "$root/failure.yaml"
  assert_absent "$root/failure.yaml"
  if docker compose --file "$root/compose.yaml" --file "$root/failure.yaml" --project-name "$project" up -d --no-build >/dev/null 2>&1; then
    printf '%s\n' 'Compose failure injection unexpectedly succeeded' >&2
    exit 1
  fi
  rm -f "$failed"
  [ ! -e "$failed" ]
  printf '%s\n' 'failure-interruption-cleanup=verified'
fi

assert_absent "$root/compose.yaml"
printf 'runtime=%s plaintext=absent-from-yaml-engine-state-logs-process-snapshot cleanup=complete\n' "$runtime"
cleanup
trap - EXIT HUP INT TERM
