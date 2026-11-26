#!/usr/bin/env bash
# Times mm against a remote environment that is already set up and deployed,
# and fails when a bar is missed. It only reads: nothing is planned or applied.
#
#   cd PROJECT && MM_BIN=/path/to/release/mm scripts/live-timing.sh --environment production
#
# Run it from the project, as the user whose device key the environment
# accepts, with a release build: a debug build is several times slower.
# Every line of the result is a median over MM_TIMING_RUNS runs (default 7).
#
#   MM_BIN                      the mm to time (default: mm on PATH)
#   MM_TIMING_RUNS              runs per command (default 7)
#   MM_TIMING_SERVICE           a service to read logs of (default: none, only the log of
#                               every service is read)
#   MM_TIMING_PRINT_BUDGET_MS   --help, --version, completion (default 50)
#   MM_TIMING_FRAME_BUDGET_MS   first frame of the terminal UI (default 150)
#   MM_TIMING_READ_BUDGET_MS    `mm service status` on kept connections (default 500)
#   MM_TIMING_LOG_BUDGET_MS     the first line of `mm service logs --tail 1` on kept
#                               connections (default 500)
#   MM_TIMING_LOADED            text that is on screen once the terminal UI has read the
#                               environment (default "running")
#
# With --deploy it times deployments instead, and changes the environment:
# it swaps the image tag of the first service in each of MM_TIMING_DEPLOY_FILES
# between MM_TIMING_IMAGE_A and MM_TIMING_IMAGE_B, commits, pushes, plans and
# applies, then plans and applies again with nothing changed. Run it from a
# project such as examples/cluster-heavy whose nodes deploy from the pushed
# branch.
#
#   MM_TIMING_DEPLOY_FILES          Compose files to change (default
#                                   "api.yaml auth.yaml search.yaml")
#   MM_TIMING_IMAGE_A, _B           the two tags (default nginx:1.27-alpine and
#                                   nginx:1.26-alpine)
#   MM_TIMING_DEPLOY_BUDGET_MS      `mm deploy apply` of the change (default 60000)
#   MM_TIMING_UNCHANGED_BUDGET_MS   `mm deploy apply` of no change (default 15000)
#
# The read and log budgets depend on the hosts: a read costs the round trips
# to the farthest host plus what its engine takes to answer. The script
# prints the cold and the kept time of a status; the kept one must be the
# smaller. Timing the log of every service needs a line in some service's
# log, and a key that may read every service's log.
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
mm=${MM_BIN:-mm}
runs=${MM_TIMING_RUNS:-7}
print_budget=${MM_TIMING_PRINT_BUDGET_MS:-50}
frame_budget=${MM_TIMING_FRAME_BUDGET_MS:-150}
read_budget=${MM_TIMING_READ_BUDGET_MS:-500}
log_budget=${MM_TIMING_LOG_BUDGET_MS:-500}
loaded=${MM_TIMING_LOADED:-running}
selection=()
deploy=false
while (($# > 0)); do
    case $1 in
        --environment | -e) selection=(--environment "$2"); shift 2 ;;
        --deploy) deploy=true; shift ;;
        *) printf 'usage: %s [--environment NAME] [--deploy]\n' "$0" >&2; exit 2 ;;
    esac
done

# deploy_once LABEL BUDGET_MS: plans, then times `mm deploy apply` of that
# plan, and counts a miss when it is over the budget.
deploy_once() {
    local label=$1 budget=$2 hash started elapsed
    hash=$("$mm" "${selection[@]}" --output json deploy plan | python3 -I -c 'import json, sys; print(json.load(sys.stdin)["plan_hash"])')
    started=$(python3 -I -c 'import time; print(int(time.monotonic() * 1000))')
    "$mm" "${selection[@]}" deploy apply --plan "$hash" --yes >/dev/null
    elapsed=$(($(python3 -I -c 'import time; print(int(time.monotonic() * 1000))') - started))
    if ((elapsed > budget)); then
        printf 'FAIL  %-38s %8d ms   (budget %d ms)\n' "$label" "$elapsed" "$budget"
        return 1
    fi
    printf 'ok    %-38s %8d ms   (budget %d ms)\n' "$label" "$elapsed" "$budget"
}

if $deploy; then
    image_a=${MM_TIMING_IMAGE_A:-nginx:1.27-alpine}
    image_b=${MM_TIMING_IMAGE_B:-nginx:1.26-alpine}
    read -r -a files <<<"${MM_TIMING_DEPLOY_FILES:-api.yaml auth.yaml search.yaml}"
    for file in "${files[@]}"; do
        python3 -I - "$file" "$image_a" "$image_b" <<'PY'
import sys
path, first, second = sys.argv[1:]
text = open(path, encoding="utf-8").read()
at = min((i for i in (text.find(first), text.find(second)) if i >= 0), default=-1)
if at < 0:
    sys.exit(f"{path} uses neither {first} nor {second}")
old, new = (first, second) if text.startswith(first, at) else (second, first)
open(path, "w", encoding="utf-8").write(text[:at] + new + text[at + len(old):])
PY
    done
    git add -- "${files[@]}"
    git commit --quiet -m "Change the image of one service in ${#files[@]} stacks"
    git push --quiet
    failures=0
    deploy_once 'mm deploy apply, a changed image' "${MM_TIMING_DEPLOY_BUDGET_MS:-60000}" || failures=$((failures + 1))
    deploy_once 'mm deploy apply, nothing changed' "${MM_TIMING_UNCHANGED_BUDGET_MS:-15000}" || failures=$((failures + 1))
    if ((failures > 0)); then
        printf '%s bar(s) missed\n' "$failures" >&2
        exit 1
    fi
    printf 'every bar held\n'
    exit 0
fi

failures=0
# median LABEL BUDGET_MS COMMAND...: prints the median wall time and counts a
# miss when it is over the budget (0 means no budget).
median() {
    local label=$1 budget=$2
    shift 2
    python3 -I - "$label" "$budget" "$runs" "$@" <<'PY' || failures=$((failures + 1))
import statistics, subprocess, sys, time
label, budget, runs, command = sys.argv[1], float(sys.argv[2]), int(sys.argv[3]), sys.argv[4:]
times = []
for _ in range(runs):
    started = time.monotonic()
    done = subprocess.run(command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    times.append((time.monotonic() - started) * 1000)
    if done.returncode != 0:
        sys.stderr.write(done.stderr.decode("utf-8", "replace"))
        sys.exit(f"FAIL  {label}: exit {done.returncode}")
value = statistics.median(times)
missed = budget > 0 and value > budget
print(f"{'FAIL' if missed else 'ok  '}  {label:38s} {value:8.1f} ms   (min {min(times):.1f}, max {max(times):.1f}, {runs} runs"
      + (f", budget {budget:.0f} ms)" if budget > 0 else ")"))
sys.exit(1 if missed else 0)
PY
}

# first_line LABEL BUDGET_MS COMMAND...: the same for the time until the
# command prints its first line. The command is left to end by itself.
first_line() {
    local label=$1 budget=$2
    shift 2
    python3 -I - "$label" "$budget" "$runs" "$@" <<'PY' || failures=$((failures + 1))
import statistics, subprocess, sys, time
label, budget, runs, command = sys.argv[1], float(sys.argv[2]), int(sys.argv[3]), sys.argv[4:]
times = []
for _ in range(runs):
    started = time.monotonic()
    process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    line = process.stdout.readline()
    times.append((time.monotonic() - started) * 1000)
    _, errors = process.communicate()
    if process.returncode != 0 or not line:
        sys.stderr.write(errors.decode("utf-8", "replace"))
        sys.exit(f"FAIL  {label}: " + (f"exit {process.returncode}" if process.returncode != 0 else "printed no line"))
value = statistics.median(times)
missed = budget > 0 and value > budget
print(f"{'FAIL' if missed else 'ok  '}  {label:38s} {value:8.1f} ms   (min {min(times):.1f}, max {max(times):.1f}, {runs} runs"
      + (f", budget {budget:.0f} ms)" if budget > 0 else ")"))
sys.exit(1 if missed else 0)
PY
}

printf 'mm: %s (%s)\n' "$("$mm" --version)" "$mm"

median 'mm --version' "$print_budget" "$mm" --version
median 'mm --help' "$print_budget" "$mm" --help
median 'shell completion' "$print_budget" "$mm" __generate completions bash

# One command with reuse off closes what was kept, so the next one is cold.
cold() { MM_SSH_REUSE=off "$mm" "${selection[@]}" team whoami >/dev/null; }
status() { "$mm" "${selection[@]}" service status >/dev/null; }
now_ms() { python3 -I -c 'import time; print(int(time.monotonic() * 1000))'; }

cold
before=$(now_ms)
status
cold_ms=$(($(now_ms) - before))
printf 'ok    %-38s %8.1f ms   (1 run, makes the connections)\n' 'mm service status, first connection' "$cold_ms"
median 'mm service status, kept connections' "$read_budget" "$mm" "${selection[@]}" service status
before=$(now_ms)
status
kept_ms=$(($(now_ms) - before))
if ((kept_ms >= cold_ms)); then
    printf 'FAIL  a status on kept connections (%s ms) is not faster than the first (%s ms)\n' "$kept_ms" "$cold_ms"
    failures=$((failures + 1))
fi
median 'mm team whoami' 0 "$mm" "${selection[@]}" team whoami
median 'mm deploy status' 0 "$mm" "${selection[@]}" deploy status
if "$mm" "${selection[@]}" target nodes >/dev/null 2>&1; then
    median 'mm target nodes' 0 "$mm" "${selection[@]}" target nodes
    if "$mm" "${selection[@]}" target mesh status >/dev/null 2>&1; then
        median 'mm target mesh status' 0 "$mm" "${selection[@]}" target mesh status
    fi
fi
# Logs on kept connections: how long until the first line is there, for one
# service and for every service of the environment.
if [[ -n ${MM_TIMING_SERVICE:-} ]]; then
    first_line 'mm service logs SERVICE, first line' "$log_budget" "$mm" "${selection[@]}" service logs "$MM_TIMING_SERVICE" --tail 1
    median "mm service logs --tail 50" 0 "$mm" "${selection[@]}" service logs "$MM_TIMING_SERVICE" --tail 50
fi
first_line 'mm service logs, first line' "$log_budget" "$mm" "${selection[@]}" service logs --tail 1

# The terminal UI on kept connections: the first frame, then the
# environment read into it.
frame() {
    local label=$1
    python3 -I - "$label" "$frame_budget" "$script_dir/time-terminal-ui.py" "$loaded" "$mm" "${selection[@]}" <<'PY' || failures=$((failures + 1))
import json, statistics, subprocess, sys
label, budget, tool, loaded, command = sys.argv[1], float(sys.argv[2]), sys.argv[3], sys.argv[4], sys.argv[5:]
frames, reads = [], []
for _ in range(3):
    done = subprocess.run([sys.executable, "-I", tool, "--mark", loaded, "--timeout", "60", "--"] + command,
                          stdout=subprocess.PIPE)
    result = json.loads(done.stdout)
    if done.returncode != 0 or result["first_frame_ms"] is None:
        sys.exit(f"FAIL  {label}: the terminal UI did not show {loaded!r}: {result}")
    frames.append(result["first_frame_ms"])
    reads.append(result["marks"][loaded])
frame, read = statistics.median(frames), statistics.median(reads)
missed = frame > budget
print(f"{'FAIL' if missed else 'ok  '}  {label + ', first frame':38s} {frame:8.1f} ms   (3 runs, budget {budget:.0f} ms)")
print(f"ok    {label + ', environment read':38s} {read:8.1f} ms   (3 runs)")
sys.exit(1 if missed else 0)
PY
}
frame 'terminal UI, kept connections'

if ((failures > 0)); then
    printf '%s bar(s) missed\n' "$failures" >&2
    exit 1
fi
printf 'every bar held\n'
