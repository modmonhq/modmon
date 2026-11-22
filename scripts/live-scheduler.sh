#!/bin/sh
set -eu

runtime=${1:-}
binary=${MM_BIN:-}
case "$runtime" in
  docker-compose) engine=docker ;;
  podman-compose) engine=podman ;;
  *) printf '%s\n' "usage: MM_BIN=/path/to/mm $0 {docker-compose|podman-compose}" >&2; exit 2 ;;
esac
[ -x "$binary" ] || { printf '%s\n' "MM_BIN must name an executable mm binary" >&2; exit 2; }

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

temporary=$(mktemp -d "${TMPDIR:-/tmp}/mm-scheduler-live.XXXXXX")
project="$temporary/project"
home="$temporary/home"
mkdir -p "$project" "$home"

cleanup() {
  "$engine" ps -a --filter name=mm-job-019c527b --format '{{.ID}}' |
    xargs -r "$engine" rm --force >/dev/null 2>&1 || true
  rm -rf "$temporary"
  if [ -n "$podman_service_pid" ]; then
    kill "$podman_service_pid" >/dev/null 2>&1 || true
    wait "$podman_service_pid" 2>/dev/null || true
    rm -f "$podman_socket"
  fi
}
trap cleanup EXIT HUP INT TERM

cat >"$project/compose.yaml" <<'YAML'
services:
  worker:
    image: docker.io/library/busybox@sha256:fd8d9aa63ba2f0982b5304e1ee8d3b90a210bc1ffb5314d980eb6962f1a9715d
    command: ["sleep", "3600"]
    cpus: 0.25
    mem_limit: 64m
    pids_limit: 64
YAML

preview=$(
  "$binary" --root "$project" --mm-home "$home" --output json \
    init --name scheduler-live
)
plan=$(printf '%s' "$preview" | node -e '
let value="";process.stdin.on("data",chunk=>value+=chunk);process.stdin.on("end",()=>{
  const parsed=JSON.parse(value);process.stdout.write(parsed.plan_hash);
});')
"$binary" --root "$project" --mm-home "$home" --output json \
  init --name scheduler-live --plan "$plan" --yes >/dev/null
# The fixture replaces the initialized random project identity with a stable one;
# reset only its temporary state while retaining the initialized local target.
rm -f "$home/state.sqlite3" "$home/state.sqlite3-wal" "$home/state.sqlite3-shm"

cat >"$project/.config/mm.yaml" <<'YAML'
schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: scheduler-live
  development-environment: local
default-environment: local
target: local
runtime: { driver: docker-compose }
development-mail: { enabled: false }
stacks:
  app: { files: [compose.yaml] }
modules:
  fixture:
    path: .
    services:
      worker: { stack: app, role: job }
environments:
  local: {}
jobs:
  print:
    module: fixture
    service: worker
    command: ["printf", "scheduler-live-output\\n"]
    timeout: 30s
    retry: { attempts: 1, backoff: 1s, max-backoff: 1s }
    output-limit-bytes: 4096
    history-limit: 20
  fail:
    module: fixture
    service: worker
    command: ["false"]
    timeout: 30s
    retry: { attempts: 2, backoff: 1s, max-backoff: 1s }
  slow:
    module: fixture
    service: worker
    command: ["sh", "-c", "echo slow-live-started; sleep 120"]
    timeout: 3s
    retry: { attempts: 1, backoff: 1s, max-backoff: 1s }
schedules:
  every-minute:
    job: print
    cron: "* * * * *"
    timezone: UTC
    concurrency: forbid
    misfire: run-once
YAML

common="--root $project --mm-home $home --runtime $runtime --output json"
review=$("$binary" $common schedule reconcile)
reviewed=$(printf '%s' "$review" | node -e '
let value="";process.stdin.on("data",chunk=>value+=chunk);process.stdin.on("end",()=>{
  const parsed=JSON.parse(value);process.stdout.write(parsed.plan_hash);
});')
"$binary" $common schedule reconcile --plan "$reviewed" --yes >/dev/null

manual=$("$binary" $common job run print)
printf '%s' "$manual" | node -e '
let value="";process.stdin.on("data",chunk=>value+=chunk);process.stdin.on("end",()=>{
  const run=JSON.parse(value);
  if(run.state!=="succeeded"||run.exit_code!==0||!run.output.includes("scheduler-live-output")) process.exit(1);
  console.log(`manual run ${run.id} state=${run.state} exit=${run.exit_code}`);
});'

set +e
failure=$("$binary" $common job run fail)
failure_status=$?
set -e
[ "$failure_status" -eq 3 ]
printf '%s' "$failure" | node -e '
let value="";process.stdin.on("data",chunk=>value+=chunk);process.stdin.on("end",()=>{
  const run=JSON.parse(value);
  if(run.state!=="failed"||run.attempt!==2||run.exit_code===0) process.exit(1);
  console.log(`failure run ${run.id} attempts=${run.attempt} state=${run.state}`);
});'

set +e
slow=$("$binary" $common job run slow)
slow_status=$?
set -e
[ "$slow_status" -ne 0 ]
printf '%s' "$slow" | node -e '
let value="";process.stdin.on("data",chunk=>value+=chunk);process.stdin.on("end",()=>{
  const run=JSON.parse(value);
  if(run.state!=="timed_out"||run.error_code!=="MM_JOB_TIMEOUT"||!run.output.includes("slow-live-started")) process.exit(1);
  console.log(`timed-out run ${run.id} state=${run.state} kept its output`);
});'

seconds=$(date -u +%-S)
sleep_for=$((61 - seconds))
sleep "$sleep_for"
count=0
while [ "$count" -lt 10 ]; do
  "$binary" $common schedule tick >/dev/null
  history=$("$binary" $common schedule history every-minute --limit 5)
  if printf '%s' "$history" | node -e '
let value="";process.stdin.on("data",chunk=>value+=chunk);process.stdin.on("end",()=>{
  const runs=JSON.parse(value);
  process.exit(runs.some(run=>run.trigger==="schedule"&&run.state==="succeeded")?0:1);
});'; then
    printf '%s\n' "scheduled run state=succeeded"
    break
  fi
  count=$((count + 1))
  sleep 1
done
[ "$count" -lt 10 ]

remaining=$("$engine" ps -a --filter name=mm-job-019c527b --format '{{.ID}}' | wc -l)
[ "$remaining" -eq 0 ]
printf '%s\n' "runtime=$runtime artifacts=clean durability=sqlite"
