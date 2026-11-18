#!/bin/sh
set -eu

runtime=${1:-docker-compose}
case "$runtime" in
  docker-compose) engine=docker ;;
  podman-compose) engine=podman ;;
  *)
    printf '%s\n' "usage: $0 {docker-compose|podman-compose}" >&2
    exit 2
    ;;
esac

podman_service_pid=
if [ "$runtime" = podman-compose ]; then
  runtime_dir=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
  podman_socket="$runtime_dir/podman/podman.sock"
  if [ -S "$podman_socket" ] && ! podman --remote --url "unix://$podman_socket" info >/dev/null 2>&1; then
    rm -f "$podman_socket"
  fi
  if [ ! -S "$podman_socket" ]; then
    mkdir -p "$runtime_dir/podman"
    podman system service --time=0 "unix://$podman_socket" >/dev/null 2>&1 &
    podman_service_pid=$!
    attempts=0
    while [ ! -S "$podman_socket" ] && [ "$attempts" -lt 10 ]; do
      attempts=$((attempts + 1))
      sleep 1
    done
    podman --remote --url "unix://$podman_socket" info >/dev/null 2>&1 || {
      printf '%s\n' "Podman API socket did not start" >&2
      exit 3
    }
  fi
fi

MM_BIN=${MM_BIN:-target/debug/mm}
root=${TMPDIR:-/tmp}/mm-development-mounts-live-$$
project=$root/project
home=$root/home
plan=$root/mount-plan.json
deploy=$root/deploy-plan.json
compose_project=
overlay=
units=$root/units
units_project=

cleanup_units() {
  [ -n "$units_project" ] || return 0
  "$engine" ps --all --quiet --filter "label=com.docker.compose.project=$units_project" |
    xargs -r "$engine" rm --force >/dev/null 2>&1 || true
  "$engine" volume ls --quiet --filter "label=com.docker.compose.project=$units_project" |
    xargs -r "$engine" volume rm --force >/dev/null 2>&1 || true
  "$engine" network ls --quiet --filter "label=com.docker.compose.project=$units_project" |
    xargs -r "$engine" network rm >/dev/null 2>&1 || true
}

cleanup() {
  cleanup_units
  if [ -n "$compose_project" ] && [ -n "$overlay" ]; then
    "$engine" compose --file "$project/compose.yaml" --file "$overlay" \
      --project-name "$compose_project" down --remove-orphans >/dev/null 2>&1 || true
  fi
  if [ -d "$home" ]; then
    "$MM_BIN" --mm-home "$home" dev tls uninstall >/dev/null 2>&1 || true
    for volume in $("$engine" volume ls --quiet 2>/dev/null); do
      case "$volume" in
        mm-dev-519c527b6242-*) ;;
        *) continue ;;
      esac
      [ "$("$engine" volume inspect --format '{{ index .Labels "io.mm.resource-kind" }}' "$volume" 2>/dev/null)" = volume ] &&
        "$engine" volume rm "$volume" >/dev/null 2>&1 || true
    done
  fi
  rm -rf -- "$root"
  if [ -n "$podman_service_pid" ]; then
    kill "$podman_service_pid" >/dev/null 2>&1 || true
    wait "$podman_service_pid" 2>/dev/null || true
    rm -f "$podman_socket"
  fi
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$project/.config" "$project/app/vendor" "$project/app/storage" \
  "$project/app/bootstrap/cache" "$home"
chmod 700 "$home"
cat >"$project/app/artisan" <<'EOF'
#!/bin/sh
EOF
cat >"$project/app/composer.json" <<'EOF'
{"name":"example/live-mount"}
EOF
printf '%s\n' before >"$project/app/live.txt"
cat >"$project/compose.yaml" <<'YAML'
services:
  web:
    image: docker.io/library/alpine:3.22
    working_dir: /app
    command: ["sh", "-c", "sleep 300"]
    cpus: 1
    mem_limit: 128m
    pids_limit: 64
YAML
cat >"$project/.config/mm.yaml" <<YAML
schema-version: 1
project:
  id: 519c527b-6242-7b43-bfd4-bd60e27c1de3
  name: development-mounts-live
  development-environment: local
default-environment: local
target: local
runtime:
  driver: $runtime
ingress:
  automatic-local: false
development-mail:
  enabled: false
stacks:
  app:
    files: [compose.yaml]
modules:
  app:
    path: app
    services:
      web:
        stack: app
environments:
  local: {}
YAML

"$MM_BIN" --mm-home "$home" dev setup --no-trust >/dev/null
if ! "$MM_BIN" --mm-home "$home" --output json target list | grep -q '"name": "local"'; then
  printf '%s\n' "local target was not initialized" >&2
  exit 1
fi

"$MM_BIN" --root "$project" --mm-home "$home" --output json \
  dev mounts plan app >"$plan"
mount_hash=$(jq -r .plan_hash "$plan")
proposal_hash=$mount_hash
test "$(jq -r .proposal.intent.target "$plan")" = /app
test "$(jq -r '.proposal.intent.preserve[] | select(.path == "/app/vendor") | .path' "$plan")" = /app/vendor
"$MM_BIN" --root "$project" --mm-home "$home" --output json \
  dev mounts configure app --plan "$mount_hash" >/dev/null

"$MM_BIN" --root "$project" --mm-home "$home" --output json deploy plan >"$deploy"
deploy_hash=$(jq -r .plan_hash "$deploy")
first_deploy_hash=$deploy_hash
compose_project=$(jq -r '.stacks[0].compose_project_name' "$deploy")
overlay=$(jq -r '.stacks[0].artifact.generated_overlay_path' "$deploy")
volume=$(jq -r '.preserved_volumes[0].physical_name' "$deploy")
"$MM_BIN" --root "$project" --mm-home "$home" deploy --yes apply --plan "$deploy_hash" >/dev/null
container=$("$engine" ps --quiet --filter "label=com.docker.compose.project=$compose_project" --filter label=com.docker.compose.service=web)
test -n "$container"
test "$("$engine" exec "$container" cat /app/live.txt)" = before
printf '%s\n' after >"$project/app/live.txt"
test "$("$engine" exec "$container" cat /app/live.txt)" = after
test "$("$engine" volume inspect --format '{{ index .Labels "io.mm.resource-kind" }}' "$volume")" = volume

"$MM_BIN" --root "$project" --mm-home "$home" --output json \
  dev mounts plan app >"$plan"
mount_hash=$(jq -r .plan_hash "$plan")
"$MM_BIN" --root "$project" --mm-home "$home" --output json \
  dev mounts disable app --plan "$mount_hash" >/dev/null
"$MM_BIN" --root "$project" --mm-home "$home" --output json deploy plan >"$deploy"
deploy_hash=$(jq -r .plan_hash "$deploy")
cleanup_deploy_hash=$deploy_hash
overlay=$(jq -r '.stacks[0].artifact.generated_overlay_path' "$deploy")
"$MM_BIN" --root "$project" --mm-home "$home" deploy --yes apply --plan "$deploy_hash" >/dev/null
if "$engine" volume inspect "$volume" >/dev/null 2>&1; then
  printf '%s\n' "owned preserved volume was not cleaned after disable" >&2
  exit 1
fi

units_mm() {
  "$MM_BIN" --root "$units" --mm-home "$home" "$@"
}

status_volume() {
  "$engine" volume ls --quiet --filter "label=com.docker.compose.project=$units_project" \
    --filter label=com.docker.compose.volume=status-data
}

running_services() {
  for service in status-queue status-web web; do
    if [ -n "$("$engine" ps --quiet --filter "label=com.docker.compose.project=$units_project" \
      --filter "label=com.docker.compose.service=$service")" ]; then
      printf '%s ' "$service"
    fi
  done
}

mkdir -p "$units/.config" "$units/app" "$units/status" "$units/queue"
cat >"$units/compose.yaml" <<'YAML'
services:
  web:
    image: docker.io/library/alpine:3.22
    command: ["sh", "-c", "sleep 300"]
    mem_limit: 64m
  status-web:
    image: docker.io/library/alpine:3.22
    command: ["sh", "-c", "sleep 300"]
    mem_limit: 64m
    depends_on: [status-queue]
    volumes: [status-data:/data]
  status-queue:
    image: docker.io/library/alpine:3.22
    command: ["sh", "-c", "sleep 300"]
    mem_limit: 64m
volumes:
  status-data: {}
YAML
cat >"$units/.config/mm.yaml" <<YAML
schema-version: 1
project:
  id: 619c527b-6242-7b43-bfd4-bd60e27c1de3
  name: development-units-live
  development-environment: local
default-environment: local
target: local
runtime:
  driver: $runtime
ingress:
  automatic-local: false
development-mail:
  enabled: false
stacks:
  app:
    files: [compose.yaml]
modules:
  app:
    path: app
    services:
      web:
        stack: app
  status:
    path: status
    development: optional
    services:
      status-web:
        stack: app
  queue:
    path: queue
    development: optional
    services:
      status-queue:
        stack: app
jobs:
  status-report:
    module: status
    service: status-web
    run: after-deploy
    command: ["sh", "-c", "date > /data/report"]
environments:
  local: {}
YAML

units_mm --output json deploy plan >"$root/units-plan.json"
test "$(jq -r '[.excluded_services[] | select(.off_locally.development == "optional") | .service] | sort | join(" ")' "$root/units-plan.json")" = "status-queue status-web"
test "$(jq -r '.excluded_services[] | select(.service == "status-web") | .off_locally.unit' "$root/units-plan.json")" = status
units_mm deploy plan >"$root/units-plan.txt"
grep -q 'off locally (optional): status/status-web; turn it on: mm dev enable status' "$root/units-plan.txt"
units_project=$(jq -r '.stacks[0].compose_project_name' "$root/units-plan.json")
units_mm deploy --yes apply --plan "$(jq -r .plan_hash "$root/units-plan.json")" >/dev/null
test "$(running_services)" = "web "

units_mm dev enable status >"$root/units-enable.txt"
grep -q 'queue/status-queue' "$root/units-enable.txt"
grep -q '^    - status$' "$units/.config/mm.overrides.yaml"
units_mm --output json deploy plan >"$root/units-plan.json"
test "$(jq '.excluded_services // [] | length' "$root/units-plan.json")" = 0
units_mm deploy --yes apply --plan "$(jq -r .plan_hash "$root/units-plan.json")" >/dev/null
test "$(running_services)" = "status-queue status-web web "
test -n "$(status_volume)"
"$engine" run --rm -v "$(status_volume):/data" docker.io/library/alpine:3.22 \
  test -s /data/report

units_mm dev disable status --yes >"$root/units-disable.txt" 2>&1
test "$(running_services)" = "web "
test -n "$(status_volume)"
"$engine" run --rm -v "$(status_volume):/data" docker.io/library/alpine:3.22 \
  test -s /data/report
if grep -q development "$units/.config/mm.overrides.yaml"; then
  printf '%s\n' "mm dev disable left the unit in the local overrides" >&2
  exit 1
fi

sed -i 's/^    files: \[compose.yaml\]$/    files: [compose.yaml]\n    ingress:\n      - domain: status\n        service: status-web\n        port: 8080/' "$units/.config/mm.yaml"
sed -i 's/^  local: {}$/  local:\n    domains: {status: status.units-live.test}/' "$units/.config/mm.yaml"
units_mm --output json deploy plan >"$root/units-routes-off.json"
test "$(jq '[.route_diff.added[]? | select(.behavior.service == "status-web")] | length' "$root/units-routes-off.json")" = 0
units_mm --output json dev up --with status >"$root/units-routes-on.json"
test "$(jq -r '.route_diff.added[] | select(.behavior.service == "status-web") | .host' "$root/units-routes-on.json")" = status.units-live.test
test ! -e "$units/.config/mm.overrides.yaml" || ! grep -q status "$units/.config/mm.overrides.yaml"

sed -i 's/^  web:$/  web:\n    depends_on: [status-web]/' "$units/compose.yaml"
if units_mm --output json config validate >"$root/units-validate.json" 2>&1; then
  printf '%s\n' "an always-on service that needs an optional one that is off validated" >&2
  exit 1
fi
grep -q MM_DEVELOPMENT_DEPENDENCY "$root/units-validate.json"
grep -q 'app/web runs in local and needs status/status-web' "$root/units-validate.json"
grep -q 'mm dev enable status' "$root/units-validate.json"
units_mm dev enable status >/dev/null
units_mm --output json config validate >/dev/null
cleanup_units

printf '%s\n' \
  "reviewed Laravel proposal, immediate live edit, owned preservation, disable, and cleanup passed ($runtime)" \
  "optional unit off by default, enabled with its dependency and job, disabled with its volume kept, routes following it, and the dependency refusal passed ($runtime)" \
  "mount plan: $proposal_hash" \
  "live deployment plan: $first_deploy_hash" \
  "cleanup deployment plan: $cleanup_deploy_hash" \
  "owned volume removed: $volume"
