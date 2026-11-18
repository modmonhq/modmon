#!/usr/bin/env bash
# Live cluster deployment: three Compose hosts, each running sshd, the mm
# relay, mmd, and its own Docker engine, addressed as the nodes of one
# `nodes:` target. Covers node setup, `mm target nodes`, one plan with a
# section per node under one hash, apply, status that lists a service once
# with its node and container, logs and exec on the node that runs the
# service, access sync on every node, a request that reaches the wrong
# node, and planning for a subset while one node's mmd is stopped. Then
# placement: a service on every node, one
# pinned, one on a group, a relative bind mount on every node, a service
# moved between nodes (started on the new node before it stops on the old
# one), a node emptied of its last service, and Compose keys that only
# Docker Swarm reads refused with what to write instead. And reads across
# nodes: the status lists each service once with a row per node and the
# deployment the nodes run, the log of a service on three nodes (or of every
# service) is one stream in the order the lines were written, `--node` reads
# some nodes, `--follow` shows lines as they are written, a node whose mmd
# is stopped or hangs is one line in the stream and a note in the status
# while the others answer, a line of 100 000 characters is cut and marked
# or arrives whole, and a container that ran once and exited does not make
# its service degraded. And the terminal UI, driven on a pseudo-terminal by
# scripts/drive-terminal-ui.py: it draws at once and lists each service with
# a row for every node, follows one node's container or every node's in
# order, shows each node's step while a deployment it applied runs, and
# marks a node whose mmd hangs while its keys still work. And the mesh: each
# node's mmd makes its own WireGuard key and brings up mm0, `mm target mesh
# status` shows every link and names the port to open for a pair that is
# blocked, a second apply changes nothing, a changed peer leaves the others
# alone, and the private key shows up in no argument, output, or log. And
# networks: a network of mm.yaml spans the nodes that use it, containers on
# different nodes reach each other only on a network they share, an internal
# network has no way out, nothing reaches a container from outside the mesh,
# a node that already uses part of the address space fails the plan with the
# key to set, and a rule removed by hand is reported, planned, and repaired.
# And names:
# a service name resolves to the service's healthy containers on the asking
# node, or on every node when the asking node has none, `tasks.<service>` to
# the ones on every node, an unhealthy container and a node that stops
# answering leave the answers, the last local container turning unhealthy
# sends the next lookup to the other nodes, other names go to the
# node's own resolvers, and neither the resolver nor the exchange between
# nodes answers anyone else. And shared volumes: no container reaches the
# storage port on its node or through the mesh, the plan names the node
# that keeps each volume and the nodes that mount it, a deployment creates
# the directory on the storage node first and is refused before it changes
# anything when a node cannot reach storage, the status shows sizes and
# mounts, a volume that leaves the configuration keeps its files, and only
# the delete command removes them. These containers run no kernel NFS
# server, so a mount from another node is not exercised here.
# And config files: a template is rendered on every node that runs the
# service and mounted read-only, a changed binding replaces that service on
# every node and no other container, and the superseded version leaves the
# host.
# And a daemon that cannot wedge itself: while a node is in a slow rollout
# step, more changes to that node wait their turn, reads and commands are
# answered, the relay of a client that is killed leaves, and everything
# finishes. And nothing locks an environment: a step in progress is listed
# with its ID and `mm deploy cancel` stops it and rolls the node back; a
# killed client, mmd killed with SIGKILL or stopped with SIGSTOP in the
# middle of a step, an engine CLI that hangs on one node, and a dropped
# connection each end the deployment with a named node and step, reads keep
# answering, nothing outlives its deadline, and the next deployment needs no
# manual step. (systemd's watchdog is checked on real hosts, not here.)
# And operating: a user at `operate` restarts a service one node
# at a time, each healthy before the next, and runs a job in a fresh
# container on its service's node, or on the node and as the user the job
# names. And jobs with a deployment: a migration runs after what its service
# depends on is healthy and before the service changes, a failing one leaves
# the old version running, an after-deploy job runs once the deployment
# converged, a once job runs detached and never again, and a deployment
# killed during its migration blocks nothing. And ingress: the proxy on
# app-1 serves a route with the
# certificate the environment provides (never the one an earlier ACME
# order left in its state), mounts no engine socket, and sends
# requests to a healthy web on its own node, else on the others; requests
# over a kept connection, over new ones, and slow ones see no failure while
# web rolls out (start-first, keeping its volume, then stop-first), a web is
# added by hand,
# and two leave; a leaving container gets no request once every node
# confirmed, and its leave goes on across an mmd restart.
# MM_LIVE_ONLY=ingress runs only that, right after the nodes, the mesh, and
# access are ready. And autoscale: web grows from one container to three
# under CPU load and back to one, a node whose mmd stops hands the decision
# to the next, a deployment in the middle keeps the count, and requests
# through the ingress and by name see no failure while containers come and
# drain, and one restarts in place without dropping a request.
# MM_LIVE_ONLY=autoscale runs only that. And ingress balance: with web on
# every node, local-first sends the proxy's requests to its own node's web,
# and `ingress.balance: spread` to every node's, with no failed request while
# it changes; MM_LIVE_ONLY=balance runs only that. And restarts: a web whose
# health check keeps failing is restarted after 10 s and answered again, one
# that keeps failing is restarted twice and then left out of the routes, even
# once healthy, until `mm service restart`, and nothing is restarted while a
# deployment holds the environment; MM_LIVE_ONLY=health runs balance and
# that. MM_LIVE_ONLY=low-ports runs only
# a check that a service that is not root listens on port 80, on Podman as
# on Docker Engine. And release facts: a plan refuses a declared input that
# is missing or malformed, env and config-file templates read the release,
# its name and inputs, status shows them per node, and a rollback runs with
# the inputs it ran with before (MM_LIVE_ONLY=release-facts runs only
# that). And Cloudflare Tunnel ingress, with a stand-in connector on every
# node: no node takes web traffic from outside once the environment moves off
# public ingress, the connectors start one node at a time, each once it is
# ready, a request through any node's connector reaches its route, a route's
# allowlist answers 404 to the rest, headers are set, a longer path goes
# first, a websocket upgrades, the token is in its file only, status and
# doctor report a stopped connector, and a connector that never connects
# stops the rollout on its node (MM_LIVE_ONLY=tunnel runs only that). And
# the managed secret store: OpenBao members installed from a
# pinned archive, initialised, unsealed by mmd after every node restarts,
# read and written with one member down, out of every container's reach,
# snapshotted, restored onto a store made again, filled from the age store,
# and upgraded one member at a time (MM_LIVE_ONLY=secret-store runs only
# that). And push delivery, last because it
# takes git off the nodes: the nodes lose their origin and git itself,
# mm sends each commit's tracked files to every node of a plan, nothing from
# the working tree arrives, a bundle changed on the way is refused and
# leaves nothing, and a rollback uploads nothing.
#
# The nodes are prepared by this script (account, sshd, mmd, node name, mesh
# state), not by `mm target setup`, which needs systemd and real hosts; check
# that with scripts/live-cluster-setup.sh.
#
# Requires Docker with privileged containers and network access for the
# first run. Each node gets 1 CPU and 1 GB at low CPU priority. Set
# MM_LIVE_PREBUILT_DIR to a directory holding Debian bookworm builds of `mm`
# and `mmd` to skip the build, MM_LIVE_KEEP=1 to keep the containers, and
# MM_LIVE_TUI_OUT to a directory to keep the terminal UI's screens in.
# Set MM_LIVE_ENGINE=podman to run every stage on nodes that run rootful
# Podman (Debian 13) with Docker Compose as its Compose provider, as `mm
# target setup` installs them for `runtime: {driver: podman-compose}`. No
# systemd runs in these containers, so a loop on each node runs the health
# checks Podman leaves to systemd timers.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TMP=$(mktemp -d)
NETWORK="mm-cluster-$RANDOM-$RANDOM"
CLIENT="${NETWORK}-client"
NODES="app-1 app-2 app-3"
PROJECT_ID=019c527b-6242-7b43-bfd4-bd60e27c1de6
WORKLOAD_IMAGE=${MM_LIVE_WORKLOAD_IMAGE:-busybox:1.36.1}
# The nodes' engine. On a Podman node mm's forward chains enter from the
# mangle table's FORWARD instead of DOCKER-USER. Both images carry python3
# for the stand-in of systemd's file descriptor store.
ENGINE=${MM_LIVE_ENGINE:-docker}
case $ENGINE in
  docker) NODE_IMAGE=mm-live-cluster-node:bookworm-fdstore RUNTIME=docker-compose FORWARD_TABLE=filter FORWARD_FROM=DOCKER-USER ;;
  podman) NODE_IMAGE=mm-live-cluster-node:trixie-podman-fdstore RUNTIME=podman-compose FORWARD_TABLE=mangle FORWARD_FROM=FORWARD ;;
  *) echo "MM_LIVE_ENGINE is docker or podman, not '$ENGINE'" >&2; exit 2 ;;
esac
TARGET_IMAGE=${MM_LIVE_TARGET_IMAGE:-$NODE_IMAGE}
CLIENT_IMAGE=${MM_LIVE_CLIENT_IMAGE:-mm-live-cluster-client:bookworm}
# What `mm service logs --max-line-bytes` is given to read a very long line
# whole: the largest line mm keeps.
WHOLE_LINE_BYTES=1048576

container() { printf '%s-%s' "$NETWORK" "$1"; }
on() { node=$1; shift; docker exec "$(container "$node")" "$@"; }
step() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
# has FILE TEXT...: every TEXT is in FILE.
has() {
  local file=$1 text; shift
  for text in "$@"; do
    grep -qF -- "$text" "$file" || { cat "$file" >&2; fail "expected '$text' in the output above"; }
  done
}

cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    for node in $NODES; do
      printf -- '--- %s\n' "$node" >&2
      on "$node" sh -c 'tail -n 20 /tmp/mmd.log; tail -n 5 /var/lib/mm/audit.jsonl' >&2 2>/dev/null || true
    done
  fi
  if [ -n "${MM_LIVE_KEEP:-}" ]; then
    echo "kept the containers on network $NETWORK (MM_LIVE_KEEP); remove them when done" >&2
    return "$status"
  fi
  # With its anonymous volume: each node keeps its engine's data in one.
  for node in $NODES; do docker rm -f -v "$(container "$node")" >/dev/null 2>&1 || true; done
  docker rm -f "$CLIENT" >/dev/null 2>&1 || true
  docker network rm "$NETWORK" >/dev/null 2>&1 || true
  rm -rf "$TMP"
  return "$status"
}
trap cleanup EXIT

mkdir -p "$TMP/build" "$TMP/keys" "$TMP/repo/.config" "$TMP/repo/api"
if [ -n "${MM_LIVE_PREBUILT_DIR:-}" ]; then
  BIN_DIR=$MM_LIVE_PREBUILT_DIR
else
  step "building mm and mmd in the resource-limited Rust image"
  docker run --rm --cpus=1 --cpu-shares=256 --memory=4g --pids-limit=256 \
    --user "$(id -u):$(id -g)" -e HOME=/tmp -e CARGO_HOME=/tmp/cargo \
    -e CARGO_TARGET_DIR=/build -e CARGO_BUILD_JOBS=1 -e CARGO_PROFILE_DEV_DEBUG=0 \
    -e CARGO_PROFILE_DEV_CODEGEN_UNITS=256 \
    -v "$ROOT:/workspace:ro" -v "$TMP/build:/build" -w /workspace \
    mm-rust:1.99.1 cargo build --locked --all-features --bin mm --bin mmd
  BIN_DIR="$TMP/build/debug"
fi

step "images: nodes with sshd, git, and $ENGINE; a client with ssh and git"
if [ "$ENGINE" = podman ]; then
  docker image inspect "$TARGET_IMAGE" >/dev/null 2>&1 || docker build -q -t "$TARGET_IMAGE" - >/dev/null <<'DOCKERFILE'
FROM debian:trixie-slim
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      openssh-server git ca-certificates iptables nftables procps \
      wireguard-tools iproute2 iputils-ping nfs-common \
      podman docker-compose netavark aardvark-dns crun conmon python3 \
 && rm -rf /var/lib/apt/lists/*
DOCKERFILE
fi
docker image inspect "$TARGET_IMAGE" >/dev/null 2>&1 || docker build -q -t "$TARGET_IMAGE" - >/dev/null <<'DOCKERFILE'
FROM debian:bookworm-slim
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      openssh-server git ca-certificates curl gnupg iptables procps \
      wireguard-tools iproute2 iputils-ping python3 \
 && install -m 0755 -d /etc/apt/keyrings \
 && curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc \
 && echo "deb [signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian bookworm stable" >/etc/apt/sources.list.d/docker.list \
 && apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      docker-ce docker-ce-cli containerd.io docker-compose-plugin \
 && rm -rf /var/lib/apt/lists/*
DOCKERFILE
docker image inspect "$CLIENT_IMAGE" >/dev/null 2>&1 || docker build -q -t "$CLIENT_IMAGE" - >/dev/null <<'DOCKERFILE'
FROM debian:bookworm-slim
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends openssh-client git ca-certificates python3 \
 && rm -rf /var/lib/apt/lists/* \
 && git config --global --add safe.directory '*'
DOCKERFILE

# What a Podman node needs besides its packages. The first part stands in
# for a host of its own: the container's processes leave the root control
# group so Podman can hand controllers to its containers, its storage and
# events stay off the container's overlay and off a journal, and no systemd
# means no transient units. The rest is what `mm target setup` writes:
# userns auto as the default, from the `containers` range, and the range
# the services mmd runs get their own user ids from.
PODMAN_NODE_SETUP='set -e
mkdir -p /sys/fs/cgroup/init /run/podman /etc/containers/containers.conf.d
xargs -rn1 </sys/fs/cgroup/cgroup.procs >/sys/fs/cgroup/init/cgroup.procs 2>/dev/null || true
sed -e "s/ / +/g" -e "s/^/+/" </sys/fs/cgroup/cgroup.controllers >/sys/fs/cgroup/cgroup.subtree_control 2>/dev/null || true
printf "[storage]\ndriver = \"vfs\"\nrunroot = \"/run/containers/storage\"\ngraphroot = \"/var/lib/containers/storage\"\n" >/etc/containers/storage.conf
printf "[containers]\nlog_driver = \"k8s-file\"\n[engine]\ncgroup_manager = \"cgroupfs\"\nevents_logger = \"file\"\n" >/etc/containers/containers.conf.d/90-live.conf
printf "[containers]\nuserns = \"auto\"\n" >/etc/containers/containers.conf.d/50-mm.conf
for file in /etc/subuid /etc/subgid; do
  printf "containers:2147483647:1073741825\nmm-services:3221225472:1073676288\n" >>"$file"
done'
# Podman runs a container's health check from a systemd timer; with no
# systemd this loop does, every two seconds.
PODMAN_HEALTH_LOOP='while :; do for c in $(podman ps -q); do podman healthcheck run "$c" >/dev/null 2>&1 || true; done; sleep 2; done'

step "three nodes, each with its own $ENGINE engine"
docker network create "$NETWORK" >/dev/null
for node in $NODES; do
  if [ "$ENGINE" = podman ]; then
    # More processes than a Docker node: on vfs storage Podman copies an
    # image layer with each container's user ids, a few threads per copy,
    # and a stack recreates its containers at once (512 ran out in the
    # push stage: "pthread_create failed: Resource temporarily unavailable").
    docker run -d --privileged --name "$(container "$node")" --hostname "$node" \
      --network "$NETWORK" --volume /var/lib/containers \
      --cpus=1 --cpu-shares=256 --memory=1g --pids-limit=2048 "$TARGET_IMAGE" sleep infinity >/dev/null
    docker exec "$(container "$node")" sh -c "$PODMAN_NODE_SETUP"
    # Docker Compose, Podman's Compose provider here, talks to this socket,
    # as it does to podman.socket on a host.
    docker exec -d "$(container "$node")" sh -c 'podman system service --time=0 unix:///run/podman/podman.sock >/tmp/podman.log 2>&1'
    docker exec -d "$(container "$node")" sh -c "$PODMAN_HEALTH_LOOP"
  else
    docker run -d --privileged --name "$(container "$node")" --hostname "$node" \
      --network "$NETWORK" --volume /var/lib/docker \
      --cpus=1 --cpu-shares=256 --memory=1g --pids-limit=512 "$TARGET_IMAGE" sleep infinity >/dev/null
    # The inner engine cannot stack overlayfs on the container's own root.
    docker exec -d "$(container "$node")" sh -c 'dockerd --storage-driver=vfs >/tmp/dockerd.log 2>&1'
  fi
  # `engine` is the node's engine command for what this script does there.
  docker exec "$(container "$node")" sh -c "ln -sf \"\$(command -v $ENGINE)\" /usr/local/bin/engine"
done
# With an init process: mm keeps its SSH connections open in background
# `ssh` processes, and something has to reap each one when it closes.
docker run -d --init --name "$CLIENT" --network "$NETWORK" \
  --cpus=0.5 --cpu-shares=256 --memory=512m --pids-limit=128 "$CLIENT_IMAGE" sleep infinity >/dev/null
for node in $NODES; do
  for _ in $(seq 1 60); do on "$node" engine info >/dev/null 2>&1 && break; sleep 1; done
  on "$node" engine info >/dev/null 2>&1 || fail "$node: $ENGINE did not start"
  if [ "$ENGINE" = podman ]; then
    for _ in $(seq 1 30); do on "$node" test -S /run/podman/podman.sock && break; sleep 1; done
    on "$node" test -S /run/podman/podman.sock || fail "$node: Podman's API socket did not open"
  fi
done
# Placement puts services on every node, so each needs the workload image.
for node in $NODES; do
  if docker image inspect "$WORKLOAD_IMAGE" >/dev/null 2>&1; then
    docker save "$WORKLOAD_IMAGE" | docker exec -i "$(container "$node")" engine load >/dev/null
  else
    on "$node" engine pull -q "$WORKLOAD_IMAGE" >/dev/null
  fi
done

step "keys: an offline break-glass key, and developers' keys made by mm"
ssh-keygen -q -t ed25519 -N '' -C ops@offline -f "$TMP/keys/ops"
OPS_KEY=$(cut -d' ' -f1,2 "$TMP/keys/ops.pub")
OPS_PRIVATE=$(cat "$TMP/keys/ops")
docker cp "$BIN_DIR/mm" "$CLIENT:/usr/local/bin/mm"
json() { python3 -c "import json,sys; value=json.load(sys.stdin); print($1)"; }
# The keys stay inside mm: the script only ever reads the public line that
# `mm key show` prints for the users file.
for user in alice bob; do
  docker exec "$CLIENT" mm --mm-home "/home-$user" key create >/dev/null
done
public_key_of() { docker exec "$CLIENT" mm --mm-home "/home-$1" --output json key show | json 'value["public_key"]'; }
ALICE_KEY=$(public_key_of alice)
BOB_KEY=$(public_key_of bob)

step "each node runs the relay account, sshd, and mmd, and knows its own name"
cat >"$TMP/host.yaml" <<YAML
schema-version: 2
break-glass:
  - name: ops
    key: $OPS_KEY
projects:
  $PROJECT_ID:
    name: cluster-live
    origin: /srv/git/cluster-live.git
    environments: [production]
YAML
cat >"$TMP/mmd.json" <<'JSON'
{
  "schema_version": 2,
  "socket_path": "/run/mm/mm.sock",
  "state_root": "/var/lib/mm",
  "mm_home": "/var/lib/mm-home",
  "socket_uid": 0,
  "socket_gid": 991,
  "idle_timeout_ms": 30000,
  "host_policy_path": "/etc/mm/host.yaml",
  "relay_uid": 991
}
JSON
# Warnings, and what the keeper of leaving containers and the autoscaler do:
# a failure is then read from /tmp/mmd.log.
MMD_LOG=warn,mm::adapters::leaving=info,mm::adapters::autoscale=info
start_mmd() { docker exec -d -e RUST_LOG="$MMD_LOG" "$(container "$1")" sh -c '/usr/libexec/mmd >>/tmp/mmd.log 2>&1'; }
wait_mmd() {
  for _ in $(seq 1 60); do on "$1" test -S /run/mm/mm.sock && return 0; sleep 0.5; done
  fail "$1: mmd did not open its socket"
}
# As `systemctl stop mmd` leaves a host: no daemon and no socket.
stop_mmd() { on "$1" sh -c 'pkill -x mmd; sleep 1; rm -f /run/mm/mm.sock'; }
# start_kept_mmd NODE: mmd under a stand-in for systemd's file descriptor
# store, which keeps the sockets mmd hands it across a restart, as
# mmd.service does. `stop_mmd` stops both.
start_kept_mmd() {
  docker exec -d "$(container "$1")" sh -c 'python3 /usr/local/bin/live-fd-store --gap 1 /run/mm-notify.sock /run/fd-store.json /usr/libexec/mmd >>/tmp/mmd.log 2>&1'
}
index=1
for node in $NODES; do
  name=$(container "$node")
  docker cp "$BIN_DIR/mm" "$name:/usr/bin/mm"
  docker cp "$BIN_DIR/mmd" "$name:/usr/libexec/mmd"
  docker cp "$ROOT/packaging/mm-relay-shell" "$name:/usr/libexec/mm-relay-shell"
  docker cp "$ROOT/scripts/live-fd-store.py" "$name:/usr/local/bin/live-fd-store"
  docker cp "$ROOT/packaging/sshd/90-mm-relay.conf" "$name:/etc/ssh/sshd_config.d/90-mm-relay.conf"
  on "$node" sh -c '
    set -e
    chmod 755 /usr/bin/mm /usr/libexec/mmd /usr/libexec/mm-relay-shell
    groupadd --system --gid 991 mm
    useradd --system --uid 991 --gid 991 --home-dir /nonexistent --no-create-home --shell /usr/libexec/mm-relay-shell mm
    echo /usr/libexec/mm-relay-shell >> /etc/shells
    mkdir -p /run/sshd /run/mm /etc/mm /var/lib/mm /var/lib/mm-home /srv/mm/projects /srv/git
    chown root:991 /run/mm && chmod 750 /run/mm
    chmod 700 /etc/mm /var/lib/mm
    sshd -t'
  docker cp "$TMP/host.yaml" "$name:/etc/mm/host.yaml"
  docker cp "$TMP/mmd.json" "$name:/etc/mm/mmd.json"
  # What `mm target setup` writes on a real node.
  printf 'MM_NODE_NAME=%s\nMM_PRIVATE_INTERFACE=mm0\nMM_PRIVATE_ADDRESS=10.90.0.%s\nMM_RUNTIME=%s\n' "$node" "$index" "$RUNTIME" >"$TMP/node.env"
  docker cp "$TMP/node.env" "$name:/etc/mm/node.env"
  on "$node" sh -c 'chown root:root /etc/mm/* && chmod 600 /etc/mm/host.yaml /etc/mm/mmd.json && chmod 644 /etc/mm/node.env'
  on "$node" /usr/libexec/mmd access validate >/dev/null
  docker exec -d "$name" sh -c '/usr/sbin/sshd -D -e >/tmp/sshd.log 2>&1'
  index=$((index + 1))
done

host_key_of() { on "$1" cut -d' ' -f1,2 /etc/ssh/ssh_host_ed25519_key.pub; }
daemon_of() { on "$1" /usr/libexec/mmd --print-identity; }
# node_lines NAME:HOST_OF...: a `nodes:` list. Each entry takes its name from
# NAME and its address and keys from the node HOST_OF.
node_lines() {
  local entry name host
  for entry in "$@"; do
    name=${entry%%:*} host=${entry##*:}
    printf '      - name: %s\n        host: %s\n        host-key: %s\n        daemon-fingerprint: %s\n' \
      "$name" "$(container "$host")" "$(host_key_of "$host")" "$(daemon_of "$host")"
  done
}
NODE_LINES=$(node_lines app-1:app-1 app-2:app-2 app-3:app-3)
# The services under modules.api, and what follows the target's nodes.
SERVICE_LINES='      api: {stack: app}'
CLUSTER_LINES=''
# The top-level networks: block, once a stage declares networks.
NETWORK_LINES=''
# What follows `files:` of the stack, once a stage shares a volume.
STACK_LINES=''
# Top-level keys of mm.yaml, once a stage hardens the services.
TOP_LINES=''
# The top-level bindings: block, once a stage renders a template.
BINDING_LINES=''
# The production environment's runtime: the default of a target that lists
# nodes, or Podman.
RUNTIME_LINES=''
[ "$ENGINE" = podman ] && RUNTIME_LINES='    runtime: {driver: podman-compose}'
# The top-level jobs: block, once a stage declares jobs.
JOB_LINES=''
# What follows `path:` of module api, once a stage gives services values.
MODULE_LINES=''
# What follows `target:` of the production environment.
ENVIRONMENT_LINES=''
# $1: users block.
write_config() {
  cat >"$TMP/repo/.config/mm.yaml" <<YAML
schema-version: 1
default-environment: production
project:
  id: $PROJECT_ID
  name: cluster-live
target: local
$TOP_LINES
$NETWORK_LINES
$BINDING_LINES
stacks:
  app:
    project-name: cluster-live-{environment}-{stack}
    files: [compose.yaml]
$STACK_LINES
modules:
  api:
    path: api
$MODULE_LINES
    services:
$SERVICE_LINES
targets:
  cluster:
    nodes:
$NODE_LINES
$CLUSTER_LINES
environments:
  local: {}
  production:
    target: cluster
$RUNTIME_LINES
$ENVIRONMENT_LINES
$JOB_LINES
$1
YAML
}
touch "$TMP/repo/api/.keep"
cat >"$TMP/repo/compose.yaml" <<YAML
services:
  api:
    image: $WORKLOAD_IMAGE
    command: ["sh", "-c", "echo cluster-live-started && exec sleep infinity"]
YAML
git_repo() { git -C "$TMP/repo" -c user.name=live -c user.email=live@example.test "$@"; }
users() {
  local user key grant
  printf 'users:\n'
  for user in "$@"; do
    key=$ALICE_KEY; [ "$user" = bob ] && key=$BOB_KEY
    grant=''; [ "$user" = alice ] && [ -n "${ALICE_SECRETS:-}" ] && grant=', secrets: true'
    printf '  %s:\n    keys: ["%s"]\n    access:\n      production: {level: deploy, exec: [api]%s}\n' "$user" "$key" "$grant"
  done
  # carol, once she has a key: an operator, who restarts and runs jobs.
  if [ -n "${CAROL_KEY:-}" ]; then
    printf '  carol:\n    keys: ["%s"]\n    access:\n      production: {level: operate}\n' "$CAROL_KEY"
  fi
}
commit() { write_config "$1"; git_repo add .; git_repo commit --quiet -m "$2"; }
# Stand-in for a git host: every node fetches from its own root-owned copy.
publish() {
  local node
  rm -rf "$TMP/origin.git"
  git clone --quiet --mirror "$TMP/repo" "$TMP/origin.git"
  for node in $NODES; do
    on "$node" rm -rf /srv/git/cluster-live.git
    docker cp "$TMP/origin.git" "$(container "$node"):/srv/git/cluster-live.git"
    on "$node" chown -R root:root /srv/git/cluster-live.git
  done
  docker exec "$CLIENT" rm -rf /project
  docker cp "$TMP/repo" "$CLIENT:/project"
}
git_repo init --quiet --initial-branch=main
commit "$(users alice)" "alice may deploy production"
publish

for node in $NODES; do start_mmd "$node"; done
for node in $NODES; do wait_mmd "$node"; done

# No --runtime: a target that lists nodes runs Docker Compose by default, and
# the production environment names Podman when the nodes run it.
mm_as() {
  local user=$1; shift
  if [ "$user" = ops ]; then
    docker exec -e MM_SSH_KEY="$OPS_PRIVATE" "$CLIENT" mm --root /project --mm-home /home-ops "$@"
  else
    docker exec "$CLIENT" mm --root /project --mm-home "/home-$user" "$@"
  fi
}

step "mm target nodes lists every node before anything is deployed"
mm_as ops target nodes | tee "$TMP/nodes.log"
has "$TMP/nodes.log" "Target cluster: 3 node(s)" app-1 app-2 app-3 "nothing yet"
[ "$(grep -c ' ready ' "$TMP/nodes.log")" = 3 ] || fail "every node should be ready"
mm_as ops --output json target nodes >"$TMP/nodes.json"
[ "$(json '[n["name"] for n in value["nodes"]]' <"$TMP/nodes.json")" = "['app-1', 'app-2', 'app-3']" ] \
  || fail "the JSON node list is not in declared order"
[ "$(json 'all(n["reachable"] and n["state"] == "ready" for n in value["nodes"])' <"$TMP/nodes.json")" = True ] \
  || fail "the JSON node list should mark every node ready"
mm_as ops target trust cluster | tee "$TMP/trust.log"
has "$TMP/trust.log" "app-1:" "app-2:" "app-3:"
if grep -q 'member-' "$TMP/trust.log"; then fail "pins should name nodes"; fi

step "the mesh: every node's mmd makes its own key and brings up mm0"
# What cluster setup does on real hosts, by hand: read each node's public
# key, then hand every node its interface and the other nodes as peers.
MESH_KEYS=""
for node in $NODES; do
  MESH_KEYS="$MESH_KEYS $node=$(on "$node" /usr/libexec/mmd mesh key | json 'value["public-key"]')"
done
# mesh_state NODE [NAME=KEY...]: the interface state of NODE, with the given
# keys in place of the ones the nodes reported.
mesh_state() {
  local node=$1; shift
  # shellcheck disable=SC2086 # the key list is split into words on purpose
  python3 - "$node" "$NETWORK" $MESH_KEYS "$@" <<'PY'
import json, sys
node, network, entries = sys.argv[1], sys.argv[2], sys.argv[3:]
keys = dict(entry.split("=", 1) for entry in entries)
names = ["app-1", "app-2", "app-3"]
print(json.dumps({
    "interface": "mm0", "address": "10.90.0.%d" % (names.index(node) + 1), "prefix": 24,
    "port": 51820, "mtu": 1420,
    "peers": [{
        "name": name, "public-key": keys[name], "endpoint": "%s-%s:51820" % (network, name),
        "allowed-ips": ["10.90.0.%d/32" % (names.index(name) + 1)],
    } for name in names if name != node],
}))
PY
}
mesh_apply() { local node=$1; mesh_state "$@" | docker exec -i "$(container "$node")" /usr/libexec/mmd mesh apply; }
# reaches NODE ADDRESS: a new WireGuard session can take a few seconds.
reaches() {
  on "$1" sh -c "for attempt in 1 2 3 4 5 6 7 8 9 10 11 12; do ping -c 1 -W 2 -q $2 >/dev/null 2>&1 && exit 0; sleep 1; done; exit 1"
}
# Between app-1 and app-3 the mesh port is closed, as a provider firewall
# that was never opened would leave it.
address_of() { docker inspect --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$(container "$1")"; }
on app-1 iptables -I INPUT -p udp --dport 51820 -s "$(address_of app-3)" -j DROP
on app-3 iptables -I INPUT -p udp --dport 51820 -s "$(address_of app-1)" -j DROP
for node in $NODES; do
  mesh_apply "$node" >"$TMP/mesh-apply-$node.json"
  [ "$(json 'value["changed"]' <"$TMP/mesh-apply-$node.json")" = True ] || fail "$node: the first apply should change the node"
  on "$node" sh -c 'ip -4 -o addr show dev mm0 | grep -q "inet 10.90.0." && ip link show dev mm0 | grep -q "mtu 1420"' \
    || fail "$node: mm0 does not hold its address and MTU"
  on "$node" sh -c '[ "$(stat -c "%U %a" /var/lib/mm/mesh/private.key)" = "root 600" ]' || fail "$node: the mesh key is not a root-only file"
done
reaches app-1 10.90.0.2 || fail "app-1 does not reach app-2 over the mesh"
reaches app-2 10.90.0.3 || fail "app-2 does not reach app-3 over the mesh"
on app-2 ping -c 2 -W 3 -q -M do -s 1392 10.90.0.3 >/dev/null || fail "a packet of the mesh MTU does not get from app-2 to app-3"
on app-1 ping -c 1 -W 2 -q 10.90.0.3 >/dev/null 2>&1 && fail "app-1 reached app-3 although the mesh port is closed between them"

step "mm target mesh status names the port to open between the two nodes that do not connect"
if mm_as ops target mesh status >"$TMP/mesh-blocked.log" 2>&1; then
  cat "$TMP/mesh-blocked.log" >&2; fail "mesh status should exit non-zero while a pair has no handshake"
fi
cat "$TMP/mesh-blocked.log"
has "$TMP/mesh-blocked.log" "Target cluster: mesh between 3 node(s)" "mm0 up" "10.90.0.1" "UDP 51820" "mtu 1420" \
  MM_MESH_NO_HANDSHAKE "'app-1' and 'app-3' have no WireGuard handshake" \
  "UDP port 51820 must be open between $(container app-1) (app-1) and $(container app-3) (app-3) in both directions"
[ "$(grep -c MM_MESH_ "$TMP/mesh-blocked.log")" = 1 ] || fail "only the blocked pair should be reported"
mm_as ops doctor >"$TMP/mesh-doctor.log" 2>&1 || true
has "$TMP/mesh-doctor.log" MM_MESH_NO_HANDSHAKE

step "with the port open every pair has a handshake"
on app-1 iptables -D INPUT -p udp --dport 51820 -s "$(address_of app-3)" -j DROP
on app-3 iptables -D INPUT -p udp --dport 51820 -s "$(address_of app-1)" -j DROP
reaches app-1 10.90.0.3 || fail "app-1 does not reach app-3 over the mesh"
mm_as ops target mesh status | tee "$TMP/mesh.log"
has "$TMP/mesh.log" "Every node has a recent handshake with every other." "PEER" "HANDSHAKE"
mm_as ops --output json target mesh status >"$TMP/mesh.json"
[ "$(json '[(n["name"], n["status"]["interface"], n["status"]["up"], len(n["status"]["peers"])) for n in value["nodes"]]' <"$TMP/mesh.json")" \
  = "[('app-1', 'mm0', True, 2), ('app-2', 'mm0', True, 2), ('app-3', 'mm0', True, 2)]" ] || fail "the JSON mesh status should list every node with two peers"
[ "$(json 'value["problems"]' <"$TMP/mesh.json")" = "[]" ] || fail "a connected mesh should report no problem"
[ "$(json 'all(p["handshake_age_seconds"] <= 180 and p["received_bytes"] > 0 and p["name"] for n in value["nodes"] for p in n["status"]["peers"])' <"$TMP/mesh.json")" = True ] \
  || fail "every peer should have a recent handshake and traffic"

step "a second apply changes nothing; a changed peer leaves the other one alone"
for node in $NODES; do
  [ "$(mesh_apply "$node" | json '(value["changed"], value["actions"])')" = "(False, [])" ] || fail "$node: applying the same state again should change nothing"
done
received() { on app-1 wg show mm0 transfer | awk -v key="$1" '$1 == key {print $2}'; }
KEY_2=$(on app-2 /usr/libexec/mmd mesh show | json 'value["public-key"]')
KEY_3=$(on app-3 /usr/libexec/mmd mesh show | json 'value["public-key"]')
BEFORE=$(received "$KEY_2")
# app-3 replaces its key pair, as `mm target mesh rotate app-3` has it do;
# app-1 then takes the new key for it.
NEW_3=$(on app-3 /usr/libexec/mmd mesh rotate --stage | json 'value["public-key"]')
[ "$NEW_3" != "$KEY_3" ] || fail "the staged key should be a new one"
mesh_apply app-1 "app-3=$NEW_3" >"$TMP/mesh-rekey.json"
[ "$(json 'len([a for a in value["actions"] if "app-2" in a])' <"$TMP/mesh-rekey.json")" = 0 ] || fail "the entry for app-2 should not be touched"
[ "$(json 'len([a for a in value["actions"] if "app-3" in a])' <"$TMP/mesh-rekey.json")" = 2 ] || fail "the entry for app-3 should be removed and added"
mesh_apply app-2 "app-3=$NEW_3" >/dev/null
[ "$(on app-3 /usr/libexec/mmd mesh rotate --activate | json 'value["public-key"]')" = "$NEW_3" ] || fail "app-3 should switch to the staged key"
reaches app-1 10.90.0.3 || fail "app-1 does not reach app-3 with its new key"
reaches app-2 10.90.0.3 || fail "app-2 does not reach app-3 with its new key"
AFTER=$(received "$KEY_2")
[ "$BEFORE" -gt 0 ] && [ "$AFTER" -ge "$BEFORE" ] || fail "the link from app-1 to app-2 was reset ($BEFORE -> $AFTER bytes)"
# Later entries win in mesh_state.
MESH_KEYS="$MESH_KEYS app-3=$NEW_3"
mm_as ops target mesh status >"$TMP/mesh-rotated.log" || { cat "$TMP/mesh-rotated.log" >&2; fail "the mesh should be healthy after the rotation"; }

step "mmd restores the mesh from its own state, and the private key is nowhere but in its file"
on app-2 ip link delete dev mm0
on app-2 /usr/libexec/mmd mesh up | tee "$TMP/mesh-up.log"
has "$TMP/mesh-up.log" "mesh: created the interface mm0"
reaches app-2 10.90.0.1 || fail "app-2 does not reach app-1 after its mesh was restored"
for node in $NODES; do
  # Compared on the node, with the key as a pattern file: it is never printed.
  on "$node" sh -c '
    for file in /tmp/mmd.log /tmp/sshd.log /var/lib/mm/audit.jsonl /var/lib/mm/mesh/mesh.json /var/lib/mm/mesh/public.key; do
      [ ! -e "$file" ] || ! grep -qF -f /var/lib/mm/mesh/private.key "$file" || { echo "the private key is in $file" >&2; exit 1; }
    done
    for directory in /proc/[0-9]*; do
      ! tr "\\0" "\\n" <"$directory/cmdline" 2>/dev/null | grep -qF -f /var/lib/mm/mesh/private.key || { echo "the private key is in a process argument" >&2; exit 1; }
    done
    /usr/libexec/mmd mesh show >/tmp/mesh-show.json
    /usr/libexec/mmd mesh status >/tmp/mesh-status.json
    ! grep -qF -f /var/lib/mm/mesh/private.key /tmp/mesh-show.json /tmp/mesh-status.json' || fail "$node: the mesh private key appears outside its file"
done
# What the client and the applies printed, searched on each node.
for node in $NODES; do
  for file in "$TMP"/mesh*.json "$TMP"/mesh*.log; do
    docker cp "$file" "$(container "$node"):/tmp/client-$(basename "$file")" >/dev/null
  done
  on "$node" sh -c '! grep -lF -f /var/lib/mm/mesh/private.key /tmp/client-mesh*' || fail "$node: its private key is in the output of mm or mmd"
done

step "access is synced on every node under one plan hash"
if mm_as alice team whoami >"$TMP/denied.log" 2>&1; then fail "alice reached a node before the users file was applied"; fi
mm_as ops team sync | tee "$TMP/sync-plan.log"
has "$TMP/sync-plan.log" "Cluster team sync plan" "Node app-1" "Node app-2" "Node app-3"
SYNC=$(mm_as ops --output json team sync | json 'value["plan_hash"]')
mm_as ops team sync --plan "$SYNC" --yes | tee "$TMP/sync.log"
[ "$(grep -c ' applied ' "$TMP/sync.log")" = 3 ] || fail "the sync should apply on three nodes"
for node in $NODES; do
  on "$node" sh -c 'grep -c "^restrict," /etc/ssh/mm_authorized_keys' | grep -qx 2 \
    || fail "$node: the sync should add alice's key"
done

# --- ingress ------------------------------------------------------------------
# The routes of a cluster come in through the ingress proxy on app-1. Its web
# answers with its own hostname (the start of its container's id), after
# thirty seconds for `?slow`, and writes a line to its log for each request.
TRAEFIK_IMAGE=docker.io/library/traefik@sha256:16acb89c6db341182970d6fdafece31303b0a380a8ed7aa51682e225229bf1d2
INGRESS_HOST=shop.cluster.test
INGRESS_DRAIN=35
ingress_compose() {
  cat <<YAML
services:
  web:
    image: $WORKLOAD_IMAGE
    command: ["httpd", "-f", "-p", "8080", "-h", "/www"]
    volumes: ["./web:/www/cgi-bin:ro", "webdata:/data"]
    labels: {release: "$1"}
    healthcheck: {test: ["CMD", "wget", "-q", "-O", "/dev/null", "http://127.0.0.1:8080/cgi-bin/health"], interval: 1s, timeout: 2s, retries: 2}
    stop_grace_period: 5s
volumes:
  webdata: {}
YAML
}
# The client's requests: `loops HOST OUT` sends requests over one kept
# connection and over a new connection each, until /tmp/stop-loops exists;
# `slow HOST COUNT OUT` sends COUNT requests that take thirty seconds at once.
# Each writes what it saw to OUT as JSON.
cat >"$TMP/load.py" <<'PY'
import http.client, json, os, ssl, sys, threading, time

mode, host, out = sys.argv[1], sys.argv[2], sys.argv[-1]
context = ssl.create_default_context(cafile="/tmp/ingress.crt")
lock = threading.Lock()
result = {"requests": {}, "failures": [], "served": {}, "connections": 0, "slow": []}


def record(kind, body, error):
    with lock:
        result["requests"][kind] = result["requests"].get(kind, 0) + 1
        if error:
            result["failures"].append("%.3f %s: %s" % (time.time(), kind, error))
        else:
            result["served"][body] = result["served"].get(body, 0) + 1


def fetch(connection, path):
    connection.request("GET", path)
    response = connection.getresponse()
    body = response.read().decode(errors="replace").strip()
    error = None if response.status == 200 else "status %d %s" % (response.status, body[:80])
    return body, error, response.getheader("Connection", "").lower() == "close"


def connect(timeout):
    with lock:
        result["connections"] += 1
    return http.client.HTTPSConnection(host, 443, context=context, timeout=timeout)


def kept():
    connection, socket = None, None
    while not os.path.exists("/tmp/stop-loops"):
        try:
            connection = connection or connect(60)
            body, error, closed = fetch(connection, "/cgi-bin/id")
            record("kept", body, error)
            # http.client opens a closed connection again by itself.
            if socket is not None and connection.sock is not socket:
                with lock:
                    result["connections"] += 1
            socket = connection.sock
            if closed:
                connection.close()
                connection = None
        except Exception as error:
            record("kept", "", repr(error))
            connection = None
        time.sleep(0.1)


def fresh():
    while not os.path.exists("/tmp/stop-loops"):
        connection = http.client.HTTPSConnection(host, 443, context=context, timeout=60)
        try:
            body, error, _ = fetch(connection, "/cgi-bin/id")
            record("fresh", body, error)
        except Exception as error:
            record("fresh", "", repr(error))
        finally:
            connection.close()
        time.sleep(0.2)


def slow():
    began = time.time()
    try:
        body, error, _ = fetch(http.client.HTTPSConnection(host, 443, context=context, timeout=120), "/cgi-bin/id?slow")
        record("slow", body, error)
        with lock:
            result["slow"].append([began, time.time(), body, error])
    except Exception as error:
        record("slow", "", repr(error))


workers = [threading.Thread(target=kept), threading.Thread(target=fresh)] if mode == "loops" else \
    [threading.Thread(target=slow) for _ in range(int(sys.argv[3]))]
for worker in workers:
    worker.start()
for worker in workers:
    worker.join()
with open(out + ".part", "w") as written:
    json.dump(result, written)
os.rename(out + ".part", out)
PY
# load_result FILE EXPRESSION: EXPRESSION over the client's result in FILE,
# once the requests that write it are done.
load_result() {
  local waited
  for waited in $(seq 1 120); do docker exec "$CLIENT" test -e "$1" && break; sleep 1; done
  docker exec "$CLIENT" cat "$1" | json "$2"
}
# web_of NODE: the id of the node's web container that Compose runs.
web_of() { on "$1" engine ps -q --no-trunc --filter label=com.docker.compose.project --filter label=com.docker.compose.service=web | head -n 1; }
# leave_of NODE ID FIELD: a field of the container's leave on the node, or
# nothing once the leave is over.
leave_of() { on "$1" sh -c "cat /var/lib/mm/leaving/$2.json 2>/dev/null" | json "value.get('$3', '')" 2>/dev/null || true; }
# confirmed NODE ID: waits until every node confirmed the leave; prints when.
confirmed() {
  local at=''
  for _ in $(seq 1 60); do
    at=$(leave_of "$1" "$2" confirmed-unix-ms)
    [ -n "$at" ] && { printf '%s' "$at"; return 0; }
    sleep 0.5
  done
  fail "the leave of ${2:0:12} on $1 was not confirmed by every node within 30 s"
}
# gone_at NODE ID LIMIT: waits up to LIMIT seconds for the container to be
# removed; prints when it was seen gone, in milliseconds.
gone_at() {
  for _ in $(seq 1 "$3"); do
    on "$1" engine inspect "$2" >/dev/null 2>&1 || { date +%s%3N; return 0; }
    sleep 1
  done
  fail "container ${2:0:12} on $1 is still there after $3 s"
}
# requests_after NODE ID MS [UNTIL]: how many requests the container logged
# after the moment MS (and before UNTIL, in milliseconds too), from its log
# kept on the node.
requests_after() {
  on "$1" cat "/tmp/log-$2" | python3 -c "
import sys, datetime
after = int(sys.argv[1]) / 1000
until = int(sys.argv[2]) / 1000 if len(sys.argv) > 2 else float('inf')
late = 0
for line in sys.stdin:
    stamp, _, text = line.partition(' ')
    if not text.startswith('request'):
        continue
    when = datetime.datetime.fromisoformat(stamp[:26].rstrip('Z') + '+00:00').timestamp()
    late += after < when < until
print(late)" "$3" ${4:+"$4"}
}
# started_ms NODE ID: when the engine last started the container, in
# milliseconds; Docker prints RFC 3339, Podman a space and its zone.
started_ms() {
  on "$1" engine inspect --format '{{.State.StartedAt}}' "$2" | python3 -c "
import datetime, sys
value = 'T'.join(sys.stdin.read().split()[:2]).rstrip('Z')
whole, _, fraction = value.partition('.')
start = datetime.datetime.fromisoformat(whole).replace(tzinfo=datetime.timezone.utc).timestamp()
print(int((start + float('0.' + (fraction or '0')[:6])) * 1000))"
}
# keep_log NODE ID: keeps the container's log on the node after it is gone.
keep_log() { docker exec -d "$(container "$1")" sh -c "engine logs -f --timestamps $2 >/tmp/log-$2 2>&1"; }
# secret_on NODE NAME FILE: what `mm target setup` pushes to a node's secret
# store, by hand: the file, as base64 on one line.
secret_on() {
  # The host store and its identity, as setup's first push makes them.
  on "$1" /usr/libexec/mmd secrets recipient --project "$PROJECT_ID" --environment production >/dev/null
  docker exec "$(container "$1")" rm -rf /tmp/secret-repo
  docker cp "$TMP/repo" "$(container "$1"):/tmp/secret-repo"
  base64 -w0 "$3" | docker exec -i -e HOME=/tmp/secret-home -e MM_KEY_VAULT=file "$(container "$1")" \
    mm --root /tmp/secret-repo --mm-home /var/lib/mm-home --environment production secret set "$2" --stdin --local >/dev/null
}
# What app-1's proxy runs and sees, for a failure.
ingress_diagnostics() {
  # shellcheck disable=SC2016 # expands on the node
  on app-1 sh -c '
    cat /var/lib/mm/ingress/dynamic/mm.yaml
    engine logs --tail 40 mm-ingress-proxy-1 2>&1
    engine inspect --format "{{json .NetworkSettings.Networks}}" mm-ingress-proxy-1
    for server in $(grep -o "http://[0-9.]*:[0-9]*" /var/lib/mm/ingress/dynamic/mm.yaml); do
      printf "%s: " "$server"; engine exec mm-ingress-proxy-1 wget -q -T 3 -O - "$server/cgi-bin/id" 2>&1 || echo unreachable
    done
    iptables -t "$1" -L MM-CLUSTER -v -n
    tail -n 30 /tmp/mmd.log' sh "$FORWARD_TABLE" >&2 || true
}
deploy_ingress() {
  local hash
  hash=$(mm_as alice --output json deploy plan | json 'value["plan_hash"]')
  mm_as alice deploy apply --plan "$hash" --yes 2>&1
}

ingress_stages() {
  step "ingress: the nodes have the proxy's image; the client trusts a certificate the environment provides"
  for node in $NODES; do on "$node" engine pull -q "$TRAEFIK_IMAGE" >/dev/null; done
  docker exec "$CLIENT" openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj "/CN=$INGRESS_HOST" \
    -addext "subjectAltName=DNS:$INGRESS_HOST" -keyout /tmp/ingress.key -out /tmp/ingress.crt 2>/dev/null
  docker cp "$CLIENT:/tmp/ingress.crt" "$TMP/ingress.crt"
  docker cp "$CLIENT:/tmp/ingress.key" "$TMP/ingress.key"
  docker cp "$TMP/load.py" "$CLIENT:/tmp/load.py"
  docker exec "$CLIENT" sh -c "echo '$(address_of app-1) $INGRESS_HOST' >>/etc/hosts"
  mkdir -p "$TMP/repo/web"
  # shellcheck disable=SC2016 # expands when the script runs in the container
  printf '#!/bin/sh\nprintf "Content-Type: text/plain\\r\\n\\r\\n"\necho "request $QUERY_STRING" >/proc/1/fd/1\n[ "$QUERY_STRING" = slow ] && sleep 30\nhostname\n' >"$TMP/repo/web/id"
  printf '#!/bin/sh\nprintf "Content-Type: text/plain\\r\\n\\r\\nok\\n"\n' >"$TMP/repo/web/health"
  chmod 755 "$TMP/repo/web/id" "$TMP/repo/web/health"
  ingress_compose v1 >"$TMP/repo/compose.yaml"
  NETWORK_LINES='secrets:
  tls.cert: {storage: {provider: age, location: tls.cert}}
  tls.key: {storage: {provider: age, location: tls.key}}'
  STACK_LINES='    ingress: {domain: shop, service: web, port: 8080}'
  # The first rollout below is start-first, the second stop-first.
  SERVICE_LINES="      web: {stack: app, placement: all, drain: ${INGRESS_DRAIN}s, update: {order: start-first}}"
  CLUSTER_LINES=''
  ENVIRONMENT_LINES="    domains: {shop: $INGRESS_HOST}
    tls: {certificate: tls.cert, key: tls.key}"
  commit "$(users alice bob)" "web behind the ingress"
  publish
  secret_on app-1 tls.cert "$TMP/ingress.crt"
  secret_on app-1 tls.key "$TMP/ingress.key"
  # The host had a certificate from ACME before: app-1's proxy finds it in
  # its ACME state when it starts. Valid for 90 days, so the proxy does not
  # try to renew it.
  docker exec "$CLIENT" openssl req -x509 -newkey rsa:2048 -nodes -days 90 -subj "/CN=$INGRESS_HOST" \
    -addext "subjectAltName=DNS:$INGRESS_HOST" -keyout /tmp/stale.key -out /tmp/stale.crt 2>/dev/null
  docker exec "$CLIENT" python3 -c "
import base64, json
read = lambda path: base64.b64encode(open(path, 'rb').read()).decode()
print(json.dumps({'mm-http': {'Account': None, 'Certificates': [{'domain': {'main': '$INGRESS_HOST'}, 'certificate': read('/tmp/stale.crt'), 'key': read('/tmp/stale.key'), 'Store': 'default'}]}}))" >"$TMP/stale-acme.json"
  on app-1 sh -c 'mkdir -p /var/lib/mm/ingress/acme && chmod 700 /var/lib/mm/ingress/acme'
  docker cp "$TMP/stale-acme.json" "$(container app-1):/var/lib/mm/ingress/acme/acme.json"
  on app-1 sh -c "chown root:root /var/lib/mm/ingress/acme/acme.json && chmod 600 /var/lib/mm/ingress/acme/acme.json"

  step "ingress: the plan names the node that takes the traffic and every route"
  mm_as alice deploy plan | tee "$TMP/ingress-plan.log"
  has "$TMP/ingress-plan.log" "ingress: app-1 takes the traffic on ports 80 and 443" \
    "https://$INGRESS_HOST -> app/web port 8080" "HTTPS uses the certificate in secrets tls.cert and tls.key"
  deploy_ingress | tee "$TMP/ingress-apply.log"
  [ "$(grep -c ' applied ' "$TMP/ingress-apply.log")" = 3 ] || fail "three nodes should report applied"

  step "ingress: the proxy runs on app-1 only, read-only, with no capability and no engine socket"
  # shellcheck disable=SC2016 # Go templates
  on app-1 engine inspect mm-ingress-proxy-1 --format '{{json .Mounts}}|{{.HostConfig.ReadonlyRootfs}}|{{json .HostConfig.CapDrop}}|{{json .HostConfig.CapAdd}}' >"$TMP/proxy.txt"
  cat "$TMP/proxy.txt"
  if grep -q 'docker.sock\|podman.sock\|/var/run\|/run/docker\|/run/podman' "$TMP/proxy.txt"; then fail "the proxy mounts the engine's socket"; fi
  has "$TMP/proxy.txt" '|true|'
  # Docker lists what Compose dropped; Podman lists each capability.
  [ "$ENGINE" = podman ] || has "$TMP/proxy.txt" '["ALL"]|null'
  # shellcheck disable=SC2016 # expands in the proxy's container
  [ "$(on app-1 engine exec mm-ingress-proxy-1 sh -c 'grep -E "^(CapEff|NoNewPrivs):" /proc/self/status | tr -s "\t" " " | tr "\n" " "')" = "CapEff: 0000000000000000 NoNewPrivs: 1 " ] \
    || fail "the proxy runs with a capability or without no-new-privileges"
  for node in app-2 app-3; do
    if on "$node" engine inspect mm-ingress-proxy-1 >/dev/null 2>&1; then fail "$node runs a proxy"; fi
  done
  docker exec "$CLIENT" python3 -c "
import http.client, ssl
connection = http.client.HTTPSConnection('$INGRESS_HOST', 443, context=ssl.create_default_context(cafile='/tmp/ingress.crt'), timeout=30)
connection.request('GET', '/cgi-bin/id')
response = connection.getresponse()
assert response.status == 200, response.status
print('served by', response.read().decode().strip(), 'with the environment\'s certificate')
http = http.client.HTTPConnection('$INGRESS_HOST', 80, timeout=30)
http.request('GET', '/cgi-bin/id')
redirect = http.getresponse()
assert redirect.status in (301, 308) and redirect.getheader('Location', '').startswith('https://'), redirect.status
print('plain HTTP is sent to HTTPS:', redirect.status)" || { ingress_diagnostics; fail "no answer through the proxy on app-1"; }

  step "ingress: the certificate ACME gave the host before is gone, and never served"
  if on app-1 grep -q "$INGRESS_HOST" /var/lib/mm/ingress/acme/acme.json; then fail "app-1's ACME state still holds $INGRESS_HOST"; fi
  docker exec "$CLIENT" python3 -c "
import hashlib, socket, ssl
provided = ssl.PEM_cert_to_DER_cert(open('/tmp/ingress.crt').read())
context = ssl.create_default_context()
context.check_hostname = False
context.verify_mode = ssl.CERT_NONE
served = {}
for _ in range(60):
    with socket.create_connection(('$INGRESS_HOST', 443), timeout=10) as raw:
        with context.wrap_socket(raw, server_hostname='$INGRESS_HOST') as tls:
            der = tls.getpeercert(binary_form=True)
    key = 'provided' if der == provided else hashlib.sha256(der).hexdigest()[:16]
    served[key] = served.get(key, 0) + 1
print('certificates served on 60 new connections:', served)
assert served == {'provided': 60}, served" || { ingress_diagnostics; fail "app-1 serves another certificate than the environment's"; }

  step "ingress: no failed request while web rolls out, a container is added by hand, and two leave"
  declare -A FIRST
  for node in $NODES; do
    FIRST[$node]=$(web_of "$node")
    # What the old web writes to its volume, the new one has to read and
    # change (on Podman: the same user ids of the service in both).
    on "$node" engine exec "${FIRST[$node]}" sh -c "echo written-by-v1 >/data/kept" || fail "$node: web cannot write its volume"
  done
  docker exec -d "$CLIENT" python3 /tmp/load.py loops "$INGRESS_HOST" /tmp/load-main.json
  sleep 5
  ingress_compose v2 >"$TMP/repo/compose.yaml"
  commit "$(users alice bob)" "web v2"
  publish
  deploy_ingress | tee "$TMP/ingress-roll.log"
  has "$TMP/ingress-roll.log" "==> app-3: app" "==> app-1: app: started the new web next to the old"
  declare -A SECOND
  for node in $NODES; do
    SECOND[$node]=$(web_of "$node")
    if [ -z "${SECOND[$node]}" ] || [ "${SECOND[$node]}" = "${FIRST[$node]}" ]; then fail "$node does not run a new web"; fi
    if on "$node" engine inspect "${FIRST[$node]}" >/dev/null 2>&1; then fail "$node keeps its old web"; fi
    [ "$(on "$node" engine exec "${SECOND[$node]}" sh -c "cat /data/kept && echo written-by-v2 >>/data/kept && wc -l </data/kept")" = "written-by-v1
2" ] || fail "$node: the new web does not keep and change what the old one wrote to the volume"
  done
  printf '  ok   the new web on every node started next to the old one and kept its volume\n'
  # The default stop-first: each node's web leaves before the engine
  # replaces it, while the other nodes serve.
  SERVICE_LINES="      web: {stack: app, placement: all, drain: ${INGRESS_DRAIN}s}"
  ingress_compose v3 >"$TMP/repo/compose.yaml"
  commit "$(users alice bob)" "web v3, stopped first"
  publish
  deploy_ingress | tee "$TMP/ingress-roll-stop.log"
  [ "$(grep -c ' applied ' "$TMP/ingress-roll-stop.log")" = 3 ] || fail "three nodes should report applied"
  local ever=''
  for node in $NODES; do
    THIRD=$(web_of "$node")
    if [ -z "$THIRD" ] || [ "$THIRD" = "${SECOND[$node]}" ]; then fail "$node does not run web v3"; fi
    ever="$ever $node:${FIRST[$node]:0:12},${SECOND[$node]:0:12},${THIRD:0:12}"
    SECOND[$node]=$THIRD
  done
  # A second web on app-1, started by hand on the same network.
  local checkout network extra
  # shellcheck disable=SC2016 # Go templates
  checkout=$(on app-1 engine inspect --format '{{range .Mounts}}{{if eq .Destination "/www/cgi-bin"}}{{.Source}}{{end}}{{end}}' "${SECOND[app-1]}")
  # shellcheck disable=SC2016 # Go templates
  network=$(on app-1 engine inspect --format '{{range $name, $value := .NetworkSettings.Networks}}{{$name}} {{end}}' "${SECOND[app-1]}" | tr ' ' '\n' | grep -- '-ingress$')
  extra=$(on app-1 engine run -d --network "$network" --label com.docker.compose.service=web \
    -v "$checkout:/www/cgi-bin:ro" --health-cmd 'wget -q -O /dev/null http://127.0.0.1:8080/cgi-bin/health' \
    --health-interval 1s "$WORKLOAD_IMAGE" httpd -f -p 8080 -h /www)
  for _ in $(seq 1 30); do on app-1 engine logs "$extra" 2>&1 | grep -q '^request' && break; sleep 1; done
  on app-1 engine logs "$extra" 2>&1 | grep -q '^request' || fail "the web added by hand gets no request"
  # Slow requests to both of app-1's webs, then both leave.
  docker exec -d "$CLIENT" python3 /tmp/load.py slow "$INGRESS_HOST" 4 /tmp/load-slow.json
  sleep 3
  keep_log app-1 "${SECOND[app-1]}"
  keep_log app-1 "$extra"
  on app-1 /usr/libexec/mmd container leave "${SECOND[app-1]}" --drain "${INGRESS_DRAIN}s" | tee "$TMP/leave.json"
  [ "$(json 'value["drain-ms"]' <"$TMP/leave.json")" = "$((INGRESS_DRAIN * 1000))" ] || fail "the leave has another drain"
  local first_at extra_at first_gone extra_gone
  first_at=$(confirmed app-1 "${SECOND[app-1]}")
  on app-1 /usr/libexec/mmd container leave "$extra" >"$TMP/leave-extra.json"
  [ "$(json 'value["drain-ms"]' <"$TMP/leave-extra.json")" = 60000 ] || fail "a container without a drain label leaves with the default of 1m"
  extra_at=$(confirmed app-1 "$extra")
  first_gone=$(gone_at app-1 "${SECOND[app-1]}" 90)
  extra_gone=$(gone_at app-1 "$extra" 90)
  docker exec "$CLIENT" touch /tmp/stop-loops
  load_result /tmp/load-main.json 'value' >"$TMP/load-main.txt"
  load_result /tmp/load-slow.json 'value' >"$TMP/load-slow.txt"
  docker exec "$CLIENT" cat /tmp/load-main.json >"$TMP/load-main.json"
  docker exec "$CLIENT" cat /tmp/load-slow.json >"$TMP/load-slow.json"
  json '"requests %s, connections %d, failures %d" % (value["requests"], value["connections"], len(value["failures"]))' <"$TMP/load-main.json"
  json '"\n".join(value["failures"][:20])' <"$TMP/load-main.json"
  [ "$(json 'len(value["failures"])' <"$TMP/load-main.json")" = 0 ] || fail "requests failed while web rolled out and containers came and went"
  [ "$(json 'value["connections"]' <"$TMP/load-main.json")" = 1 ] || fail "the kept connection had to be opened again"
  [ "$(json 'len(value["failures"])' <"$TMP/load-slow.json")" = 0 ] || fail "a slow request failed: $(json 'value["failures"]' <"$TMP/load-slow.json")"
  [ "$(json 'value["requests"].get("slow", 0)' <"$TMP/load-slow.json")" = 4 ] || fail "not every slow request came back"
  local served
  served=$(json '" ".join(sorted(value["served"]))' <"$TMP/load-main.json")
  printf '  served by: %s\n' "$served"
  for entry in $ever; do
    node=${entry%%:*} reached=''
    for id in $(printf '%s' "${entry#*:}" | tr ',' ' '); do
      case " $served " in *" $id "*) reached=$id ;; esac
    done
    [ -n "$reached" ] || fail "no request reached a web on $node (${entry#*:})"
  done
  case " $(json '" ".join(row[2] for row in value["slow"])' <"$TMP/load-slow.json") " in
    *" ${SECOND[app-1]:0:12} "*|*" ${extra:0:12} "*) ;;
    *) fail "no slow request was served by a container that left" ;;
  esac
  for left in "${SECOND[app-1]}:$first_at:$first_gone" "$extra:$extra_at:$extra_gone"; do
    IFS=: read -r id at gone <<<"$left"
    late=$(requests_after app-1 "$id" "$((at + 1000))")
    printf '  %s: confirmed at %s, %s request(s) after that, gone %s ms after\n' "${id:0:12}" "$at" "$late" "$((gone - at))"
    [ "$late" = 0 ] || fail "${id:0:12} got $late request(s) after every node confirmed it was leaving"
  done
  [ "$((first_gone - first_at))" -ge "$((INGRESS_DRAIN * 1000))" ] || fail "web left before its drain was over"

  step "ingress: a drain goes on across an mmd restart"
  local leaving at
  leaving=$(web_of app-2)
  on app-2 /usr/libexec/mmd container leave "$leaving" --drain 20s >/dev/null
  at=$(confirmed app-2 "$leaving")
  stop_mmd app-2
  sleep 2
  start_mmd app-2
  wait_mmd app-2
  sleep 5
  on app-2 engine inspect "$leaving" >/dev/null 2>&1 || fail "the restart stopped the leaving container early"
  [ "$(leave_of app-2 "$leaving" confirmed-unix-ms)" = "$at" ] || fail "the restart lost the leave's confirmation"
  if on app-1 grep -q "$(on app-2 engine inspect --format '{{range .NetworkSettings.Networks}}{{.IPAddress}} {{end}}' "$leaving" | awk '{print $1}'):" \
    /var/lib/mm/ingress/dynamic/mm.yaml; then fail "the proxy sends to the leaving container again"; fi
  gone=$(gone_at app-2 "$leaving" 60)
  [ "$((gone - at))" -ge 20000 ] || fail "the leaving container stopped $((gone - at)) ms after it was confirmed, before its drain of 20 s"
  printf '  ok   the leave went on across the restart and ended %s ms after the confirmation\n' "$((gone - at))"
}

# --- autoscale ----------------------------------------------------------------
# web autoscales between one and three containers with short windows: up
# above 30% for 10 s, down below 10% for 15 s. Its containers have no CPU
# limit (the nodes' engines cannot set one), so a share is of one core; each
# keeps half a core busy (one second on, one off) while web/burn-on exists in
# its node's checkout. A caller on
# app-2 asks for web by name the whole time; the client asks through the
# ingress proxy on app-1 over a kept connection, over new ones, and slowly.
AUTOSCALE_DRAIN=35
autoscale_compose() {
  cat <<YAML
services:
  web:
    image: $WORKLOAD_IMAGE
    command: ["sh", "-c", "(while :; do if [ -e /www/cgi-bin/burn-on ]; then timeout 1 sh -c 'while :; do :; done'; fi; sleep 1; done) & exec httpd -f -p 8080 -h /www"]
    volumes: ["./web:/www/cgi-bin:ro"]
    labels: {release: "$1"}
    healthcheck: {test: ["CMD", "wget", "-q", "-O", "/dev/null", "http://127.0.0.1:8080/cgi-bin/health"], interval: 1s, timeout: 2s, retries: 2}
    stop_grace_period: 5s
  caller:
    image: $WORKLOAD_IMAGE
    command: ["sh", "-c", "while :; do if wget -q -T 10 -O - http://web:8080/cgi-bin/id?name; then :; else echo FAILED; fi; sleep 0.2; done"]
    stop_grace_period: 1s
YAML
}
# webs NODE: the ids of the node's running web containers that are not
# leaving.
webs() {
  on "$1" sh -c 'for id in $(engine ps -q --no-trunc --filter label=com.docker.compose.service=web --filter label=com.docker.compose.oneoff=False); do
    [ -e /var/lib/mm/leaving/$id.json ] || echo $id; done'
}
web_count() {
  local total=0 node
  for node in $NODES; do total=$((total + $(webs "$node" | grep -c . || true))); done
  printf '%s' "$total"
}
# scaled_to COUNT LIMIT: waits up to LIMIT seconds until the nodes run COUNT
# webs, healthy and not leaving; prints the seconds it took.
scaled_to() {
  local began now healthy node id
  began=$(date +%s)
  while :; do
    now=$(date +%s)
    if [ "$(web_count)" = "$1" ]; then
      healthy=1
      for node in $NODES; do
        for id in $(webs "$node"); do
          # shellcheck disable=SC2016 # Go template
          [ "$(on "$node" engine inspect --format '{{.State.Health.Status}}' "$id")" = healthy ] || healthy=''
        done
      done
      [ -n "$healthy" ] && { printf '%s' "$((now - began))"; return 0; }
    fi
    [ "$((now - began))" -lt "$2" ] || { autoscale_diagnostics; fail "web did not reach $1 container(s) within $2 s (it runs $(web_count))"; }
    sleep 1
  done
}
autoscale_diagnostics() {
  local node
  for node in $NODES; do
    printf -- '--- %s\n' "$node" >&2
    on "$node" sh -c 'engine ps --format "{{.Names}} {{.Status}}"; ls /var/lib/mm/scale/wants /var/lib/mm/leaving 2>/dev/null; cat /var/lib/mm/scale/wants/*.json 2>/dev/null; grep -i autoscal /tmp/mmd.log | tail -n 15' >&2 || true
  done
  mm_as alice service status >&2 || true
}
# leaving_web NODE LIMIT: waits up to LIMIT seconds for a web of the node to
# begin leaving; prints its id.
leaving_web() {
  local id
  for _ in $(seq 1 "$2"); do
    id=$(on "$1" sh -c 'ls /var/lib/mm/leaving 2>/dev/null' | sed -n 's/\.json$//p' | head -n 1)
    [ -n "$id" ] && { printf '%s' "$id"; return 0; }
    sleep 1
  done
  autoscale_diagnostics
  fail "no web on $1 began to leave within $2 s"
}
audited() { on "$1" sh -c 'grep "\"result\":\"autoscale\"" /var/lib/mm/audit.jsonl || true'; }
# when WHAT: notes the moment WHAT happened, for the timeline at the end.
when() { printf '%s %s\n' "$(date -u +%FT%T.%3N)" "$*" >>"$TMP/timeline"; }
# burn touch|"rm -f": every web on every node burns half a core, or stops.
burn() {
  local node
  # shellcheck disable=SC2016 # expands on the node
  for node in $NODES; do on "$node" sh -c "for web in /srv/mm/projects/*/production/*/web; do $1 \$web/burn-on; done"; done
}

autoscale_stages() {
  step "autoscale: web behind the ingress with one container, a caller asking for it by name"
  local node
  for node in $NODES; do on "$node" engine pull -q "$TRAEFIK_IMAGE" >/dev/null; done
  docker exec "$CLIENT" openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj "/CN=$INGRESS_HOST" \
    -addext "subjectAltName=DNS:$INGRESS_HOST" -keyout /tmp/ingress.key -out /tmp/ingress.crt 2>/dev/null
  docker cp "$CLIENT:/tmp/ingress.crt" "$TMP/ingress.crt"
  docker cp "$CLIENT:/tmp/ingress.key" "$TMP/ingress.key"
  docker cp "$TMP/load.py" "$CLIENT:/tmp/load.py"
  docker exec "$CLIENT" sh -c "grep -q ' $INGRESS_HOST' /etc/hosts || echo '$(address_of app-1) $INGRESS_HOST' >>/etc/hosts"
  docker exec "$CLIENT" rm -f /tmp/stop-loops /tmp/load-main.json /tmp/load-slow.json
  mkdir -p "$TMP/repo/web"
  # shellcheck disable=SC2016 # expands when the script runs in the container
  printf '#!/bin/sh\nprintf "Content-Type: text/plain\\r\\n\\r\\n"\necho "request $QUERY_STRING" >/proc/1/fd/1\n[ "$QUERY_STRING" = slow ] && sleep 30\nhostname\n' >"$TMP/repo/web/id"
  # web/sick in the checkout makes the health check time out (fail open step).
  printf '#!/bin/sh\n[ -e /www/cgi-bin/sick ] && sleep 5 && exit 1\nprintf "Content-Type: text/plain\\r\\n\\r\\nok\\n"\n' >"$TMP/repo/web/health"
  chmod 755 "$TMP/repo/web/id" "$TMP/repo/web/health"
  autoscale_compose v1 >"$TMP/repo/compose.yaml"
  NETWORK_LINES='networks:
  backend: {}
secrets:
  tls.cert: {storage: {provider: age, location: tls.cert}}
  tls.key: {storage: {provider: age, location: tls.key}}'
  STACK_LINES='    ingress: {domain: shop, service: web, port: 8080}'
  SERVICE_LINES="      web:
        stack: app
        placement: all
        networks: [backend]
        drain: ${AUTOSCALE_DRAIN}s
        autoscale:
          scale: {min: 1, max: 3}
          metric:
            up: {greater: 30%, for: 10s}
            down: {less: 10%, for: 15s}
      caller: {stack: app, placement: app-2, networks: [backend]}"
  CLUSTER_LINES=''
  ENVIRONMENT_LINES="    domains: {shop: $INGRESS_HOST}
    tls: {certificate: tls.cert, key: tls.key}"
  commit "$(users alice bob)" "web autoscales"
  publish
  secret_on app-1 tls.cert "$TMP/ingress.crt"
  secret_on app-1 tls.key "$TMP/ingress.key"
  mm_as alice deploy plan | tee "$TMP/autoscale-plan.log"
  has "$TMP/autoscale-plan.log" "autoscale app/web: 1..3 containers over 3 node(s), 1 now (1 here); cpu above 30% adds one, below 10% removes one" \
    "autoscale app/web: 1..3 containers over 3 node(s), 1 now (0 here)"
  deploy_ingress | tee "$TMP/autoscale-apply.log"
  [ "$(grep -c ' applied ' "$TMP/autoscale-apply.log")" = 3 ] || fail "three nodes should report applied"
  scaled_to 1 60 >/dev/null
  [ "$(webs app-1 | grep -c .)" = 1 ] || fail "app-1 should run the one web"
  local caller
  caller=$(on app-2 engine ps -q --no-trunc --filter label=com.docker.compose.service=caller --filter label=com.docker.compose.oneoff=False)
  [ -n "$caller" ] || fail "app-2 runs no caller"

  step "autoscale: CPU above 30% adds one container at a time, and app-2 decides when app-1's mmd stops"
  docker exec -d "$CLIENT" python3 /tmp/load.py loops "$INGRESS_HOST" /tmp/load-main.json
  sleep 3
  burn touch
  when burn on
  local to_two to_three
  to_two=$(scaled_to 2 120)
  printf '  ok   2 containers %s s after the load began\n' "$to_two"
  when 2 containers
  [ "$(webs app-2 | grep -c .)" = 1 ] || fail "the second container should run on app-2, the node with the fewest"
  local up_web
  up_web=$(webs app-2)
  stop_mmd app-1
  when app-1 mmd stopped
  to_three=$(scaled_to 3 150)
  when 3 containers
  printf '  ok   3 containers %s s after app-1'"'"'s mmd stopped\n' "$to_three"
  audited app-2 | tail -n 1 | tee "$TMP/autoscale-audit.log"
  has "$TMP/autoscale-audit.log" '"node":"app-2"' '"from":2' '"to":3' 'above 30% for 10s'
  start_mmd app-1
  wait_mmd app-1
  when app-1 mmd started
  [ "$(audited app-1 | grep -c '"to":2')" = 1 ] || fail "app-1 should have audited the change to 2"
  local id
  for node in $NODES; do for id in $(webs "$node"); do keep_log "$node" "$id"; done; done
  sleep 12
  mm_as alice service status | tee "$TMP/autoscale-status.log"
  has "$TMP/autoscale-status.log" "autoscale web: 3 running, 3 wanted (1..3), decided on app-1" \
    "last change: 2 -> 3 at" "cpu over the last 10 s:"
  mm_as alice --output json service status >"$TMP/autoscale-status.json"
  [ "$(json '[s["scale"]["wanted"] for stack in value["stacks"] for s in stack["services"] if s.get("scale")]' <"$TMP/autoscale-status.json")" = "[3]" ] \
    || fail "the JSON status should carry the count"

  step "autoscale: a deployment in the middle keeps the count"
  autoscale_compose v2 >"$TMP/repo/compose.yaml"
  commit "$(users alice bob)" "web v2"
  publish
  mm_as alice deploy plan | tee "$TMP/autoscale-plan-3.log"
  has "$TMP/autoscale-plan-3.log" "1..3 containers over 3 node(s), 3 now (1 here)"
  # The plan checked the new commit out on every node: its webs burn too.
  burn touch
  when deployment began
  deploy_ingress | tee "$TMP/autoscale-roll.log"
  when deployment ended
  [ "$(grep -c ' applied ' "$TMP/autoscale-roll.log")" = 3 ] || fail "three nodes should report applied"
  [ "$(web_count)" = 3 ] || { autoscale_diagnostics; fail "the deployment changed the count to $(web_count)"; }
  printf '  ok   3 containers after the deployment, as before it\n'
  for node in $NODES; do for id in $(webs "$node"); do keep_log "$node" "$id"; done; done

  step "autoscale: mm service restart drops no request: one container at a time, each drained first"
  local restarting=() entry id at restart_pid late
  for node in $NODES; do restarting+=("$node:$(webs "$node")"); done
  when restart of web asked
  mm_as alice service restart web >"$TMP/restart.log" 2>&1 &
  restart_pid=$!
  # Each node in turn: its web leaves to restart, every node confirms it.
  local confirmations=()
  for entry in "${restarting[@]}"; do
    node=${entry%%:*}; id=${entry#*:}
    for _ in $(seq 1 240); do [ -n "$(leave_of "$node" "$id" marked-unix-ms)" ] && break; sleep 0.5; done
    [ "$(leave_of "$node" "$id" restart)" = True ] || { cat "$TMP/restart.log"; fail "the restart of ${id:0:12} on $node did not leave first"; }
    at=$(confirmed "$node" "$id")
    when "$node web confirmed restarting"
    confirmations+=("$node:$id:$at")
  done
  wait "$restart_pid" || { cat "$TMP/restart.log" >&2; autoscale_diagnostics; fail "mm service restart web failed"; }
  when restart of web ended
  cat "$TMP/restart.log"
  has "$TMP/restart.log" "Restarted web on app-1, then app-2, then app-3, one node at a time"
  scaled_to 3 60 >/dev/null
  for entry in "${confirmations[@]}"; do
    IFS=: read -r node id at <<<"$entry"
    if [ "$(webs "$node")" != "$id" ]; then
      on "$node" sh -c 'engine ps -a --no-trunc --filter label=com.docker.compose.service=web --format "{{.ID}} {{.Status}} {{.CreatedAt}}"; grep -iE "leav|autoscal|restart" /tmp/mmd.log | tail -n 20' >&2 || true
      fail "$node runs $(webs "$node" | tr '\n' ' ')instead of the web that restarted, ${id:0:12}"
    fi
    # Between the confirmation and the container's new start nothing
    # reached it: its kept log is of the run before the restart.
    late=$(requests_after "$node" "$id" "$((at + 1000))" "$(started_ms "$node" "$id")")
    printf '  ok   %s on %s restarted in place, %s request(s) between its confirmation and its restart, answered again once healthy\n' \
      "${id:0:12}" "$node" "$late"
    [ "$late" = 0 ] || fail "${id:0:12} got $late request(s) after every node confirmed it was restarting"
    keep_log "$node" "$id"
  done

  step "autoscale: CPU below 10% removes one container at a time; each drains before it stops"
  burn "rm -f"
  when burn off
  docker exec -d "$CLIENT" python3 /tmp/load.py slow "$INGRESS_HOST" 6 /tmp/load-slow.json
  local third third_at third_gone second second_at second_gone
  third=$(leaving_web app-3 90)
  when app-3 web began to leave
  third_at=$(confirmed app-3 "$third")
  when app-3 web leave confirmed
  # Slow requests by name from app-2, one every 4 s: those in flight on
  # app-2's web when it begins to leave finish within its drain.
  # shellcheck disable=SC2016 # expands in the caller
  on app-2 engine exec -d "$caller" sh -c 'for i in $(seq 1 30); do (wget -q -T 90 -O - "http://web:8080/cgi-bin/id?slow" >/tmp/slow-$i 2>&1 || echo FAILED >/tmp/slow-$i) & sleep 4; done; wait'
  third_gone=$(gone_at app-3 "$third" 90)
  when app-3 web gone
  scaled_to 2 30 >/dev/null
  second=$(leaving_web app-2 90)
  when app-2 web began to leave
  second_at=$(confirmed app-2 "$second")
  when app-2 web leave confirmed
  second_gone=$(gone_at app-2 "$second" 90)
  when app-2 web gone
  scaled_to 1 30 >/dev/null
  [ "$(webs app-1 | grep -c .)" = 1 ] || fail "the last container should be app-1's"
  audited app-1 | tail -n 2 | tee "$TMP/autoscale-audit-down.log"
  has "$TMP/autoscale-audit-down.log" '"from":3' '"to":2' '"from":2' '"to":1' 'below 10% for 15s'

  step "autoscale: what the requests saw"
  docker exec "$CLIENT" touch /tmp/stop-loops
  docker exec "$CLIENT" cat /tmp/load-main.json >/dev/null 2>&1 || load_result /tmp/load-main.json 'value' >/dev/null
  load_result /tmp/load-main.json 'value' >/dev/null
  load_result /tmp/load-slow.json 'value' >/dev/null
  docker exec "$CLIENT" cat /tmp/load-main.json >"$TMP/autoscale-main.json"
  docker exec "$CLIENT" cat /tmp/load-slow.json >"$TMP/autoscale-slow.json"
  json '"through the ingress: requests %s, connections %d, failures %d" % (value["requests"], value["connections"], len(value["failures"]))' <"$TMP/autoscale-main.json"
  json '"  served: %s" % value["served"]' <"$TMP/autoscale-main.json"
  json '"\n".join(value["failures"][:20])' <"$TMP/autoscale-main.json"
  json '"slow: requests %s, failures %d, served by %s" % (value["requests"], len(value["failures"]), sorted({row[2] for row in value["slow"]}))' <"$TMP/autoscale-slow.json"
  local by_name failed_by_name
  on app-2 engine logs --timestamps "$caller" >"$TMP/caller-stamped.log" 2>&1
  cut -d' ' -f2- "$TMP/caller-stamped.log" >"$TMP/caller.log"
  by_name=$(grep -c '^[0-9a-f]\{12\}$' "$TMP/caller.log" || true)
  failed_by_name=$(grep -c FAILED "$TMP/caller.log" || true)
  printf '  by name from app-2: %s answers, %s failures; answered by: %s\n' "$by_name" "$failed_by_name" \
    "$(grep '^[0-9a-f]\{12\}$' "$TMP/caller.log" | sort | uniq -c | awk '{printf "%s %s; ", $2, $1}')"
  printf '  timeline:\n'; sed 's/^/    /' "$TMP/timeline"
  if [ "$failed_by_name" != 0 ]; then
    printf '  failures by name, with the answer before and after each:\n'
    grep -n -B2 -A2 -v '^[0-9TZ:.+-]* [0-9a-f]\{12\}$' "$TMP/caller-stamped.log" | sed 's/^/    /' | head -n 60
  fi
  [ "$(json 'len(value["failures"])' <"$TMP/autoscale-main.json")" = 0 ] || fail "requests through the ingress failed while web scaled"
  [ "$(json 'value["connections"]' <"$TMP/autoscale-main.json")" = 1 ] || fail "the kept connection had to be opened again"
  [ "$(json 'len(value["failures"])' <"$TMP/autoscale-slow.json")" = 0 ] || fail "a slow request failed: $(json 'value["failures"]' <"$TMP/autoscale-slow.json")"
  [ "$failed_by_name" = 0 ] || fail "$failed_by_name request(s) by name failed"
  [ "$(json 'len(value["served"])' <"$TMP/autoscale-main.json")" -ge 2 ] || fail "requests through the ingress reached one container only"
  grep -qx "${up_web:0:12}" "$TMP/caller.log" || fail "the caller on app-2 never got an answer from the web autoscale started there (${up_web:0:12})"
  local slow_answers
  for _ in $(seq 1 60); do
    [ "$(on app-2 engine exec "$caller" sh -c 'ls /tmp/slow-* 2>/dev/null | wc -l')" = 30 ] \
      && [ -z "$(on app-2 engine exec "$caller" sh -c 'for f in /tmp/slow-*; do [ -s "$f" ] || echo "$f"; done')" ] && break
    sleep 2
  done
  slow_answers=$(on app-2 engine exec "$caller" sh -c 'cat /tmp/slow-*' | sort | uniq -c | tr '\n' ';')
  printf '  slow requests by name: %s\n' "$slow_answers"
  case "$slow_answers" in *FAILED*) fail "a slow request by name failed: $slow_answers" ;; esac
  case "$slow_answers" in *"${second:0:12}"*) ;; *) fail "no slow request by name was served by app-2's web that left" ;; esac
  for left in "app-3:$third:$third_at:$third_gone" "app-2:$second:$second_at:$second_gone"; do
    IFS=: read -r node id at gone <<<"$left"
    late=$(requests_after "$node" "$id" "$((at + 1000))")
    total=$(on "$node" cat "/tmp/log-$id" | grep -c ' request' || true)
    printf '  %s on %s: %s request(s) in all, confirmed leaving at %s, %s request(s) after that, gone %s ms after\n' \
      "${id:0:12}" "$node" "$total" "$at" "$late" "$((gone - at))"
    [ "$late" = 0 ] || fail "${id:0:12} got $late request(s) after every node confirmed it was leaving"
    [ "$((gone - at))" -ge "$((AUTOSCALE_DRAIN * 1000))" ] || fail "${id:0:12} stopped before its drain was over"
  done
  # The sampler read every container's cgroup files: no node fell back to
  # asking the engine for usage.
  for node in $NODES; do
    if on "$node" grep -q 'shows no cgroup v2 counters' /tmp/mmd.log; then
      on "$node" sh -c 'ls /sys/fs/cgroup; cat /proc/$(engine ps -q --filter label=com.docker.compose.service=web | head -n 1 | xargs engine inspect --format "{{.State.Pid}}")/cgroup' >&2 || true
      fail "$node read container usage from the engine: the sampler does not know where $ENGINE keeps a container's cgroup"
    fi
  done
  printf '  ok   every node read its containers'"'"' CPU from their cgroup files\n'

  step "autoscale: a service whose only container fails its health check is still answered (fail open)"
  # web/sick in the checkout fails web's health check; app-1 runs the one web.
  local sick_web sick_at answered_after failed_before failed_after
  sick_web=$(webs app-1)
  failed_before=$(on app-2 engine logs "$caller" 2>&1 | grep -c FAILED || true)
  on app-1 sh -c 'for web in /srv/mm/projects/*/production/*/web; do touch $web/sick; done'
  for _ in $(seq 1 60); do
    # shellcheck disable=SC2016 # Go template
    [ "$(on app-1 engine inspect --format '{{.State.Health.Status}}' "$sick_web")" = unhealthy ] && break
    sleep 1
  done
  # shellcheck disable=SC2016 # Go template
  [ "$(on app-1 engine inspect --format '{{.State.Health.Status}}' "$sick_web")" = unhealthy ] || fail "web did not turn unhealthy"
  sick_at=$(date +%s%3N)
  when web unhealthy
  sleep 8
  # By name: the caller kept getting the web; through the ingress: answers.
  answered_after=$(on app-2 engine logs --timestamps "$caller" 2>&1 | python3 -c "
import sys, datetime
after = int(sys.argv[1]) / 1000
print(sum(1 for line in sys.stdin
          if line.split(' ', 1)[-1].strip() == sys.argv[2][:12]
          and datetime.datetime.fromisoformat(line[:26].rstrip('Z') + '+00:00').timestamp() > after))" "$sick_at" "$sick_web")
  failed_after=$(on app-2 engine logs "$caller" 2>&1 | grep -c FAILED || true)
  docker exec "$CLIENT" python3 -c "
import http.client, ssl
context = ssl.create_default_context(cafile='/tmp/ingress.crt')
for _ in range(10):
    connection = http.client.HTTPSConnection('$INGRESS_HOST', 443, context=context, timeout=20)
    connection.request('GET', '/cgi-bin/id')
    response = connection.getresponse()
    body = response.read().decode().strip()
    assert response.status == 200, (response.status, body)
print('through the ingress: 10 of 10 answered by', body)" | tee "$TMP/fail-open-ingress.log"
  printf '  by name from app-2 while web is unhealthy: %s answers by %s, %s new failures\n' \
    "$answered_after" "${sick_web:0:12}" "$((failed_after - failed_before))"
  [ "$answered_after" -ge 10 ] || fail "the caller got no answer from the unhealthy web (fail open)"
  [ "$failed_after" = "$failed_before" ] || fail "requests by name failed while the only web was unhealthy"
  mm_as alice service status | tee "$TMP/fail-open-status.log"
  has "$TMP/fail-open-status.log" "answered while unhealthy"
  mm_as alice deploy status | tee "$TMP/fail-open-deploy-status.log"
  has "$TMP/fail-open-deploy-status.log" "web on backend is answered while unhealthy"
  mm_as alice doctor >"$TMP/fail-open-doctor.log" 2>&1 || true
  has "$TMP/fail-open-doctor.log" "MM_SERVICE_ANSWERED_UNHEALTHY"
  on app-1 sh -c 'rm -f /srv/mm/projects/*/production/*/web/sick'
  scaled_to 1 60 >/dev/null
  when web healthy again

  step "autoscale: a service that stops autoscaling takes its count with it"
  for node in $NODES; do
    [ -n "$(on "$node" sh -c 'ls /var/lib/mm/scale/wants 2>/dev/null')" ] || fail "$node keeps no count of web"
  done
  SERVICE_LINES="      web:
        stack: app
        placement: all
        networks: [backend]
        drain: ${AUTOSCALE_DRAIN}s
      caller: {stack: app, placement: app-2, networks: [backend]}"
  commit "$(users alice bob)" "web stops autoscaling"
  publish
  deploy_ingress | tee "$TMP/autoscale-off.log"
  [ "$(grep -c ' applied ' "$TMP/autoscale-off.log")" = 3 ] || fail "three nodes should report applied"
  scaled_to 3 90 >/dev/null
  for node in $NODES; do
    [ -z "$(on "$node" sh -c 'ls /var/lib/mm/scale/wants /var/lib/mm/scale/deployed 2>/dev/null | grep json')" ] \
      || fail "$node still keeps a count or a record of web: $(on "$node" ls -R /var/lib/mm/scale)"
  done
  printf '  ok   one web on each node again, and no node keeps a count\n'
}

# A service whose user is not root listens on port 80: Docker Engine lets
# it, and mm's overlay does the same on Podman.
# --- balance and health -------------------------------------------------------
# web on every node behind the proxy on app-1, with a short drain. Its health
# check fails while /state/sick (a tmpfs: a restart clears it) or /data/sick
# (its volume: a restart keeps it) exists in the container.
health_compose() {
  cat <<YAML
services:
  web:
    image: $WORKLOAD_IMAGE
    command: ["httpd", "-f", "-p", "8080", "-h", "/www"]
    volumes: ["./web:/www/cgi-bin:ro", "webdata:/data"]
    tmpfs: [/state]
    labels: {release: "$1"}
    healthcheck: {test: ["CMD", "wget", "-q", "-T", "2", "-O", "/dev/null", "http://127.0.0.1:8080/cgi-bin/health"], interval: 1s, timeout: 3s, retries: 2}
    stop_grace_period: 5s
volumes:
  webdata: {}
YAML
}
# served_by FILE: the short ids of the webs the client's requests reached.
served_by() { docker exec "$CLIENT" cat "$1" | json '" ".join(sorted(value["served"]))'; }
# loads OUT: requests over a kept connection and new ones until stop_loads.
loads() { docker exec "$CLIENT" rm -f /tmp/stop-loops "$1"; docker exec -d "$CLIENT" python3 /tmp/load.py loops "$INGRESS_HOST" "$1"; }
# stop_loads OUT: stops them and fails on any failed request.
stop_loads() {
  docker exec "$CLIENT" touch /tmp/stop-loops
  load_result "$1" '"requests %s, connections %d, failures %d" % (value["requests"], value["connections"], len(value["failures"]))'
  docker exec "$CLIENT" cat "$1" | json '"\n".join(value["failures"][:20])'
  [ "$(docker exec "$CLIENT" cat "$1" | json 'len(value["failures"])')" = 0 ] || fail "$2: requests failed"
  [ "$(docker exec "$CLIENT" cat "$1" | json 'value["connections"]')" = 1 ] || fail "$2: the kept connection had to be opened again"
}
balance_stages() {
  step "balance: web on every node behind app-1's proxy; local-first sends app-1's requests to its own web"
  local node
  for node in $NODES; do on "$node" engine pull -q "$TRAEFIK_IMAGE" >/dev/null; done
  docker exec "$CLIENT" openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj "/CN=$INGRESS_HOST" \
    -addext "subjectAltName=DNS:$INGRESS_HOST" -keyout /tmp/ingress.key -out /tmp/ingress.crt 2>/dev/null
  docker cp "$CLIENT:/tmp/ingress.crt" "$TMP/ingress.crt"
  docker cp "$CLIENT:/tmp/ingress.key" "$TMP/ingress.key"
  docker cp "$TMP/load.py" "$CLIENT:/tmp/load.py"
  docker exec "$CLIENT" sh -c "grep -q ' $INGRESS_HOST' /etc/hosts || echo '$(address_of app-1) $INGRESS_HOST' >>/etc/hosts"
  mkdir -p "$TMP/repo/web"
  # shellcheck disable=SC2016 # expands when the script runs in the container
  printf '#!/bin/sh\nprintf "Content-Type: text/plain\\r\\n\\r\\n"\necho "request $QUERY_STRING" >/proc/1/fd/1\nhostname\n' >"$TMP/repo/web/id"
  printf '#!/bin/sh\n{ [ -e /state/sick ] || [ -e /data/sick ]; } && sleep 5 && exit 1\nprintf "Content-Type: text/plain\\r\\n\\r\\nok\\n"\n' >"$TMP/repo/web/health"
  chmod 755 "$TMP/repo/web/id" "$TMP/repo/web/health"
  health_compose v1 >"$TMP/repo/compose.yaml"
  NETWORK_LINES='secrets:
  tls.cert: {storage: {provider: age, location: tls.cert}}
  tls.key: {storage: {provider: age, location: tls.key}}'
  STACK_LINES='    ingress: {domain: shop, service: web, port: 8080}'
  SERVICE_LINES='      web: {stack: app, placement: all, drain: 2s}'
  CLUSTER_LINES=''
  ENVIRONMENT_LINES="    domains: {shop: $INGRESS_HOST}
    tls: {certificate: tls.cert, key: tls.key}"
  commit "$(users alice bob)" "web on every node, local-first"
  publish
  # Every node redacts its logs with the environment's secrets.
  for node in $NODES; do
    secret_on "$node" tls.cert "$TMP/ingress.crt"
    secret_on "$node" tls.key "$TMP/ingress.key"
  done
  deploy_ingress >"$TMP/balance-apply.log" 2>&1 || { cat "$TMP/balance-apply.log"; fail "web behind the ingress did not deploy"; }
  declare -A WEB
  for node in $NODES; do WEB[$node]=$(web_of "$node"); [ -n "${WEB[$node]}" ] || fail "$node runs no web"; done
  loads /tmp/load-local.json
  sleep 8
  stop_loads /tmp/load-local.json "local-first"
  [ "$(served_by /tmp/load-local.json)" = "${WEB[app-1]:0:12}" ] \
    || fail "local-first: requests reached $(served_by /tmp/load-local.json), not only app-1's web ${WEB[app-1]:0:12}"
  printf '  ok   every request went to app-1'"'"'s own web\n'

  step "balance: spread sends requests to the webs on every node, with no failed request while it changes"
  ENVIRONMENT_LINES="    domains: {shop: $INGRESS_HOST}
    tls: {certificate: tls.cert, key: tls.key}
    ingress: {balance: spread}"
  commit "$(users alice bob)" "spread over every node"
  publish
  mm_as alice deploy plan | tee "$TMP/balance-plan.log"
  has "$TMP/balance-plan.log" "sends each request to a healthy container on any node" \
    "https://$INGRESS_HOST -> app/web port 8080, spread over every node"
  loads /tmp/load-spread.json
  sleep 3
  deploy_ingress >"$TMP/balance-spread.log" 2>&1 || { cat "$TMP/balance-spread.log"; fail "the spread deployment failed"; }
  # Each commit is a checkout of its own that web mounts: every web is new.
  for node in $NODES; do WEB[$node]=$(web_of "$node"); done
  on app-1 cat /var/lib/mm/ingress/dynamic/mm.yaml >"$TMP/balance-dynamic.yaml"
  [ "$(grep -c 'url: http://' "$TMP/balance-dynamic.yaml")" = 3 ] || { cat "$TMP/balance-dynamic.yaml"; fail "the route does not list the three webs"; }
  has "$TMP/balance-dynamic.yaml" "strategy: p2c"
  sleep 15
  stop_loads /tmp/load-spread.json "spread"
  local served
  served=$(served_by /tmp/load-spread.json)
  printf '  served by: %s\n' "$served"
  for node in app-2 app-3; do
    case " $served " in *" ${WEB[$node]:0:12} "*) ;; *) fail "spread: no request reached $node's new web" ;; esac
  done
}

# health_audits NODE RESULT: the node's audit records of restarts mm made.
health_audits() { on "$1" sh -c "grep '\"result\":\"$2\"' /var/lib/mm/audit.jsonl || true"; }
# addresses_of NODE ID: the container's addresses, one per line.
addresses_of() {
  # shellcheck disable=SC2016 # Go templates
  on "$1" engine inspect --format '{{range .NetworkSettings.Networks}}{{.IPAddress}} {{end}}' "$2" | tr ' ' '\n' | grep .
}
# routed NODE ID: whether app-1's proxy sends requests to the container.
routed() {
  local address
  for address in $(addresses_of "$1" "$2"); do
    on app-1 grep -q "http://$address:" /var/lib/mm/ingress/dynamic/mm.yaml && return 0
  done
  return 1
}
health_of() { on "$1" engine inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{end}}' "$2"; }
health_stages() {
  step "health: a web whose check keeps failing is restarted once unhealthy for 10s, and is answered again"
  SERVICE_LINES='      web: {stack: app, placement: all, drain: 2s, health: {after: 10s, max-restarts: 2, window: 10m}}'
  commit "$(users alice bob)" "web is restarted after 10s unhealthy, twice at most"
  publish
  mm_as alice deploy plan | tee "$TMP/health-plan.log"
  has "$TMP/health-plan.log" "health app/web: restart after 10s unhealthy, at most 2 times in 10m"
  deploy_ingress >"$TMP/health-apply.log" 2>&1 || { cat "$TMP/health-apply.log"; fail "the health settings did not deploy"; }
  declare -A WEB
  local node
  for node in $NODES; do WEB[$node]=$(web_of "$node"); done
  loads /tmp/load-health.json
  local sick=${WEB[app-3]} began sick_at restarted_at
  began=$(started_ms app-3 "$sick")
  on app-3 engine exec "$sick" touch /state/sick
  sick_at=$(date +%s%3N)
  for _ in $(seq 1 90); do [ "$(started_ms app-3 "$sick")" != "$began" ] && break; sleep 1; done
  restarted_at=$(started_ms app-3 "$sick")
  [ "$restarted_at" != "$began" ] || { on app-3 tail -n 40 /tmp/mmd.log >&2; fail "app-3's unhealthy web was not restarted within 90 s"; }
  [ "$(web_of app-3)" = "$sick" ] || fail "the restart replaced app-3's web instead of restarting it"
  [ "$((restarted_at - sick_at))" -ge 10000 ] || fail "app-3's web was restarted $((restarted_at - sick_at)) ms after it fell sick, before 10 s"
  printf '  ok   restarted %s ms after it fell sick\n' "$((restarted_at - sick_at))"
  for _ in $(seq 1 30); do [ "$(health_of app-3 "$sick")" = healthy ] && routed app-3 "$sick" && break; sleep 1; done
  [ "$(health_of app-3 "$sick")" = healthy ] || fail "app-3's restarted web is not healthy"
  routed app-3 "$sick" || fail "app-1's proxy does not send requests to app-3's restarted web"
  local audit=''
  for _ in $(seq 1 30); do
    audit=$(health_audits app-3 health-restart | tail -n 1)
    [ -n "$audit" ] && break
    sleep 1
  done
  printf '%s\n' "$audit" | tee "$TMP/health-audit.log"
  has "$TMP/health-audit.log" '"principal":"mmd health"' '"service":"web"' '"reason":"unhealthy for 1'
  mm_as alice service status | tee "$TMP/health-status.log"
  has "$TMP/health-status.log" "restarted web: 1 time by mm, the last" "on app-3"
  mm_as alice service logs web --node app-3 --tail 200 >"$TMP/health-logs.log"
  grep -v '| request' "$TMP/health-logs.log" || true
  has "$TMP/health-logs.log" "mm: restarted"

  step "health: a web that fails again after each restart is restarted twice, then left out until mm service restart"
  local flapping=${WEB[app-2]} limited=''
  on app-2 engine exec "$flapping" touch /data/sick
  for _ in $(seq 1 150); do
    limited=$(health_audits app-2 health-restart-limit)
    [ -n "$limited" ] && break
    sleep 1
  done
  [ -n "$limited" ] || { on app-2 tail -n 40 /tmp/mmd.log >&2; fail "app-2's web never reached its restart limit"; }
  printf '%s\n' "$limited" | tee "$TMP/health-limit.log"
  has "$TMP/health-limit.log" '"code":"MM_SERVICE_RESTART_LIMIT"' "after 2 restarts within 10m"
  [ "$(health_audits app-2 health-restart | grep -c .)" = 2 ] || fail "app-2's web was not restarted exactly twice"
  on app-2 engine exec "$flapping" rm -f /data/sick
  for _ in $(seq 1 30); do [ "$(health_of app-2 "$flapping")" = healthy ] && break; sleep 1; done
  [ "$(health_of app-2 "$flapping")" = healthy ] || fail "app-2's web did not pass its check again"
  sleep 8
  if routed app-2 "$flapping"; then fail "the proxy sends requests to a web past its restart limit while others serve"; fi
  [ "$(health_audits app-2 health-restart | grep -c .)" = 2 ] || fail "app-2's web was restarted past its limit"
  mm_as alice service status | tee "$TMP/health-limit-status.log"
  has "$TMP/health-limit-status.log" "restart limit web: mm stopped restarting" "MM_SERVICE_RESTART_LIMIT" "no longer restarted"
  mm_as alice doctor >"$TMP/health-doctor.log" 2>&1 || true
  has "$TMP/health-doctor.log" "MM_SERVICE_RESTART_LIMIT"
  mm_as alice service restart web | tee "$TMP/health-restart.log"
  for _ in $(seq 1 30); do routed app-2 "$flapping" && break; sleep 1; done
  routed app-2 "$flapping" || fail "mm service restart did not bring app-2's web back into the routes"
  if mm_as alice service status | grep -q "restart limit web"; then fail "the status still reports the limit after mm service restart"; fi

  step "health: no restart while a deployment holds the environment"
  JOB_LINES='jobs:
  hold:
    module: api
    service: web
    run: before-deploy
    timeout: 2m
    command: ["sleep", "25"]'
  health_compose v2 >"$TMP/repo/compose.yaml"
  commit "$(users alice bob)" "web v2 after a slow job"
  publish
  sick=$(web_of app-3)
  on app-3 engine exec "$sick" touch /state/sick
  sick_at=$(date +%s%3N)
  deploy_ingress >"$TMP/health-rollout.log" 2>&1 || { cat "$TMP/health-rollout.log"; fail "the deployment with a slow job failed"; }
  local ended
  ended=$(date +%s%3N)
  [ "$((ended - sick_at))" -ge 25000 ] || fail "the deployment took $((ended - sick_at)) ms, too short to show anything"
  python3 - "$sick_at" "$ended" "$(health_audits app-3 health-restart)" <<'PY' || fail "app-3 restarted its web during the deployment"
import json, sys
began, ended = int(sys.argv[1]), int(sys.argv[2])
during = [line for line in sys.argv[3].splitlines() if line and began <= json.loads(line)["timestamp_unix_ms"] <= ended]
print("restarts during the deployment:", len(during))
sys.exit(1 if during else 0)
PY
  [ "$(web_of app-3)" != "$sick" ] || fail "the deployment did not replace app-3's web"
  JOB_LINES=''
  stop_loads /tmp/load-health.json "restarts"
}

low_ports_stages() {
  step "low ports: a service that runs as a user other than root listens on port 80"
  cat >"$TMP/repo/compose.yaml" <<YAML
services:
  low:
    image: $WORKLOAD_IMAGE
    user: "1000:1000"
    command: ["httpd", "-f", "-p", "80", "-h", "/tmp"]
YAML
  SERVICE_LINES='      low: {stack: app, placement: app-1}'
  commit "$(users alice bob)" "a service that is not root on port 80"
  publish
  mm_as alice deploy plan | tee "$TMP/low-ports-plan.log"
  if [ "$ENGINE" = podman ]; then
    has "$TMP/low-ports-plan.log" "ports: low may listen below 1024 as any user"
  elif grep -q 'may listen below 1024' "$TMP/low-ports-plan.log"; then
    fail "Docker Engine lets every container listen below 1024; the plan should not say so"
  fi
  deploy_ingress | tee "$TMP/low-ports-apply.log"
  [ "$(grep -c ' applied ' "$TMP/low-ports-apply.log")" = 3 ] || fail "three nodes should report applied"
  local low
  low=$(on app-1 engine ps -q --filter label=com.docker.compose.service=low)
  [ -n "$low" ] || fail "low is not running on app-1"
  [ "$(on app-1 engine exec "$low" id -u)" = 1000 ] || fail "low should run as user 1000"
  on app-1 engine exec "$low" sh -c 'wget -q -S -O /dev/null http://127.0.0.1:80/ 2>&1 || true' | grep -q 'HTTP/1' \
    || fail "low does not answer on port 80"
  printf '  ok   low runs as user 1000 and answers on port 80 on %s\n' "$ENGINE"
}

job_services_stages() {
  step "job services: ops runs as a service on app-2; web's secret file for local is neither mounted nor required in production"
  cat >"$TMP/repo/compose.yaml" <<YAML
services:
  web:
    image: $WORKLOAD_IMAGE
    command: ["sh", "-c", "exec sleep infinity"]
    stop_grace_period: 1s
  ops:
    image: $WORKLOAD_IMAGE
    command: ["sh", "-c", "echo ops-started && exec sleep infinity"]
YAML
  printf 'admin-password-for-the-live-stage\n' >"$TMP/admin.password"
  # A secret file needs the module's environment template.
  [ -f "$TMP/repo/api/.env.example" ] || : >"$TMP/repo/api/.env.example"
  local networks=$NETWORK_LINES module=$MODULE_LINES node ops mounts web_container first second
  local ALICE_SECRETS=1
  MODULE_LINES='    env: {template: .env.example}'
  NETWORK_LINES="$networks
secrets:
  admin.password: {storage: {provider: age, location: admin.password}}
  local.token: {storage: {provider: age, location: local.token}}"
  local web='      web:
        stack: app
        placement: all
        secret-files:
          local-token: {secret: local.token, environments: [local]}'
  SERVICE_LINES="$web
      ops: {stack: app, placement: app-2}"
  JOB_LINES=''
  commit "$(users alice bob)" "ops as a service, and a secret file only local mounts"
  publish
  deploy_ingress | tee "$TMP/job-services-before.log"
  [ -n "$(on app-2 engine ps -q --filter label=com.docker.compose.service=ops)" ] || fail "ops does not run on app-2"
  web_container=$(on app-1 engine ps -q --filter label=com.docker.compose.service=web | head -n 1)
  if on app-1 engine exec "$web_container" test -e /run/secrets/local-token; then
    fail "web has the secret file that only local mounts"
  fi
  mm_as alice --output json secret status >"$TMP/job-services-values.json"
  [ "$(json '[v.get("required", False) or bool(v.get("services")) for n in value["nodes"] for v in n["values"] if v["key"] == "local.token"]' <"$TMP/job-services-values.json")" = "[False, False, False]" ] \
    || fail "production's hosts count local.token, which only local mounts, as a value web needs"

  step "job services: with role: job, ops has no container, and its job runs on app-2 with a secret no running container holds"
  SERVICE_LINES="$web
      ops:
        stack: app
        role: job
        placement: app-2
        secret-files:
          admin: {secret: admin.password}"
  JOB_LINES='jobs:
  bootstrap:
    module: api
    service: ops
    run: before-deploy
    timeout: 2m
    command: ["sh", "-c", "test -s /run/secrets/admin && echo admin-secret-on-$(hostname) $(sha256sum /run/secrets/admin | cut -c1-12)"]'
  commit "$(users alice bob)" "a job service with a secret only its job reads"
  publish
  secret_on app-2 admin.password "$TMP/admin.password"
  mm_as alice deploy plan | tee "$TMP/job-services-plan.log"
  has "$TMP/job-services-plan.log" "job only: api/ops, no container until one of its jobs runs" "bootstrap (before-deploy) on app-2"
  mm_as alice --output json deploy plan >"$TMP/job-services-plan.json"
  if grep -q 'local.token\|local-token' "$TMP/job-services-plan.json"; then
    fail "the plan for production lists the secret file that only local mounts"
  fi
  grep -q 'admin.password' "$TMP/job-services-plan.json" || fail "the plan does not list the job service's secret"
  deploy_ingress | tee "$TMP/job-services-apply.log"
  has "$TMP/job-services-apply.log" "job bootstrap"
  mm_as alice deploy logs | tee "$TMP/job-services-logs.log"
  has "$TMP/job-services-logs.log" "app-2 bootstrap | admin-secret-on-"
  for node in $NODES; do
    ops=$(on "$node" engine ps -a -q --filter label=com.docker.compose.service=ops --filter label=com.docker.compose.oneoff=False)
    [ -z "$ops" ] || fail "$node has a container of the job service ops: $ops"
    # shellcheck disable=SC2016 # expands on the node
    mounts=$(on "$node" sh -c 'for id in $(engine ps -q); do engine inspect --format "{{range .Mounts}}{{.Destination}} {{end}}" "$id"; done')
    case " $mounts " in
      *" /run/secrets/"*) fail "a running container on $node holds a secret file: $mounts" ;;
    esac
    [ -n "$(on "$node" engine ps -q --filter label=com.docker.compose.service=web)" ] || fail "web does not run on $node"
  done
  printf '  ok   no node runs a container of ops or holds its secret in a running container\n'
  first=$(sed -n 's/.*app-2 bootstrap | admin-secret-on-[^ ]* //p' "$TMP/job-services-logs.log" | tail -n 1)

  step "job services: a value staged for the job service reaches its job at the next deployment"
  printf 'second-admin-password' | docker exec -i "$CLIENT" mm --root /project --mm-home /home-alice secret set admin.password --stdin \
    | tee "$TMP/job-services-stage.log"
  has "$TMP/job-services-stage.log" "Staged admin.password"
  mm_as alice deploy redeploy --yes 2>&1 | tee "$TMP/job-services-redeploy.log"
  has "$TMP/job-services-redeploy.log" "job bootstrap"
  mm_as alice deploy logs | tee "$TMP/job-services-logs-2.log"
  second=$(sed -n 's/.*app-2 bootstrap | admin-secret-on-[^ ]* //p' "$TMP/job-services-logs-2.log" | tail -n 1)
  [ "$second" = "$(printf 'second-admin-password' | sha256sum | cut -c1-12)" ] \
    || fail "the job read '$second' after the redeploy, not the staged value (it read '$first' before)"
  [ -z "$(on app-2 engine ps -a -q --filter label=com.docker.compose.service=ops --filter label=com.docker.compose.oneoff=False)" ] \
    || fail "the redeploy started a container of ops"
  mm_as alice service status | tee "$TMP/job-services-status.log"
  has "$TMP/job-services-status.log" "job only"
  if grep -q "service 'ops' is deployed here and has no container" "$TMP/job-services-status.log"; then
    fail "the status calls the job service missing"
  fi
  JOB_LINES=''
  NETWORK_LINES=$networks
  MODULE_LINES=$module
}

# MM_LIVE_ONLY=job-services runs only that, right after access is synced.
if [ "${MM_LIVE_ONLY:-}" = job-services ]; then
  job_services_stages
  printf '\nPASS: a job service and environment-bound secret files\n'
  exit 0
fi
# Release facts: templates read the release a deployment runs, the
# inputs CI gives the plan are part of it and of what each node records, and
# a rollback runs with the inputs of the deployment it returns to.
# shellcheck disable=SC2016 # the variables expand in web's container
release_facts_stages() {
  local TOP_LINES BINDING_LINES MODULE_LINES SERVICE_LINES commit node first second
  local ALICE_SECRETS=1
  # web_says NODE COMMAND: the command's output in the node's web container.
  web_says() {
    on "$1" sh -c 'engine exec "$(engine ps -q --filter label=com.docker.compose.service=web)" sh -c "$1"' sh "$2"
  }
  step "release facts: a plan without a declared input, or with a malformed one, is refused"
  printf 'services:\n  web:\n    image: %s\n    command: ["sh", "-c", "exec sleep infinity"]\n    stop_grace_period: 1s\n' \
    "$WORKLOAD_IMAGE" >"$TMP/repo/compose.yaml"
  printf 'BUILD_ID= # mm:from=RELEASE.BUILD_ID required\nAPP_VERSION= # mm:from=APP.VERSION\n' >"$TMP/repo/api/release.env"
  printf 'build {{ RELEASE.BUILD_ID }} release {{ MM.RELEASE }} commit {{ MM.COMMIT }}\n' >"$TMP/repo/api/release.conf.tmpl"
  TOP_LINES='release-inputs:
  BUILD_ID: {pattern: "[0-9]+"}'
  BINDING_LINES='bindings:
  APP.VERSION: {value-ref: MM.RELEASE}'
  MODULE_LINES='    env: {template: release.env}'
  SERVICE_LINES='      web:
        stack: app
        placement: all
        config-files:
          /etc/release.conf: {template: release.conf.tmpl}'
  commit "$(users alice bob)" "web reads the release it runs"
  publish
  commit=$(git_repo rev-parse HEAD)
  if mm_as alice deploy plan >"$TMP/release-missing.log" 2>&1; then fail "a plan without BUILD_ID was made"; fi
  has "$TMP/release-missing.log" MM_RELEASE_INPUT "--release-input BUILD_ID=VALUE"
  if mm_as alice deploy plan --release-input BUILD_ID=12a >"$TMP/release-malformed.log" 2>&1; then
    fail "a plan with a BUILD_ID that is not a number was made"
  fi
  has "$TMP/release-malformed.log" MM_RELEASE_INPUT "does not match its pattern"
  if mm_as alice env set RELEASE.BUILD_ID 7 >"$TMP/release-staged.log" 2>&1; then
    fail "a release input was staged on the hosts"
  fi
  has "$TMP/release-staged.log" "is a release fact"

  step "release facts: the inputs are part of the plan, and every node runs with them"
  first=$(mm_as alice --output json deploy plan --release-input BUILD_ID=1 | json 'value["plan_hash"]')
  second=$(mm_as alice --output json deploy plan --release-input BUILD_ID=2 | json 'value["plan_hash"]')
  [ "$first" != "$second" ] || fail "another input value made the same plan $first"
  mm_as alice deploy plan --release-input BUILD_ID=1 | tee "$TMP/release-plan.log"
  has "$TMP/release-plan.log" "BUILD_ID=1" "sha-${commit:0:12}"
  first=$(mm_as alice --output json deploy plan --release-input BUILD_ID=1 | json 'value["plan_hash"]')
  mm_as alice deploy apply --plan "$first" --yes | tee "$TMP/release-apply-1.log"
  [ "$(grep -c ' applied ' "$TMP/release-apply-1.log")" = 3 ] || fail "three nodes should report applied"
  for node in $NODES; do
    [ "$(web_says "$node" 'echo $BUILD_ID $APP_VERSION')" = "1 sha-${commit:0:12}" ] \
      || fail "$node: web runs with '$(web_says "$node" 'echo $BUILD_ID $APP_VERSION')'"
    [ "$(web_says "$node" 'cat /etc/release.conf')" = "build 1 release sha-${commit:0:12} commit $commit" ] \
      || fail "$node: web's config file is '$(web_says "$node" 'cat /etc/release.conf')'"
  done
  printf '  ok   every node runs web with BUILD_ID=1 in its environment and its config file\n'

  step "release facts: the next release's name and inputs replace web, and status shows them per node"
  second=$(mm_as alice --output json deploy plan --release v2 --release-input BUILD_ID=2 | json 'value["plan_hash"]')
  mm_as alice deploy apply --plan "$second" --yes | tee "$TMP/release-apply-2.log"
  [ "$(grep -c ' applied ' "$TMP/release-apply-2.log")" = 3 ] || fail "three nodes should report applied"
  for node in $NODES; do
    [ "$(web_says "$node" 'echo $BUILD_ID $APP_VERSION')" = "2 v2" ] \
      || fail "$node: web runs with '$(web_says "$node" 'echo $BUILD_ID $APP_VERSION')'"
  done
  mm_as alice deploy status | tee "$TMP/release-status.log"
  [ "$(grep -c 'runs: .* release v2  inputs BUILD_ID=2' "$TMP/release-status.log")" = 3 ] \
    || fail "status should show release v2 with BUILD_ID=2 on every node"
  [ "$(grep -c 'before: .* inputs BUILD_ID=1' "$TMP/release-status.log")" = 3 ] \
    || fail "status should show BUILD_ID=1 as what every node ran before"

  step "release facts: a rollback runs the same commit with the inputs it ran with before"
  if mm_as alice deploy rollback --release-input BUILD_ID=3 >"$TMP/release-rollback-refused.log" 2>&1; then
    fail "a rollback took a release input"
  fi
  has "$TMP/release-rollback-refused.log" "leave out --release and --release-input"
  mm_as alice deploy rollback --yes | tee "$TMP/release-rollback.log"
  has "$TMP/release-rollback.log" "with inputs BUILD_ID=1"
  for node in $NODES; do
    [ "$(web_says "$node" 'echo $BUILD_ID $APP_VERSION')" = "1 sha-${commit:0:12}" ] \
      || fail "$node: after the rollback web runs with '$(web_says "$node" 'echo $BUILD_ID $APP_VERSION')'"
    [ "$(web_says "$node" 'cat /etc/release.conf')" = "build 1 release sha-${commit:0:12} commit $commit" ] \
      || fail "$node: after the rollback web's config file is '$(web_says "$node" 'cat /etc/release.conf')'"
  done
  printf '  ok   the rollback returned every node to BUILD_ID=1\n'

  step "release facts: a redeploy keeps the release the hosts run, and takes no other"
  if mm_as alice deploy redeploy --release-input BUILD_ID=3 >"$TMP/release-redeploy-refused.log" 2>&1; then
    fail "a redeploy took a release input"
  fi
  has "$TMP/release-redeploy-refused.log" MM_RELEASE_INPUT "a redeploy keeps the release"
  mm_as alice deploy redeploy --yes | tee "$TMP/release-redeploy.log"
  has "$TMP/release-redeploy.log" "with inputs BUILD_ID=1"
  for node in $NODES; do
    [ "$(web_says "$node" 'echo $BUILD_ID $APP_VERSION')" = "1 sha-${commit:0:12}" ] \
      || fail "$node: after the redeploy web runs with '$(web_says "$node" 'echo $BUILD_ID $APP_VERSION')'"
  done
  printf '  ok   the redeploy kept BUILD_ID=1 on every node\n'
}

# --- tunnel -------------------------------------------------------------------
# Cloudflare Tunnel ingress (`type: cloudflared`) on every node, with a
# stand-in for cloudflared: a busybox image with the `cloudflared` script
# below and Traefik. It reads the token file and the configuration mm writes,
# answers /ready on the metrics address, and runs Traefik as the edge on the
# node's loopback address (the metrics port + 1000), sending each host of the
# configuration where the configuration says. A request to the edge takes the
# way a request through Cloudflare takes once it reaches the connector. The
# token's `s` says how the stand-in behaves: `slow…` is ready after 8 s,
# `never…` never.
TUNNEL_IMAGE=localhost/mm-live-cloudflared:stub
TUNNEL_ID=6ff42ae2-765d-4adf-8112-31c55c1551ef
TUNNEL_SHOP=shop.tunnel.test
TUNNEL_HOOKS=hooks.tunnel.test
# tunnel_token BEHAVIOUR FILE: a token of the tunnel in FILE, as
# `cloudflared tunnel token` prints it, with a secret no other token has.
tunnel_token() {
  printf '{"a":"0123456789abcdef0123456789abcdef","t":"%s","s":"%s-%s%s%s"}' "$TUNNEL_ID" "$1" "$RANDOM" "$RANDOM" "$RANDOM" | base64 -w0 >"$2"
}
# secret_on base64-encodes the file it is given: the secret is the token.
tunnel_secret() { base64 -d "$2" >"$2.json"; secret_on "$1" cloudflare.tunnel-token "$2.json"; }
connector_of() { on "$1" engine ps -q --no-trunc --filter label=io.mm.component=cluster-tunnel | head -n 1; }
# edge NODE PYTHON: runs PYTHON on the node, with `edge(method, host, path)`
# sending one request to the stand-in's edge and returning (status, headers,
# body).
edge() {
  on "$1" python3 -c "
import http.client, json, glob
port = 8960
def edge(method, host, path):
    connection = http.client.HTTPConnection('127.0.0.1', port, timeout=20)
    connection.request(method, path, headers={'Host': host})
    response = connection.getresponse()
    return response.status, dict(response.getheaders()), response.read().decode(errors='replace')
$2"
}
tunnel_diagnostics() {
  # shellcheck disable=SC2016 # expands on the node
  on "$1" sh -c '
    cat /var/lib/mm/ingress/compose.yaml /var/lib/mm/ingress/tunnels/*/config.yaml
    for container in $(engine ps -aq --filter label=io.mm.component=cluster-tunnel) mm-ingress-proxy-1; do
      engine logs --tail 30 "$container" 2>&1
    done
    tail -n 30 /tmp/mmd.log' >&2 || true
}

tunnel_stages() {
  step "tunnel: every node has the stand-in connector; web and a websocket backend run behind public ingress first"
  local node
  for node in $NODES; do on "$node" engine pull -q "$TRAEFIK_IMAGE" >/dev/null; done
  mkdir -p "$TMP/stub" "$TMP/repo/web" "$TMP/repo/ws"
  cat >"$TMP/stub/cloudflared" <<'SH'
#!/bin/sh
set -eu
config='' metrics='' token=''
while [ $# -gt 0 ]; do
  case $1 in
    --config) config=$2; shift ;;
    --metrics) metrics=$2; shift ;;
    --token-file) token=$2; shift ;;
  esac
  shift
done
secret=$(base64 -d "$token")
port=${metrics##*:}
rule=$(sed -n 's/^.*hostname: *//p' "$config" | while read -r host; do printf 'Host(`%s`) || ' "$host"; done)
proxy=$(sed -n 's/^.*service: *\(http:.*\)$/\1/p' "$config" | head -n 1)
printf 'http:\n  routers:\n    edge:\n      rule: "%s"\n      service: proxy\n  services:\n    proxy:\n      loadBalancer:\n        servers: [{url: "%s"}]\n' "${rule% || }" "$proxy" >/tmp/edge.yaml
mkdir -p /tmp/www
case $secret in *'"s":"slow'*) sleep 8 ;; esac
case $secret in *'"s":"never'*) ;; *) printf '{"status":200,"readyConnections":4,"connectorId":"stub"}' >/tmp/www/ready ;; esac
httpd -p "$metrics" -h /tmp/www
exec traefik --entrypoints.edge.address="127.0.0.1:$((port + 1000))" --providers.file.filename=/tmp/edge.yaml --log.level=ERROR
SH
  chmod 755 "$TMP/stub/cloudflared"
  printf 'FROM %s AS traefik\nFROM %s\nCOPY --from=traefik /usr/local/bin/traefik /usr/local/bin/traefik\nCOPY cloudflared /usr/local/bin/cloudflared\nENTRYPOINT ["cloudflared", "--no-autoupdate"]\n' \
    "$TRAEFIK_IMAGE" "$WORKLOAD_IMAGE" >"$TMP/stub/Dockerfile"
  for node in $NODES; do
    on "$node" rm -rf /tmp/stub
    docker cp "$TMP/stub" "$(container "$node"):/tmp/stub"
    on "$node" env DOCKER_BUILDKIT=0 engine build -q -t "$TUNNEL_IMAGE" /tmp/stub >/dev/null
  done
  # shellcheck disable=SC2016 # expands when the script runs in the container
  printf '#!/bin/sh\nprintf "Content-Type: text/plain\\r\\n\\r\\n"\necho "request $REQUEST_METHOD $QUERY_STRING" >/proc/1/fd/1\nhostname\n' >"$TMP/repo/web/id"
  # shellcheck disable=SC2016 # expands when the script runs in the container
  printf '#!/bin/sh\nprintf "Content-Type: text/plain\\r\\n\\r\\n"\necho "hooks $REQUEST_METHOD $PATH_INFO" >/proc/1/fd/1\necho "hooks $REQUEST_METHOD $PATH_INFO"\n' >"$TMP/repo/web/hooks"
  printf '#!/bin/sh\nprintf "Content-Type: text/plain\\r\\n\\r\\nok\\n"\n' >"$TMP/repo/web/health"
  # A websocket server in busybox: nc runs this for each connection. It
  # answers the upgrade, sends one text frame, and echoes what it gets.
  cat >"$TMP/repo/ws/ws.sh" <<'SH'
#!/bin/sh
key=''
while IFS= read -r line; do
  line=$(printf '%s' "$line" | tr -d '\r')
  [ -z "$line" ] && break
  case $line in [Ss]ec-[Ww]eb[Ss]ocket-[Kk]ey:*) key=$(printf '%s' "${line#*:}" | tr -d ' ') ;; esac
done
accept=$(printf '%s258EAFA5-E914-47DA-95CA-C5AB0DC85B11' "$key" | sha1sum | cut -d' ' -f1 | xxd -r -p | base64)
printf 'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: %s\r\n\r\n' "$accept"
printf '\201\005hello'
exec cat
SH
  chmod 755 "$TMP/repo/web/id" "$TMP/repo/web/hooks" "$TMP/repo/web/health" "$TMP/repo/ws/ws.sh"
  cat >"$TMP/repo/compose.yaml" <<YAML
services:
  web:
    image: $WORKLOAD_IMAGE
    command: ["httpd", "-f", "-p", "8080", "-h", "/www"]
    volumes: ["./web:/www/cgi-bin:ro"]
    healthcheck: {test: ["CMD", "wget", "-q", "-O", "/dev/null", "http://127.0.0.1:8080/cgi-bin/health"], interval: 1s, timeout: 2s, retries: 2}
    stop_grace_period: 2s
  socket:
    image: $WORKLOAD_IMAGE
    command: ["nc", "-lk", "-p", "8081", "-e", "/ws/ws.sh"]
    volumes: ["./ws:/ws:ro"]
    stop_grace_period: 1s
YAML
  NETWORK_LINES='secrets:
  cloudflare.tunnel-token: {storage: {provider: age, location: cloudflare.tunnel-token}}'
  STACK_LINES='    ingress:
      - {domain: shop, service: web, port: 8080}
      - {domain: shop, route: /app, service: socket, port: 8081}
      - domain: hooks
        service: web
        port: 8080
        allow:
          - {methods: [GET, HEAD], path: "^/cgi-bin/hooks/status$"}
          - {methods: [POST], path: "^/cgi-bin/hooks/[^/]+/events$"}'
  SERVICE_LINES='      web: {stack: app, placement: all, drain: 2s}
      socket: {stack: app, placement: app-2, drain: 2s}'
  CLUSTER_LINES='    cluster: {ingress: all}'
  local domains="    domains: {shop: $TUNNEL_SHOP, hooks: $TUNNEL_HOOKS}"
  ENVIRONMENT_LINES="$domains
    ingress: {headers: {X-Robots-Tag: noindex}}"
  commit "$(users alice bob)" "web and a websocket backend behind public ingress on every node"
  publish
  deploy_ingress >"$TMP/tunnel-public.log" 2>&1 || { cat "$TMP/tunnel-public.log"; fail "the routes did not deploy behind public ingress"; }
  for node in $NODES; do
    docker exec "$CLIENT" python3 -c "
import socket
socket.create_connection(('$(address_of "$node")', 80), timeout=5).close()" || fail "$node does not take port 80 with public ingress"
  done
  printf '  ok   every node takes port 80 while the environment uses public ingress\n'

  step "tunnel: type cloudflared without a token does not load; a plan without the token's value names the command"
  ENVIRONMENT_LINES="$domains
    ingress: {type: cloudflared}"
  write_config "$(users alice bob)"
  docker cp "$TMP/repo/.config/mm.yaml" "$CLIENT:/project/.config/mm.yaml"
  if mm_as alice config validate >"$TMP/tunnel-no-token.log" 2>&1; then fail "type: cloudflared without a token loaded"; fi
  has "$TMP/tunnel-no-token.log" "cloudflared.token" "mm secret set"
  ENVIRONMENT_LINES="$domains
    ingress:
      type: cloudflared
      cloudflared: {token: cloudflare.tunnel-token, image: $TUNNEL_IMAGE}
      headers: {X-Robots-Tag: noindex}"
  commit "$(users alice bob)" "the routes through a Cloudflare Tunnel"
  publish
  if mm_as alice deploy plan >"$TMP/tunnel-unset.log" 2>&1; then fail "a plan without the tunnel token's value was made"; fi
  has "$TMP/tunnel-unset.log" MM_TUNNEL_TOKEN_MISSING "secret set cloudflare.tunnel-token"

  step "tunnel: the plan names the tunnel and each node's hosts; connectors start one node at a time, each once it is ready"
  tunnel_token slow "$TMP/token-slow"
  for node in $NODES; do tunnel_secret "$node" "$TMP/token-slow"; done
  mm_as alice deploy plan | tee "$TMP/tunnel-plan.log"
  has "$TMP/tunnel-plan.log" "ingress: Cloudflare Tunnel $TUNNEL_ID, token in secret cloudflare.tunnel-token; app-1, app-2, app-3 run a connector each, open no port" \
    "app-1: the connector sends $TUNNEL_HOOKS, $TUNNEL_SHOP to this node's proxy and answers 404 to any other host" \
    "app-3: the connector sends $TUNNEL_HOOKS, $TUNNEL_SHOP to this node's proxy" \
    "https://$TUNNEL_SHOP/app -> app/socket port 8081, sets X-Robots-Tag" \
    "https://$TUNNEL_HOOKS -> app/web port 8080, lets through GET|HEAD ^/cgi-bin/hooks/status\$, POST ^/cgi-bin/hooks/[^/]+/events\$ and answers 404 to the rest"
  deploy_ingress | tee "$TMP/tunnel-apply.log"
  [ "$(grep -c ' applied ' "$TMP/tunnel-apply.log")" = 3 ] || { for node in $NODES; do tunnel_diagnostics "$node"; done; fail "three nodes should report applied"; }
  declare -A CONNECTOR STARTED
  for node in $NODES; do
    CONNECTOR[$node]=$(connector_of "$node")
    [ -n "${CONNECTOR[$node]}" ] || { tunnel_diagnostics "$node"; fail "$node runs no connector"; }
    STARTED[$node]=$(started_ms "$node" "${CONNECTOR[$node]}")
  done
  printf '  connectors started at app-1 %s, app-2 %s, app-3 %s\n' "${STARTED[app-1]}" "${STARTED[app-2]}" "${STARTED[app-3]}"
  [ "$((STARTED[app-2] - STARTED[app-1]))" -ge 8000 ] && [ "$((STARTED[app-3] - STARTED[app-2]))" -ge 8000 ] \
    || fail "a node started its connector before the one before it was ready"
  printf '  ok   each node started its connector once the previous one was ready (8 s)\n'

  step "tunnel: no node takes web traffic from outside; the proxy and the connector listen on the loopback address only"
  for node in $NODES; do
    docker exec "$CLIENT" python3 -c "
import socket, sys
open_ports = []
for port in (80, 443, 7955, 7960, 8960):
    try:
        socket.create_connection(('$(address_of "$node")', port), timeout=3).close()
        open_ports.append(port)
    except OSError:
        pass
sys.exit('open: %s' % open_ports if open_ports else 0)" || fail "$node takes connections from outside"
    if on "$node" grep -q '80:8080\|443:8443' /var/lib/mm/ingress/compose.yaml; then fail "$node's proxy still publishes 80 or 443"; fi
    on "$node" sh -c "grep -q '127.0.0.1:7955:8081' /var/lib/mm/ingress/compose.yaml" || fail "$node's proxy does not take the tunnel on 127.0.0.1:7955"
  done
  on app-1 cat "/var/lib/mm/ingress/tunnels/$PROJECT_ID-production/config.yaml" | tee "$TMP/tunnel-config.yaml"
  has "$TMP/tunnel-config.yaml" "hostname: $TUNNEL_HOOKS" "hostname: $TUNNEL_SHOP" "service: http://127.0.0.1:7955"
  [ "$(tail -n 1 "$TMP/tunnel-config.yaml")" = "- service: http_status:404" ] || fail "the connector's last rule is not 404"
  printf '  ok   ports 80, 443, 7955, 7960 and 8960 are closed from outside on every node\n'

  step "tunnel: requests through each node's connector reach the routes; the allowlist answers 404 to the rest"
  for node in $NODES; do
    edge "$node" "
status, headers, body = edge('GET', '$TUNNEL_SHOP', '/cgi-bin/id')
assert status == 200, (status, body)
assert headers.get('X-Robots-Tag') == 'noindex', headers
for method, path, expected in [
    ('GET', '/cgi-bin/hooks/status', 200), ('HEAD', '/cgi-bin/hooks/status', 200),
    ('POST', '/cgi-bin/hooks/n1/events', 200), ('POST', '/cgi-bin/hooks/n1', 404),
    ('DELETE', '/cgi-bin/hooks/n1/events', 404), ('GET', '/cgi-bin/hooks/n1', 404),
    ('GET', '/cgi-bin/id', 404),
]:
    status, headers, body = edge(method, '$TUNNEL_HOOKS', path)
    assert status == expected, (method, path, status, body)
    if expected == 404:
        assert body.strip() == '404 page not found', (method, path, body)
status, _, _ = edge('GET', 'other.tunnel.test', '/')
assert status == 404, status
print('ok')" || { tunnel_diagnostics "$node"; fail "requests through $node's connector were not answered as the routes say"; }
  done
  local refused
  refused=$(for node in $NODES; do on "$node" sh -c 'for web in $(engine ps -q --filter label=com.docker.compose.service=web); do engine logs "$web" 2>&1; done'; done | grep -c 'hooks POST /n1$\|hooks DELETE\|hooks GET /n1$' || true)
  [ "$refused" = 0 ] || fail "$refused request(s) the allowlist refuses reached web"
  printf '  ok   allowed requests reach web with X-Robots-Tag, the rest get 404 from the proxy\n'

  step "tunnel: a websocket upgrades through the connector and the proxy, to socket on app-2 ahead of web at /"
  for node in app-1 app-3; do
    edge "$node" "
import base64, hashlib, os, socket
key = base64.b64encode(os.urandom(16)).decode()
stream = socket.create_connection(('127.0.0.1', port), timeout=20)
stream.sendall(('GET /app/ws HTTP/1.1\r\nHost: $TUNNEL_SHOP\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: %s\r\nSec-WebSocket-Version: 13\r\n\r\n' % key).encode())
head = b''
while b'\r\n\r\n' not in head:
    head += stream.recv(1)
lines = head.decode().split('\r\n')
assert lines[0].startswith('HTTP/1.1 101'), lines
expected = base64.b64encode(hashlib.sha1((key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest()).decode()
fields = dict(line.split(': ', 1) for line in lines[1:] if ': ' in line)
assert {name.lower(): value for name, value in fields.items()}.get('sec-websocket-accept') == expected, lines
frame = b''
while len(frame) < 7:
    frame += stream.recv(7 - len(frame))
assert frame == b'\x81\x05hello', frame
mask = os.urandom(4)
sent = bytes([0x81, 0x84]) + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(b'ping'))
stream.sendall(sent)
echoed = b''
while len(echoed) < len(sent):
    echoed += stream.recv(len(sent) - len(echoed))
assert echoed == sent, echoed
print('ok')" || { tunnel_diagnostics "$node"; fail "the websocket did not upgrade through $node's connector"; }
  done
  printf '  ok   the upgrade, a frame from the server, and an echo came through\n'

  step "tunnel: the token is in its file only, readable by the connector alone"
  base64 -d "$TMP/token-slow" | python3 -c "import json, sys; print(json.load(sys.stdin)['s'])" >"$TMP/token-slow.secret"
  cat "$TMP/token-slow" >>"$TMP/token-slow.secret"
  printf '\n' >>"$TMP/token-slow.secret"
  for node in $NODES; do
    docker cp "$TMP/token-slow.secret" "$(container "$node"):/tmp/token.patterns"
    # shellcheck disable=SC2016 # expands on the node
    on "$node" sh -c '
      table=/var/lib/mm/ingress/tunnels/'"$PROJECT_ID"'-production
      [ "$(stat -c "%u %a" "$table/token")" = "65532 400" ] || { echo "the token file is $(stat -c "%u %a" "$table/token")" >&2; exit 1; }
      grep -qF -f /tmp/token.patterns "$table/token" || { echo "the token file does not hold the token" >&2; exit 1; }
      found=$(grep -rlF -f /tmp/token.patterns /var/lib/mm /tmp/mmd.log /etc/mm 2>/dev/null | grep -v "^$table/token$" || true)
      [ -z "$found" ] || { echo "the token is in $found" >&2; exit 1; }
      for directory in /proc/[0-9]*; do
        for part in cmdline environ; do
          ! tr "\\0" "\\n" <"$directory/$part" 2>/dev/null | grep -qF -f /tmp/token.patterns || { echo "the token is in $directory/$part" >&2; exit 1; }
        done
      done
      for container in $(engine ps -aq); do
        ! engine inspect "$container" | grep -qF -f /tmp/token.patterns || { echo "the token is in the inspect output of $container" >&2; exit 1; }
      done' || fail "$node: the tunnel token appears outside its file"
  done
  docker cp "$TMP/token-slow.secret" "$CLIENT:/tmp/token.patterns"
  mm_as alice --output json deploy plan >"$TMP/tunnel-plan.json" 2>&1 || true
  for file in "$TMP"/tunnel-*.log "$TMP"/tunnel-plan.json; do docker cp "$file" "$CLIENT:/tmp/" >/dev/null; done
  docker exec "$CLIENT" sh -c '! grep -rlF -f /tmp/token.patterns /tmp/tunnel-* /home-alice /home-bob /project' || fail "the tunnel token is in the client's output or mm home"
  printf '  ok   0400 for user 65532; in no log, plan, Compose file, inspect output, argument or environment\n'

  step "tunnel: status and doctor show each connector's connections; a stopped connector is reported and the others serve"
  mm_as alice service status | tee "$TMP/tunnel-status.log"
  has "$TMP/tunnel-status.log" "node app-1: the tunnel connector holds 4 connections to Cloudflare" "node app-3: the tunnel connector holds 4 connections to Cloudflare"
  mm_as alice doctor >"$TMP/tunnel-doctor.log" 2>&1 || true
  if grep -q MM_TUNNEL "$TMP/tunnel-doctor.log"; then cat "$TMP/tunnel-doctor.log"; fail "doctor reports a tunnel problem while every connector is ready"; fi
  on app-3 engine stop "${CONNECTOR[app-3]}" >/dev/null
  mm_as alice service status | tee "$TMP/tunnel-status-stopped.log"
  has "$TMP/tunnel-status-stopped.log" "node app-3: the tunnel connector does not answer"
  mm_as alice doctor >"$TMP/tunnel-doctor-stopped.log" 2>&1 || true
  has "$TMP/tunnel-doctor-stopped.log" MM_TUNNEL_NOT_CONNECTED "node 'app-3'"
  edge app-1 "assert edge('GET', '$TUNNEL_SHOP', '/cgi-bin/id')[0] == 200" || fail "app-1 stopped serving while app-3's connector was stopped"
  on app-3 engine start "${CONNECTOR[app-3]}" >/dev/null
  printf '  ok   app-3 was reported as not connected while app-1 served\n'

  step "tunnel: a connector that does not connect stops the rollout on its node, which goes back to the one it ran"
  tunnel_token never "$TMP/token-never"
  tunnel_secret app-1 "$TMP/token-never"
  local before
  before=$(on app-1 sha256sum "/var/lib/mm/ingress/tunnels/$PROJECT_ID-production/token")
  if deploy_ingress >"$TMP/tunnel-never.log" 2>&1; then fail "a deployment whose connector never connects was applied"; fi
  cat "$TMP/tunnel-never.log"
  has "$TMP/tunnel-never.log" "on node 'app-1'" "holds no connection to Cloudflare after 90 s" "went back to the routes and connector it ran before"
  for node in app-2 app-3; do
    [ "$(connector_of "$node")" = "${CONNECTOR[$node]}" ] || fail "$node changed its connector after app-1 failed"
  done
  [ "$(on app-1 sha256sum "/var/lib/mm/ingress/tunnels/$PROJECT_ID-production/token")" = "$before" ] || fail "app-1 did not go back to the token it ran"
  for _ in $(seq 1 30); do edge app-1 "assert edge('GET', '$TUNNEL_SHOP', '/cgi-bin/id')[0] == 200" 2>/dev/null && break; sleep 1; done
  edge app-1 "assert edge('GET', '$TUNNEL_SHOP', '/cgi-bin/id')[0] == 200" || { tunnel_diagnostics app-1; fail "app-1 does not serve with the connector it ran before"; }
  tunnel_secret app-1 "$TMP/token-slow"
  printf '  ok   app-1 went back to its connector; app-2 and app-3 were not changed\n'

  step "tunnel: without routes every node takes its connector and proxy away"
  STACK_LINES=''
  ENVIRONMENT_LINES=''
  NETWORK_LINES=''
  commit "$(users alice bob)" "no routes"
  publish
  deploy_ingress >"$TMP/tunnel-withdraw.log" 2>&1 || { cat "$TMP/tunnel-withdraw.log"; fail "the deployment without routes failed"; }
  for node in $NODES; do
    [ -z "$(connector_of "$node")" ] || fail "$node keeps a connector"
    if on "$node" engine inspect mm-ingress-proxy-1 >/dev/null 2>&1; then fail "$node keeps the proxy"; fi
    on "$node" test ! -e "/var/lib/mm/ingress/tunnels/$PROJECT_ID-production" || fail "$node keeps the tunnel's files"
  done
  SERVICE_LINES='      api: {stack: app}'
  CLUSTER_LINES=''
}

# --- the managed secret store (request 02) --------------------------------------
# Setup's node commands by hand; openssl signs in place of setup's keeper.
STORE_VERSION=2.4.4
STORE_SHA256=a02f6e3e710616ae2687592a65bcbd72989500686d505bbd35ec5402b2847923
STORE_URL=https://github.com/openbao/openbao/releases/download/v$STORE_VERSION/bao_${STORE_VERSION}_Linux_x86_64.tar.gz
STORE_NEXT=2.5.5
STORE_NEXT_SHA256=2c5577707e97fc95c2086950f39880ead5e45b356c94388e5cb606f5a5c2b697
STORE_NEXT_URL=https://github.com/openbao/openbao/releases/download/v$STORE_NEXT/bao_${STORE_NEXT}_Linux_x86_64.tar.gz
store_stages() {
  local TOP_LINES MODULE_LINES SERVICE_LINES ALICE_SECRETS=1 id node index name revision before after sync leader through
  id="production-$(printf %s "$PROJECT_ID" | tr -d - | tail -c 12)"
  mkdir -p "$TMP/store"
  # store NODE COMMAND: `mmd openbao COMMAND` on NODE, stdin passed on.
  store() { docker exec -i "$(container "$1")" /usr/libexec/mmd openbao "$2" --project "$PROJECT_ID" --environment production; }
  store_status() { store "$1" status </dev/null; }
  # listening_only_inside NODE INDEX PORT...: the member listens on each PORT
  # of the node's mesh address, on 127.0.0.1:8200, and nowhere else.
  listening_only_inside() {
    local node=$1 index=$2 address listening port
    shift 2
    listening=$(on "$node" ss -Hltn | awk '{print $4}' | grep -E ':820[01]$' | sort -u | tr '\n' ' ')
    for port in "$@"; do
      case " $listening" in
        *" 10.90.0.$index:$port "*) ;;
        *) fail "$node: the member does not listen on 10.90.0.$index:$port: '$listening'" ;;
      esac
    done
    case " $listening" in *" 127.0.0.1:8200 "*) ;; *) fail "$node: the member does not listen on 127.0.0.1:8200" ;; esac
    for address in $listening; do
      case $address in
        "10.90.0.$index:"*|127.0.0.1:*) ;;
        *) fail "$node: the member listens on $address, beyond loopback and its mesh address" ;;
      esac
    done
  }
  store_recipient_of() {
    on "$1" /usr/libexec/mmd secrets recipient --project "$PROJECT_ID" --environment production | json 'value["recipient"]'
  }
  unit() { on "$1" systemctl "$2" "mm-openbao-$id.service"; }

  step "secret store: every node installs OpenBao $STORE_VERSION from an archive with the pinned digest"
  if [ -n "${MM_LIVE_OPENBAO_ARCHIVE:-}" ]; then
    cp "$MM_LIVE_OPENBAO_ARCHIVE" "$TMP/store/archive.tar.gz"
  else
    curl -fsSL --max-time 600 -o "$TMP/store/archive.tar.gz" "$STORE_URL"
  fi
  echo "$STORE_SHA256  $TMP/store/archive.tar.gz" | sha256sum -c --quiet - || fail "the archive is not OpenBao $STORE_VERSION"
  cat >"$TMP/store/systemctl" <<'SH'
#!/bin/sh
# A stand-in for systemctl: start runs a unit's ExecStart as its User.
set -eu
command=$1; shift
[ "${1:-}" = --quiet ] && shift
unit=${1:-}
pidfile=/run/live-$unit.pid
running() {
  [ -f "$pidfile" ] || return 1
  state=$(awk '{print $3}' "/proc/$(cat "$pidfile")/stat" 2>/dev/null)
  [ -n "$state" ] && [ "$state" != Z ]
}
stop() { if running; then kill "$(cat "$pidfile")"; while running; do sleep 0.2; done; fi; rm -f "$pidfile"; }
case $command in
  daemon-reload|enable) ;;
  is-active) running ;;
  stop) stop ;;
  start|restart)
    [ "$command" = restart ] && stop
    running && exit 0
    exec_start=$(sed -n 's/^ExecStart=//p' "/etc/systemd/system/$unit")
    user=$(sed -n 's/^User=//p' "/etc/systemd/system/$unit")
    setsid setpriv --reuid="$user" --regid="$user" --init-groups sh -c "exec $exec_start" >>"/tmp/$unit.log" 2>&1 </dev/null &
    echo $! >"$pidfile" ;;
  *) echo "live systemctl: '$command' is not stood in for" >&2; exit 1 ;;
esac
SH
  chmod 755 "$TMP/store/systemctl"
  for node in $NODES; do
    name=$(container "$node")
    on "$node" mkdir -p /var/lib/mm-openbao/archives /etc/systemd/system
    docker cp "$TMP/store/archive.tar.gz" "$name:/var/lib/mm-openbao/archives/${STORE_URL##*/}"
    docker cp "$TMP/store/systemctl" "$name:/usr/bin/systemctl"
    on "$node" sh -c "$(cat "$ROOT/core/src/adapters/target/cluster_shell/store-install.sh")" mm-setup \
      "$id" "$STORE_VERSION" x86_64 "$STORE_URL" "$STORE_SHA256"
    on "$node" test -x "/usr/libexec/mm-openbao/$STORE_VERSION/bao" || fail "$node: bao is not installed"
    [ "$(on "$node" stat -c '%U %a' "/var/lib/mm-openbao/$id/data")" = "mm-openbao 700" ] \
      || fail "$node: the store's data is not the mm-openbao account's alone"
  done

  step "secret store: each node makes its own TLS key; the environment's authority signs it; the members start"
  openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 30 -subj "/CN=live secret store" \
    -addext "basicConstraints=critical,CA:TRUE,pathlen:0" -addext "keyUsage=critical,keyCertSign,cRLSign" \
    -keyout "$TMP/store/ca.key" -out "$TMP/store/ca.pem" 2>/dev/null
  # member_spec NODE [CERTIFICATE [VERSION]]: what setup gives the node's mmd.
  member_spec() {
    python3 - "$1" "$TMP/store/ca.pem" "${2:-}" "${3:-$STORE_VERSION}" <<'PY'
import json, sys
node, ca, certificate, version = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
spec = {"provider": "deploy-secrets", "node": node, "version": version,
        "members": [{"name": "app-%d" % i, "address": "10.90.0.%d" % i} for i in (1, 2, 3)],
        "ca-pem": open(ca).read()}
if certificate:
    spec["certificate-pem"] = open(certificate).read()
print(json.dumps(spec))
PY
  }
  index=1
  for node in $NODES; do
    member_spec "$node" | store "$node" member >"$TMP/store/$node-request.json"
    json 'value["csr"]' <"$TMP/store/$node-request.json" >"$TMP/store/$node.csr"
    on "$node" test -f "/etc/mm-openbao/$id/member-key.pem" || fail "$node made no key of its own"
    printf 'subjectAltName=IP:10.90.0.%s,IP:127.0.0.1,DNS:%s,DNS:localhost\nextendedKeyUsage=serverAuth,clientAuth\nkeyUsage=critical,digitalSignature\n' \
      "$index" "$node" >"$TMP/store/$node.ext"
    openssl x509 -req -in "$TMP/store/$node.csr" -CA "$TMP/store/ca.pem" -CAkey "$TMP/store/ca.key" -CAcreateserial \
      -days 60 -extfile "$TMP/store/$node.ext" -out "$TMP/store/$node.pem" 2>/dev/null
    member_spec "$node" "$TMP/store/$node.pem" | store "$node" member >"$TMP/store/$node-member.json"
    [ "$(json '(value["changed"], value["initialized"], value["sealed"])' <"$TMP/store/$node-member.json")" = "(True, False, True)" ] \
      || { cat "$TMP/store/$node-member.json"; fail "$node: the member did not start uninitialised"; }
    [ "$(member_spec "$node" | store "$node" member | json '(value.get("csr"), value["changed"])')" = "(None, False)" ] \
      || fail "$node: a second run changed the member or asked for another certificate"
    listening_only_inside "$node" "$index" 8200
    index=$((index + 1))
  done

  step "secret store: app-1 initialises the store; app-2 and app-3 unseal with keys sealed to their own mmd"
  operator=$(store_recipient_of app-1)
  python3 - "$operator" $(for node in $NODES; do printf '%s=%s ' "$node" "$(store_recipient_of "$node")"; done) <<'PY' >"$TMP/store/init-request.json"
import json, sys
print(json.dumps({"operator": sys.argv[1], "recipients": dict(entry.split("=", 1) for entry in sys.argv[2:])}))
PY
  store app-1 init <"$TMP/store/init-request.json" >"$TMP/store/init.json"
  [ "$(json 'sorted(value["sealed"])' <"$TMP/store/init.json")" = "['app-2', 'app-3']" ] || fail "app-1 sealed no document for app-2 and app-3"
  for node in app-2 app-3; do
    json "value['sealed']['$node']" <"$TMP/store/init.json" | store "$node" join >"$TMP/store/$node-join.json"
    [ "$(json '(value["member"], value["sealed"])' <"$TMP/store/$node-join.json")" = "(True, False)" ] || fail "$node did not unseal"
  done
  if json "value['sealed']['app-3']" <"$TMP/store/init.json" | store app-2 join >/dev/null 2>&1; then
    fail "app-2 opened a document sealed to app-3"
  fi
  store_whole() {
    local node
    for node in $NODES; do
      store_status "$node" >"$TMP/store/$node-status.json"
      [ "$(json '(value["sealed"], value.get("leader") is not None, sorted((p["name"], p["voter"]) for p in value.get("peers", [])))' <"$TMP/store/$node-status.json")" \
        = "(False, True, [('app-1', True), ('app-2', True), ('app-3', True)])" ] || return 1
    done
  }
  for _ in $(seq 1 45); do store_whole && break; sleep 2; done
  store_whole || { cat "$TMP/store/"*-status.json; fail "the store is not three unsealed voters with a leader"; }
  index=1
  for node in $NODES; do listening_only_inside "$node" "$index" 8200 8201; index=$((index + 1)); done
  if grep -l "keys_base64\|root_token\|secret_id\|role_id\|BEGIN EC PRIVATE KEY\|BEGIN PRIVATE KEY" "$TMP/store/"*.json; then
    fail "a store command printed a key or a login"
  fi
  printf '  ok   three members, one leader, the root token revoked, nothing secret printed\n'

  step "secret store: a value set over the relay is planned, delivered, and then one version in the store"
  TOP_LINES='providers:
  deploy-secrets:
    driver: openbao
    mode: managed
    scope: environment
    release: "2.4"
secrets:
  db.password: {storage: {provider: deploy-secrets, location: db/password}}
  api.token: {storage: {provider: age, location: api.token}}'
  [ -f "$TMP/repo/api/.env.example" ] || : >"$TMP/repo/api/.env.example"
  MODULE_LINES='    env: {template: .env.example}'
  SERVICE_LINES='      web:
        stack: app
        placement: all
        secret-files:
          db: {secret: db.password}
          token: {secret: api.token}'
  cat >"$TMP/repo/compose.yaml" <<YAML
services:
  web:
    image: $WORKLOAD_IMAGE
    command: ["sh", "-c", "exec sleep infinity"]
    stop_grace_period: 1s
YAML
  commit "$(users alice bob)" "web reads its database password from the managed store"
  publish
  # alice's `secrets: true` reaches the nodes with an access sync.
  sync=$(mm_as ops --output json team sync | json 'value["plan_hash"]')
  mm_as ops team sync --plan "$sync" --yes >/dev/null
  if mm_as alice deploy plan >"$TMP/store/plan-unset.log" 2>&1; then fail "a plan without the password was made"; fi
  has "$TMP/store/plan-unset.log" "db.password" "secret set"
  printf 'store-value-of-the-live-stage-one' | docker exec -i "$CLIENT" mm --root /project --mm-home /home-alice secret set db.password --stdin \
    | tee "$TMP/store/set-1.log"
  has "$TMP/store/set-1.log" "Staged db.password"
  printf 'api-token-of-the-live-stage' | docker exec -i "$CLIENT" mm --root /project --mm-home /home-alice secret set api.token --stdin >/dev/null
  deploy_ingress | tee "$TMP/store/deploy-1.log"
  web_digest() { on "$1" sh -c 'engine exec "$(engine ps -q --filter label=com.docker.compose.service=web)" sha256sum /run/secrets/db' | cut -c1-64; }
  for node in $NODES; do
    [ "$(web_digest "$node")" = "$(printf 'store-value-of-the-live-stage-one' | sha256sum | cut -c1-64)" ] || fail "$node: web has not the value set"
  done
  mm_as alice --output json secret status >"$TMP/store/status-1.json"
  revision=$(json '[v["active"]["revision"] for n in value["nodes"] for v in n["values"] if v["key"] == "db.password"][0]' <"$TMP/store/status-1.json")
  [ "$(json "(sorted({(v.get('active') or {}).get('revision') for n in value['nodes'] for v in n['values'] if v['key'] == 'db.password'}), [v.get('staged') for n in value['nodes'] for v in n['values'] if v['key'] == 'db.password'])" <"$TMP/store/status-1.json")" \
    = "(['$revision'], [None, None, None])" ] || { cat "$TMP/store/status-1.json"; fail "the value is not live with its staged revision on every node"; }
  before=$(on app-1 engine ps -q --filter label=com.docker.compose.service=web)
  mm_as alice deploy redeploy --yes >"$TMP/store/redeploy.log" 2>&1 || { cat "$TMP/store/redeploy.log"; fail "the redeploy failed"; }
  [ "$(on app-1 engine ps -q --filter label=com.docker.compose.service=web)" = "$before" ] \
    || fail "the version the store wrote replaced web again: its revision changed when it went live"
  printf '  ok   live as one revision on every node; a redeploy replaces no container\n'

  step "secret store: every node restarts at once, and each mmd unseals its own member"
  for node in $NODES; do unit "$node" stop; stop_mmd "$node"; done
  for node in $NODES; do unit "$node" start; start_mmd "$node"; done
  for node in $NODES; do wait_mmd "$node"; done
  for _ in $(seq 1 60); do store_whole && break; sleep 2; done
  store_whole || { cat "$TMP/store/"*-status.json; fail "the members did not come back unsealed without an operator"; }
  mm_as alice deploy plan >"$TMP/store/plan-after-restart.log" 2>&1 || { cat "$TMP/store/plan-after-restart.log"; fail "no plan after the restart"; }
  printf '  ok   unsealed by mmd, and a deployment plans\n'

  step "secret store: with app-3's member down, values are read and written through the others"
  unit app-3 stop
  printf 'store-value-of-the-live-stage-two' | docker exec -i "$CLIENT" mm --root /project --mm-home /home-alice secret set db.password --stdin >/dev/null
  deploy_ingress >"$TMP/store/deploy-2.log" 2>&1 || { cat "$TMP/store/deploy-2.log"; fail "the deployment with one member down failed"; }
  for node in $NODES; do
    [ "$(web_digest "$node")" = "$(printf 'store-value-of-the-live-stage-two' | sha256sum | cut -c1-64)" ] || fail "$node: web has not the second value"
  done
  mm_as alice --output json secret status >"$TMP/store/status-2.json"
  [ "$(json "(lambda live: len(live) == 1 and None not in live and '$revision' not in live)({(v.get('active') or {}).get('revision') for n in value['nodes'] for v in n['values'] if v['key'] == 'db.password'})" <"$TMP/store/status-2.json")" = True ] \
    || { cat "$TMP/store/status-2.json"; fail "the second value is not live on every node"; }
  mm_as alice deploy plan >"$TMP/store/plan-member-down.log" 2>&1 || { cat "$TMP/store/plan-member-down.log"; fail "app-3 cannot plan without its member"; }
  mm_as ops target nodes >"$TMP/store/nodes-down.log" 2>&1 || true
  has "$TMP/store/nodes-down.log" "Secret store deploy-secrets: 3 member(s)" "not running"
  mm_as alice doctor >"$TMP/store/doctor-down.log" 2>&1 || true
  has "$TMP/store/doctor-down.log" MM_SECRET_STORE_DEGRADED "app-3: mm-openbao-$id.service is not running"
  if mm_as ops -e production secret provider status deploy-secrets >"$TMP/store/provider-status-down.log" 2>&1; then
    cat "$TMP/store/provider-status-down.log"; fail "provider status passed with app-3's member down"
  fi
  has "$TMP/store/provider-status-down.log" "Secret store deploy-secrets of production on target" \
    MM_SECRET_STORE_DEGRADED "app-3: mm-openbao-$id.service is not running"
  unit app-3 start
  for _ in $(seq 1 30); do store_whole && break; sleep 2; done
  store_whole || fail "app-3's member did not come back as a voter"
  mm_as ops target nodes | tee "$TMP/store/nodes.log"
  [ "$(grep -c 'OpenBao 2.4.4 *unsealed' "$TMP/store/nodes.log")" = 3 ] || fail "mm target nodes does not show three unsealed members"
  grep -q '^Secret store deploy-secrets: 3 member(s), leader app-[123]$' "$TMP/store/nodes.log" || fail "mm target nodes names no leader"
  mm_as alice doctor >"$TMP/store/doctor.log" 2>&1 || true
  if grep -q MM_SECRET_STORE_ "$TMP/store/doctor.log"; then cat "$TMP/store/doctor.log"; fail "doctor reports the store although every member serves"; fi
  mm_as ops -e production secret provider status deploy-secrets >"$TMP/store/provider-status.log" 2>&1 \
    || { cat "$TMP/store/provider-status.log"; fail "provider status fails although every member serves"; }
  [ "$(grep -c 'OpenBao 2.4.4 *unsealed' "$TMP/store/provider-status.log")" = 3 ] \
    || { cat "$TMP/store/provider-status.log"; fail "provider status does not show three unsealed members"; }
  mm_as ops -e production --output json secret provider status deploy-secrets >"$TMP/store/provider-status.json"
  [ "$(json '(value["provider"], value["version"], [m["node"] for m in value["members"] if m["status"]["sealed"] is False], value.get("leader") is not None, value.get("diagnostics", []))' <"$TMP/store/provider-status.json")" \
    = "('deploy-secrets', '2.4.4', ['app-1', 'app-2', 'app-3'], True, [])" ] \
    || { cat "$TMP/store/provider-status.json"; fail "provider status --output json is not the members' report"; }
  printf '  ok   written with two members, read on app-3 through another, named by doctor and provider status, and app-3 back in\n'

  step "secret store: no container reaches the store's ports, on its own node or another"
  for node in $NODES; do
    for address in 127.0.0.1 10.90.0.1 10.90.0.2 10.90.0.3; do
      for port in 8200 8201; do
        if on "$node" sh -c 'engine exec "$(engine ps -q --filter label=com.docker.compose.service=web)" nc -w 3 "$1" "$2" </dev/null' sh "$address" "$port" >/dev/null 2>&1; then
          fail "web on $node reached $address:$port"
        fi
      done
    done
    on "$node" sh -c 'exec 3<>/dev/tcp/10.90.0.1/8200' 2>/dev/null || on "$node" python3 -c 'import socket; socket.create_connection(("10.90.0.1", 8200), 3)' \
      || fail "$node itself does not reach app-1's member over the mesh"
  done
  printf '  ok   web on every node reaches neither port of any member; the nodes reach each other\n'

  step "secret store: no value in what the client printed, the nodes' logs, or the audit log"
  for node in $NODES; do
    on "$node" sh -c "cat /tmp/mmd.log /tmp/mm-openbao-$id.service.log /var/lib/mm-openbao/$id/audit.log 2>/dev/null; true" >"$TMP/store/$node-logs.txt"
  done
  [ -s "$TMP/store/app-1-logs.txt" ] || fail "no logs were read"
  grep -q '"type":"request"' "$TMP/store/"*-logs.txt || fail "the audit device logged no request"
  on app-1 /usr/libexec/mmd openbao audit --project "$PROJECT_ID" --environment production --limit 5 >"$TMP/store/audit.json"
  [ "$(json 'len(value)' <"$TMP/store/audit.json")" = 5 ] || fail "mmd openbao audit printed no five records"
  if grep -l "store-value-of-the-live-stage" "$TMP/store/"*; then fail "a value is in the output or a log above"; fi
  printf '  ok   the value is in none of them; the audit device logged the requests\n'

  step "secret store: a snapshot, a store made again from nothing, and the snapshot put back"
  revision=$(json '[v["active"]["revision"] for n in value["nodes"] for v in n["values"] if v["key"] == "db.password"][0]' <"$TMP/store/status-2.json")
  mm_as ops -e production secret provider snapshot deploy-secrets --to /tmp/store.snap >"$TMP/store/snapshot.log" 2>&1 \
    || { cat "$TMP/store/snapshot.log"; fail "no snapshot was taken"; }
  docker exec "$CLIENT" sh -c 'head -n 1 /tmp/store.snap' | grep -qx -- '-----BEGIN AGE ENCRYPTED FILE-----' || fail "the snapshot is not sealed"
  for node in $NODES; do
    unit "$node" stop
    stop_mmd "$node"
    on "$node" sh -c "rm -rf /var/lib/mm-openbao/$id/data/* /var/lib/mm-home/secrets/openbao-connections.age /var/lib/mm-home/identities/openbao-connections.key"
    start_mmd "$node"
  done
  for node in $NODES; do wait_mmd "$node"; done
  for node in $NODES; do
    member_spec "$node" "$TMP/store/$node.pem" | store "$node" member >"$TMP/store/$node-fresh.json"
    [ "$(json '(value["initialized"], value["has-keys"])' <"$TMP/store/$node-fresh.json")" = "(False, False)" ] \
      || { cat "$TMP/store/$node-fresh.json"; fail "$node: the member is not fresh"; }
  done
  store app-1 init <"$TMP/store/init-request.json" >"$TMP/store/init-fresh.json"
  for node in app-2 app-3; do
    json "value['sealed']['$node']" <"$TMP/store/init-fresh.json" | store "$node" join >/dev/null
  done
  for _ in $(seq 1 45); do store_whole && break; sleep 2; done
  store_whole || fail "the fresh store is not whole"
  mm_as ops -e production secret provider restore deploy-secrets --file /tmp/store.snap | tee "$TMP/store/restore-plan.log"
  has "$TMP/store/restore-plan.log" "Restore deploy-secrets (production) from the snapshot taken on app-" "replaces every value"
  command=$(sed -n 's/^Apply: mm //p' "$TMP/store/restore-plan.log")
  [ -n "$command" ] || fail "the restore plan printed no command"
  # shellcheck disable=SC2086 # the printed command, split into its words
  mm_as ops $command >"$TMP/store/restore.log" 2>&1 || { cat "$TMP/store/restore.log"; fail "the restore failed"; }
  for _ in $(seq 1 45); do store_whole && break; sleep 2; done
  store_whole || { cat "$TMP/store/"*-status.json; fail "the restored store is not whole"; }
  mm_as alice --output json secret status >"$TMP/store/status-restored.json"
  [ "$(json "sorted({(v.get('active') or {}).get('revision') for n in value['nodes'] for v in n['values'] if v['key'] == 'db.password'})" <"$TMP/store/status-restored.json")" \
    = "['$revision']" ] || { cat "$TMP/store/status-restored.json"; fail "the restored store does not hold the value with its revision"; }
  before=$(on app-1 engine ps -q --filter label=com.docker.compose.service=web)
  mm_as alice deploy redeploy --yes >"$TMP/store/redeploy-restored.log" 2>&1 \
    || { cat "$TMP/store/redeploy-restored.log"; fail "no deployment after the restore"; }
  [ "$(on app-1 engine ps -q --filter label=com.docker.compose.service=web)" = "$before" ] \
    || fail "the restore changed the value's revision: web was replaced"
  if grep -l "store-value-of-the-live-stage" "$TMP/store/"*.log; then fail "a value is in the output above"; fi
  printf '  ok   every value back with its revision; nothing was replaced\n'

  step "secret store: a secret moves from the age store into the store, and its container stays"
  mm_as alice secret migrate --from age --to deploy-secrets | tee "$TMP/store/migrate.log"
  has "$TMP/store/migrate.log" "api.token: written to deploy-secrets at api.token as revision"
  mm_as alice secret migrate --from age --to deploy-secrets | tee "$TMP/store/migrate-again.log"
  has "$TMP/store/migrate-again.log" "api.token: deploy-secrets holds revision"
  TOP_LINES=${TOP_LINES/'api.token: {storage: {provider: age, location: api.token}}'/'api.token: {storage: {provider: deploy-secrets, location: api.token}}'}
  commit "$(users alice bob)" "api.token moves to the managed store"
  publish
  before=$(on app-1 engine ps -q --filter label=com.docker.compose.service=web)
  deploy_ingress >"$TMP/store/deploy-migrated.log" 2>&1 || { cat "$TMP/store/deploy-migrated.log"; fail "no deployment after the move"; }
  [ "$(on app-1 engine ps -q --filter label=com.docker.compose.service=web)" = "$before" ] \
    || fail "moving api.token to the store replaced web"
  [ "$(on app-1 sh -c 'engine exec "$(engine ps -q --filter label=com.docker.compose.service=web)" sha256sum /run/secrets/token' | cut -c1-64)" \
    = "$(printf 'api-token-of-the-live-stage' | sha256sum | cut -c1-64)" ] || fail "web has not the token after the move"
  printf '  ok   moved with its revision; web kept running with the same token\n'

  step "secret store: a member that is merely down keeps its Raft entry; one whose data is gone rejoins"
  # facts NODE: what setup's plan reads of NODE's member, by its own script.
  facts() { on "$1" sh -c "$(cat "$ROOT/core/src/adapters/target/cluster_shell/store-facts.sh")" mm-setup "$id"; }
  # removals: the remove-peer requests the members' audit devices logged.
  removals() {
    local node
    for node in $NODES; do
      on "$node" sh -c "cat /var/lib/mm-openbao/$id/audit.log* 2>/dev/null; true"
    done | grep '"type":"request"' | grep -c '"path":"sys/storage/raft/remove-peer"' || true
  }
  [ "$(removals)" = 0 ] || fail "a Raft peer was removed before any member lost its data"
  unit app-3 stop
  [ "$(facts app-3 | json '(value["data"], value["record"]["initialized"])')" = "(True, True)" ] \
    || fail "app-3's member is down with its data, and its facts say otherwise"
  store_status app-1 | json 'sorted(p["name"] for p in value["peers"])' | grep -qx "\['app-1', 'app-2', 'app-3'\]" \
    || fail "app-3 left the Raft configuration while its member was only down"
  member_spec app-3 "$TMP/store/app-3.pem" | store app-3 member >"$TMP/store/app-3-down-member.json"
  [ "$(json '(value["initialized"], value["sealed"], value["changed"])' <"$TMP/store/app-3-down-member.json")" = "(True, False, False)" ] \
    || { cat "$TMP/store/app-3-down-member.json"; fail "setup's member step did not bring app-3's member back with its data"; }
  for _ in $(seq 1 45); do store_whole && break; sleep 2; done
  store_whole || fail "app-3's member did not vote again after it was down"
  [ "$(removals)" = 0 ] || fail "a member that was only down was removed from the Raft configuration"

  store app-1 versions </dev/null >"$TMP/store/versions-before.json"
  [ "$(json 'sorted(s["location"] for s in value)' <"$TMP/store/versions-before.json")" = "['api.token', 'db/password']" ] \
    || { cat "$TMP/store/versions-before.json"; fail "the store does not list every secret"; }
  [ "$(json '[len(s["versions"]) for s in value if s["location"] == "db/password"][0] > 1' <"$TMP/store/versions-before.json")" = True ] \
    || fail "db.password has no history to keep"
  unit app-3 stop
  stop_mmd app-3
  on app-3 sh -c "rm -rf /var/lib/mm-openbao/$id /etc/mm-openbao/$id /etc/systemd/system/mm-openbao-$id.service /usr/libexec/mm-openbao /var/lib/mm-home/secrets/openbao-connections.age /var/lib/mm-home/identities/openbao-connections.key"
  start_mmd app-3
  wait_mmd app-3
  [ "$(facts app-3 | json '(value["data"], value["record"])')" = "(False, None)" ] || fail "app-3's facts still show a member"
  store_status app-1 >"$TMP/store/app-1-status.json"
  [ "$(json '(value["sealed"], value.get("leader") is not None, "app-3" in [p["name"] for p in value["peers"]])' <"$TMP/store/app-1-status.json")" \
    = "(False, True, True)" ] || { cat "$TMP/store/app-1-status.json"; fail "app-1 does not serve with app-3 still in the Raft configuration"; }
  on app-3 sh -c "$(cat "$ROOT/core/src/adapters/target/cluster_shell/store-install.sh")" mm-setup \
    "$id" "$STORE_VERSION" x86_64 "$STORE_URL" "$STORE_SHA256"
  member_spec app-3 | store app-3 member >"$TMP/store/app-3-request-again.json"
  json 'value["csr"]' <"$TMP/store/app-3-request-again.json" >"$TMP/store/app-3-again.csr"
  openssl x509 -req -in "$TMP/store/app-3-again.csr" -CA "$TMP/store/ca.pem" -CAkey "$TMP/store/ca.key" -CAcreateserial \
    -days 60 -extfile "$TMP/store/app-3.ext" -out "$TMP/store/app-3.pem" 2>/dev/null
  member_spec app-3 "$TMP/store/app-3.pem" | store app-3 member >"$TMP/store/app-3-again.json"
  [ "$(json '(value["initialized"], value["has-keys"])' <"$TMP/store/app-3-again.json")" = "(False, False)" ] \
    || { cat "$TMP/store/app-3-again.json"; fail "app-3's new member is not fresh"; }
  # remove_peer THROUGH PEER: `mmd openbao remove-peer` on THROUGH.
  remove_peer() { on "$1" /usr/libexec/mmd openbao remove-peer --peer "$2" --project "$PROJECT_ID" --environment production; }
  leader=$(json 'value["leader"]' <"$TMP/store/app-1-status.json")
  through=$(printf '%s\n' app-1 app-2 | grep -vx "$leader" | head -n 1)
  if remove_peer "$through" "$through" >/dev/null 2>&1; then fail "$through removed its own member"; fi
  if remove_peer "$through" "$leader" >/dev/null 2>&1; then fail "$through removed the member that leads"; fi
  remove_peer "$through" app-3 >"$TMP/store/removed.json"
  [ "$(json '(value["peer"], value["removed"])' <"$TMP/store/removed.json")" = "('app-3', True)" ] \
    || { cat "$TMP/store/removed.json"; fail "app-3's old entry was not removed"; }
  json "value['sealed']['app-3']" <"$TMP/store/init.json" | store app-3 join >"$TMP/store/app-3-rejoin.json"
  [ "$(json '(value["member"], value["sealed"])' <"$TMP/store/app-3-rejoin.json")" = "(True, False)" ] || fail "app-3 did not unseal again"
  for _ in $(seq 1 60); do store_whole && break; sleep 2; done
  store_whole || { cat "$TMP/store/"*-status.json; fail "app-3 did not rejoin as a voter"; }
  [ "$(removals)" -ge 1 ] || fail "the removal of app-3's old entry was not audited"
  listening_only_inside app-3 3 8200 8201
  store app-3 versions </dev/null >"$TMP/store/versions-after.json"
  cmp -s "$TMP/store/versions-before.json" "$TMP/store/versions-after.json" \
    || { diff "$TMP/store/versions-before.json" "$TMP/store/versions-after.json"; fail "app-3 does not read back every secret with its versions"; }
  mm_as alice --output json secret status >"$TMP/store/status-rejoined.json"
  [ "$(json "sorted({(v.get('active') or {}).get('revision') for n in value['nodes'] for v in n['values'] if v['key'] == 'db.password'})" <"$TMP/store/status-rejoined.json")" \
    = "['$revision']" ] || { cat "$TMP/store/status-rejoined.json"; fail "a node reads another revision after app-3 rejoined"; }
  before=$(on app-3 engine ps -q --filter label=com.docker.compose.service=web)
  mm_as alice deploy redeploy --yes >"$TMP/store/redeploy-rejoined.log" 2>&1 \
    || { cat "$TMP/store/redeploy-rejoined.log"; fail "no deployment after app-3 rejoined"; }
  [ "$(on app-3 engine ps -q --filter label=com.docker.compose.service=web)" = "$before" ] \
    || fail "app-3 read another value after it rejoined: web was replaced"
  if grep -l "store-value-of-the-live-stage" "$TMP/store/"*.json; then fail "a value is in the output above"; fi
  printf '  ok   the member that was down kept its entry; the wiped one rejoined as a voter with every version\n'

  step "secret store: an upgrade to OpenBao $STORE_NEXT, one member at a time, the leader last"
  if [ -n "${MM_LIVE_OPENBAO_NEXT_ARCHIVE:-}" ]; then
    cp "$MM_LIVE_OPENBAO_NEXT_ARCHIVE" "$TMP/store/next.tar.gz"
  else
    curl -fsSL --max-time 600 -o "$TMP/store/next.tar.gz" "$STORE_NEXT_URL"
  fi
  echo "$STORE_NEXT_SHA256  $TMP/store/next.tar.gz" | sha256sum -c --quiet - || fail "the archive is not OpenBao $STORE_NEXT"
  leader=$(json 'value["leader"]' <"$TMP/store/app-1-status.json")
  for node in $(printf '%s\n' $NODES | grep -vx "$leader") "$leader"; do
    docker cp "$TMP/store/next.tar.gz" "$(container "$node"):/var/lib/mm-openbao/archives/${STORE_NEXT_URL##*/}"
    on "$node" sh -c "$(cat "$ROOT/core/src/adapters/target/cluster_shell/store-install.sh")" mm-setup \
      "$id" "$STORE_NEXT" x86_64 "$STORE_NEXT_URL" "$STORE_NEXT_SHA256"
    member_spec "$node" "$TMP/store/$node.pem" "$STORE_NEXT" | store "$node" member >"$TMP/store/$node-upgrade.json"
    [ "$(json '(value["changed"], value["sealed"])' <"$TMP/store/$node-upgrade.json")" = "(True, False)" ] \
      || { cat "$TMP/store/$node-upgrade.json"; fail "$node: the member did not come back unsealed on $STORE_NEXT"; }
    for _ in $(seq 1 45); do store_whole && break; sleep 2; done
    store_whole || fail "the store is not whole after $node's upgrade"
    mm_as alice deploy plan >/dev/null 2>&1 || fail "no plan while the members run two releases"
  done
  mm_as ops target nodes >"$TMP/store/nodes-upgraded.log"
  [ "$(grep -c "OpenBao $STORE_NEXT *unsealed" "$TMP/store/nodes-upgraded.log")" = 3 ] || { cat "$TMP/store/nodes-upgraded.log"; fail "not every member runs $STORE_NEXT"; }
  mm_as alice doctor >"$TMP/store/doctor-upgraded.log" 2>&1 || true
  if grep -q MM_SECRET_STORE_ "$TMP/store/doctor-upgraded.log"; then cat "$TMP/store/doctor-upgraded.log"; fail "doctor reports the upgraded store"; fi
  mm_as alice --output json secret status >"$TMP/store/status-upgraded.json"
  [ "$(json "sorted({(v.get('active') or {}).get('revision') for n in value['nodes'] for v in n['values'] if v['key'] == 'db.password'})" <"$TMP/store/status-upgraded.json")" \
    = "['$revision']" ] || fail "the upgrade lost the value"
  printf '  ok   every member on %s, the value kept, a plan made between each member\n' "$STORE_NEXT"
}

# MM_LIVE_ONLY=secret-store runs only that, right after access is synced.
if [ "${MM_LIVE_ONLY:-}" = secret-store ]; then
  store_stages
  printf '\nPASS: the managed secret store across three nodes\n'
  exit 0
fi
# MM_LIVE_ONLY=low-ports runs only that, right after access is synced.
if [ "${MM_LIVE_ONLY:-}" = low-ports ]; then
  low_ports_stages
  printf '\nPASS: a service that is not root on port 80\n'
  exit 0
fi
# MM_LIVE_ONLY=release-facts runs only that, right after access is synced.
if [ "${MM_LIVE_ONLY:-}" = release-facts ]; then
  release_facts_stages
  printf '\nPASS: release facts across three nodes\n'
  exit 0
fi
# MM_LIVE_ONLY=tunnel runs the tunnel stages right here and stops.
if [ "${MM_LIVE_ONLY:-}" = tunnel ]; then
  tunnel_stages
  printf '\nPASS: Cloudflare Tunnel ingress across three nodes\n'
  exit 0
fi
# MM_LIVE_ONLY=ingress runs the ingress stages right here and stops.
if [ "${MM_LIVE_ONLY:-}" = ingress ]; then
  ingress_stages
  printf '\nPASS: ingress across three nodes\n'
  exit 0
fi
# MM_LIVE_ONLY=balance runs the ingress balance stages right here and stops.
if [ "${MM_LIVE_ONLY:-}" = balance ]; then
  balance_stages
  printf '\nPASS: ingress balance across three nodes\n'
  exit 0
fi
# MM_LIVE_ONLY=health runs the balance stages and then the health stages.
if [ "${MM_LIVE_ONLY:-}" = health ]; then
  balance_stages
  health_stages
  printf '\nPASS: ingress balance and health restarts across three nodes\n'
  exit 0
fi
# MM_LIVE_ONLY=autoscale runs the autoscale stages right here and stops.
if [ "${MM_LIVE_ONLY:-}" = autoscale ]; then
  autoscale_stages
  printf '\nPASS: autoscale across three nodes\n'
  exit 0
fi

step "one deployment plan: a section per node, one hash over all of them"
mm_as alice deploy plan | tee "$TMP/plan.log"
has "$TMP/plan.log" "Cluster deployment plan" "target:   cluster (app-1, app-2, app-3)" \
  "Node app-1" "stack:  app" "Node app-2" "Node app-3" "runs nothing: no service is placed on this node" \
  "deploy apply --plan"
[ "$(grep -c 'runs nothing' "$TMP/plan.log")" = 2 ] || fail "two nodes should run nothing"
mm_as alice --output json deploy plan >"$TMP/plan.json"
PLAN=$(json 'value["plan_hash"]' <"$TMP/plan.json")
[ "$(json '[n["node"] for n in value["nodes"]]' <"$TMP/plan.json")" = "['app-1', 'app-2', 'app-3']" ] \
  || fail "the plan should have a section per node, in declared order"
[ "$(json 'len({n["plan"]["binding"]["plan_hash"] for n in value["nodes"]} | {value["plan_hash"]})' <"$TMP/plan.json")" = 4 ] \
  || fail "each node has its own section hash, and the cluster hash is none of them"

step "a node refuses its own section hash: only the cluster hash applies"
SECTION=$(json 'value["nodes"][0]["plan"]["binding"]["plan_hash"]' <"$TMP/plan.json")
if mm_as alice deploy apply --plan "$SECTION" --yes >"$TMP/section.log" 2>&1; then
  fail "a node section was applied on its own"
fi
has "$TMP/section.log" "holds no reviewed cluster plan"
on app-1 engine ps --format '{{.Names}}' | grep -q . && fail "the refused apply started a container"

step "apply walks the nodes and reports each one"
mm_as alice deploy apply --plan "$PLAN" --yes 2>&1 | tee "$TMP/apply.log"
has "$TMP/apply.log" "==> app-1" "==> app-2" "==> app-3" "runs nothing"
[ "$(grep -c ' applied ' "$TMP/apply.log")" = 3 ] || fail "three nodes should report applied"
on app-1 engine ps --format '{{.Names}}' | grep -q api || fail "app-1 should run the api container"
for node in app-2 app-3; do
  [ -z "$(on "$node" engine ps -q)" ] || fail "$node should run nothing"
done
if mm_as alice deploy apply --plan "$PLAN" --yes >"$TMP/replay.log" 2>&1; then fail "a plan was applied twice"; fi
has "$TMP/replay.log" "already applied its part"

step "status asks every node; logs and exec go to the node that runs the service"
COMMIT=$(git_repo rev-parse HEAD)
mm_as alice service status | tee "$TMP/status.log"
has "$TMP/status.log" "NODE" "CONTAINER" "deployed:    commit ${COMMIT:0:12}  "
grep -E '^\s+app\s+api\s+running 1/1\s+app-1\s+api-1\s+running\s+Up ' "$TMP/status.log" >/dev/null \
  || fail "the api row should say running 1/1, with its container on app-1"
mm_as alice --output json service status >"$TMP/status.json"
[ "$(json '[(s["name"], [n["node"] for n in s["nodes"]], [c["node"] for c in s["containers"]]) for stack in value["stacks"] for s in stack["services"]]' <"$TMP/status.json")" \
  = "[('api', ['app-1'], ['app-1'])]" ] || fail "the JSON status should list the service once, with the node of its share and of its container"
[ "$(json 'any("node" in s for stack in value["stacks"] for s in stack["services"])' <"$TMP/status.json")" = False ] \
  || fail "a service of the JSON status should name its nodes under nodes, not under node"
[ "$(json '[(n["name"], n["reachable"]) for n in value["nodes"]]' <"$TMP/status.json")" = "[('app-1', True), ('app-2', True), ('app-3', True)]" ] \
  || fail "the JSON status should list every node"
mm_as alice service logs api | tee "$TMP/logs.log"
has "$TMP/logs.log" "app-1 api-1 | cluster-live-started"
# shellcheck disable=SC2016 # the inner command expands on the node
[ "$(mm_as alice service exec api -- hostname)" = "$(on app-1 sh -c 'engine inspect --format "{{.Config.Hostname}}" $(engine ps -q)')" ] \
  || fail "exec did not run in the container on app-1"
mm_as alice target nodes | tee "$TMP/nodes-deployed.log"
[ "$(grep -c "${COMMIT:0:12}" "$TMP/nodes-deployed.log")" = 3 ] || fail "every node should record the deployed commit"
[ "$(grep -c 'runs nothing' "$TMP/nodes-deployed.log")" = 2 ] || fail "two nodes should record that they run nothing"

step "a request that reaches another node is refused by that node"
# The same hosts and keys with two names exchanged: app-2's address now
# answers as app-3. Nothing new to trust, and nothing may be applied.
NODE_LINES=$(node_lines app-1:app-1 app-2:app-3 app-3:app-2)
commit "$(users alice)" "exchange the names of two nodes"
publish
if mm_as alice deploy plan >"$TMP/wrong-node.log" 2>&1; then fail "a plan was made on mis-addressed nodes"; fi
has "$TMP/wrong-node.log" MM_NODE_MISMATCH "this request is for node 'app-2'" "is node 'app-3'" "mm target setup record"
mm_as alice target nodes >"$TMP/wrong-nodes.log" 2>&1 && fail "target nodes should exit non-zero for a wrong node"
[ "$(grep -c 'wrong node' "$TMP/wrong-nodes.log")" = 2 ] || { cat "$TMP/wrong-nodes.log" >&2; fail "two nodes should be reported as the wrong node"; }
NODE_LINES=$(node_lines app-1:app-1 app-2:app-2 app-3:app-3)
commit "$(users alice bob)" "restore the node names and add bob"
publish

step "a stopped mmd fails the plan with the node, the address, and what to run"
stop_mmd app-3
if mm_as alice deploy plan >"$TMP/down.log" 2>&1; then fail "a plan was made with a node down"; fi
has "$TMP/down.log" MM_UNREACHABLE "node 'app-3' ($(container app-3):22)" \
  "accepts SSH, but mmd is not running there" "systemctl start mmd" \
  "add \`--nodes app-1,app-2\`" "app-3 keeps what it runs now"
on app-1 sh -c 'ls /var/lib/mm/seals | wc -l' >"$TMP/seals-before"

step "--nodes plans and applies for the nodes that answer"
mm_as alice deploy plan --nodes app-1,app-2 | tee "$TMP/subset-plan.log"
has "$TMP/subset-plan.log" "target:   cluster (app-1, app-2)" "left out: app-3 (it keeps what it runs now)"
SUBSET=$(mm_as alice --output json deploy plan --nodes app-1,app-2 | json 'value["plan_hash"]')
mm_as alice deploy apply --plan "$SUBSET" --yes 2>&1 | tee "$TMP/subset-apply.log"
has "$TMP/subset-apply.log" "left out     unchanged"
[ "$(grep -c ' applied ' "$TMP/subset-apply.log")" = 2 ] || fail "two nodes should report applied"
if mm_as alice deploy apply --plan "$SUBSET" --nodes app-1 --yes >"$TMP/apply-nodes.log" 2>&1; then
  fail "apply accepted --nodes"
fi
has "$TMP/apply-nodes.log" "a plan hash already fixes the nodes"
if mm_as alice deploy plan --nodes app-9 >"$TMP/unknown-node.log" 2>&1; then fail "an unknown node was accepted"; fi
has "$TMP/unknown-node.log" "'app-9', which is not a node of target 'cluster'" "app-1, app-2, app-3"

step "the stopped node is reported, not hidden, by nodes, status, and doctor"
if mm_as alice target nodes >"$TMP/nodes-down.log" 2>&1; then fail "target nodes should exit non-zero with a node down"; fi
has "$TMP/nodes-down.log" "unreachable" "mmd is not running there" "pass \`--nodes\`"
NEW_COMMIT=$(git_repo rev-parse HEAD)
[ "$(grep -c "${NEW_COMMIT:0:12}" "$TMP/nodes-down.log")" = 2 ] || fail "the two planned nodes should record the new commit"
mm_as alice service status | tee "$TMP/status-down.log"
has "$TMP/status-down.log" "app-1" "node app-3: accepts SSH, but mmd is not running there"
mm_as alice doctor >"$TMP/doctor.log" 2>&1 || true
has "$TMP/doctor.log" MM_NODE_UNREACHABLE "node 'app-3'"

step "a subset access sync adds bob on the nodes that answer"
SYNC=$(mm_as ops --output json team sync --nodes app-1,app-2 | json 'value["plan_hash"]')
mm_as ops team sync --plan "$SYNC" --yes >/dev/null
mm_as bob team whoami | tee "$TMP/bob.log"
has "$TMP/bob.log" '"level":"deploy"'

step "the node comes back: the cluster plans as one again"
start_mmd app-3
wait_mmd app-3
mm_as alice target nodes | tee "$TMP/nodes-back.log"
[ "$(grep -c ' ready ' "$TMP/nodes-back.log")" = 3 ] || fail "every node should be ready again"
mm_as alice doctor >"$TMP/doctor-back.log" 2>&1 || true
has "$TMP/doctor-back.log" MM_NODE_COMMIT_SKEW
PLAN=$(mm_as alice --output json deploy plan | json 'value["plan_hash"]')
mm_as alice deploy apply --plan "$PLAN" --yes 2>&1 | tee "$TMP/final-apply.log"
[ "$(grep -c ' applied ' "$TMP/final-apply.log")" = 3 ] || fail "three nodes should report applied"
mm_as alice target nodes | tee "$TMP/nodes-final.log"
[ "$(grep -c "${NEW_COMMIT:0:12}" "$TMP/nodes-final.log")" = 3 ] || fail "every node should record the new commit"
mm_as alice doctor >"$TMP/doctor-final.log" 2>&1 || true
if grep -q 'MM_NODE_' "$TMP/doctor-final.log"; then cat "$TMP/doctor-final.log" >&2; fail "doctor still reports a node problem"; fi

# --- placement ----------------------------------------------------------------
# services_on NODE: the Compose services with a running container, sorted; a
# job's one-off container, which carries its service's label, is not one.
services_on() {
  on "$1" engine ps --filter label=com.docker.compose.oneoff=False --format '{{.Label "com.docker.compose.service"}}' | sort | tr '\n' ' ' | sed 's/ $//'
}
# plan_services FILE NODE: the services a plan's JSON places on a node.
plan_services() {
  json "' '.join(s['service'] for n in value['nodes'] if n['node'] == '$2' for s in (n['plan']['plan'] or {'node': {'services': []}})['node']['services'])" <"$1"
}
deploy() {
  local hash
  hash=$(mm_as alice --output json deploy plan | json 'value["plan_hash"]')
  mm_as alice deploy apply --plan "$hash" --yes 2>&1
}
# Podman gives a container no NET_RAW by default, which `ping` needs; the
# stages ping between containers.
WORKLOAD_CAPS=''
[ "$ENGINE" = podman ] && WORKLOAD_CAPS='    cap_add: [NET_RAW]
'
workload() { printf '  %s:\n    image: %s\n    command: ["sh", "-c", "echo %s-started && exec sleep infinity"]\n    stop_grace_period: 1s\n%s' "$1" "$WORKLOAD_IMAGE" "$1" "$WORKLOAD_CAPS"; }
# rows_of FILE SERVICE: the nodes a text status lists for the service, on
# one line: the node of its own row, then of each row under it.
rows_of() {
  awk -v service="$2" '
    $1 == "app" && $2 == service { inside = 1; print $5; next }
    inside && $1 ~ /^app-[0-9]+$/ { print $1; next }
    { inside = 0 }' "$1" | tr '\n' ' ' | sed 's/ $//'
}

step "placement: one service on every node, one pinned, one on a group"
printf 'served from the checkout\n' >"$TMP/repo/api/site.txt"
{
  printf 'services:\n'
  workload api
  workload web
  printf '    depends_on: [db]\n    volumes: ["./api/site.txt:/site.txt:ro"]\n'
  workload db
  printf '    volumes: [dbdata:/data]\n'
  workload worker
  printf 'volumes:\n  dbdata: {}\n'
} >"$TMP/repo/compose.yaml"
SERVICE_LINES='      api: {stack: app}
      web: {stack: app, placement: all, drain: 2s}
      db: {stack: app, placement: app-3}
      worker: {stack: app, placement: workers}'
CLUSTER_LINES='    cluster:
      groups: {workers: [app-2, app-3]}'
commit "$(users alice bob)" "place services on nodes"
publish
mm_as alice deploy plan | tee "$TMP/placed-plan.log"
has "$TMP/placed-plan.log" "services: app/api, app/web" "services: app/web, app/worker" \
  "services: app/db, app/web, app/worker" \
  "depends_on not kept: app/web depends on db, which runs on app-3"
if grep -q 'runs nothing' "$TMP/placed-plan.log"; then fail "every node runs a service now"; fi
mm_as alice --output json deploy plan >"$TMP/placed-plan.json"
[ "$(plan_services "$TMP/placed-plan.json" app-1)" = "api web" ] || fail "the JSON plan should place api and web on app-1"
[ "$(plan_services "$TMP/placed-plan.json" app-2)" = "web worker" ] || fail "the JSON plan should place web and worker on app-2"
[ "$(plan_services "$TMP/placed-plan.json" app-3)" = "db web worker" ] || fail "the JSON plan should place db, web and worker on app-3"
PLACED=$(json 'value["plan_hash"]' <"$TMP/placed-plan.json")
mm_as alice deploy apply --plan "$PLACED" --yes 2>&1 | tee "$TMP/placed-apply.log"
[ "$(grep -c ' applied ' "$TMP/placed-apply.log")" = 3 ] || fail "three nodes should report applied"
[ "$(services_on app-1)" = "api web" ] || fail "app-1 runs: $(services_on app-1)"
[ "$(services_on app-2)" = "web worker" ] || fail "app-2 runs: $(services_on app-2)"
[ "$(services_on app-3)" = "db web worker" ] || fail "app-3 runs: $(services_on app-3)"
on app-3 engine volume ls --format '{{.Name}}' | grep -q dbdata || fail "app-3 should have db's volume"
for node in app-1 app-2; do
  if on "$node" engine volume ls --format '{{.Name}}' | grep -q dbdata; then fail "$node has a volume only db uses"; fi
done

step "a relative bind mount is the node's own checkout on every node"
for node in $NODES; do
  # shellcheck disable=SC2016 # the inner command expands on the node
  [ "$(on "$node" sh -c 'engine exec "$(engine ps -q --filter label=com.docker.compose.service=web)" cat /site.txt')" = "served from the checkout" ] \
    || fail "$node: web does not see the file from its checkout"
done
mm_as alice service status | tee "$TMP/placed-status.log"
[ "$(rows_of "$TMP/placed-status.log" web)" = "app-1 app-2 app-3" ] || fail "status should list web with a row for each of its three nodes"
grep -E '^\s+app\s+web\s+running 3/3\s+app-1\s' "$TMP/placed-status.log" >/dev/null || fail "status should say web is running 3/3"
grep -E '^\s+app\s+db\s+running 1/1\s+app-3\s' "$TMP/placed-status.log" >/dev/null || fail "status should list db on app-3"

step "a second plan of the applied commit starts and stops nothing"
mm_as alice --output json deploy plan >"$TMP/replan.json"
[ "$(json "sum(len(n['plan']['plan']['node'].get('starting', [])) + len(n['plan']['plan']['node'].get('leaving', [])) for n in value['nodes'])" <"$TMP/replan.json")" = 0 ] \
  || fail "a second plan of the applied commit should start and stop nothing"
[ "$(json 'value["plan_hash"]' <"$TMP/replan.json")" != "$PLACED" ] || fail "a plan hash is never reused"

# --- status and logs across nodes ---------------------------------------------
# in_service NODE SERVICE COMMAND: a shell command in the service's container.
in_service() {
  # shellcheck disable=SC2016 # the inner command expands on the node
  on "$1" sh -c 'engine exec "$(engine ps -q --filter "label=com.docker.compose.service=$1")" sh -c "$2"' sh "$2" "$3"
}
# says NODE SERVICE WORD: the service's container on the node writes WORD as
# a line of its own output, which the engine records like any other.
says() { in_service "$1" "$2" "echo $3 > /proc/1/fd/1"; }
# json_lines EXPRESSION: the expression over `lines`, the JSON objects on
# standard input, one per line. at(record) is when the engine recorded the
# line, as something that sorts.
json_lines() {
  python3 -c "
import json, sys
lines = [json.loads(line) for line in sys.stdin if line.strip()]
def at(record):
    seconds, _, fraction = record['timestamp'].rstrip('Z').partition('.')
    return (seconds, fraction.ljust(9, '0'))
print($1)"
}
# of_service SERVICE EXPRESSION: the expression over `found`, the entries the
# JSON status on standard input has for the service.
of_service() {
  json "(lambda found: $2)([s for stack in value['stacks'] for s in stack['services'] if s['name'] == '$1'])"
}
# stop_client WORD: interrupts the mm in the client container that has WORD
# among its arguments, as Ctrl-C would, and ends it if it stays.
stop_client() {
  docker exec "$CLIENT" python3 -c '
import os, signal, sys, time
def running():
    found = []
    for pid in filter(str.isdigit, os.listdir("/proc")):
        try:
            command = open(f"/proc/{pid}/cmdline", "rb").read().split(b"\0")
        except OSError:
            continue
        if os.path.basename(command[0]) == b"mm" and sys.argv[1].encode() in command:
            found.append(int(pid))
    return found
for sent in (signal.SIGINT, signal.SIGKILL):
    for pid in running():
        try:
            os.kill(pid, sent)
        except ProcessLookupError:
            pass
    for _ in range(50):
        if not running():
            sys.exit(0)
        time.sleep(0.1)
sys.exit("the client did not end")
' "$1"
}
# As a daemon that hangs leaves a host: its socket accepts, nothing answers.
freeze_mmd() { on "$1" pkill -STOP -x mmd; }
thaw_mmd() { on "$1" pkill -CONT -x mmd; }
# every_node_ready [SECONDS]: all three nodes answer again, at once or
# within SECONDS.
every_node_ready() {
  local waited=0
  until mm_as alice target nodes >"$TMP/nodes-ready.log" 2>&1 && [ "$(grep -c ' ready ' "$TMP/nodes-ready.log")" = 3 ]; do
    [ "$waited" -lt "${1:-20}" ] || { cat "$TMP/nodes-ready.log" >&2; fail "the nodes do not all answer again"; }
    sleep 1; waited=$((waited + 1))
  done
}

step "status: each service once, a row for its container on every node, and the deployment the nodes run"
PLACED_COMMIT=$(git_repo rev-parse HEAD)
[ "$(grep -cE '^\s+app\s+web\s' "$TMP/placed-status.log")" = 1 ] || fail "status should name web in one row"
[ "$(rows_of "$TMP/placed-status.log" api)" = "app-1" ] || fail "status should list api on app-1"
[ "$(rows_of "$TMP/placed-status.log" worker)" = "app-2 app-3" ] || fail "status should list worker on the two nodes of its group"
[ "$(grep -cE '^\s+app-[123]\s+web-[0-9]+\s+running\s+Up ' "$TMP/placed-status.log")" = 2 ] \
  || fail "the rows under web should each name a node, its container, and how long it is up"
[ "$(grep -c 'deployed:' "$TMP/placed-status.log")" = 1 ] || fail "every node runs the same release: the status should say so in one line"
has "$TMP/placed-status.log" "deployed:    commit ${PLACED_COMMIT:0:12}  " "  by alice  plan ${PLACED:0:12}"
if grep -q 'differs from' "$TMP/placed-status.log"; then fail "the status reports a difference from the deployment right after it"; fi
mm_as alice --output json service status >"$TMP/placed-status.json"
[ "$(of_service web '(len(found), [n["node"] for n in found[0]["nodes"]], sorted(c["node"] for c in found[0]["containers"]), found[0]["state"], found[0]["running_count"], found[0]["desired_count"])' <"$TMP/placed-status.json")" \
  = "(1, ['app-1', 'app-2', 'app-3'], ['app-1', 'app-2', 'app-3'], 'running', 3, 3)" ] \
  || { cat "$TMP/placed-status.json" >&2; fail "the JSON status should have one entry for web, with three nodes and a container on each"; }
[ "$(of_service web '(sorted({tuple(sorted(n)) for n in found[0]["nodes"]}), [(n["state"], n["starting"], n["desired_count"], n["running_count"]) for n in found[0]["nodes"]])' <"$TMP/placed-status.json")" \
  = "([('desired_count', 'healthy_count', 'node', 'running_count', 'starting', 'state')], [('running', False, 1, 1), ('running', False, 1, 1), ('running', False, 1, 1)])" ] \
  || { cat "$TMP/placed-status.json" >&2; fail "each node's share of web should say its state and counts"; }
[ "$(json 'any("node" in s for stack in value["stacks"] for s in stack["services"])' <"$TMP/placed-status.json")" = False ] \
  || fail "a service of the JSON status should name its nodes under nodes, not under node"
[ "$(json '(value["deployment"]["commit"], value["deployment"]["deployed_by"], value["deployment"]["plan_hash"], isinstance(value["deployment"]["deployed_at_unix_ms"], int))' <"$TMP/placed-status.json")" \
  = "('$PLACED_COMMIT', 'alice', '$PLACED', True)" ] || fail "the JSON status should carry the deployment: its commit, who applied it, and the plan"
[ "$(json '[(n["name"], n["deployment"]["commit"], n.get("drift", []), n.get("network_drift", [])) for n in value["nodes"]]' <"$TMP/placed-status.json")" \
  = "[('app-1', '$PLACED_COMMIT', [], []), ('app-2', '$PLACED_COMMIT', [], []), ('app-3', '$PLACED_COMMIT', [], [])]" ] \
  || fail "every node of the JSON status should carry its deployment and nothing that differs from it"

step "logs: one stream from the service's container on every node, in the order the lines were written"
for round in 1 2; do
  for node in $NODES; do says "$node" web "merged-$round-from-$node"; done
done
mm_as alice service logs web --tail 20 | tee "$TMP/merged.log"
if grep -vE '^app-[123] web-[0-9]+ \| ' "$TMP/merged.log"; then fail "every line should begin with its node and its container"; fi
has "$TMP/merged.log" "app-1 web-1 | merged-1-from-app-1" "app-2 web-1 | merged-1-from-app-2" "app-3 web-1 | merged-2-from-app-3"
[ "$(grep -oE 'merged-[12]-from-app-[123]$' "$TMP/merged.log" | tr '\n' ' ')" \
  = "merged-1-from-app-1 merged-1-from-app-2 merged-1-from-app-3 merged-2-from-app-1 merged-2-from-app-2 merged-2-from-app-3 " ] \
  || fail "the lines of three nodes are not in the order they were written"
mm_as alice --output json service logs web --tail 20 >"$TMP/merged.jsonl"
[ "$(json_lines 'sorted({r["node"] for r in lines})' <"$TMP/merged.jsonl")" = "['app-1', 'app-2', 'app-3']" ] \
  || { cat "$TMP/merged.jsonl" >&2; fail "the JSON log should have lines of all three nodes"; }
[ "$(json_lines '(lambda stamps: len(stamps) >= 9 and stamps == sorted(stamps))([at(r) for r in lines])' <"$TMP/merged.jsonl")" = True ] \
  || { cat "$TMP/merged.jsonl" >&2; fail "the JSON log is not in the order of its timestamps"; }
[ "$(json_lines '[r["sequence"] for r in lines] == list(range(1, len(lines) + 1))' <"$TMP/merged.jsonl")" = True ] \
  || fail "the JSON log should number its lines from one"
[ "$(json_lines 'sorted({(r["identity"]["project_id"], r["identity"]["environment"], r["identity"]["stack"], r["identity"].get("module"), r["identity"]["service"]) for r in lines})' <"$TMP/merged.jsonl")" \
  = "[('$PROJECT_ID', 'production', 'app', 'api', 'web')]" ] || { cat "$TMP/merged.jsonl" >&2; fail "every line of the JSON log should say which service it is of"; }
[ "$(json_lines 'all(r["source"] in ("stdout", "stderr") and r["container"].endswith("-web-1") and r["container"] != "web-1" and not r["truncated"] and not r["lossy_utf8"] and r["reconnect"] == 0 for r in lines)' <"$TMP/merged.jsonl")" = True ] \
  || { cat "$TMP/merged.jsonl" >&2; fail "every line of the JSON log should name its stream and the engine's name of its container"; }

step "logs without a service: every service of the environment, merged the same way"
mm_as alice service logs | tee "$TMP/every.log"
grep -Eq '^app-1 +api-1 +\| api-started$' "$TMP/every.log" || fail "the log of every service should have api's line from app-1"
grep -Eq '^app-3 +db-1 +\| db-started$' "$TMP/every.log" || fail "the log of every service should have db's line from app-3"
grep -Eq '^app-2 +worker-1 +\| worker-started$' "$TMP/every.log" || fail "the log of every service should have worker's line from app-2"
has "$TMP/every.log" "merged-2-from-app-3"
mm_as alice --output json service logs >"$TMP/every.jsonl"
[ "$(json_lines 'sorted({r["identity"]["service"] for r in lines})' <"$TMP/every.jsonl")" = "['api', 'db', 'web', 'worker']" ] \
  || { cat "$TMP/every.jsonl" >&2; fail "the JSON log of every service should have lines of all four"; }
[ "$(json_lines 'sorted({(r["node"], r["identity"]["service"]) for r in lines if r["identity"]["service"] != "web"})' <"$TMP/every.jsonl")" \
  = "[('app-1', 'api'), ('app-2', 'worker'), ('app-3', 'db'), ('app-3', 'worker')]" ] \
  || { cat "$TMP/every.jsonl" >&2; fail "each line should name the node its service runs on"; }
[ "$(json_lines '(lambda stamps: stamps == sorted(stamps))([at(r) for r in lines]) and [r["sequence"] for r in lines] == list(range(1, len(lines) + 1))' <"$TMP/every.jsonl")" = True ] \
  || { cat "$TMP/every.jsonl" >&2; fail "the JSON log of every service is not one numbered stream in the order of its timestamps"; }

step "logs --node: only the nodes that are named, and a name that is no node is refused with the nodes"
mm_as alice service logs web --node app-2 --tail 20 | tee "$TMP/one-node.log"
has "$TMP/one-node.log" "app-2 web-1 | merged-1-from-app-2" "app-2 web-1 | merged-2-from-app-2"
if grep -vE '^app-2 ' "$TMP/one-node.log"; then fail "--node app-2 printed a line of another node"; fi
[ "$(mm_as alice --output json service logs web --node app-2 --tail 20 | json_lines 'sorted({r["node"] for r in lines})')" = "['app-2']" ] \
  || fail "the JSON log of --node app-2 should have lines of app-2 only"
[ "$(mm_as alice --output json service logs web --node app-1 --node app-3 --tail 20 | json_lines 'sorted({r["node"] for r in lines})')" = "['app-1', 'app-3']" ] \
  || fail "--node given twice should read those two nodes"
if mm_as alice service logs web --node app-9 >"$TMP/logs-unknown-node.log" 2>&1; then fail "logs accepted a node that does not exist"; fi
has "$TMP/logs-unknown-node.log" "'app-9', which is not a node of target 'cluster'" "app-1, app-2, app-3"
if mm_as alice service logs api --node app-2 >"$TMP/logs-not-placed.log" 2>&1; then fail "logs of api were read from a node that does not run it"; fi
has "$TMP/logs-not-placed.log" "no node of target 'cluster' that --node names runs a service named 'api'" "mm service status"

step "logs --follow: lines of every node arrive as they are written, in that order"
mm_as alice service logs web --follow --tail 0 >"$TMP/follow.log" 2>&1 &
FOLLOW_PID=$!
# A node's stream is open once a line written there shows up.
FOLLOWED=yes
for node in $NODES; do
  for _ in $(seq 1 30); do
    says "$node" web "follow-ready-$node"
    sleep 0.5
    grep -q "follow-ready-$node" "$TMP/follow.log" && continue 2
  done
  FOLLOWED=no
done
if [ "$FOLLOWED" = yes ]; then
  says app-2 web follow-first; sleep 1
  says app-3 web follow-second; sleep 1
  says app-1 web follow-third; sleep 2
fi
stop_client --follow || { cat "$TMP/follow.log" >&2; fail "the following mm could not be stopped"; }
wait "$FOLLOW_PID" || true
[ "$FOLLOWED" = yes ] || { cat "$TMP/follow.log" >&2; fail "a followed log did not show a line written on every node"; }
cat "$TMP/follow.log"
has "$TMP/follow.log" "app-2 web-1 | follow-first" "app-3 web-1 | follow-second" "app-1 web-1 | follow-third"
[ "$(grep -oE 'follow-(first|second|third)$' "$TMP/follow.log" | tr '\n' ' ')" = "follow-first follow-second follow-third " ] \
  || fail "the followed lines of three nodes are not in the order they were written"
if grep -q 'merged-' "$TMP/follow.log"; then fail "--tail 0 showed lines from before the read began"; fi

step "a node whose mmd is stopped is one line in the log and a note in the status; the others answer"
# Everything is read first and judged after the daemons run again.
stop_mmd app-2
DEAD_CODE=0
mm_as alice service logs web --tail 20 >"$TMP/dead.log" 2>"$TMP/dead.err" || DEAD_CODE=$?
DEAD_JSON_CODE=0
mm_as alice --output json service logs web --tail 20 >"$TMP/dead.jsonl" 2>>"$TMP/dead.err" || DEAD_JSON_CODE=$?
DEAD_FROM=$(date +%s)
DEAD_STATUS_CODE=0
mm_as alice service status >"$TMP/dead-status.log" 2>&1 || DEAD_STATUS_CODE=$?
DEAD_STATUS_TOOK=$(($(date +%s) - DEAD_FROM))
mm_as alice --output json service status >"$TMP/dead-status.json" 2>>"$TMP/dead.err" || true
stop_mmd app-1
stop_mmd app-3
NONE_CODE=0
mm_as alice service logs web --tail 20 >"$TMP/none.log" 2>&1 || NONE_CODE=$?
for node in $NODES; do start_mmd "$node"; done
for node in $NODES; do wait_mmd "$node"; done
every_node_ready
cat "$TMP/dead.log"
[ "$DEAD_CODE" = 0 ] || { cat "$TMP/dead.err" >&2; fail "logs should exit 0 while two of three nodes answer, not $DEAD_CODE"; }
[ "$(grep -c '^app-[123] *!' "$TMP/dead.log")" = 1 ] || fail "the log should have exactly one line about a node"
has "$TMP/dead.log" "app-2 ! no answer: accepts SSH, but mmd is not running there" "systemctl start mmd"
[ "$(tail -n 1 "$TMP/dead.log" | cut -c1-7)" = "app-2 !" ] || fail "the line about the node should follow the lines that were read"
grep -q '^app-1 web-1 | ' "$TMP/dead.log" || fail "the lines of app-1 are missing while app-2 is down"
grep -q '^app-3 web-1 | ' "$TMP/dead.log" || fail "the lines of app-3 are missing while app-2 is down"
if grep -q '^app-2 web' "$TMP/dead.log"; then fail "a line of app-2 was read although its mmd is stopped"; fi
[ "$DEAD_JSON_CODE" = 0 ] || { cat "$TMP/dead.err" >&2; fail "the JSON log should exit 0 while two of three nodes answer, not $DEAD_JSON_CODE"; }
[ "$(json_lines '[(r["node"], r.get("container"), "no answer" in r["message"]) for r in lines if r["source"] == "runtime"]' <"$TMP/dead.jsonl")" \
  = "[('app-2', None, True)]" ] || { cat "$TMP/dead.jsonl" >&2; fail "the JSON log should have exactly one record about a node, for app-2"; }
[ "$(json_lines 'sorted({r["node"] for r in lines if r["source"] != "runtime"})' <"$TMP/dead.jsonl")" = "['app-1', 'app-3']" ] \
  || fail "the JSON log should have the lines of the two nodes that answer"
[ "$(json_lines '[r["sequence"] for r in lines] == list(range(1, len(lines) + 1))' <"$TMP/dead.jsonl")" = True ] \
  || fail "the JSON log should number the record about the node with the lines"
cat "$TMP/dead-status.log"
[ "$DEAD_STATUS_CODE" = 0 ] || fail "the status should exit 0 while two of three nodes answer, not $DEAD_STATUS_CODE"
[ "$DEAD_STATUS_TOOK" -le 10 ] || fail "the status took ${DEAD_STATUS_TOOK}s with one node down"
has "$TMP/dead-status.log" "node app-2: accepts SSH, but mmd is not running there"
# web is counted on every node it is placed on: the silent one is a row
# that says so, and web is not whole.
[ "$(rows_of "$TMP/dead-status.log" web)" = "app-1 app-2 app-3" ] || fail "the status should list web on its three nodes, app-2 as not answering"
grep -E '^\s+app\s+web\s+degraded 2/3\s' "$TMP/dead-status.log" >/dev/null || fail "web should be degraded 2/3 while app-2 does not answer"
grep -E '^\s+app-2\s+-\s+-\s+no answer\s' "$TMP/dead-status.log" >/dev/null || fail "app-2's row of web should say it does not answer"
[ "$(rows_of "$TMP/dead-status.log" api)" = "app-1" ] || fail "the status should still list api on app-1"
[ "$(json '[(n["name"], n["reachable"]) for n in value["nodes"]]' <"$TMP/dead-status.json")" = "[('app-1', True), ('app-2', False), ('app-3', True)]" ] \
  || fail "the JSON status should mark app-2 as not reachable"
[ "$(of_service web '(len(found), [(n["node"], n["state"]) for n in found[0]["nodes"]], sorted(c["node"] for c in found[0]["containers"]))' <"$TMP/dead-status.json")" \
  = "(1, [('app-1', 'running'), ('app-2', 'unknown'), ('app-3', 'running')], ['app-1', 'app-3'])" ] \
  || fail "the JSON status should list web's share on every node, app-2's as unknown, and the containers of the two that answer"
[ "$(json 'value["deployment"]["commit"]' <"$TMP/dead-status.json")" = "$PLACED_COMMIT" ] \
  || fail "the nodes that answer run one release: the JSON status should still carry it"
[ "$NONE_CODE" != 0 ] || { cat "$TMP/none.log" >&2; fail "logs exited 0 although no node answered"; }
has "$TMP/none.log" "mmd is not running there"
if grep -q ' | ' "$TMP/none.log"; then fail "logs printed lines although no node answered"; fi
mm_as alice service logs web --tail 5 >/dev/null || fail "the log does not come back with the daemons"

step "a line of 100 000 characters is cut and marked, or arrives whole, and the lines after it are intact"
in_service app-1 api 'echo before-the-long-line > /proc/1/fd/1'
in_service app-1 api '{ head -c 100000 /dev/zero | tr "\0" x; echo; } > /proc/1/fd/1'
in_service app-1 api 'echo after-the-long-line > /proc/1/fd/1; echo and-one-more > /proc/1/fd/1'
# The engine keeps a line of more than 16 KiB in pieces and counts each as
# an entry of its tail: this tail has room for the seven pieces of the line.
LONG_CODE=0
mm_as alice --output json service logs api --tail 20 >"$TMP/long.jsonl" 2>"$TMP/long.err" || LONG_CODE=$?
[ "$LONG_CODE" = 0 ] || { cat "$TMP/long.err" >&2; fail "the log with a very long line ended with exit $LONG_CODE"; }
if grep -q 'frame length must be between' "$TMP/long.jsonl" "$TMP/long.err"; then fail "a very long line ended the stream"; fi
# The last four lines: a short one as it is, the long one as its size and
# the characters it is made of.
LAST_FOUR='[(r["message"] if len(r["message"]) < 100 else (len(r["message"].encode()), sorted(set(r["message"]))), r["truncated"]) for r in lines[-4:]]'
[ "$(json_lines "$LAST_FOUR" <"$TMP/long.jsonl")" \
  = "[('before-the-long-line', False), ((65536, ['x']), True), ('after-the-long-line', False), ('and-one-more', False)]" ] \
  || { cut -c1-300 "$TMP/long.jsonl" >&2; fail "the long line should be cut at 65536 bytes and marked, between whole lines"; }
[ "$(mm_as alice service logs api --tail 20 | grep -c 'x \[line truncated\]$')" = 1 ] || fail "the text log should mark the line that was cut"
mm_as alice --output json service logs api --tail 20 --max-line-bytes "$WHOLE_LINE_BYTES" >"$TMP/whole.jsonl" 2>"$TMP/whole.err" \
  || { cat "$TMP/whole.err" >&2; fail "the log with --max-line-bytes $WHOLE_LINE_BYTES failed"; }
[ "$(json_lines "$LAST_FOUR" <"$TMP/whole.jsonl")" \
  = "[('before-the-long-line', False), ((100000, ['x']), False), ('after-the-long-line', False), ('and-one-more', False)]" ] \
  || { cut -c1-300 "$TMP/whole.jsonl" >&2; fail "the long line should arrive whole with --max-line-bytes $WHOLE_LINE_BYTES"; }

step "a container that ran once and exited in a service's project does not make the service degraded"
# What `docker compose run` leaves on a host: a container with the labels of
# the project and the service, marked as started for one command.
# shellcheck disable=SC2016 # the inner command expands on the node
WEB_PROJECT=$(on app-2 sh -c 'engine inspect --format "{{index .Config.Labels \"com.docker.compose.project\"}}" "$(engine ps -q --filter label=com.docker.compose.service=web)"')
on app-2 engine run --name mm-live-ran-once --label "com.docker.compose.project=$WEB_PROJECT" \
  --label com.docker.compose.service=web --label com.docker.compose.oneoff=True "$WORKLOAD_IMAGE" false >/dev/null 2>&1 || true
RAN_ONCE=$(on app-2 engine inspect --format '{{.State.Status}} {{.State.ExitCode}}' mm-live-ran-once 2>&1 || true)
mm_as alice service status web >"$TMP/ran-once-status.log" 2>&1 || true
mm_as alice --output json service status web >"$TMP/ran-once-status.json" 2>/dev/null || true
on app-2 engine rm -f mm-live-ran-once >/dev/null 2>&1 || true
[ "$RAN_ONCE" = "exited 1" ] || fail "the container that ran once was not left as one that exited with 1: $RAN_ONCE"
cat "$TMP/ran-once-status.log"
grep -E '^\s+app\s+web\s+running 3/3\s' "$TMP/ran-once-status.log" >/dev/null || fail "web should still be running 3/3 next to a container that ran once"
[ "$(rows_of "$TMP/ran-once-status.log" web)" = "app-1 app-2 app-3" ] || fail "the container that ran once should not be a row of web"
if grep -q 'differs from' "$TMP/ran-once-status.log"; then fail "a container that ran once is reported as a difference from the deployment"; fi
[ "$(of_service web '(found[0]["state"], found[0]["running_count"], found[0]["desired_count"], [n["state"] for n in found[0]["nodes"]], [(c["node"], c["state"]) for c in found[0]["containers"] if c.get("history")])' <"$TMP/ran-once-status.json")" \
  = "('running', 3, 3, ['running', 'running', 'running'], [('app-2', 'exited')])" ] \
  || { cat "$TMP/ran-once-status.json" >&2; fail "the JSON status should count three running containers and list the one that ran once as history"; }

step "a node whose mmd hangs is waited for five seconds: the status and the log name it and show the others"
freeze_mmd app-2
FROZEN_FROM=$(date +%s)
FROZEN_STATUS_CODE=0
mm_as alice service status >"$TMP/frozen-status.log" 2>&1 || FROZEN_STATUS_CODE=$?
FROZEN_STATUS_TOOK=$(($(date +%s) - FROZEN_FROM))
FROZEN_FROM=$(date +%s)
FROZEN_LOGS_CODE=0
mm_as alice service logs web --tail 20 >"$TMP/frozen.log" 2>&1 || FROZEN_LOGS_CODE=$?
FROZEN_LOGS_TOOK=$(($(date +%s) - FROZEN_FROM))
thaw_mmd app-2
every_node_ready 30
cat "$TMP/frozen-status.log"
[ "$FROZEN_STATUS_CODE" = 0 ] || fail "the status should exit 0 while one node hangs, not $FROZEN_STATUS_CODE"
has "$TMP/frozen-status.log" "node app-2: has not answered after 5 seconds"
[ "$(rows_of "$TMP/frozen-status.log" web)" = "app-1 app-2 app-3" ] || fail "the status should list web on its three nodes while app-2 hangs, app-2 as not answering"
grep -E '^\s+app-2\s+-\s+-\s+no answer\s' "$TMP/frozen-status.log" >/dev/null || fail "app-2's row of web should say it does not answer while it hangs"
[ "$FROZEN_STATUS_TOOK" -le 12 ] || fail "the status took ${FROZEN_STATUS_TOOK}s with one node hanging"
cat "$TMP/frozen.log"
[ "$FROZEN_LOGS_CODE" = 0 ] || fail "logs should exit 0 while one node hangs, not $FROZEN_LOGS_CODE"
[ "$(grep -c '^app-[123] *!' "$TMP/frozen.log")" = 1 ] || fail "the log should have exactly one line about a node"
has "$TMP/frozen.log" "app-2 ! no answer after 5 seconds; the other nodes' lines are shown without it"
grep -q '^app-1 web-1 | ' "$TMP/frozen.log" || fail "the lines of app-1 are missing while app-2 hangs"
grep -q '^app-3 web-1 | ' "$TMP/frozen.log" || fail "the lines of app-3 are missing while app-2 hangs"
[ "$FROZEN_LOGS_TOOK" -le 12 ] || fail "the log took ${FROZEN_LOGS_TOOK}s with one node hanging"
# --- config files -------------------------------------------------------------
# containers_on NODE: every running Compose service with its container, sorted.
containers_on() {
  on "$1" engine ps --no-trunc --format '{{.Label "com.docker.compose.service"}} {{.ID}}' | sort
}
# web_runs NODE COMMAND...: the command in the node's web container.
web_runs() {
  local node=$1; shift
  # shellcheck disable=SC2016 # the inner command expands on the node
  on "$node" sh -c 'engine exec "$(engine ps -q --filter label=com.docker.compose.service=web)" "$@"' sh "$@"
}
# versions_on NODE: how many config file versions the node keeps.
versions_on() {
  on "$1" sh -c 'find /var/lib/mm-home -type d -name .config-files -exec ls {} \; | wc -l'
}

step "config files: a template is rendered on every node that runs the service"
printf 'greeting {{ APP.GREETING }}\n' >"$TMP/repo/api/greeting.conf.tmpl"
BINDING_LINES='bindings:
  APP.GREETING: {value: hello}'
SERVICE_LINES='      api: {stack: app}
      web:
        stack: app
        placement: all
        drain: 2s
        config-files:
          /etc/greeting.conf: {template: greeting.conf.tmpl}
      db: {stack: app, placement: app-3}
      worker: {stack: app, placement: workers}'
commit "$(users alice bob)" "ship a config file with web"
publish
mm_as alice deploy plan | tee "$TMP/config-plan.log"
has "$TMP/config-plan.log" "config file: app/web /etc/greeting.conf from template api/greeting.conf.tmpl" "): new"
deploy | tee "$TMP/config-apply.log"
[ "$(grep -c ' applied ' "$TMP/config-apply.log")" = 3 ] || fail "three nodes should report applied"
for node in $NODES; do
  [ "$(web_runs "$node" cat /etc/greeting.conf)" = "greeting hello" ] || fail "$node: web does not see the rendered file"
  if web_runs "$node" sh -c 'echo changed >>/etc/greeting.conf' 2>/dev/null; then fail "$node: web can write its config file"; fi
  [ "$(versions_on "$node")" = 1 ] || fail "$node keeps $(versions_on "$node") config file versions, not 1"
  containers_on "$node" >"$TMP/config-before.$node"
done
mm_as alice deploy plan | tee "$TMP/config-replan.log"
[ "$(grep -c 'config file: app/web /etc/greeting.conf .*: unchanged' "$TMP/config-replan.log")" = 3 ] \
  || fail "a second plan should call the file unchanged on every node"

step "config files: a changed binding replaces web on every node and no other container"
BINDING_LINES='bindings:
  APP.GREETING: {value: hola}'
commit "$(users alice bob)" "change a binding the template uses"
publish
mm_as alice deploy plan | tee "$TMP/config-change.log"
[ "$(grep -c 'config file: app/web /etc/greeting.conf .*: changed, app/web is replaced' "$TMP/config-change.log")" = 3 ] \
  || fail "the plan should say on every node that web is replaced"
deploy | tee "$TMP/config-change-apply.log"
[ "$(grep -c ' applied ' "$TMP/config-change-apply.log")" = 3 ] || fail "three nodes should report applied"
for node in $NODES; do
  [ "$(web_runs "$node" cat /etc/greeting.conf)" = "greeting hola" ] || fail "$node: web does not see the changed file"
  containers_on "$node" >"$TMP/config-after.$node"
  [ "$(grep -v '^web ' "$TMP/config-before.$node")" = "$(grep -v '^web ' "$TMP/config-after.$node")" ] \
    || fail "$node: a service that mounts no config file was replaced"
  [ "$(grep '^web ' "$TMP/config-before.$node")" != "$(grep '^web ' "$TMP/config-after.$node")" ] \
    || fail "$node: web kept its container after its config file changed"
  [ "$(versions_on "$node")" = 1 ] || fail "$node keeps $(versions_on "$node") config file versions after the rollout, not 1"
done

# --- the terminal UI ------------------------------------------------------------
# The terminal UI as alice, on a pseudo-terminal in the client container,
# driven by scripts/drive-terminal-ui.py: it types the keys of each step and
# saves screens as text under $TMP/tui.
docker cp "$ROOT/scripts/drive-terminal-ui.py" "$CLIENT:/usr/local/bin/drive-terminal-ui.py"
mkdir -p "$TMP/tui"
# tui NAME STEP...: one session; the report lands in $TMP/tui/NAME.json and
# the session's timing trace (MM_TIMING) in $TMP/tui/NAME.trace.
tui() {
  local name=$1 code=0; shift
  docker exec "$CLIENT" sh -c 'rm -rf /tmp/tui && mkdir -p /tmp/tui'
  docker exec -e TERM=xterm-256color -e MM_TIMING=1 -w /tmp/tui "$CLIENT" \
    python3 -I /usr/local/bin/drive-terminal-ui.py --rows 48 --columns 160 --timeout 40 "$@" \
    -- sh -c "exec mm --root /project --mm-home /home-alice 2>/tmp/tui/$name.trace" >"$TMP/tui/$name.json" || code=$?
  docker cp "$CLIENT:/tmp/tui/." "$TMP/tui/" >/dev/null
  if [ -n "${MM_LIVE_TUI_OUT:-}" ]; then mkdir -p "$MM_LIVE_TUI_OUT" && cp -r "$TMP/tui/." "$MM_LIVE_TUI_OUT/"; fi
  cat "$TMP/tui/$name.json"
  [ "$code" = 0 ] || { for screen in "$TMP"/tui/*.screen; do printf -- '--- %s\n' "$screen" >&2; cat "$screen" >&2; done; fail "terminal UI session $name: a step did not happen"; }
}
# screen NAME TEXT...: every TEXT is on the saved screen NAME.
screen() { local name=$1; shift; has "$TMP/tui/$name.screen" "$@"; }

step "terminal UI: it draws at once, then lists each service once with a row for every node"
tui open --step 'say=open' --step 'mark=4/4 running' --step 'key=3' --step 'mark=app-3  worker-1' \
  --step 'snap=services.screen' --step 'key=jj' --step 'snap=node-row.screen'
first_frame=$(json 'value["first_frame_ms"]' <"$TMP/tui/open.json")
first_data=$(json '[s["at_ms"] for s in value["steps"] if s.get("mark") == "4/4 running"][0]' <"$TMP/tui/open.json")
printf '  first frame %s ms, every node answered at %s ms\n' "$first_frame" "$first_data"
grep -E 'first frame|tui\.' "$TMP/tui/open.trace" || true
json 'value["first_frame_ms"] < 2000 or exit("the first frame took over 2 s")' <"$TMP/tui/open.json" >/dev/null
screen services "web  3 nodes" "app-1  web-1" "app-2  web-1" "app-3  web-1" "worker  2 nodes" "db  app-3" "api  app-1"
# The rows beneath a service are rows to act on: the details name the node.
screen node-row "Node      app-1"

step "terminal UI: the log of one node's container, and of the service on every node in order"
for node in $NODES; do says "$node" web "tui-$node"; done
# api is the first row, then web and its rows for app-1, app-2, and app-3.
tui logs --step 'mark=4/4 running' --step 'key=3' --step 'key=jjj' --step 'key=l' --step 'mark=tui-app-2' \
  --step 'wait=1' --step 'snap=node-logs.screen' --step 'key={esc}' --step 'key=k' --step 'key=k' --step 'key=l' \
  --step 'mark=tui-app-3' --step 'wait=1' --step 'snap=service-logs.screen'
screen node-logs "Logs · web-1  on app-2" "app-2 web-1 │ tui-app-2"
if grep -q 'tui-app-1' "$TMP/tui/node-logs.screen"; then fail "the log of app-2's container shows a line of app-1"; fi
screen service-logs "Logs · web  3 nodes, in order" "app-1 web-1 │ tui-app-1" "app-2 web-1 │ tui-app-2" "app-3 web-1 │ tui-app-3"

step "terminal UI: the deploy review is the plan, and each node's step shows while the apply runs"
{
  printf 'services:\n'
  workload api
  workload web | sed 's/web-started/web-started-from-the-tui/'
  printf '    depends_on: [db]\n    volumes: ["./api/site.txt:/site.txt:ro"]\n'
  workload db
  printf '    volumes: [dbdata:/data]\n'
  workload worker
  printf 'volumes:\n  dbdata: {}\n'
} >"$TMP/repo/compose.yaml"
commit "$(users alice bob)" "web says where it was deployed from"
publish
TUI_COMMIT=$(git_repo rev-parse HEAD)
# Each snap waits for the frame to be drawn whole: the review is a long
# frame, and a snap right after its title can catch it half written.
tui deploy --timeout 120 --step 'mark=4/4 running' --step 'key=3' --step 'key=u' --step 'mark=Deploy to production?' \
  --step 'mark=Cluster deployment plan' --step 'wait=1' --step 'snap=review.screen' --step 'key=G' --step 'wait=1' \
  --step 'snap=review-end.screen' --step 'key=g' \
  --step 'key={enter}' --step 'mark=Deploying to production' \
  --step 'mark=stacks applied: app' --step 'snap=deploying.screen' --step 'mark=enter close' --step 'wait=1' --step 'snap=deployed.screen'
screen review "Cluster deployment plan" "Node app-1" "rollout:" "enter deploy"
# The plan scrolls to its end: the last node's section, with the config
# file web still mounts from the stage before.
screen review-end "Node app-3" "config file: app/web /etc/greeting.conf from template api/greeting.conf.tmpl"
# A node that finished its turn while the others had not: the apply was
# shown as it ran, not after it.
screen deploying "Deploying to production" "stacks applied: app"
if grep -q 'enter close' "$TMP/tui/deploying.screen"; then fail "the apply had ended before a node's progress was seen"; fi
screen deployed "app-1  done" "app-2  done" "app-3  done" "on 3 node(s) of cluster"
mm_as alice target nodes | tee "$TMP/tui/nodes.log"
[ "$(grep -c "${TUI_COMMIT:0:12}" "$TMP/tui/nodes.log")" = 3 ] || fail "every node should run the commit deployed from the terminal UI"

step "terminal UI: a node whose mmd hangs is marked, its services are not whole, and keys still work"
freeze_mmd app-2
tui silent --step 'key=3' --step 'mark=2 of 3 nodes answered' --step 'snap=waiting.screen' --step 'key=2' \
  --step 'mark=ENVIRONMENT' --step 'key=3' --step 'mark=app-2 not answering' --step 'snap=silent.screen'
thaw_mmd app-2
every_node_ready 30
screen waiting "app-2  not answered yet"
screen silent "app-2 not answering" "app-2  no answer" "app-1  web-1" "app-3  web-1"

# --- networks -----------------------------------------------------------------
# address_on NODE SERVICE NETWORK: its address on that mm network.
address_on() {
  # shellcheck disable=SC2016 # the inner command expands on the node
  on "$1" sh -c 'engine inspect "$(engine ps -q --filter "label=com.docker.compose.service=$1")" --format "{{range \$name, \$network := .NetworkSettings.Networks}}{{\$name}} {{\$network.IPAddress}}{{println}}{{end}}"' sh "$2" \
    | awk -v suffix="-$3" '$1 ~ suffix"$" {print $2}'
}
# pings NODE SERVICE ADDRESS: the service's container on the node gets an
# answer from the address. A new WireGuard session can take a few seconds.
pings() {
  # shellcheck disable=SC2016 # the inner command expands on the node
  on "$1" sh -c 'engine exec "$(engine ps -q --filter "label=com.docker.compose.service=$1")" ping -c 3 -W 2 "$2" >/dev/null 2>&1' sh "$2" "$3"
}
reaches_over() { pings "$1" "$2" "$3" || fail "$2 on $1 does not reach $4 ($3)"; printf '  ok   %s on %s reaches %s (%s)\n' "$2" "$1" "$4" "$3"; }
cut_off_from() { if pings "$1" "$2" "$3"; then fail "$2 on $1 reaches $4 ($3) and must not"; fi; printf '  ok   %s on %s cannot reach %s (%s)\n' "$2" "$1" "$4" "$3"; }

step "networks: a network of mm.yaml spans the nodes that run a service on it"
NETWORK_LINES='networks:
  backend: {}
  frontend: {}
  sealed: {internal: true}'
SERVICE_LINES='      api: {stack: app, networks: [backend, frontend]}
      web: {stack: app, placement: all, networks: [backend], drain: 2s}
      db: {stack: app, placement: app-3, networks: [frontend]}
      worker: {stack: app, placement: workers, networks: [sealed]}'
commit "$(users alice bob)" "networks that span the nodes"
publish
mm_as alice deploy plan | tee "$TMP/net-plan.log"
has "$TMP/net-plan.log" "networks span the nodes: addresses from 10.64.0.0/12, between nodes through mm0 (MTU 1420)" \
  "backend: gets 10.64.0.0/24 of 10.64.0.0/20" "frontend: gets 10.64.16.0/24 of 10.64.16.0/20" \
  "backend: gets 10.64.1.0/24 of 10.64.0.0/20" "sealed: gets 10.64.32.0/24 of 10.64.32.0/20, internal" \
  "backend: gets 10.64.2.0/24 of 10.64.0.0/20" "sealed: gets 10.64.33.0/24 of 10.64.32.0/20, internal" \
  "routes through the mesh:" "+ 10.64.1.0/24 to app-2" "firewall rules:" \
  '+ -A MM-CLUSTER -d 10.64.0.0/12 -m comment --comment "mm cluster networks" -j DROP' "mm internal network"
deploy | tee "$TMP/net-apply.log"
[ "$(grep -c ' applied ' "$TMP/net-apply.log")" = 3 ] || fail "three nodes should report applied"

step "containers on different nodes reach each other only on a network they share"
API_F=$(address_on app-1 api frontend)
WEB_2=$(address_on app-2 web backend)
WEB_3=$(address_on app-3 web backend)
DB=$(address_on app-3 db frontend)
WORKER_3=$(address_on app-3 worker sealed)
for address in "$API_F" "$WEB_2" "$WEB_3" "$DB" "$WORKER_3"; do
  case $address in 10.6[4-9].*|10.7[0-9].*) ;; *) fail "'$address' is not an address of cluster.networks" ;; esac
  # The first and the last address of a node's range are kept back.
  case $address in *.0|*.255) fail "'$address' is the first or last address of its node's range" ;; esac
done
reaches_over app-2 web "$WEB_3" "web on app-3 over backend"
reaches_over app-1 api "$WEB_2" "web on app-2 over backend"
reaches_over app-1 api "$DB" "db on app-3 over frontend, the second network of api"
reaches_over app-3 db "$API_F" "api on app-1 over frontend"
reaches_over app-2 worker "$WORKER_3" "worker on app-3 over sealed"
cut_off_from app-2 web "$DB" "db on app-3, which is on frontend only"
cut_off_from app-3 db "$WEB_3" "web on its own node, which is on backend only"
cut_off_from app-2 worker "$WEB_3" "web on app-3, which is on backend only"
cut_off_from app-2 web "$WORKER_3" "worker on app-3, which is on sealed only"
# The way out: the client container stands in for the rest of the world.
OUTSIDE=$(docker inspect --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$CLIENT")
reaches_over app-2 web "$OUTSIDE" "an address outside the cluster"
cut_off_from app-2 worker "$OUTSIDE" "an address outside the cluster from the internal network"
# A node's own address is not a member of a network, and nothing reaches a
# container address that is routed at a node from outside the mesh.
if on app-2 ping -c 2 -W 2 "$WEB_3" >/dev/null 2>&1; then fail "the host app-2 reaches a container on app-3"; fi
# shellcheck disable=SC2016 # the inner command expands on the node
on app-1 sh -c 'ip route add "$1/32" via "$2" || exit 1; ping -c 2 -W 1 "$1" >/dev/null 2>&1; reached=$?; ip route del "$1/32"; [ "$reached" -ne 0 ]' sh "$WEB_3" "$(address_of app-3)" \
  || fail "a container on app-3 answers a packet that did not come through the mesh"
printf '  ok   nothing reaches %s on app-3 from a host or from outside the mesh\n' "$WEB_3"

step "the mesh carries the hosts and the cluster's networks, and nothing else"
# connects NODE ADDRESS PORT: whether the node itself completes a TCP handshake.
connects() { on "$1" timeout 3 bash -c "exec 3<>/dev/tcp/$2/$3" 2>/dev/null; }
# container_connects NODE ADDRESS PORT: the same from a container on the
# engine's default bridge of the node, a network of that host alone.
container_connects() { on "$1" engine run --rm --network bridge "$WORKLOAD_IMAGE" nc -w 2 -z "$2" "$3" >/dev/null 2>&1; }
# Podman gives a container no NET_RAW, which `ping` needs.
PING_CAPS=''
[ "$ENGINE" = podman ] && PING_CAPS='--cap-add=NET_RAW'
# container_pings NODE ADDRESS: a container on the node's own bridge gets an
# answer from the address.
container_pings() {
  # shellcheck disable=SC2086 # empty, or one flag
  on "$1" engine run --rm --network bridge $PING_CAPS "$WORKLOAD_IMAGE" ping -c 2 -W 2 "$2" >/dev/null 2>&1
}
# Every node loaded the chains with its mesh, before a container ran, and
# each network a deployment stored since is in them.
for index in 1 2 3; do
  node=app-$index
  on "$node" iptables -t "$FORWARD_TABLE" -S MM-MESH >"$TMP/mesh-forward-$node.log"
  on "$node" iptables -S MM-MESH-IN >"$TMP/mesh-input-$node.log"
  has "$TMP/mesh-forward-$node.log" '-m conntrack --ctstate RELATED,ESTABLISHED -m comment --comment "mm replies" -j RETURN' \
    '-o mm0 -m comment --comment "mm network ' '-A MM-MESH -o mm0 -m comment --comment "mm mesh guard" -j DROP'
  has "$TMP/mesh-input-$node.log" '-A MM-MESH-IN -i lo ' "-s 10.90.0.0/24 -d 10.90.0.$index/32 -i mm0 " \
    "-A MM-MESH-IN -d 10.90.0.$index/32 -m comment --comment \"mm mesh guard\" -j DROP" '-i mmn-'
  [ "$(tail -n 1 "$TMP/mesh-forward-$node.log" | grep -c -- '-j DROP')" = 1 ] || fail "$node: the last rule of MM-MESH is not the drop"
  on "$node" iptables -t "$FORWARD_TABLE" -C "$FORWARD_FROM" -j MM-MESH || fail "$node: $FORWARD_FROM ($FORWARD_TABLE) does not enter MM-MESH"
  on "$node" iptables -C INPUT -j MM-MESH-IN || fail "$node: INPUT does not enter MM-MESH-IN"
  [ -z "$(on "$node" /usr/libexec/mmd mesh up)" ] || fail "$node: \`mmd mesh up\` changed something that was in place"
done
# The hosts talk to each other over the mesh as before: the name exchange.
reaches app-2 10.90.0.1 || fail "app-2 itself does not reach app-1 over the mesh"
connects app-2 10.90.0.1 7953 || fail "app-2 itself does not reach app-1's name exchange"
connects app-1 10.90.0.1 7953 || fail "app-1 does not reach its own name exchange"
# A container on a network of its host alone reaches no node through the
# mesh, and not its own node's mesh address either.
for target in 10.90.0.1 10.90.0.2; do
  if container_connects app-2 "$target" 7953; then fail "a container on app-2's own bridge reaches $target:7953"; fi
done
if container_pings app-2 10.90.0.1; then
  fail "a container on app-2's own bridge gets an answer from app-1's mesh address"
fi
container_pings app-2 "$OUTSIDE" || fail "a container on app-2's own bridge lost its way out"
printf '  ok   a container on a network of app-2 alone reaches the outside, and no mesh address\n'
# An address of the mesh, or the one a node has on a cluster network, that
# is routed at the node's public interface from outside the mesh is dropped.
GATEWAY_3=$(on app-3 sh -c "ip -4 -o addr show | awk '\$2 ~ /^mmn-/ {print \$4}' | head -n 1 | cut -d/ -f1")
[ -n "$GATEWAY_3" ] || fail "app-3 has no address on a cluster network"
# shellcheck disable=SC2016 # the inner command expands on the node
outside_mesh() {
  on app-1 sh -c 'ip route add "$1/32" via "$2" || exit 2; timeout 3 bash -c "exec 3<>/dev/tcp/$1/$3" 2>/dev/null; reached=$?; ip route del "$1/32"; exit "$reached"' sh "$1" "$(address_of app-3)" "$2"
}
if outside_mesh 10.90.0.3 7953; then fail "app-3 completes a handshake on its mesh address from outside the mesh"; fi
if outside_mesh "$GATEWAY_3" 53; then fail "app-3 completes a handshake on $GATEWAY_3 from outside the mesh"; fi
printf '  ok   app-3 takes 10.90.0.3 on mm0 and %s on its bridge only\n' "$GATEWAY_3"
# Without the rule the same connection is accepted: that is what the chain
# is for. The node reports the missing rule, and its next `mesh up` loads it.
on app-3 iptables -D INPUT -j MM-MESH-IN
outside_mesh 10.90.0.3 7953 || fail "the control did not connect: this check proves nothing"
mm_as alice target mesh status >"$TMP/mesh-guard-status.log" 2>&1 || true
has "$TMP/mesh-guard-status.log" MM_MESH_GUARD "node 'app-3'" "INPUT does not enter MM-MESH-IN" "systemctl reload mm-mesh"
mm_as alice doctor >"$TMP/mesh-guard-doctor.log" 2>&1 || true
has "$TMP/mesh-guard-doctor.log" MM_MESH_GUARD
on app-3 /usr/libexec/mmd mesh up | tee "$TMP/mesh-guard-repair.log"
has "$TMP/mesh-guard-repair.log" "mesh: loaded the firewall chains MM-MESH and MM-MESH-IN (INPUT does not enter MM-MESH-IN)"
if outside_mesh 10.90.0.3 7953; then fail "the repaired rules let a connection from outside the mesh through"; fi
mm_as alice target mesh status >"$TMP/mesh-guard-status.log" 2>&1 || true
if grep -q MM_MESH_GUARD "$TMP/mesh-guard-status.log"; then fail "the repaired rules are still reported"; fi

step "every node holds its addresses and rules, and a second plan changes nothing about them"
for node in $NODES; do
  on "$node" /usr/libexec/mmd network show >"$TMP/network-$node.json"
  [ "$(json 'value.get("drift", [])' <"$TMP/network-$node.json")" = "[]" ] || { cat "$TMP/network-$node.json" >&2; fail "$node reports drift"; }
  [ "$(json 'value["stored"]["table"]["networks"]["'"$(printf '%s' "$PROJECT_ID" | tr -d - | cut -c1-12)"'-production-backend"]["block"]' <"$TMP/network-$node.json")" = "10.64.0.0/20" ] \
    || fail "$node does not hold the block of backend"
done
mm_as alice doctor >"$TMP/net-doctor.log" 2>&1 || true
if grep -q MM_NETWORK_DRIFT "$TMP/net-doctor.log"; then cat "$TMP/net-doctor.log" >&2; fail "doctor reports network drift"; fi
mm_as alice --output json deploy plan >"$TMP/net-replan.json"
[ "$(json 'any(n.get("routes") or n.get("firewall") or n.get("repairs") or any(w["assigned"] or w.get("replaces") for w in n.get("networks", [])) for n in value["networks"]["nodes"])' <"$TMP/net-replan.json")" = False ] \
  || fail "a second plan should change nothing about the networks"

step "a node that already routes part of cluster.networks elsewhere fails the plan with the key to set"
on app-2 ip route add 10.70.0.0/20 dev eth0
if mm_as alice deploy plan >"$TMP/net-conflict.log" 2>&1; then fail "the plan accepted addresses that app-2 already routes elsewhere"; fi
has "$TMP/net-conflict.log" "node 'app-2' already has the route 10.70.0.0/20 through eth0, which overlaps cluster.networks 10.64.0.0/12" \
  'Set `cluster.networks` under targets.' "such as 10.32.0.0/12"
on app-2 ip route del 10.70.0.0/20 dev eth0
mm_as alice deploy plan >/dev/null || fail "the plan still fails after the route is gone"
printf '  ok   the plan names app-2, its route, and the key, and passes once the route is gone\n'

step "a rule removed by hand is reported, shown by the plan, and repaired by its apply"
on app-2 iptables -t "$FORWARD_TABLE" -D MM-CLUSTER 3
cut_off_from app-1 api "$WEB_2" "web on app-2 while its rule is missing"
mm_as alice service status >"$TMP/net-drift-status.log" 2>&1 || true
has "$TMP/net-drift-status.log" "node app-2: cluster networks differ from the last deployment" "missing rule"
mm_as alice doctor >"$TMP/net-drift-doctor.log" 2>&1 || true
has "$TMP/net-drift-doctor.log" MM_NETWORK_DRIFT "systemctl restart mm-network"
mm_as alice deploy plan >"$TMP/net-drift-plan.log"
has "$TMP/net-drift-plan.log" "repairs what differs on the node from the last deployment" "missing rule"
deploy >/dev/null
reaches_over app-1 api "$WEB_2" "web on app-2 again"

# --- names --------------------------------------------------------------------
# resolves NODE SERVICE NAME: the addresses the service's container on the
# node gets for the name, sorted, on one line; nothing when it has none.
resolves() {
  # The final dot keeps the lookup tool from adding the search domains this
  # machine's own resolver configuration may list.
  { in_service "$1" "$2" "nslookup -type=a $3. 2>/dev/null" || true; } \
    | awk '/^Name:/ {found = 1} found && /^Address/ {print $NF}' | sort -V | tr '\n' ' ' | sed 's/ $//'
}
sorted() { printf '%s\n' "$@" | sort -V | tr '\n' ' ' | sed 's/ $//'; }
# name_becomes NODE SERVICE NAME ADDRESSES [SECONDS]: the lookup gives exactly
# these addresses, at once or within SECONDS.
name_becomes() {
  local got='' waited=0 limit=${5:-10}
  while :; do
    got=$(resolves "$1" "$2" "$3")
    [ "$got" = "$4" ] && break
    [ "$waited" -lt "$limit" ] || fail "$2 on $1 resolves $3 to '$got' instead of '$4' after ${limit}s"
    sleep 1; waited=$((waited + 1))
  done
  printf '  ok   %s on %s resolves %s to %s (after %ss)\n' "$2" "$1" "$3" "${4:-nothing}" "$waited"
}
names_compose() {
  printf 'services:\n'
  workload api
  printf '%s' "${1:-}"
  # web says it is healthy for as long as /tmp/ok exists.
  printf '  web:\n    image: %s\n    command: ["sh", "-c", "touch /tmp/ok && echo web-started && exec sleep infinity"]\n    stop_grace_period: 1s\n%s' "$WORKLOAD_IMAGE" "$WORKLOAD_CAPS"
  printf '    healthcheck: {test: ["CMD", "test", "-f", "/tmp/ok"], interval: 2s, timeout: 2s, retries: 1}\n'
  printf '    depends_on: [db]\n    volumes: ["./api/site.txt:/site.txt:ro"]\n'
  workload db
  printf '    volumes: [dbdata:/data]\n'
  workload worker
  printf 'volumes:\n  dbdata: {}\n'
}

step "names: a service name is the node's own healthy containers, or else every node's"
has "$TMP/net-plan.log" "service names: each node answers its own containers" "on the same node, or else on every node"
names_compose >"$TMP/repo/compose.yaml"
commit "$(users alice bob)" "web has a health check"
publish
deploy >/dev/null
WEB_1=$(address_on app-1 web backend)
WEB_2=$(address_on app-2 web backend)
WEB_3=$(address_on app-3 web backend)
ALL_WEB=$(sorted "$WEB_1" "$WEB_2" "$WEB_3")
# tasks.<service>: every node's, the same answer everywhere.
name_becomes app-1 api tasks.web "$ALL_WEB" 30
name_becomes app-3 web tasks.web "$ALL_WEB" 30
# <service>: web runs on every node, so each node answers with its own.
name_becomes app-1 api web "$WEB_1"
name_becomes app-2 web web "$WEB_2"
name_becomes app-3 web web "$WEB_3"
name_becomes app-1 api web.app-3 "$WEB_3"
name_becomes app-1 api db "$(address_on app-3 db frontend)"
name_becomes app-2 web api "$(address_on app-1 api backend)"
# web is not on frontend, so db is not its to see; worker is on the internal
# network, where the node answers as well.
name_becomes app-2 web db ""
name_becomes app-2 worker worker "$(address_on app-2 worker sealed)"
name_becomes app-2 worker tasks.worker "$(sorted "$(address_on app-2 worker sealed)" "$(address_on app-3 worker sealed)")"
# Every other name goes to the node's own resolvers: here the engine that
# runs the nodes, which knows the client container.
name_becomes app-2 web "$CLIENT" "$OUTSIDE"
in_service app-2 web 'cat /etc/resolv.conf' >"$TMP/resolv.conf"
has "$TMP/resolv.conf" "nameserver 10.64.1.1" "options ndots:0"
mm_as alice service status | tee "$TMP/names-status.log"
has "$TMP/names-status.log" "RESOLVES TO" "$WEB_1 (app-1), $WEB_2 (app-2), $WEB_3 (app-3); each node resolves to its own"
# api and db run on one node: every node gives the same answer, and the line says no more.
if grep -E '^  (api|db) .*resolve' "$TMP/names-status.log"; then fail "a service on one node should not say which nodes answer with their own"; fi
mm_as alice --output json service status web >"$TMP/names-status.json"
[ "$(json 'sorted(a["node"] for n in value["names"]["names"] for a in n["addresses"])' <"$TMP/names-status.json")" = "['app-1', 'app-2', 'app-3']" ] \
  || fail "the JSON status should list web's address on every node"
[ "$(json 'sorted((l["node"], l["addresses"], l["local"]) for n in value["names"]["names"] for l in n["lookups"])' <"$TMP/names-status.json")" = "[('app-1', ['$WEB_1'], True), ('app-2', ['$WEB_2'], True), ('app-3', ['$WEB_3'], True)]" ] \
  || { cat "$TMP/names-status.json" >&2; fail "the JSON status should say what web resolves to on each node"; }

step "an unhealthy container leaves the answers on every node, and comes back"
# asks_on_app_3 NAME: what another container of backend on app-3 gets from
# the node, sorted, on one line.
asks_on_app_3() {
  # shellcheck disable=SC2016 # the inner command expands on the node
  on app-3 sh -c 'engine run --rm --network "$(engine network ls --format "{{.Name}}" | grep -- "-backend$")" "$1" nslookup -type=a "$2". 10.64.2.1 2>/dev/null' sh "$WORKLOAD_IMAGE" "$1" \
    | awk '/^Name:/ {found = 1} found && /^Address/ {print $NF}' | sort -V | tr '\n' ' ' | sed 's/ $//'
}
[ "$(asks_on_app_3 web)" = "$WEB_3" ] || fail "app-3 does not answer its own containers with its own web"
in_service app-3 web 'rm /tmp/ok'
name_becomes app-1 api tasks.web "$(sorted "$WEB_1" "$WEB_2")" 10
name_becomes app-1 api web "$WEB_1"
name_becomes app-1 api web.app-3 ""
# On its own node the last local container is gone from the answer, so the
# node answers with the other nodes' at once: never with nothing.
for _ in 1 2 3 4 5 6 7 8 9 10; do
  got=$(asks_on_app_3 web)
  [ "$got" = "$WEB_3" ] || break
  sleep 1
done
[ "$got" = "$(sorted "$WEB_1" "$WEB_2")" ] || fail "app-3 answers its own containers with '$got' after its web turned unhealthy, not with the other nodes' web"
printf '  ok   with its own web unhealthy, app-3 answers web with %s\n' "$got"
# The container itself still finds its own name: its health check may ask.
name_becomes app-3 web web "$WEB_3"
mm_as alice service status web >"$TMP/names-unhealthy.log"
has "$TMP/names-unhealthy.log" "left out: $WEB_3 (app-3, unhealthy)" "app-1 and app-2 resolve to their own, app-3 to all"
in_service app-3 web 'touch /tmp/ok'
name_becomes app-1 api tasks.web "$ALL_WEB" 10
for _ in 1 2 3 4 5 6 7 8 9 10; do
  got=$(asks_on_app_3 web)
  [ "$got" != "$WEB_3" ] || break
  sleep 1
done
[ "$got" = "$WEB_3" ] || fail "app-3 answers web with '$got' after its own web is healthy again"

step "a node whose mmd stops answering drops out of the other nodes' answers"
stop_mmd app-3
name_becomes app-1 api tasks.web "$(sorted "$WEB_1" "$WEB_2")" 20
mm_as alice service status >"$TMP/names-down.log" 2>&1 || true
has "$TMP/names-down.log" "from node 'app-3' at 10.90.0.3:7953" "mm target mesh status"
start_mmd app-3
wait_mmd app-3
name_becomes app-1 api tasks.web "$ALL_WEB" 30

step "an mmd upgrade, one node at a time, under traffic: no container restarts, and no lookup or request fails"
# in_service_detached NODE SERVICE COMMAND: as in_service, left running.
in_service_detached() {
  # shellcheck disable=SC2016 # the inner command expands on the node
  on "$1" sh -c 'engine exec -d "$(engine ps -q --filter "label=com.docker.compose.service=$1")" sh -c "$2"' sh "$2" "$3"
}
started_at() { on "$1" sh -c 'engine inspect -f "{{.Name}} {{.State.StartedAt}}" $(engine ps -q) | sort'; }
kept_names() { on "$1" cat /run/fd-store.json | json '" ".join(value["names"])'; }
for node in $NODES; do stop_mmd "$node"; start_kept_mmd "$node"; done
for node in $NODES; do wait_mmd "$node"; done
name_becomes app-1 api tasks.web "$ALL_WEB" 30
index=1
for node in $NODES; do
  # The request socket, lookups on the gateway over UDP and TCP, and the
  # exchange between nodes on the mesh address.
  missing=''
  for _ in $(seq 1 20); do
    kept=$(kept_names "$node") missing=''
    for name in unix-mm.sock udp-10.64. tcp-10.64. "tcp-10.90.0.$index-7953"; do
      case "$kept" in *"$name"*) ;; *) missing="$missing $name" ;; esac
    done
    [ -z "$missing" ] && break
    sleep 0.5
  done
  [ -z "$missing" ] || fail "$node: mmd did not hand$missing to the store: $kept"
  printf '  ok   %s keeps: %s\n' "$node" "$kept"
  started_at "$node" >"$TMP/upgrade-before-$node.txt"
  index=$((index + 1))
done
# Every web serves a page; on each node a container asks for the other two
# nodes' web by name, over and over: each lookup goes to its own node's mmd.
for node in $NODES; do in_service "$node" web 'echo up >/tmp/index.html && httpd -p 8080 -h /tmp'; done
# A failure is noted with when the request began and ended, and what a
# lookup of the name right after it found.
loop() {
  in_service_detached "$1" "$2" "rm -f /tmp/loop.*; while [ ! -f /tmp/loop.stop ]; do for target in $3; do began=\$(date +%s); if wget -q -T 3 -O /dev/null http://\$target:8080/; then echo ok >>/tmp/loop.ok; else echo \"\$began-\$(date +%s) \$target (then: \$(nslookup \$target 2>&1 | grep -c Address) addresses)\" >>/tmp/loop.fail; fi; done; sleep 0.2; done"
}
# The nodes' mmd started a moment ago: the loops begin once every node
# answers for the others' web, so they measure the upgrade, not the start.
name_becomes app-1 api web.app-2 "$WEB_2" 30
name_becomes app-1 api web.app-3 "$WEB_3" 30
name_becomes app-2 web web.app-3 "$WEB_3" 30
name_becomes app-2 web web.app-1 "$WEB_1" 30
name_becomes app-3 web web.app-1 "$WEB_1" 30
name_becomes app-3 web web.app-2 "$WEB_2" 30
loop app-1 api "web.app-2 web.app-3"
loop app-2 web "web.app-3 web.app-1"
loop app-3 web "web.app-1 web.app-2"
sleep 3
for node in $NODES; do
  old=$(on "$node" pgrep -x mmd)
  # What `mm target setup` does with a new release on one node: both files
  # are replaced, then mmd restarts. The store keeps its sockets meanwhile.
  docker cp "$BIN_DIR/mmd" "$(container "$node"):/usr/libexec/mmd.new"
  docker cp "$BIN_DIR/mm" "$(container "$node"):/usr/bin/mm.new"
  restarted=$(date +%s)
  on "$node" sh -c 'chmod 755 /usr/libexec/mmd.new /usr/bin/mm.new && mv /usr/libexec/mmd.new /usr/libexec/mmd && mv /usr/bin/mm.new /usr/bin/mm && pkill -HUP -f "^python3 /usr/local/bin/live-fd-store"'
  # A request that arrives while mmd restarts waits for the next one.
  mm_as alice target nodes >"$TMP/upgrade-nodes-$node.log" 2>&1 || { cat "$TMP/upgrade-nodes-$node.log" >&2; fail "a request during $node's restart failed"; }
  [ "$(grep -c ' ready ' "$TMP/upgrade-nodes-$node.log")" = 3 ] || { cat "$TMP/upgrade-nodes-$node.log" >&2; fail "a node was not ready during $node's restart"; }
  for _ in $(seq 1 20); do
    new=$(on "$node" pgrep -x mmd || true)
    [ -n "$new" ] && [ "$new" != "$old" ] && break
    sleep 0.5
  done
  [ -n "$new" ] && [ "$new" != "$old" ] || fail "$node: mmd did not start again"
  printf '  ok   %s: mmd %s replaced by %s (restarted at %s)\n' "$node" "$old" "$new" "$restarted"
  sleep 4
done
for node in $NODES; do in_service "$node" "$([ "$node" = app-1 ] && echo api || echo web)" 'touch /tmp/loop.stop'; done
sleep 1
for node in $NODES; do
  service=$([ "$node" = app-1 ] && echo api || echo web)
  answered=$(in_service "$node" "$service" 'cat /tmp/loop.ok 2>/dev/null | wc -l')
  failed=$(in_service "$node" "$service" 'cat /tmp/loop.fail 2>/dev/null || true')
  if [ -n "$failed" ]; then
    failing=$node
    for each in $NODES; do printf -- '--- %s mmd.log\n' "$each" >&2; on "$each" tail -n 30 /tmp/mmd.log >&2 || true; done
    fail "$failing: requests by name failed during the upgrade: $failed"
  fi
  [ "$answered" -gt 50 ] || fail "$node: only $answered requests by name were made"
  printf '  ok   %s: %s requests by name, none failed\n' "$node" "$answered"
  started_at "$node" >"$TMP/upgrade-after-$node.txt"
  diff "$TMP/upgrade-before-$node.txt" "$TMP/upgrade-after-$node.txt" || fail "$node: a container started again during the upgrade"
done
name_becomes app-1 api tasks.web "$ALL_WEB" 10
name_becomes app-3 web web.app-2 "$WEB_2" 10

step "the resolver and the exchange between nodes answer nobody else"
# A container asks another node's resolver, another node's exchange port, and
# its own node's exchange port: no answer, no byte.
if in_service app-2 web 'nslookup -type=a web. 10.64.0.1' >/dev/null 2>&1; then fail "the resolver of app-1 answers a container of app-2"; fi
[ "$(in_service app-2 web 'nc -w 3 10.90.0.1 7953 </dev/null 2>/dev/null | wc -c')" = 0 ] || fail "a container reads the exchange of another node"
[ "$(in_service app-2 web 'nc -w 3 10.90.0.2 7953 </dev/null 2>/dev/null | wc -c')" = 0 ] || fail "a container reads the exchange of its own node"
# The node itself is a member of the mesh and reads what its peer runs.
on app-2 bash -c 'exec 3<>/dev/tcp/10.90.0.1/7953 && cat <&3' >"$TMP/exchange.json"
[ "$(json 'value["node"]' <"$TMP/exchange.json")" = app-1 ] || fail "app-2 does not read the records of app-1"
if grep -q 'host/' "$TMP/exchange.json"; then fail "a node sends what stays on it"; fi
# From outside the mesh nothing listens: not on the node's own address.
for node in $NODES; do
  docker exec "$CLIENT" python3 -c '
import socket, sys
for port in (53, 7953):
    try:
        socket.create_connection((sys.argv[1], port), timeout=2).close()
    except OSError:
        continue
    sys.exit(f"{sys.argv[1]} accepts connections on port {port}")
' "$(address_of "$node")" || fail "$node answers from outside the mesh"
  # Podman's own resolver answers the containers of a network of the host
  # only, on that network's gateway; mm's networks have it off.
  listening=$(on "$node" ss -Hlntup | grep -v '"aardvark-dns"' | awk '$5 ~ /:(53|7953)$/ {print $5}' | grep -v -E '^(10\.90\.0\.[0-9]+:7953|10\.(6[4-9]|7[0-9])\.[0-9]+\.[0-9]+:53|127\.)' || true)
  [ -z "$listening" ] || fail "$node listens for names on: $listening"
done
printf '  ok   only the node'"'"'s own containers get answers, and only mesh peers read the exchange\n'

step "a Compose resolver key is refused where the nodes answer names"
names_compose '    dns: [192.0.2.53]
' >"$TMP/repo/compose.yaml"
commit "$(users alice bob)" "api names its own resolver"
publish
if mm_as alice config validate >"$TMP/names-dns.log" 2>&1; then fail "config validate accepted dns: on a cluster network"; fi
has "$TMP/names-dns.log" MM_COMPOSE_DNS "service 'api' sets \`dns\`" "change /etc/resolv.conf on the nodes"
names_compose >"$TMP/repo/compose.yaml"
commit "$(users alice bob)" "api asks the node again"
publish

# --- rolling deploys ----------------------------------------------------------
# web_command NODE: what the node's web container was started with.
web_command() {
  # shellcheck disable=SC2016 # the inner command expands on the node
  on "$1" sh -c 'engine inspect --format "{{json .Config.Cmd}}" $(engine ps -q --filter label=com.docker.compose.service=web)'
}
# web_ids NODE: the node's web containers, running or not.
web_ids() { on "$1" engine ps -aq --no-trunc --filter label=com.docker.compose.service=web | sort | tr '\n' ' ' | sed 's/ $//'; }
# rolling_compose TAG HEALTH [READY] [HEALTH_OPTIONS]: the names project with
# a web that prints TAG and has the health check HEALTH. READY is how its
# command makes /tmp/ok.
rolling_compose() {
  printf 'services:\n'
  workload api
  printf '  web:\n    image: %s\n    command: ["sh", "-c", "%s echo web-%s-started; exec sleep infinity"]\n    stop_grace_period: 1s\n%s' \
    "$WORKLOAD_IMAGE" "${3:-touch /tmp/ok;}" "$1" "$WORKLOAD_CAPS"
  printf '    healthcheck: {test: ["CMD-SHELL", "%s"], interval: 2s, timeout: 2s, retries: 1%s}\n' "$2" "${4:-}"
  printf '    depends_on: [db]\n    volumes: ["./api/site.txt:/site.txt:ro"]\n'
  workload db
  printf '    volumes: [dbdata:/data]\n'
  workload worker
  printf 'volumes:\n  dbdata: {}\n'
}
ROLLING_SERVICES=$SERVICE_LINES

step "rolling: the plan says the order, the window, and what happens on failure"
mm_as alice deploy plan | tee "$TMP/roll-plan.log"
has "$TMP/roll-plan.log" "rollout: app on app-1, then app-2, then app-3" \
  "a stack may take 10m on a node to become healthy (deploy.convergence-timeout)" \
  "on failure every node this deployment changed returns to what it ran before" \
  "restart: unless-stopped added to"
mm_as alice deploy status | tee "$TMP/roll-status-before.log"
has "$TMP/roll-status-before.log" "Deployments of cluster [production]" "runs:" "No rollout is in progress."
DEPLOYED_BEFORE=$(mm_as alice --output json deploy status | json 'value["nodes"][0]["status"]["current"]["commit"]')

step "rolling: a health check that fails on the second node rolls the first two back"
WEB_BEFORE_1=$(web_ids app-1); WEB_BEFORE_3=$(web_ids app-3)
# On app-2 (whose backend gateway is 10.64.1.1) the new web is never
# healthy; on the other nodes it is.
rolling_compose v2 "! grep -q 'nameserver 10.64.1.1' /etc/resolv.conf" >"$TMP/repo/compose.yaml"
commit "$(users alice bob)" "a web that is unhealthy on app-2"
publish
if deploy >"$TMP/roll-fail.log" 2>&1; then cat "$TMP/roll-fail.log" >&2; fail "a deployment that is unhealthy on app-2 went through"; fi
cat "$TMP/roll-fail.log"
has "$TMP/roll-fail.log" "==> app-1: app" "==> app-2: app" "==> app-2: rolling back" "==> app-1: rolling back" \
  "FAILED       service 'web' of stack 'app': its health check fails (MM_RUNTIME_UNHEALTHY); rolled back to what it ran before" \
  "last log lines of the failed container:" "web-v2-started" \
  "rolled back  runs what it ran before" "not started  unchanged" \
  "MM_APPLY_STOPPED" "failed on node 'app-2'" \
  "Rolled back to what they ran before: app-1, app-2." "Not changed: app-3."
if grep -q '==> app-3' "$TMP/roll-fail.log"; then fail "the rollout reached app-3"; fi
for node in app-1 app-2; do
  case "$(web_command "$node")" in *web-started*) ;; *) fail "$node does not run the old web again: $(web_command "$node")" ;; esac
done
[ "$(web_ids app-3)" = "$WEB_BEFORE_3" ] || fail "app-3's web container was touched"
[ "$(web_ids app-1)" != "$WEB_BEFORE_1" ] || fail "app-1 was not changed and rolled back at all"
name_becomes app-1 api tasks.web "$(sorted "$(address_on app-1 web backend)" "$(address_on app-2 web backend)" "$(address_on app-3 web backend)")" 30
mm_as alice deploy status | tee "$TMP/roll-status-failed.log"
has "$TMP/roll-status-failed.log" "was rolled back: the node runs what it ran before" "No rollout is in progress."
[ "$(mm_as alice --output json deploy status | json 'sorted(set(n["status"]["current"]["commit"] for n in value["nodes"]))')" = "['$DEPLOYED_BEFORE']" ] \
  || fail "a rolled-back deployment changed what a node records as deployed"
[ "$(grep -c 'rollout-rollback' <(on app-1 cat /var/lib/mm/audit.jsonl))" -ge 1 ] || fail "the rollback is not in app-1's audit log"

step "rolling: start-first starts the new web next to the old one on each node"
rolling_compose v3 "test -f /tmp/ok" >"$TMP/repo/compose.yaml"
# On Podman this web keeps the host's user ids: a service with
# `host-user-namespace` rolls out start-first too.
WEB_HOST_IDS=''
[ "$ENGINE" = podman ] && WEB_HOST_IDS=', host-user-namespace: "start-first with the host ids"'
SERVICE_LINES="      api: {stack: app, networks: [backend, frontend]}
      web: {stack: app, placement: all, networks: [backend], update: {order: start-first}, drain: 2s$WEB_HOST_IDS}
      db: {stack: app, placement: app-3, networks: [frontend]}
      worker: {stack: app, placement: workers, networks: [sealed]}"
commit "$(users alice bob)" "web v3, started before the old one stops"
publish
mm_as alice deploy plan >"$TMP/roll-first-plan.log"
has "$TMP/roll-first-plan.log" "app: web starts before the old container stops (update.order: start-first)"
deploy | tee "$TMP/roll-first.log"
has "$TMP/roll-first.log" "==> app-1: app: started the new web next to the old" "==> app-3: app: started the new web next to the old"
if [ "$ENGINE" = podman ]; then
  in_service app-1 web "cat /proc/self/uid_map" | awk '$1 == 0 && $2 == 0 {found = 1} END {exit !found}' \
    || fail "web, which keeps the host's ids, has ids of its own: $(in_service app-1 web "cat /proc/self/uid_map")"
fi
for node in $NODES; do
  [ "$(web_ids "$node" | wc -w)" = 1 ] || fail "$node keeps $(web_ids "$node" | wc -w) web containers after a start-first rollout"
  case "$(web_command "$node")" in *web-v3-started*) ;; *) fail "$node does not run web v3: $(web_command "$node")" ;; esac
done
name_becomes app-1 api tasks.web "$(sorted "$(address_on app-1 web backend)" "$(address_on app-2 web backend)" "$(address_on app-3 web backend)")" 30
DEPLOYED_V3=$(mm_as alice --output json deploy status | json 'value["nodes"][0]["status"]["current"]["commit"]')
[ "$DEPLOYED_V3" != "$DEPLOYED_BEFORE" ] || fail "the finished deployment was not recorded"

step "rolling: mm deploy rollback returns every node to what it ran before"
mm_as alice deploy rollback --yes 2>&1 | tee "$TMP/roll-back.log"
has "$TMP/roll-back.log" "Rollback: undoes the deployment of commit ${DEPLOYED_V3:0:12}" \
  "returns app-1, app-2, app-3 to commit $DEPLOYED_BEFORE" "Cluster deployment plan" "Deployment "
for node in $NODES; do
  case "$(web_command "$node")" in *web-started*) ;; *) fail "$node does not run the earlier web: $(web_command "$node")" ;; esac
done
mm_as alice deploy status | tee "$TMP/roll-status-back.log"
has "$TMP/roll-status-back.log" "runs:     commit ${DEPLOYED_BEFORE:0:12}" "before:   commit ${DEPLOYED_V3:0:12}"

step "rolling: a second deployment while one runs is refused with who runs it"
# web takes fifteen seconds to become healthy, which keeps the first
# deployment busy on app-1.
rolling_compose v4 "test -f /tmp/ok" "{ sleep 15; touch /tmp/ok; } &" ", start_period: 60s" >"$TMP/repo/compose.yaml"
SERVICE_LINES=$ROLLING_SERVICES
commit "$(users alice bob)" "a web that takes its time"
publish
SLOW=$(mm_as alice --output json deploy plan | json 'value["plan_hash"]')
SECOND=$(mm_as bob --output json deploy plan | json 'value["plan_hash"]')
mm_as alice deploy apply --plan "$SLOW" --yes >"$TMP/roll-slow.log" 2>&1 &
SLOW_PID=$!
sleep 6
if mm_as bob deploy apply --plan "$SECOND" --yes >"$TMP/roll-second.log" 2>&1; then fail "a second deployment ran next to the first"; fi
has "$TMP/roll-second.log" MM_DEPLOY_RUNNING "alice started deploying plan ${SLOW:0:12}" "mm deploy status"
mm_as bob deploy status >"$TMP/roll-status-running.log"
has "$TMP/roll-status-running.log" "is in progress" "alice is deploying plan ${SLOW:0:12} right now"
wait "$SLOW_PID" || { cat "$TMP/roll-slow.log" >&2; fail "the first deployment did not finish"; }
for node in $NODES; do
  case "$(web_command "$node")" in *web-v4-started*) ;; *) fail "$node does not run web v4" ;; esac
done
names_compose >"$TMP/repo/compose.yaml"
commit "$(users alice bob)" "the names project again"
publish
deploy >/dev/null

# --- the daemon cannot wedge itself --------------------------------------------
# relays NODE: the relay processes on a node, one per line.
relays() { on "$1" sh -c "ps -eo pid=,ppid=,etimes=,args= | grep '[r]elay serve' || true"; }
# steady_relays NODE: how many there are, not counting one that only passes
# through (a client's sign of life every few seconds).
steady_relays() {
  local first second
  first=$(relays "$1" | wc -l); sleep 1; second=$(relays "$1" | wc -l)
  [ "$first" -lt "$second" ] && echo "$first" || echo "$second"
}

# let_web_go NODE: lets the web container that waits on a node become healthy.
let_web_go() {
  # shellcheck disable=SC2016 # expands on the node
  on "$1" sh -c 'for c in $(engine ps -q --filter label=com.docker.compose.service=web); do engine exec "$c" touch /tmp/go; done' >/dev/null 2>&1 || true
}

step "a slow rollout step, more changes to the same node, and reads: the daemon keeps answering"
# web becomes healthy when this script lets it, which keeps app-1 in its
# step for as long as the checks take. Two access syncs for app-1 arrive
# meanwhile. Before, the first of them stopped the daemon's thread in the
# kernel, waiting for a file lock the step held: the deployment never
# finished, nothing was answered again, and the relay of every client
# stayed behind.
rolling_compose v5 "test -f /tmp/ok" "{ while [ ! -f /tmp/go ]; do sleep 1; done; touch /tmp/ok; } &" ", start_period: 300s" >"$TMP/repo/compose.yaml"
SERVICE_LINES=$ROLLING_SERVICES
commit "$(users alice bob)" "a web that waits to be let go"
publish
SLOW=$(mm_as alice --output json deploy plan | json 'value["plan_hash"]')
mm_as alice deploy apply --plan "$SLOW" --yes >"$TMP/wedge-slow.log" 2>&1 &
SLOW_PID=$!
# Once app-1 began its part its access change is in place, so an access
# plan made now is a plan of what the node enforces.
rolling_on_1() { mm_as bob --output json deploy status | json '((value["nodes"][0].get("status") or {}).get("rollout") or {}).get("state")'; }
for _ in $(seq 1 60); do [ "$(rolling_on_1)" = running ] && break; sleep 1; done
[ "$(rolling_on_1)" = running ] || { cat "$TMP/wedge-slow.log" >&2; fail "app-1 never began the slow rollout"; }
# Reads and a command in a container are answered while the step runs.
mm_as bob service status >"$TMP/wedge-service.log" || { cat "$TMP/wedge-service.log" >&2; fail "service status failed during the step"; }
has "$TMP/wedge-service.log" app-1 app-2 app-3
mm_as alice service exec api -- echo in-the-step | grep -q in-the-step || fail "a command in a container did not run during the step"
SYNC=$(mm_as ops --output json team sync --nodes app-1 | json 'value["plan_hash"]')
GONE=$(mm_as ops --output json team sync --nodes app-1 | json 'value["plan_hash"]')
mm_as ops team sync --plan "$SYNC" --yes >"$TMP/wedge-sync.log" 2>&1 &
SYNC_PID=$!
mm_as ops team sync --plan "$GONE" --yes >"$TMP/wedge-gone.log" 2>&1 &
# A read while the step runs and two changes wait behind it.
mm_as bob deploy status >"$TMP/wedge-status.log" 2>&1 &
STATUS_PID=$!
sleep 2
kill -0 "$SYNC_PID" 2>/dev/null \
  || { cat "$TMP/wedge-sync.log" >&2; fail "the access sync did not have to wait for the step"; }
# No thread of app-1's daemon waits in the kernel for a file lock.
# shellcheck disable=SC2016 # expands on the node
on app-1 sh -c 'ps -L -o tid=,wchan:32=,comm= -p "$(pidof mmd)"' >"$TMP/wedge-threads.log"
if grep -q locks_lock "$TMP/wedge-threads.log"; then cat "$TMP/wedge-threads.log" >&2; fail "a thread of app-1's mmd waits for a file lock"; fi

# One of the waiting clients is killed. Its relay on app-1 leaves at once,
# and the daemon gives the request up without ever running it.
WAITING=$(steady_relays app-1)
[ "$WAITING" -ge 3 ] || { relays app-1 >&2; fail "expected the relays of the step and of two waiting syncs on app-1"; }
docker exec "$CLIENT" python3 -c '
import os, signal, sys
for pid in filter(str.isdigit, os.listdir("/proc")):
    try:
        command = open(f"/proc/{pid}/cmdline", "rb").read().split(b"\0")
    except OSError:
        continue
    if os.path.basename(command[0]) == b"mm" and sys.argv[1].encode() in command:
        os.kill(int(pid), signal.SIGKILL)
        print("killed the client", pid)
' "$GONE" | grep -q "killed the client" || fail "the waiting client was not there to be killed"
left=no
for _ in $(seq 1 6); do
  [ "$(steady_relays app-1)" -lt "$WAITING" ] && { left=yes; break; }
done
[ "$left" = yes ] || { relays app-1 >&2; fail "the relay of a killed client stayed on app-1"; }
kill -0 "$SYNC_PID" 2>/dev/null || { cat "$TMP/wedge-sync.log" >&2; fail "the access sync ended before the step did"; }
wait "$STATUS_PID" || { cat "$TMP/wedge-status.log" >&2; fail "deploy status failed while changes waited on app-1"; }
has "$TMP/wedge-status.log" "is in progress" "alice is deploying plan ${SLOW:0:12} right now"

# The step is let go: the deployment finishes, and the sync that waited its
# turn goes through.
for _ in $(seq 1 300); do
  kill -0 "$SLOW_PID" 2>/dev/null || break
  let_web_go app-1; let_web_go app-2; let_web_go app-3
  sleep 1
done
if kill -0 "$SLOW_PID" 2>/dev/null; then
  cat "$TMP/wedge-slow.log" >&2
  # shellcheck disable=SC2016 # expands on the node
  on app-1 sh -c 'ps -L -o tid,stat,wchan:32,comm -p "$(pidof mmd)"; ls -l /proc/"$(pidof mmd)"/fd | grep -c access.lock' >&2
  fail "the deployment hangs: app-1's daemon is wedged"
fi
wait "$SLOW_PID" || { cat "$TMP/wedge-slow.log" >&2; fail "the deployment did not finish with other changes waiting on app-1"; }
wait "$SYNC_PID" || { cat "$TMP/wedge-sync.log" >&2; fail "the access sync that waited its turn did not go through"; }
[ "$(grep -c ' applied ' "$TMP/wedge-sync.log")" = 1 ] || { cat "$TMP/wedge-sync.log" >&2; fail "the access sync should apply on app-1"; }
on app-1 grep -Eq '"error_code":"MM_CANCELLED".*"method":"apply_team_sync".*"result":"failed"' /var/lib/mm/audit.jsonl \
  || fail "app-1's daemon did not give up the request of the killed client"
wait || true
for node in $NODES; do
  case "$(web_command "$node")" in *web-v5-started*) ;; *) fail "$node does not run web v5" ;; esac
  for _ in $(seq 1 10); do [ -z "$(relays "$node")" ] && break; sleep 1; done
  [ -z "$(relays "$node")" ] || { relays "$node" >&2; fail "$node keeps a relay process after every client is gone"; }
done
names_compose >"$TMP/repo/compose.yaml"
commit "$(users alice bob)" "the names project once more"
publish
deploy >/dev/null

# --- nothing locks an environment ----------------------------------------------
# operations_on NODE: how many operations the node lists as running now.
operations_on() {
  mm_as bob --output json deploy status \
    | json "sum(len((n.get('status') or {}).get('operations') or []) for n in value['nodes'] if n['node'] == '$1')"
}
# rollout_of NODE: the state of the node's rollout and whether it holds the
# environment, such as `running true`, or `none False`.
rollout_of() {
  mm_as bob --output json deploy status \
    | json "(lambda r: '%s %s' % (r.get('state', 'none'), r.get('active', False)))(next(((n.get('status') or {}).get('rollout') or {} for n in value['nodes'] if n['node'] == '$1'), {}))"
}
# wait_for WHAT SECONDS COMMAND: until COMMAND succeeds, or fail naming WHAT.
wait_for() {
  local what=$1 seconds=$2 command=$3
  for _ in $(seq 1 "$seconds"); do eval "$command" && return 0; sleep 1; done
  fail "$what within $seconds s"
}
# in_step NODE: the node lists the stack step of a deployment as running.
in_step() { mm_as bob deploy status 2>/dev/null | grep -q "running:  deployment (stack 'app')"; }
# kill_client TEXT: kills the mm client process whose arguments hold TEXT.
kill_client() {
  docker exec "$CLIENT" python3 -c '
import os, signal, sys
for pid in filter(str.isdigit, os.listdir("/proc")):
    try:
        command = open(f"/proc/{pid}/cmdline", "rb").read().split(b"\0")
    except OSError:
        continue
    if os.path.basename(command[0]) == b"mm" and sys.argv[1].encode() in command:
        os.kill(int(pid), signal.SIGKILL)
        print("killed the client", pid)
' "$1" | grep -q "killed the client" || fail "the client of $1 was not there to be killed"
}
# environment_free: no node holds the environment for a deployment.
environment_free() {
  [ "$(mm_as bob --output json deploy status | json "all(not ((n.get('status') or {}).get('rollout') or {}).get('active', False) for n in value['nodes'])")" = True ]
}
# orphan_relays NODE: relay processes whose parent is gone.
orphan_relays() { relays "$1" | awk '$2 == 1'; }
# a_waiting_web TAG: a commit whose web waits until /tmp/go, keeping the
# deployment in its step on app-1. $1 of commit: the lines that follow
# `target:` of the environment, then the users block.
a_waiting_web() {
  rolling_compose "$1" "test -f /tmp/ok" "{ while [ ! -f /tmp/go ]; do sleep 1; done; touch /tmp/ok; } &" ", start_period: 300s" >"$TMP/repo/compose.yaml"
  SERVICE_LINES=$ROLLING_SERVICES
  commit "${2:-}$(users alice bob)" "web $1 waits to be let go"
  publish
}
# names_again: the names project once more, deployed with no manual step.
names_again() {
  wait_for "the environment to be free" 90 environment_free
  names_compose >"$TMP/repo/compose.yaml"
  commit "$(users alice bob)" "the names project after $1"
  publish
  deploy >"$TMP/after-$1.log" || { cat "$TMP/after-$1.log" >&2; fail "the deployment after $1 needed a manual step"; }
  for node in $NODES; do
    case "$(web_command "$node")" in *web-started*) ;; *) fail "$node does not run the names web after $1" ;; esac
  done
}

step "a service whose container exits right after it starts fails the deployment, without a health check, restarted or not"
# Compose starts the container and returns while `exit 1` has not run yet:
# the first look finds it running. It used to count as healthy then.
{
  printf 'services:\n'
  workload api
  printf '  web:\n    image: %s\n    command: ["sh", "-c", "exit 1"]\n    restart: "no"\n' "$WORKLOAD_IMAGE"
  workload db
  printf '    volumes: [dbdata:/data]\n'
  workload worker
  printf 'volumes:\n  dbdata: {}\n'
} >"$TMP/repo/compose.yaml"
SERVICE_LINES=$ROLLING_SERVICES
commit "$(users alice bob)" "a web that exits at once"
publish
if deploy >"$TMP/exits.log" 2>&1; then cat "$TMP/exits.log" >&2; fail "a web that exits at once was deployed"; fi
cat "$TMP/exits.log"
has "$TMP/exits.log" "service 'web' of stack 'app': its container exited with code 1 (MM_RUNTIME_EXITED)" "rolled back to what it ran before"
# Without `restart:` a cluster node restarts it: a restart loop, up for a
# moment between its exits. It fails the step too, before any name gate.
sed -i '/restart: "no"/d' "$TMP/repo/compose.yaml"
commit "$(users alice bob)" "a web that exits at once, restarted by the engine"
publish
started=$(date +%s)
if deploy >"$TMP/loops.log" 2>&1; then cat "$TMP/loops.log" >&2; fail "a web in a restart loop was deployed"; fi
cat "$TMP/loops.log"
# Docker shows the container `restarting` between its exits. Podman shows
# it exited, and a look that finds it so fails the step at once.
if [ "$ENGINE" = podman ]; then
  grep -Eq "service 'web' of stack 'app': its container (keeps exiting with code 1 and the engine restarts it \(MM_RUNTIME_RESTARTING\)|exited with code 1 \(MM_RUNTIME_EXITED\))" "$TMP/loops.log" \
    || fail "a restart loop on Podman did not fail with the exit code"
else
  has "$TMP/loops.log" "service 'web' of stack 'app': its container keeps exiting with code 1 and the engine restarts it (MM_RUNTIME_RESTARTING)"
fi
if grep -q 'not every node resolves them' "$TMP/loops.log"; then fail "a restart loop was taken for healthy containers"; fi
[ $(($(date +%s) - started)) -le 90 ] || fail "a restart loop took more than 90 s to fail the deployment"
names_again "a web that exits at once"

step "no lockout: a step in progress is listed with its ID, and mm deploy cancel stops it and rolls the node back"
WEB_BEFORE=$(web_command app-1)
a_waiting_web f1
CANCEL=$(mm_as alice --output json deploy plan | json 'value["plan_hash"]')
mm_as alice deploy apply --plan "$CANCEL" --yes >"$TMP/cancel-apply.log" 2>&1 &
CANCEL_PID=$!
wait_for "app-1 reaches the stack step" 90 in_step
has "$TMP/cancel-apply.log" "==> operation ${CANCEL:0:12}: deploying to 'production'"
mm_as bob deploy status | tee "$TMP/cancel-status.log"
has "$TMP/cancel-status.log" "running:  deployment (stack 'app') ${CANCEL:0:12} by alice since" "mm deploy cancel ${CANCEL:0:12}"
# Reads answer while the step runs, and the apply says every 15 s how it is.
mm_as bob service status >"$TMP/cancel-service.log" || { cat "$TMP/cancel-service.log" >&2; fail "service status failed during the step"; }
mm_as bob service logs api --tail 5 >/dev/null || fail "service logs failed during the step"
wait_for "a heartbeat line" 30 'grep -q "app-1: app, still running after" "$TMP/cancel-apply.log"'
mm_as bob deploy cancel "${CANCEL:0:12}" | tee "$TMP/cancel.log"
has "$TMP/cancel.log" "stopped deployment (stack 'app') ${CANCEL:0:12} by alice" "returned to what it ran before plan ${CANCEL:0:12}"
if wait "$CANCEL_PID"; then cat "$TMP/cancel-apply.log" >&2; fail "the cancelled deployment succeeded"; fi
cat "$TMP/cancel-apply.log"
has "$TMP/cancel-apply.log" "cancelled by bob"
[ "$(web_command app-1)" = "$WEB_BEFORE" ] || fail "app-1 does not run what it ran before: $(web_command app-1)"
on app-1 grep -q '"cancelled_by":"bob"' /var/lib/mm/audit.jsonl || fail "app-1's audit log does not name who cancelled"
on app-1 grep '"result":"cancelled"' /var/lib/mm/audit.jsonl | grep -q '"started_by":"alice"' || fail "app-1's audit log does not name who started it"
mm_as bob deploy status >"$TMP/cancel-after.log"
has "$TMP/cancel-after.log" "No rollout is in progress."
if grep -q 'running:' "$TMP/cancel-after.log"; then cat "$TMP/cancel-after.log" >&2; fail "something still runs after the cancellation"; fi
names_again cancel

step "no lockout: a client killed in the middle of a step: the node stops the step and the environment is free at once"
a_waiting_web f2
KILLED=$(mm_as alice --output json deploy plan | json 'value["plan_hash"]')
mm_as alice deploy apply --plan "$KILLED" --yes >"$TMP/killed-apply.log" 2>&1 &
wait_for "app-1 reaches the stack step" 90 in_step
kill_client "$KILLED"
wait_for "app-1 frees the environment" 30 '[ "$(rollout_of app-1)" = "running False" ]'
[ "$(operations_on app-1)" = 0 ] || fail "app-1 still runs the step of a client that is gone"
mm_as bob deploy status | tee "$TMP/killed-status.log"
has "$TMP/killed-status.log" "was left UNFINISHED" "the client that started it went away"
wait_for "the killed client's relays to leave" 15 '[ -z "$(orphan_relays app-1)" ]'
# The nodes that only held the lock for the client give it up when their
# lease runs out, a minute after the client was last heard from.
wait_for "the other nodes to free the environment" 90 environment_free
wait || true
names_again "a killed client"

step "no lockout: mmd killed with SIGKILL in the middle of a step and started again: the next deployment needs no manual step"
a_waiting_web f3
SIGKILLED=$(mm_as alice --output json deploy plan | json 'value["plan_hash"]')
mm_as alice deploy apply --plan "$SIGKILLED" --yes >"$TMP/sigkill-apply.log" 2>&1 &
SIGKILL_PID=$!
wait_for "app-1 reaches the stack step" 90 in_step
on app-1 pkill -KILL -x mmd
if wait "$SIGKILL_PID"; then fail "the deployment went through without app-1's mmd"; fi
cat "$TMP/sigkill-apply.log"
has "$TMP/sigkill-apply.log" "node 'app-1'" "ended the connection before it answered"
# What systemd does on a real node; the killed daemon left its socket.
on app-1 rm -f /run/mm/mm.sock
start_mmd app-1; wait_mmd app-1
mm_as bob deploy status | tee "$TMP/sigkill-status.log"
has "$TMP/sigkill-status.log" "was left UNFINISHED" "mmd on this node restarted at"
wait_for "the relays of the killed daemon's sessions to leave" 15 '[ -z "$(orphan_relays app-1)" ]'
names_again "a killed mmd"

step "no lockout: mmd stopped with SIGSTOP: reads name the node and answer for the others, and the apply ends with a timeout naming node and step"
a_waiting_web f4 '    deploy: {progress-timeout: 30s}
'
STOPPED=$(mm_as alice --output json deploy plan | json 'value["plan_hash"]')
mm_as alice deploy apply --plan "$STOPPED" --yes >"$TMP/sigstop-apply.log" 2>&1 &
SIGSTOP_PID=$!
wait_for "app-1 reaches the stack step" 90 in_step
on app-1 pkill -STOP -x mmd
read_started=$(date +%s)
mm_as bob service status >"$TMP/sigstop-service.log" || { cat "$TMP/sigstop-service.log" >&2; fail "service status failed with app-1's mmd stopped"; }
[ $(($(date +%s) - read_started)) -le 20 ] || fail "service status took longer than 20 s with app-1's mmd stopped"
has "$TMP/sigstop-service.log" "app-2" "app-3" "node app-1:"
apply_started=$(date +%s)
if wait "$SIGSTOP_PID"; then fail "the deployment went through with app-1's mmd stopped"; fi
cat "$TMP/sigstop-apply.log"
has "$TMP/sigstop-apply.log" "error[MM_TIMEOUT]" "node 'app-1' showed no progress on step 'app' of operation ${STOPPED:0:12}" "deploy.progress-timeout" "mm deploy status"
[ $(($(date +%s) - apply_started)) -le 180 ] || fail "the apply took longer than three minutes to give up on a stopped node"
on app-1 pkill -CONT -x mmd
wait_for "app-1 frees the environment once it runs again" 30 '[ "$(rollout_of app-1 | cut -d" " -f2)" = False ]'
# Nothing the stopped step began goes on once the environment is free: the
# old web it had begun to take out is routed to again, or its stop ended
# first. A leave left going removed it seconds later, under the next plan
# ("deployment plan changed" at apply).
pending=$(on app-1 sh -c 'ls /var/lib/mm/leaving 2>/dev/null')
[ -z "$pending" ] || fail "app-1 is free while a leave of the stopped step goes on: $pending"
names_again "a stopped mmd"

step "no lockout: the engine CLI hangs on one node: reads answer, the apply fails with a timeout naming the node, and nothing outlives its deadline"
rolling_compose f5 "test -f /tmp/ok" >"$TMP/repo/compose.yaml"
SERVICE_LINES=$ROLLING_SERVICES
commit "$(users alice bob)" "web f5"
publish
HUNG=$(mm_as alice --output json deploy plan | json 'value["plan_hash"]')
# From here every engine command (`docker` or `podman`) app-2's mmd starts
# never ends. Status and logs read the engine's socket and do not start it.
# Other callers (on a Podman node, the loop that runs health checks) get
# the real program.
ENGINE_PATH=$(on app-2 sh -c "command -v $ENGINE")
printf '%s\n' '#!/bin/sh' '[ "$(cat /proc/$PPID/comm)" = mmd ] && exec sleep 100000' "exec $ENGINE_PATH.real \"\$@\"" \
  | docker exec -i "$(container app-2)" sh -c 'mv "$1" "$1.real" && cat >"$1" && chmod 755 "$1"' sh "$ENGINE_PATH"
mm_as alice deploy apply --plan "$HUNG" --yes >"$TMP/hang-apply.log" 2>&1 &
HANG_PID=$!
sleep 20
read_started=$(date +%s)
mm_as bob service status >"$TMP/hang-service.log" || { cat "$TMP/hang-service.log" >&2; fail "service status failed with app-2's engine CLI hanging"; }
[ $(($(date +%s) - read_started)) -le 10 ] || fail "service status took longer than 10 s with app-2's engine CLI hanging"
has "$TMP/hang-service.log" "app-1" "app-2" "app-3"
if wait "$HANG_PID"; then fail "the deployment went through with app-2's engine CLI hanging"; fi
cat "$TMP/hang-apply.log"
has "$TMP/hang-apply.log" "error[MM_TIMEOUT]" "failed on node 'app-2'" "Not changed: app-2, app-3"
grep -Eq 'did not answer in time|timed out after' "$TMP/hang-apply.log" || fail "the hanging engine is not reported as a timeout"
if grep -q 'direct_environment_delivery' "$TMP/hang-apply.log"; then fail "a hanging engine is reported as a missing capability"; fi
# Every engine command app-2's mmd started was ended at its deadline.
LATE=$(on app-2 sh -c "ps -eo etimes=,args= | awk '\$2 == \"sleep\" && \$3 == \"100000\" && \$1 > 75'")
[ -z "$LATE" ] || fail "processes outlived their deadline on app-2: $LATE"
on app-2 mv "$ENGINE_PATH.real" "$ENGINE_PATH"
names_again "a hanging engine"

step "no lockout: a dropped connection: the node stops the step of a client it no longer hears from, its relay ends, and the environment is free"
a_waiting_web f6
DROPPED=$(mm_as alice --output json deploy plan | json 'value["plan_hash"]')
mm_as alice deploy apply --plan "$DROPPED" --yes >"$TMP/dropped-apply.log" 2>&1 &
DROPPED_PID=$!
wait_for "app-1 reaches the stack step" 90 in_step
docker network disconnect "$NETWORK" "$CLIENT"
# Nothing tells the node: its sshd probes the session (ClientAliveInterval
# 15, in practice every 30 s with Debian 12's OpenSSH) and gives up only
# after about three minutes. The client's word every 15 s stops coming, and
# mmd stops a step whose client it has not heard from for two minutes; the
# relay ends with the step's answer. Debian 13's sshd (the Podman nodes)
# ended the session first in the local runs: the step stops then, as for a
# client that went away.
wait_for "app-1's relays to end with the step" 150 '[ "$(relays app-1 | wc -l)" = 0 ]'
on app-1 grep -Eq "the client of a rollout step was not heard from for 120 s|deployment \(stack 'app'\) ${DROPPED:0:12} was stopped because the client that started it went away" /tmp/mmd.log \
  || fail "app-1 did not stop the step of a client it no longer heard from"
docker network connect "$NETWORK" "$CLIENT"
wait_for "the deployment to give up" 120 '! kill -0 "$DROPPED_PID" 2>/dev/null'
if wait "$DROPPED_PID"; then fail "the deployment went through without its connection"; fi
wait_for "app-1 frees the environment" 30 '[ "$(rollout_of app-1 | cut -d" " -f2)" = False ]'
[ "$(operations_on app-1)" = 0 ] || fail "app-1 still runs the step of a connection that dropped"
names_again "a dropped connection"

step "a move: worker leaves the group for app-1, and starts there before it stops"
SERVICE_LINES='      api: {stack: app}
      web: {stack: app, placement: all, drain: 2s}
      db: {stack: app, placement: app-3}
      worker: {stack: app, placement: app-1}'
commit "$(users alice bob)" "move worker to app-1"
publish
mm_as alice deploy plan | tee "$TMP/move-plan.log"
has "$TMP/move-plan.log" "moves:" "app/worker: starts on app-1, then stops on app-2" \
  "app/worker: starts on app-1, then stops on app-3" "starts here: app/worker" \
  "stops here: app/worker, after it runs on app-1"
if mm_as alice deploy plan --nodes app-2,app-3 >"$TMP/move-subset.log" 2>&1; then
  fail "a plan that leaves out the node a service moves to was accepted"
fi
has "$TMP/move-subset.log" MM_PLACEMENT "moves from app-2 to app-1" "Add app-1 to --nodes"
[ "$(services_on app-2)" = "web worker" ] || fail "the refused plan changed app-2"
MOVE_FROM=$(date -u +%Y-%m-%dT%H:%M:%SZ)
deploy | tee "$TMP/move-apply.log"
has "$TMP/move-apply.log" "==> app-2: stopping worker (now on app-1)" "==> app-3: stopping worker (now on app-1)" \
  "then stopped, now that they run elsewhere: app/worker"
[ "$(services_on app-1)" = "api web worker" ] || fail "app-1 runs: $(services_on app-1)"
[ "$(services_on app-2)" = "web" ] || fail "app-2 runs: $(services_on app-2)"
[ "$(services_on app-3)" = "db web" ] || fail "app-3 runs: $(services_on app-3)"
# shellcheck disable=SC2016 # the inner command expands on the node
STARTED=$(on app-1 sh -c 'engine inspect --format "{{json .State.StartedAt}}" "$(engine ps -q --filter label=com.docker.compose.service=worker)"' | tr -d '"')
sleep 2 # `--until` counts whole seconds: let the last stop fall before it
# The events so far: Podman lists them with --stream=false, and none with
# --until.
until_now() {
  if [ "$ENGINE" = podman ]; then echo --stream=false; else echo "--until=$(date -u +%Y-%m-%dT%H:%M:%SZ)"; fi
}
for node in app-2 app-3; do
  STOPPED=$(on "$node" engine events --since "$MOVE_FROM" "$(until_now)" \
    --filter event=die --filter label=com.docker.compose.service=worker --format '{{.TimeNano}}' | head -n 1)
  [ -n "$STOPPED" ] || fail "$node: no record of worker stopping"
  python3 - "$STARTED" "$STOPPED" "$node" <<'PY' || fail "worker stopped on the old node before it started on the new one"
import datetime, sys
started, stopped, node = sys.argv[1], int(sys.argv[2]), sys.argv[3]
head, _, fraction = started.rstrip("Z").partition(".")
start = datetime.datetime.fromisoformat(head).replace(tzinfo=datetime.timezone.utc).timestamp() + float("0." + (fraction or "0"))
stop = stopped / 1e9
print(f"  worker started on app-1 {stop - start:.1f}s before it stopped on {node}")
sys.exit(0 if start < stop else 1)
PY
done

step "a node's last service leaves: its project goes, and it runs nothing"
SERVICE_LINES='      api: {stack: app}
      web: {stack: app}
      db: {stack: app}
      worker: {stack: app}'
CLUSTER_LINES=''
commit "$(users alice bob)" "everything on the first node"
publish
mm_as alice deploy plan | tee "$TMP/empty-plan.log"
has "$TMP/empty-plan.log" "app/db: starts on app-1, then stops on app-3" \
  "app/web: already runs on app-1; stops on app-2 after that node has applied" "services: none"
deploy | tee "$TMP/empty-apply.log"
[ "$(services_on app-1)" = "api db web worker" ] || fail "app-1 runs: $(services_on app-1)"
for node in app-2 app-3; do
  [ -z "$(on "$node" engine ps -aq)" ] || fail "$node should have no container left"
  [ -z "$(on "$node" engine network ls -q --filter label=com.docker.compose.project)" ] || fail "$node should have no project network left"
done
on app-3 engine volume ls --format '{{.Name}}' | grep -q dbdata || fail "a volume is never removed with its service"
mm_as alice target nodes | tee "$TMP/nodes-empty.log"
[ "$(grep -c 'runs nothing' "$TMP/nodes-empty.log")" = 2 ] || fail "two nodes should record that they run nothing"
mm_as alice deploy plan | tee "$TMP/settled-plan.log"
[ "$(grep -c 'runs nothing: no service is placed on this node' "$TMP/settled-plan.log")" = 2 ] \
  || fail "the emptied nodes should plan nothing"

step "networks no service uses any more are gone from every node, with their rules and bridges"
# Since the move, no service is on backend, frontend, or sealed.
NET_PREFIX="$(printf '%s' "$PROJECT_ID" | tr -d - | cut -c1-12)-production-"
for node in $NODES; do
  on "$node" /usr/libexec/mmd network show >"$TMP/retired-$node.json"
  for network in backend frontend sealed; do
    [ "$(json '((value.get("stored") or {}).get("table") or {}).get("networks", {}).get("'"$NET_PREFIX$network"'")' <"$TMP/retired-$node.json")" = None ] \
      || fail "$node still holds the addresses of $network"
  done
  if on "$node" iptables -t "$FORWARD_TABLE" -S MM-CLUSTER | grep -E -q -- '-production-(backend|frontend|sealed)"'; then
    fail "$node keeps MM-CLUSTER rules of a network no service uses"
  fi
  if on "$node" engine network ls --format '{{.Name}}' | grep -E -q -- '-production-(backend|frontend|sealed)$'; then
    fail "$node keeps the bridge of a network no service uses"
  fi
done
printf '  ok   backend, frontend and sealed left every node with the deployments that stopped using them\n'

# --- shared volumes -----------------------------------------------------------
# No kernel NFS server runs in these containers, so nothing here mounts a
# volume over the mesh: the storage node's own use of the directory, the
# plan, the step that makes the volumes ready, the rules, the status, and
# the delete are covered; a mount from another node is checked on real hosts.
STORAGE_DIR=/var/lib/mm/storage/$PROJECT_ID/production/app/uploads
# On Podman each service has user ids of its own, the same on every node:
# web writes the shared volume with them.
# A stand-in for the storage server: something that listens on the port.
storage_listens() {
  on app-1 engine rm -f storage-stand-in >/dev/null 2>&1 || true
  [ "$1" = yes ] || return 0
  on app-1 engine run -d --name storage-stand-in --network host "$WORKLOAD_IMAGE" httpd -f -p 10.90.0.1:2049 >/dev/null
  sleep 1
}
# port_open NODE [docker run arguments]: whether 10.90.0.1:2049 answers the
# node itself, or a container started there with those arguments.
port_open() {
  local node=$1; shift
  if [ $# -eq 0 ]; then
    on "$node" bash -c 'exec 3<>/dev/tcp/10.90.0.1/2049' 2>/dev/null
  else
    on "$node" engine run --rm "$@" "$WORKLOAD_IMAGE" nc -w 2 -z 10.90.0.1 2049 >/dev/null 2>&1
  fi
}

step "shared volumes: no container reaches the storage port, on its node or through the mesh"
storage_listens yes
port_open app-2 || fail "app-2 itself does not reach the storage port over the mesh"
# The chains that guard the mesh already keep a container on a host's own
# network away from every port of another node; these keep it away from the
# storage port by themselves, also on the storage node.
for node in $NODES; do on "$node" /usr/libexec/mmd storage up; done | tee "$TMP/storage-up.log"
[ "$(grep -c 'storage: loaded the firewall chains MM-STORAGE and MM-STORAGE-IN' "$TMP/storage-up.log")" = 3 ] || fail "every node should load the rules once"
for node in $NODES; do
  [ -z "$(on "$node" /usr/libexec/mmd storage up)" ] || fail "$node: a second \`mmd storage up\` changed something"
done
on app-1 iptables -S MM-STORAGE-IN | tee "$TMP/storage-rules.log"
has "$TMP/storage-rules.log" "-s 10.90.0.2/32 -d 10.90.0.1/32 -i mm0 -p tcp -m tcp --dport 2049" \
  "-s 10.90.0.3/32 -d 10.90.0.1/32 -i mm0 -p tcp -m tcp --dport 2049" "-d 10.90.0.1/32 -p tcp -m tcp --dport 2049"
port_open app-2 || fail "the rules cut app-2 itself off from the storage port"
port_open app-3 || fail "the rules cut app-3 itself off from the storage port"
for node in app-2 app-3; do
  if port_open "$node" --network bridge; then fail "a container on $node's own bridge reaches the storage port through the mesh"; fi
  printf '  ok   a container on %s cannot reach 10.90.0.1:2049; %s itself can\n' "$node" "$node"
done
if port_open app-1 --network bridge; then fail "a container on the storage node reaches the storage port"; fi
printf '  ok   a container on app-1 cannot reach 10.90.0.1:2049\n'
on app-2 iptables -t "$FORWARD_TABLE" -D "$FORWARD_FROM" -j MM-STORAGE
on app-2 /usr/libexec/mmd storage up | tee "$TMP/storage-repair.log"
has "$TMP/storage-repair.log" "$FORWARD_FROM does not enter MM-STORAGE"
if port_open app-2 --network bridge; then fail "the repaired rules let a container through"; fi

step "shared volumes: validation refuses a database on one and a volume no file declares"
{
  printf 'services:\n'
  workload api
  workload web
  printf '    volumes: ["uploads:/data", "scratch:/scratch"]\n'
  workload db
  printf '    volumes: [dbdata:/data]\n'
  workload worker
  printf 'volumes:\n  dbdata: {}\n  uploads: {}\n  scratch: {}\n'
} >"$TMP/repo/compose.yaml"
SERVICE_LINES="      api: {stack: app}
      web: {stack: app, placement: all, drain: 2s}
      db: {stack: app, role: database}
      worker: {stack: app}"
STACK_LINES='    shared-volumes: [uploads, dbdata, media]'
commit "$(users alice bob)" "share what cannot be shared"
publish
if mm_as alice config validate >"$TMP/shared-refused.log" 2>&1; then fail "a database on a shared volume validates"; fi
has "$TMP/shared-refused.log" MM_SHARED_VOLUME "service 'db' has role database and mounts shared volume 'dbdata'" \
  "slower than local disk" "allow-shared-volumes: true" "lists 'media'"
if mm_as alice deploy plan >"$TMP/shared-refused-plan.log" 2>&1; then fail "a database on a shared volume was planned"; fi
has "$TMP/shared-refused-plan.log" MM_SHARED_VOLUME

step "shared volumes: the plan names the node that keeps each volume and the nodes that mount it"
STACK_LINES='    shared-volumes: [uploads]'
SERVICE_LINES="      api: {stack: app}
      web: {stack: app, placement: all, drain: 2s}
      db: {stack: app, role: database}
      worker: {stack: app}"
if [ "$ENGINE" = podman ]; then
  # Two services with ids of their own cannot share one volume.
  cp "$TMP/repo/compose.yaml" "$TMP/compose-one-writer.yaml"
  sed 's|^  worker:$|  worker:\n    volumes: ["uploads:/data"]|' "$TMP/compose-one-writer.yaml" >"$TMP/repo/compose.yaml"
  grep -q 'uploads:/data' "$TMP/repo/compose.yaml" || fail "the two-writer Compose file was not written"
  commit "$(users alice bob)" "web and worker both write uploads"
  publish
  if mm_as alice config validate >"$TMP/shared-userns.log" 2>&1; then fail "two services with ids of their own sharing one volume validate"; fi
  has "$TMP/shared-userns.log" MM_USER_NAMESPACE "services 'web', 'worker' of stack 'app' all mount volume 'uploads'" \
    "Use the volume in one service" "modules.api.services.web and modules.api.services.worker"
  cp "$TMP/compose-one-writer.yaml" "$TMP/repo/compose.yaml"
fi
commit "$(users alice bob)" "share uploads between the nodes"
publish
mm_as alice deploy plan | tee "$TMP/shared-plan.log"
has "$TMP/shared-plan.log" "shared volumes: app-1 keeps the files" \
  "app/uploads: kept on app-1 in $STORAGE_DIR, used there directly, mounted over the mesh on app-2, app-3 (for web)" \
  "shared volume app/uploads: this node keeps it" "shared volume app/uploads: mounted from app-1 over the mesh for web"
if grep -q 'dbdata\|scratch' "$TMP/shared-plan.log"; then fail "the plan names a volume that is not shared"; fi
mm_as alice --output json deploy plan >"$TMP/shared-plan.json"
[ "$(json '(value["storage"]["node"], value["storage"]["volumes"][0]["mounted_on"])' <"$TMP/shared-plan.json")" = "('app-1', ['app-2', 'app-3'])" ] \
  || fail "the JSON plan should name the storage node and the mounting nodes"
if mm_as alice deploy plan --nodes app-2,app-3 >"$TMP/shared-subset.log" 2>&1; then fail "a plan that leaves out the storage node was accepted"; fi
has "$TMP/shared-subset.log" "leaves out 'app-1', the node that keeps shared volumes" "Add app-1 to \`--nodes\`"

step "shared volumes: a deployment is refused before it changes anything when storage is not there"
# app-1 was never made a storage node.
if deploy >"$TMP/shared-not-served.log" 2>&1; then fail "a node that does not serve storage created a volume"; fi
has "$TMP/shared-not-served.log" MM_STORAGE_NOT_SERVED "mm target setup plan"
on app-1 test ! -e "$STORAGE_DIR" || fail "the refused deployment created the directory"
# What `mm target setup` leaves on the storage node of a real cluster.
on app-1 sh -c 'mkdir -p /var/lib/mm/storage /etc/exports.d && chmod 755 /var/lib/mm/storage && touch /etc/exports.d/mm-storage.exports'
# The storage node is there, and app-2 and app-3 do not reach its port.
storage_listens no
if deploy >"$TMP/shared-unreachable.log" 2>&1; then fail "a node that cannot reach storage was deployed to"; fi
has "$TMP/shared-unreachable.log" MM_STORAGE_UNREACHABLE "mounts shared volumes from 'app-1' and cannot reach it" "nothing was changed"
on app-1 test ! -e "$STORAGE_DIR" || fail "the refused deployment created the directory"
[ "$(services_on app-2)" = "" ] || fail "the refused deployment changed app-2"
mm_as alice deploy status | tee "$TMP/shared-refused-status.log"
has "$TMP/shared-refused-status.log" "No rollout is in progress."
storage_listens yes

step "shared volumes: on the storage node the volume is the directory itself"
SERVICE_LINES="      api: {stack: app}
      web: {stack: app}
      db: {stack: app, role: database}
      worker: {stack: app}"
commit "$(users alice bob)" "web on the storage node"
publish
deploy | tee "$TMP/shared-apply.log"
has "$TMP/shared-apply.log" "==> app-1: created $PROJECT_ID/production/app/uploads"
# Docker Engine's containers write as the host's root. Podman gives the
# directory to the first id of web's own, which every node gives it.
OWNER=0
[ "$ENGINE" = podman ] && OWNER=$(in_service app-1 web "cat /proc/self/uid_map" | awk '$1 == 0 {print $2}')
[ "$(on app-1 stat -c '%a %u' "$STORAGE_DIR")" = "755 $OWNER" ] || fail "the volume's directory is $(on app-1 stat -c '%a %u' "$STORAGE_DIR"), web's root is $OWNER"
# shellcheck disable=SC2016 # the inner command expands on the node
on app-1 sh -c 'engine exec "$(engine ps -q --filter label=com.docker.compose.service=web)" sh -c "echo written-in-the-container > /data/a.txt"'
[ "$(on app-1 cat "$STORAGE_DIR/a.txt")" = "written-in-the-container" ] || fail "what the container wrote is not in the storage node's directory"
on app-1 engine volume ls --format '{{.Name}}' | grep -q '^mm-shared-.*-production-app-uploads-' || fail "the engine volume is not named after the shared volume"
# shellcheck disable=SC2016 # the inner command expands on the node
[ "$(on app-1 sh -c 'engine inspect --format "{{index .Config.Labels \"io.mm.shared-storage\"}}" "$(engine ps -q --filter label=com.docker.compose.service=web)"')" = local ] \
  || fail "web does not say where its storage is"
deploy | tee "$TMP/shared-apply-again.log"
if grep -q 'created ' "$TMP/shared-apply-again.log"; then fail "a second deployment created the directory again"; fi
[ "$(on app-1 cat "$STORAGE_DIR/a.txt")" = "written-in-the-container" ] || fail "a second deployment changed the volume's files"

step "mm target storage status: the storage node, each volume, and who reaches it"
mm_as alice target storage status | tee "$TMP/storage-status.log"
has "$TMP/storage-status.log" "app-1 keeps shared volumes in /var/lib/mm/storage" "app/uploads" "in 1 file; mounted on app-1" \
  "keeps the volumes and serves them on 10.90.0.1" "reaches app-1 in" "Every node reaches the storage node."
mm_as alice --output json target storage status >"$TMP/storage-status.json"
[ "$(json '(value["storage_node"], value["volumes"][0]["volume"], value["volumes"][0]["files"], value["volumes"][0]["mounted_on"], len(value["problems"]))' <"$TMP/storage-status.json")" = "('app-1', 'uploads', 1, ['app-1'], 0)" ] \
  || fail "the JSON status should list uploads with one file, mounted on app-1"
storage_listens no
if mm_as alice target storage status >"$TMP/storage-status-down.log" 2>&1; then fail "the status succeeds while no node reaches the storage node"; fi
has "$TMP/storage-status-down.log" MM_STORAGE_UNREACHABLE "node 'app-2' cannot reach shared storage on app-1" "mm target mesh status"
storage_listens yes

step "mm target storage delete: only an admin, only with the reviewed hash, and not while mounted"
if mm_as ops target storage delete uploads >"$TMP/storage-delete-in-use.log" 2>&1; then fail "a mounted volume can be deleted"; fi
has "$TMP/storage-delete-in-use.log" MM_STORAGE_IN_USE "is mounted on app-1" "nothing was deleted"

step "a volume that leaves shared-volumes keeps its files"
STACK_LINES=''
commit "$(users alice bob)" "uploads is an ordinary volume again"
publish
deploy >/dev/null
[ "$(on app-1 cat "$STORAGE_DIR/a.txt")" = "written-in-the-container" ] || fail "taking the volume out of shared-volumes removed its files"
# shellcheck disable=SC2016 # the inner command expands on the node
if on app-1 sh -c 'engine exec "$(engine ps -q --filter label=com.docker.compose.service=web)" test -e /data/a.txt'; then
  fail "web still sees the shared directory"
fi
mm_as alice target storage status | tee "$TMP/storage-status-left.log"
has "$TMP/storage-status-left.log" "not in the configuration any more; its files are kept" "mm target storage delete uploads" "mounted nowhere"
if mm_as alice target storage delete uploads >"$TMP/storage-delete-alice.log" 2>&1; then fail "a user without admin access can ask to delete a shared volume"; fi
mm_as ops target storage delete uploads | tee "$TMP/storage-delete-plan.log"
has "$TMP/storage-delete-plan.log" "Delete shared volume app/uploads" "$STORAGE_DIR with everything in it" "This cannot be undone" "--plan"
DELETE=$(mm_as ops --output json target storage delete uploads | json 'value["plan_hash"]')
on app-1 test -f "$STORAGE_DIR/a.txt" || fail "printing the plan removed data"
if mm_as ops target storage delete uploads --plan "$(printf '%064d' 0)" --yes >"$TMP/storage-delete-wrong.log" 2>&1; then fail "another hash deletes the volume"; fi
on app-1 test -f "$STORAGE_DIR/a.txt" || fail "a wrong hash removed data"
mm_as ops target storage delete uploads --plan "$DELETE" --yes | tee "$TMP/storage-deleted.log"
has "$TMP/storage-deleted.log" "Deleted shared volume app/uploads"
on app-1 test ! -e "$STORAGE_DIR" || fail "the directory is still there"
on app-1 test -d /var/lib/mm/storage || fail "the storage root went with its last volume"
on app-1 grep -q shared-volume-deleted /var/lib/mm/audit.jsonl || fail "the delete left no audit record"
storage_listens no
SERVICE_LINES='      api: {stack: app}
      web: {stack: app}
      db: {stack: app}
      worker: {stack: app}'

# --- hardening and runtimes ---------------------------------------------------
# No stage from here on pings between containers: on Podman the workloads
# drop the NET_RAW they took for it, so a hardened service keeps nothing.
WORKLOAD_CAPS=''
# Runs under gVisor and a shared volume mounted from another node are
# checked on real hosts: no NFS server and no second runtime are in these
# containers. The engine's own runtime stands in for an offered one.
# setting NODE SERVICE FORMAT: a field of the service's container.
setting() {
  # shellcheck disable=SC2016 # the inner command expands on the node
  on "$1" sh -c 'engine inspect --format "$2" "$(engine ps -q --filter "label=com.docker.compose.service=$1")"' sh "$2" "$3"
}
# confined NODE SERVICE: what the kernel says the service's processes run
# with, the same on both engines: effective capabilities, no new
# privileges, whether the root and /tmp are what hardening makes them.
confined() {
  in_service "$1" "$2" 'grep -E "^(CapEff|NoNewPrivs):" /proc/self/status | tr -s "\t" " " | tr "\n" " "; if touch /written-by-check 2>/dev/null; then printf "root writable "; else printf "root read-only "; fi; grep -c " /tmp tmpfs " /proc/mounts'
}
# The runtime each engine runs a container under when nothing names one,
# and one it does not offer.
OWN_RUNTIME=runc OTHER_RUNTIME=crun RUNTIME_FIELD='{{.HostConfig.Runtime}}' RUNTIME_HINT='runsc install'
if [ "$ENGINE" = podman ]; then
  OWN_RUNTIME=crun OTHER_RUNTIME=runc RUNTIME_FIELD='{{.OCIRuntime}}' RUNTIME_HINT='[engine.runtimes]'
fi
hardened_compose() {
  printf 'services:\n'
  workload api
  printf '%s' "${1:-}"
  # web is healthy once it wrote to the tmpfs, keeps a shared volume, and
  # reads a file secret.
  printf '  web:\n    image: %s\n    command: ["sh", "-c", "touch /tmp/ok && echo web-started && exec sleep infinity"]\n    stop_grace_period: 1s\n' "$WORKLOAD_IMAGE"
  printf '    healthcheck: {test: ["CMD", "test", "-f", "/tmp/ok"], interval: 2s, timeout: 2s, retries: 1}\n'
  printf '    volumes: ["uploads:/data"]\n    secrets: [token]\n%s' "${2:-}"
  # db writes next to its binaries and changes a file's owner.
  printf '  db:\n    image: %s\n    command: ["sh", "-c", "touch /ready && chown 1000 /ready && echo db-started && exec sleep infinity"]\n    stop_grace_period: 1s\n    cap_add: [CHOWN]\n' "$WORKLOAD_IMAGE"
  # worker reads the file secret too: on Podman it has user ids of its own.
  workload worker
  printf '    secrets: [token]\n'
  printf 'volumes:\n  uploads: {}\nsecrets:\n  token: {file: ./api/token.txt}\n'
}
HARDENED_SERVICES="      api: {stack: app}
      web: {stack: app, networks: [backend]}
      db: {stack: app, hardening: {except: {read-only: \"writes a marker next to its binaries\"}}}
      worker: {stack: app, placement: all, drain: 2s, networks: [backend], config-files: {/etc/greeting.conf: {template: greeting.conf.tmpl, mode: 0o600}}}"

step "hardening: an exception without a reason is refused"
printf 'not-a-secret\n' >"$TMP/repo/api/token.txt"
hardened_compose >"$TMP/repo/compose.yaml"
TOP_LINES='hardening: strict'
STACK_LINES='    shared-volumes: [uploads]'
SERVICE_LINES=$(printf '%s' "$HARDENED_SERVICES" | sed 's/"writes a marker next to its binaries"/""/')
commit "$(users alice bob)" "an exception without a reason"
publish
if mm_as alice config validate >"$TMP/hardening-reason.log" 2>&1; then fail "an exception without a reason validates"; fi
has "$TMP/hardening-reason.log" MM_HARDENING_REASON "modules.api.services.db.hardening.except.read-only gives no reason"
if mm_as alice deploy plan >"$TMP/hardening-reason-plan.log" 2>&1; then fail "an exception without a reason was planned"; fi

step "hardening: strict gives every service the set, and the plan shows it with each exception's reason"
SERVICE_LINES=$HARDENED_SERVICES
commit "$(users alice bob)" "harden every service"
publish
mm_as alice deploy plan | tee "$TMP/hardening-plan.log"
has "$TMP/hardening-plan.log" \
  "security: web: read-only root, no capabilities, no new privileges, default seccomp, tmpfs /tmp" \
  "security: db: writable root, capabilities CHOWN only, no new privileges, default seccomp, tmpfs /tmp" \
  "security: db: not read-only (the root filesystem stays writable): writes a marker next to its binaries" \
  "security: worker: read-only root, no capabilities"
deploy >"$TMP/hardening-apply.log" 2>&1 || { cat "$TMP/hardening-apply.log"; fail "the hardened deployment failed"; }
for node in $NODES; do
  [ "$(confined "$node" worker)" = "CapEff: 0000000000000000 NoNewPrivs: 1 root read-only 1" ] \
    || fail "worker on $node runs with $(confined "$node" worker)"
done
[ "$(confined app-1 db)" = "CapEff: 0000000000000001 NoNewPrivs: 1 root writable 1" ] \
  || fail "db runs with $(confined app-1 db)"
# What Docker Engine records, in its own words.
if [ "$ENGINE" = docker ]; then
  [ "$(setting app-1 worker '{{.HostConfig.ReadonlyRootfs}} {{.HostConfig.CapDrop}} {{.HostConfig.SecurityOpt}} {{.HostConfig.Tmpfs}}')" = "true [ALL] [no-new-privileges:true] map[/tmp:]" ] \
    || fail "worker runs with $(setting app-1 worker '{{.HostConfig.ReadonlyRootfs}} {{.HostConfig.CapDrop}} {{.HostConfig.SecurityOpt}} {{.HostConfig.Tmpfs}}')"
  [ "$(setting app-1 db '{{.HostConfig.ReadonlyRootfs}} {{.HostConfig.CapDrop}} {{.HostConfig.CapAdd}}')" = "false [ALL] [CAP_CHOWN]" ] \
    || fail "db runs with $(setting app-1 db '{{.HostConfig.ReadonlyRootfs}} {{.HostConfig.CapDrop}} {{.HostConfig.CapAdd}}')"
fi
in_service app-1 db "test -e /ready" || fail "db could not write to its root filesystem or change an owner"
if in_service app-1 web "touch /written" 2>/dev/null; then fail "web wrote to its root filesystem"; fi
if in_service app-1 web "chown 1000 /tmp/ok" 2>/dev/null; then fail "web kept a capability"; fi
printf '  ok   the root filesystem is read-only and no capability is left; db keeps what it asked for\n'

step "hardening: health checks, service names, a shared volume, and a file secret work under it"
waited=0
until [ "$(setting app-1 web '{{.State.Health.Status}}')" = healthy ]; do
  [ "$waited" -lt 30 ] || fail "web is $(setting app-1 web '{{.State.Health.Status}}') after 30s under hardening"
  sleep 1; waited=$((waited + 1))
done
printf '  ok   web wrote to its tmpfs and its health check passes\n'
in_service app-1 web "cat /etc/resolv.conf" | grep -q '^nameserver 10\.64\.' || fail "web does not ask the node's resolver"
name_becomes app-2 worker web "$(address_on app-1 web backend)" 30
name_becomes app-1 web tasks.worker "$(sorted "$(address_on app-1 worker backend)" "$(address_on app-2 worker backend)" "$(address_on app-3 worker backend)")" 30
in_service app-1 web "echo written-under-hardening > /data/h.txt" || fail "web cannot write to its shared volume"
[ "$(on app-1 cat "$STORAGE_DIR/h.txt")" = "written-under-hardening" ] || fail "what web wrote is not in the storage node's directory"
[ "$(in_service app-1 web "cat /run/secrets/token")" = "not-a-secret" ] || fail "web cannot read its file secret"
printf '  ok   web writes its shared volume and reads its file secret\n'
for node in $NODES; do
  [ "$(in_service "$node" worker "cat /etc/greeting.conf")" = "greeting hola" ] || fail "$node: worker does not see its config file under a read-only root"
  [ "$(in_service "$node" worker "cat /run/secrets/token")" = "not-a-secret" ] || fail "$node: worker cannot read its file secret"
done
printf '  ok   worker reads its config file and its file secret on every node under a read-only root\n'
if [ "$ENGINE" = podman ]; then
  # Each service has 65536 user ids of its own from mm's range, the same
  # on every node: its config file (0600) belongs to its root. The file
  # secret from the checkout is the host's root's, 0644: a stranger's that
  # every user may read.
  WORKER_MAP=$(in_service app-1 worker "cat /proc/self/uid_map" | tr -s ' ')
  WEB_MAP=$(in_service app-1 web "cat /proc/self/uid_map" | tr -s ' ')
  for map in "$WORKER_MAP" "$WEB_MAP"; do
    printf '%s\n' "$map" | awk '$1 == 0 && $2 >= 3221225472 && $3 == 65536 {found = 1} END {exit !found}' \
      || fail "a service does not have user ids of its own from mm's range: $map"
  done
  [ "$WORKER_MAP" != "$WEB_MAP" ] || fail "worker and web share user ids: $WEB_MAP"
  for node in $NODES; do
    [ "$(in_service "$node" worker "cat /proc/self/uid_map" | tr -s ' ')" = "$WORKER_MAP" ] \
      || fail "$node: worker has other ids than on app-1: $(in_service "$node" worker "cat /proc/self/uid_map")"
    [ "$(in_service "$node" worker "stat -c '%u %a' /etc/greeting.conf")" = "0 600" ] \
      || fail "$node: worker sees its config file as $(in_service "$node" worker "stat -c '%u %a' /etc/greeting.conf")"
  done
  printf '  ok   worker has the same ids of its own on every node, web others, and the config file is worker'"'"'s\n'
fi

step "runtimes: a plan keeps to what the host's policy offers"
hardened_compose "    runtime: $OTHER_RUNTIME
" >"$TMP/repo/compose.yaml"
commit "$(users alice bob)" "api under a runtime no node offers"
publish
if mm_as alice deploy plan >"$TMP/runtime-refused.log" 2>&1; then fail "a runtime no node offers was planned"; fi
has "$TMP/runtime-refused.log" "service 'app/api' sets \`runtime: $OTHER_RUNTIME\`, which this host does not offer" \
  "It offers no runtime besides its engine's default" "\`runtimes:\` in /etc/mm/host.yaml"
for node in $NODES; do
  # shellcheck disable=SC2016 # the inner command expands on the node
  on "$node" sh -c 'printf "runtimes: [%s]\nexposed-runtime: %s\n" "$1" "$1" >>/etc/mm/host.yaml && /usr/libexec/mmd access validate >/dev/null' sh "$OWN_RUNTIME"
done
if mm_as alice deploy plan >"$TMP/runtime-refused.log" 2>&1; then fail "a runtime the nodes do not offer was planned"; fi
has "$TMP/runtime-refused.log" "which this host does not offer. It offers $OWN_RUNTIME."
# api publishes a port, so the host puts it under its runtime; web names it.
hardened_compose '    ports: ["127.0.0.1:18080:8080"]
' "    runtime: $OWN_RUNTIME
" >"$TMP/repo/compose.yaml"
commit "$(users alice bob)" "api publishes a port, web names a runtime"
publish
mm_as alice deploy plan | tee "$TMP/runtime-plan.log"
has "$TMP/runtime-plan.log" \
  "security: api: read-only root, no capabilities, no new privileges, default seccomp, tmpfs /tmp, runtime $OWN_RUNTIME (this host runs what publishes a port or takes ingress traffic under it)" \
  "security: web: read-only root, no capabilities, no new privileges, default seccomp, tmpfs /tmp, runtime $OWN_RUNTIME"
if grep -q 'security: worker: .*runtime' "$TMP/runtime-plan.log"; then fail "worker, which publishes nothing, was put under a runtime"; fi
deploy >"$TMP/runtime-apply.log" 2>&1 || { cat "$TMP/runtime-apply.log"; fail "the deployment under a named runtime failed"; }
[ "$(setting app-1 api "$RUNTIME_FIELD")" = "$OWN_RUNTIME" ] || fail "api runs under $(setting app-1 api "$RUNTIME_FIELD")"
[ "$(setting app-1 web "$RUNTIME_FIELD")" = "$OWN_RUNTIME" ] || fail "web runs under $(setting app-1 web "$RUNTIME_FIELD")"

step "mm doctor runs a container under each runtime a host offers"
# The image the check runs: this machine's copy, or else the workload image
# under its name, so that no node has to pull anything.
for node in $NODES; do
  if docker image inspect hello-world >/dev/null 2>&1; then
    docker save hello-world | docker exec -i "$(container "$node")" engine load >/dev/null
  else
    on "$node" engine tag "$WORKLOAD_IMAGE" hello-world
  fi
done
SINCE=$(date +%s)
mm_as alice doctor >"$TMP/runtime-doctor.log" 2>&1 || true
if grep -q 'MM_RUNTIME_NOT_WORKING\|MM_RUNTIME_NOT_CHECKED' "$TMP/runtime-doctor.log"; then cat "$TMP/runtime-doctor.log"; fail "doctor does not find the engine's own runtime working"; fi
# A whole second later, so the last node's container is inside the window.
UNTIL=$(($(date +%s) + 1))
for node in $NODES; do
  if [ "$ENGINE" = podman ]; then
    on "$node" engine events --since "$SINCE" --stream=false --filter event=start --format '{{.Image}}' | grep -q 'hello-world' \
      || fail "doctor ran no container under the runtime $node offers"
  else
    on "$node" engine events --since "$SINCE" --until "$UNTIL" --filter event=start --format '{{json .}}' | grep -q '"image":"hello-world"' \
      || fail "doctor ran no container under the runtime $node offers"
  fi
done
printf '  ok   every node ran a container under the runtime it offers\n'
on app-3 sed -i "s/^runtimes: \\[$OWN_RUNTIME\\]\$/runtimes: [$OWN_RUNTIME, runsc]/" /etc/mm/host.yaml
mm_as alice doctor >"$TMP/runtime-doctor.log" 2>&1 || true
has "$TMP/runtime-doctor.log" MM_RUNTIME_NOT_WORKING "host 'app-3' offers the runtime 'runsc' in its policy and cannot run a container under it" \
  "has no runtime named 'runsc'" "$RUNTIME_HINT"
if grep -q "host 'app-1' offers" "$TMP/runtime-doctor.log"; then fail "doctor reports a runtime on a node that has it"; fi
on app-3 sed -i "s/^runtimes: \\[$OWN_RUNTIME, runsc\\]\$/runtimes: [$OWN_RUNTIME]/" /etc/mm/host.yaml
TOP_LINES=''
STACK_LINES=''
SERVICE_LINES='      api: {stack: app}
      web: {stack: app}
      db: {stack: app}
      worker: {stack: app}'

step "Compose keys that only Docker Swarm reads are refused with what to write"
{
  printf 'services:\n'
  workload api
  printf '    deploy:\n      mode: global\n      replicas: 3\n      endpoint_mode: dnsrr\n      placement:\n        constraints: [node.role == worker]\n'
  workload web
  workload db
  workload worker
  printf 'volumes:\n  dbdata: {}\n'
} >"$TMP/repo/compose.yaml"
commit "$(users alice bob)" "a Compose file from a Swarm"
publish
if mm_as alice deploy plan >"$TMP/swarm.log" 2>&1; then fail "a Swarm-only Compose file was planned"; fi
has "$TMP/swarm.log" MM_COMPOSE_DEPLOY_KEY "write \`placement: all\` under modules.api.services.api" \
  "\`deploy.replicas: 3\`" "\`deploy.endpoint_mode\`" "\`deploy.placement\`"
if mm_as alice config validate >"$TMP/swarm-validate.log" 2>&1; then fail "config validate accepted Swarm-only keys"; fi
has "$TMP/swarm-validate.log" MM_COMPOSE_DEPLOY_KEY
[ "$(services_on app-1)" = "api db web worker" ] || fail "the refused plan changed app-1"

# --- operate: restarts and jobs ------------------------------------------------
# operate_compose: web needs three seconds after each start before its
# health check passes, so a restart that does not wait for it shows.
operate_compose() {
  printf 'services:\n'
  workload api
  printf '  web:\n    image: %s\n' "$WORKLOAD_IMAGE"
  printf '    command: ["sh", "-c", "rm -f /tmp/ready; sleep 3; touch /tmp/ready; echo web-ready; exec sleep infinity"]\n'
  printf '    stop_grace_period: 1s\n'
  printf '    healthcheck: {test: ["CMD", "test", "-f", "/tmp/ready"], interval: 1s, timeout: 1s, retries: 60}\n'
  workload db
  workload worker
}
# started_at NODE SERVICE: when the engine last started the service's container.
started_at() { on "$1" engine inspect --format '{{.State.StartedAt}}' "$(on "$1" engine ps -q --filter "label=com.docker.compose.service=$2")"; }
# seconds_between EARLIER LATER: whole seconds from one start time to
# another, as the engine prints them: Docker in RFC 3339
# (2026-11-14T23:28:29.95Z), Podman with a space and its zone
# (2026-11-14 23:28:29.95 +0000 UTC); both in UTC on these nodes.
seconds_between() {
  python3 -c "
import datetime, sys
def at(value):
    value = 'T'.join(value.split()[:2]).rstrip('Z')
    whole, _, fraction = value.partition('.')
    return datetime.datetime.fromisoformat(whole).timestamp() + float('0.' + (fraction or '0'))
print(int(at(sys.argv[2]) - at(sys.argv[1])))" "$1" "$2"
}
job_containers() { on "$1" engine ps -a --format '{{.Names}}' | grep -c '^mm-job-' || true; }

step "operate: an operator restarts web one node at a time, each healthy before the next"
docker exec "$CLIENT" mm --mm-home /home-carol key create >/dev/null
# Values of the services come from mm.yaml, through the module's template.
printf 'GREETING=nobody\nVERSION=v0\n' >"$TMP/repo/api/.env.example"
MODULE_LINES='    env: {template: .env.example}'
CAROL_KEY=$(public_key_of carol)
operate_compose >"$TMP/repo/compose.yaml"
SERVICE_LINES='      api: {stack: app}
      web: {stack: app, placement: all, drain: 2s, env: {GREETING: hello-from-web}}
      db: {stack: app, placement: app-3}
      worker: {stack: app, placement: app-2}'
JOB_LINES='jobs:
  greet:
    module: api
    service: web
    command: ["sh", "-c", "echo job-says-$GREETING"]
    timeout: 2m'
commit "$(users alice bob)" "carol operates; web takes three seconds to be healthy; a job"
publish
deploy >/dev/null
BEFORE_1=$(started_at app-1 web)
mm_as carol service restart web | tee "$TMP/restart.log"
has "$TMP/restart.log" "Restarted web on app-1, then app-2, then app-3, one node at a time, each healthy before the next (3 containers)."
AFTER_1=$(started_at app-1 web)
AFTER_2=$(started_at app-2 web)
AFTER_3=$(started_at app-3 web)
[ "$AFTER_1" != "$BEFORE_1" ] || fail "web on app-1 was not restarted"
[ "$(seconds_between "$AFTER_1" "$AFTER_2")" -ge 3 ] || fail "app-2 restarted before app-1 was healthy ($AFTER_1, $AFTER_2)"
[ "$(seconds_between "$AFTER_2" "$AFTER_3")" -ge 3 ] || fail "app-3 restarted before app-2 was healthy ($AFTER_2, $AFTER_3)"
for node in $NODES; do
  on "$node" grep '"result":"service-restarted"' /var/lib/mm/audit.jsonl | grep -q '"principal":"carol"' \
    || fail "$node has no audit record of carol's restart"
done
if mm_as carol deploy plan >"$TMP/operate-deploy.log" 2>&1; then
  PLAN=$(mm_as carol --output json deploy plan | json 'value["plan_hash"]')
  if mm_as carol deploy apply --plan "$PLAN" --yes >>"$TMP/operate-deploy.log" 2>&1; then fail "an operator deployed"; fi
fi
has "$TMP/operate-deploy.log" MM_SECURITY

step "operate: mm job run starts a fresh container on web's first node, with web's environment"
mm_as carol job run greet | tee "$TMP/job-run.log"
has "$TMP/job-run.log" "job=greet state=Succeeded attempt=1/1 exit=Some(0) node=app-1" "job-says-hello-from-web"
[ "$(job_containers app-1)" = 0 ] || fail "the job's container is still on app-1"
mm_as carol job history greet | tee "$TMP/job-history.log"
has "$TMP/job-history.log" "job=greet state=Succeeded" "node=app-1"
on app-1 grep '"result":"job-run"' /var/lib/mm/audit.jsonl | grep -q '"principal":"carol"' || fail "app-1 has no audit record of carol's job"

step "values: alice stages a secret on every node without root; web keeps its value until a redeploy"
token_on() { in_service "$1" web 'printenv API_TOKEN || true'; }
ALICE_SECRETS=1
commit "$(users alice bob)" "alice may set secrets"
publish
deploy >/dev/null
printf 'GREETING=nobody\nVERSION=v0\nAPI_TOKEN= # mm:from=API.TOKEN required\n' >"$TMP/repo/api/.env.example"
SAVED_BINDING_LINES=$BINDING_LINES
BINDING_LINES="$BINDING_LINES
  API.TOKEN: {secret: api.token}
secrets:
  api.token:
    storage: {provider: age, location: api/token}"
commit "$(users alice bob)" "web reads API_TOKEN from a secret"
publish
if mm_as alice deploy plan >"$TMP/values-unset.log" 2>&1; then fail "a plan went through with api.token unset"; fi
has "$TMP/values-unset.log" "secret 'api.token'" "secret set api.token"
if printf 'bob-token' | docker exec -i "$CLIENT" mm --root /project --mm-home /home-bob secret set api.token --stdin >"$TMP/values-bob.log" 2>&1; then
  fail "bob staged a secret without the grant"
fi
has "$TMP/values-bob.log" MM_SECURITY
printf 'first-token' | docker exec -i "$CLIENT" mm --root /project --mm-home /home-alice secret set api.token --stdin | tee "$TMP/values-set.log"
has "$TMP/values-set.log" "Staged api.token" "app-1, app-2, app-3"
deploy >/dev/null
for node in $NODES; do
  [ "$(token_on "$node")" = first-token ] || fail "$node: web does not see the first token"
done
printf 'second-token' | docker exec -i "$CLIENT" mm --root /project --mm-home /home-alice secret set api.token --stdin >/dev/null
mm_as bob secret status api.token | tee "$TMP/values-status.log"
[ "$(grep -c 'staged: ~' "$TMP/values-status.log")" = 3 ] || fail "status should show the staged revision on every node"
mm_as bob --output json secret status >"$TMP/values-status.json"
[ "$(json 'len(set(v["staged"]["revision"] for n in value["nodes"] for v in n["values"] if v["key"] == "api.token"))' <"$TMP/values-status.json")" = 1 ] \
  || fail "the nodes stage different revisions"
if grep -q -e first-token -e second-token "$TMP/values-status.log" "$TMP/values-status.json" "$TMP/values-set.log"; then fail "a value is in the output"; fi
mm_as alice service restart web >"$TMP/values-restart.log" 2>&1 || { cat "$TMP/values-restart.log" >&2; fail "mm service restart web failed"; }
for node in $NODES; do
  [ "$(token_on "$node")" = first-token ] || fail "$node: the running web changed before a deployment"
  on "$node" grep -F '"values-staged"' /var/lib/mm/audit.jsonl | grep -qF '"principal":"alice"' || fail "$node: the stage is not audited"
  if on "$node" grep -rqs -e first-token -e second-token /var/lib/mm /tmp/mmd.log; then fail "$node: a value is in the clear on the node"; fi
done
mm_as alice deploy plan | tee "$TMP/values-plan.log"
has "$TMP/values-plan.log" "api.token" "staged by alice"
mm_as alice deploy redeploy --yes 2>&1 | tee "$TMP/values-redeploy.log"
has "$TMP/values-redeploy.log" "Redeploys commit"
for node in $NODES; do
  [ "$(token_on "$node")" = second-token ] || fail "$node: web does not see the second token after the redeploy"
done
mm_as bob secret status api.token >"$TMP/values-live.log"
if grep -q 'staged: ~' "$TMP/values-live.log"; then fail "the redeploy left the change staged"; fi
printf 'GREETING=nobody\nVERSION=v0\n' >"$TMP/repo/api/.env.example"
BINDING_LINES=$SAVED_BINDING_LINES

step "jobs on a cluster: a job runs on the node its placement names, as the user it names, and only there"
JOB_LINES='jobs:
  greet:
    module: api
    service: web
    command: ["sh", "-c", "echo job-says-$GREETING"]
    timeout: 2m
  greet-3:
    module: api
    service: web
    placement: app-3
    user: "4321:4321"
    command: ["sh", "-c", "echo job-on-$(hostname)-as-$(id -u)"]
    timeout: 2m'
commit "$(users alice bob)" "a job on app-3, as another user"
publish
deploy >/dev/null
mm_as carol job run greet-3 | tee "$TMP/job-3.log"
has "$TMP/job-3.log" "job=greet-3 state=Succeeded" "node=app-3" "-as-4321"
[ "$(job_containers app-3)" = 0 ] || fail "the job's container is still on app-3"
JOB_LINES='jobs:
  stray:
    module: api
    service: db
    placement: app-1
    command: ["true"]'
commit "$(users alice bob)" "a job where its service does not run"
publish
if mm_as alice config validate >"$TMP/job-stray.log" 2>&1; then fail "a job placed away from its service validated"; fi
has "$TMP/job-stray.log" MM_PLACEMENT "jobs.stray.placement selects app-1" "it runs on app-3"
mm_as alice deploy logs | tee "$TMP/deploy-logs-none.log"
has "$TMP/deploy-logs-none.log" "The last deployment ran no jobs."
if mm_as alice job schedule list >"$TMP/schedules-remote.log" 2>&1; then fail "remote schedules were listed from this machine"; fi
has "$TMP/schedules-remote.log" MM_REMOTE_SCHEDULES
JOB_LINES=''

# --- jobs with a deployment -------------------------------------------------
# deploy_jobs_compose VERSION: db answers on app-1; web, everywhere, needs
# db; worker on app-1 exits at once unless the migration of VERSION ran (a
# marker in the volume it shares with web there).
deploy_jobs_compose() {
  printf 'services:\n'
  workload api
  printf '  db:\n    image: %s\n' "$WORKLOAD_IMAGE"
  printf '    command: ["sh", "-c", "mkdir -p /www && echo db-answers > /www/index.html && exec httpd -f -p 80 -h /www"]\n'
  printf '    stop_grace_period: 1s\n'
  printf '    healthcheck: {test: ["CMD", "wget", "-q", "-O", "/dev/null", "http://127.0.0.1/"], interval: 1s, timeout: 1s, retries: 30}\n'
  printf '  web:\n    image: %s\n    depends_on: [db]\n' "$WORKLOAD_IMAGE"
  printf '    command: ["sh", "-c", "echo web-%s && exec sleep infinity"]\n' "$1"
  printf '    stop_grace_period: 1s\n    volumes: ["state:/state"]\n'
  printf '  worker:\n    image: %s\n' "$WORKLOAD_IMAGE"
  printf '    command: ["sh", "-c", "test -f /state/migrated-%s || { echo worker-missing-migration; exit 1; }; exec sleep infinity"]\n' "$1"
  printf '    stop_grace_period: 1s\n    volumes: ["state:/state"]\n'
  printf 'volumes:\n  state: {}\n'
}
# deploy_services VERSION: web says its version in its environment. On
# Podman web and worker share a volume, which takes the host's ids on
# both.
JOBS_KEEP_IDS=''
[ "$ENGINE" = podman ] && JOBS_KEEP_IDS=', host-user-namespace: "web and worker share the state volume"'
deploy_services() {
  printf '      api: {stack: app}\n      db: {stack: app, placement: app-1}\n'
  printf '      web: {stack: app, placement: all, drain: 2s, env: {VERSION: "%s"}%s}\n' "$1" "$JOBS_KEEP_IDS"
  printf '      worker: {stack: app, placement: app-1%s}' "$JOBS_KEEP_IDS"
}
deploy_jobs() {
  printf 'jobs:\n'
  printf '  migrate:\n    module: api\n    service: web\n    run: before-deploy\n    timeout: 2m\n'
  printf '    command: ["sh", "-c", "%s"]\n' "$1"
  printf '  clear-cache:\n    module: api\n    service: web\n    run: after-deploy\n    command: ["sh", "-c", "echo cache-cleared-$VERSION"]\n'
  printf '  restore:\n    module: api\n    service: web\n    run: once\n    timeout: 5m\n    user: "4321:4321"\n'
  printf '    command: ["sh", "-c", "sleep %s; echo restored-by-$(id -u)"]\n' "${3:-20}"
}
web_version() { on "$1" sh -c 'engine inspect --format "{{range .Config.Env}}{{println .}}{{end}}" "$(engine ps -q --filter label=com.docker.compose.service=web --filter label=com.docker.compose.oneoff=False)" | sed -n "s/^VERSION=//p"'; }

step "before-deploy: the migration runs after db is healthy and before web and worker start, on a fresh worker"
deploy_jobs_compose v1 >"$TMP/repo/compose.yaml"
SERVICE_LINES=$(deploy_services v1)
JOB_LINES=$(deploy_jobs 'wget -q -O - http://db/ && touch /state/migrated-$VERSION && echo migrated-$VERSION')
commit "$(users alice bob)" "a migration before web, a cache clear after, a restore once"
publish
mm_as alice deploy plan | tee "$TMP/jobs-plan.log"
has "$TMP/jobs-plan.log" "migrate (before-deploy) on app-1" "clear-cache (after-deploy) on app-1" "restore (once) on app-1: due"
deploy | tee "$TMP/jobs-apply.log"
has "$TMP/jobs-apply.log" "app: job migrate" "once job restore started on app-1"
[ "$(services_on app-1)" = "api db web worker" ] || fail "app-1 runs: $(services_on app-1)"
mm_as alice deploy logs | tee "$TMP/jobs-logs.log"
has "$TMP/jobs-logs.log" "app-1 migrate | db-answers" "app-1 migrate | migrated-v1" "app-1 clear-cache | cache-cleared-v1"

step "once: the restore runs detached from the client, survives mmd restarting, and is not run again"
stop_mmd app-1
start_mmd app-1
wait_mmd app-1
RESTORE=$(mm_as alice --output json job history restore | json 'value[0]["id"]')
for _ in $(seq 1 60); do
  state=$(mm_as alice --output json job status "$RESTORE" | json 'value["state"]')
  [ "$state" = succeeded ] && break
  sleep 2
done
[ "$state" = succeeded ] || fail "the once job did not finish: $state"
mm_as alice job status "$RESTORE" | tee "$TMP/jobs-restore.log"
has "$TMP/jobs-restore.log" "restored-by-4321"
deploy_jobs_compose v2 >"$TMP/repo/compose.yaml"
SERVICE_LINES=$(deploy_services v2)
commit "$(users alice bob)" "web v2"
publish
mm_as alice deploy plan | tee "$TMP/jobs-plan-2.log"
has "$TMP/jobs-plan-2.log" "restore (once) on app-1: ran"
deploy >/dev/null
[ "$(mm_as alice --output json job history restore | json 'len(value)')" = 1 ] || fail "the once job ran again"
for node in $NODES; do [ "$(web_version "$node")" = v2 ] || fail "$node does not run web v2"; done

step "before-deploy: a failing migration stops the rollout with the old version running everywhere"
deploy_jobs_compose v3 >"$TMP/repo/compose.yaml"
SERVICE_LINES=$(deploy_services v3)
JOB_LINES=$(deploy_jobs 'echo cannot-migrate-$VERSION; exit 1')
commit "$(users alice bob)" "a migration that fails"
publish
if deploy >"$TMP/jobs-fail.log" 2>&1; then fail "a failing migration deployed"; fi
has "$TMP/jobs-fail.log" "job 'migrate' exited with 1" "cannot-migrate-v3"
for node in $NODES; do [ "$(web_version "$node")" = v2 ] || fail "$node changed web although the migration failed"; done

step "a deployment killed during its migration leaves nothing that blocks the next one"
deploy_jobs_compose v4 >"$TMP/repo/compose.yaml"
SERVICE_LINES=$(deploy_services v4)
JOB_LINES=$(deploy_jobs 'sleep 15; touch /state/migrated-$VERSION; echo migrated-$VERSION')
commit "$(users alice bob)" "a slow migration"
publish
SLOWJOB=$(mm_as alice --output json deploy plan | json 'value["plan_hash"]')
mm_as alice deploy apply --plan "$SLOWJOB" --yes >"$TMP/jobs-killed.log" 2>&1 &
KILLED=$!
for _ in $(seq 1 60); do on app-1 engine ps --format '{{.Names}}' | grep -q '^mm-job-' && break; sleep 1; done
docker exec "$CLIENT" pkill -9 -f "deploy apply --plan $SLOWJOB" || true
stop_mmd app-1
wait "$KILLED" || true
start_mmd app-1
wait_mmd app-1
sleep 16
# The killed client's lock on app-2 and app-3 lasts until its lease runs
# out; app-1 gave it up when its mmd restarted. Nothing else is done.
for _ in $(seq 1 12); do
  deploy >"$TMP/jobs-after-kill.log" 2>&1 && break
  grep -q MM_DEPLOY_RUNNING "$TMP/jobs-after-kill.log" || { cat "$TMP/jobs-after-kill.log" >&2; fail "the deployment after the killed one failed"; }
  sleep 10
done
cat "$TMP/jobs-after-kill.log"
for node in $NODES; do [ "$(web_version "$node")" = v4 ] || fail "$node does not run web v4 after the killed deployment"; done
# What the cut-off run left goes without a manual step: the deployment's
# clean-up, or mmd's keeper once the dead daemon's claim on the run lapsed.
for _ in $(seq 1 30); do [ "$(job_containers app-1)" = 0 ] && break; sleep 2; done
[ "$(job_containers app-1)" = 0 ] || { on app-1 engine ps -a >&2; fail "a job container is left on app-1"; }
# The later stages declare their own jobs.
JOB_LINES=''

# --- job services and environment-bound files -------------------------------
job_services_stages

# --- ingress ------------------------------------------------------------------
ingress_stages
# --- ingress balance ----------------------------------------------------------
balance_stages
# --- restarts of unhealthy containers -------------------------------------------
health_stages
# --- Cloudflare Tunnel ingress and route rules (request 01) -------------------
tunnel_stages
# The push stages deploy without the route, its secrets, or its domain: the
# proxy is withdrawn.
NETWORK_LINES=''
STACK_LINES=''
ENVIRONMENT_LINES=''

# --- release facts ------------------------------------------------------------
release_facts_stages

# --- the managed secret store (request 02) --------------------------------------
store_stages

# --- push delivery ------------------------------------------------------------
# Last: it takes git and the origin off the nodes.
step "push delivery: a host that fetches takes no files from a client"
{
  printf 'services:\n'
  workload api
  workload web
  workload db
  workload worker
  printf 'volumes:\n  dbdata: {}\n'
} >"$TMP/repo/compose.yaml"
CLUSTER_LINES='    delivery: push'
# Hardened, and web with a config file: both are made on the node from the
# files mm sent.
TOP_LINES='hardening: strict'
SERVICE_LINES='      api: {stack: app}
      web: {stack: app, config-files: {/etc/greeting.conf: {template: greeting.conf.tmpl}}}
      db: {stack: app}
      worker: {stack: app}'
commit "$(users alice bob)" "the target is delivered by push"
# The client's checkout only: from here on there is no git host.
to_client() {
  docker exec "$CLIENT" rm -rf /project
  docker cp "$TMP/repo" "$CLIENT:/project"
  # What must never leave the client: a .env and a file nobody added.
  docker exec "$CLIENT" sh -c 'printf "DATABASE_PASSWORD=push-live-secret-value\n" >/project/.env; printf "push-live-scratch-note\n" >/project/notes.txt'
}
to_client
if mm_as alice deploy plan >"$TMP/push-fetching.log" 2>&1; then fail "a host that fetches took a pushed commit"; fi
has "$TMP/push-fetching.log" "takes no files from a client" "delivery: push"

step "push delivery: the nodes lose their origin and git, and deploy the commit mm sends"
for node in $NODES; do
  on "$node" sh -c "
    set -e
    sed -i 's|^    origin: .*|    delivery: push|' /etc/mm/host.yaml
    /usr/libexec/mmd access validate >/dev/null
    rm -rf /srv/git /var/lib/mm/projects/$PROJECT_ID/repo.git /var/lib/mm/projects/$PROJECT_ID/deploy-key*
    mv /usr/bin/git /usr/bin/git.gone"
  on "$node" grep -q 'delivery: push' /etc/mm/host.yaml || fail "$node: the host policy does not say push"
done
PUSH_COMMIT=$(git_repo rev-parse HEAD)
mm_as alice deploy plan | tee "$TMP/push-plan.log"
has "$TMP/push-plan.log" "Cluster deployment plan" "  bundle:   " "sent to app-1, app-2, app-3"
# Planning the same commit again sends nothing.
mm_as alice --output json deploy plan >"$TMP/push-plan.json"
[ "$(json 'value["bundle"].get("sent_to", [])' <"$TMP/push-plan.json")" = "[]" ] || fail "a commit the nodes hold was sent again"
PUSH_BUNDLE=$(json 'value["bundle"]["bundle_hash"]' <"$TMP/push-plan.json")
[ "$(json 'sorted(set(n["plan"]["binding"]["bundle_hash"] for n in value["nodes"]))' <"$TMP/push-plan.json")" = "['$PUSH_BUNDLE']" ] \
  || fail "the nodes did not all plan from the one bundle"
mm_as alice deploy apply --plan "$(json 'value["plan_hash"]' <"$TMP/push-plan.json")" --yes 2>&1 | tee "$TMP/push-apply.log"
has "$TMP/push-apply.log" "Deployment "
[ "$(services_on app-1)" = "api db web worker" ] || fail "app-1 runs '$(services_on app-1)' after the pushed deployment"
[ "$(confined app-1 web)" = "CapEff: 0000000000000000 NoNewPrivs: 1 root read-only 1" ] || fail "web of the pushed commit runs with $(confined app-1 web)"
[ "$(in_service app-1 web "cat /etc/greeting.conf")" = "greeting hola" ] || fail "web does not see the config file rendered from the pushed commit"
[ "$(mm_as alice --output json deploy status | json 'sorted(set(n["status"]["current"]["commit"] for n in value["nodes"]))')" = "['$PUSH_COMMIT']" ] \
  || fail "the nodes do not record the pushed commit as deployed"
for node in $NODES; do
  checkout="/srv/mm/projects/$PROJECT_ID/production/${PUSH_COMMIT:0:12}"
  on "$node" test -f "$checkout/compose.yaml" || fail "$node: the pushed commit has no checkout"
  if on "$node" test -e "$checkout/.env" || on "$node" test -e "$checkout/notes.txt"; then fail "$node: a file of the working tree arrived"; fi
  if on "$node" grep -rqs -e push-live-secret-value -e push-live-scratch-note /srv/mm /var/lib/mm /tmp/mmd.log; then
    fail "$node: something from the client's working tree is on the node"
  fi
  on "$node" grep -F 'bundle-delivered' /var/lib/mm/audit.jsonl | grep -qF "$PUSH_BUNDLE" || fail "$node: the audit log does not record the bundle"
  on "$node" grep -F '"planned"' /var/lib/mm/audit.jsonl | grep -qF "$PUSH_BUNDLE" || fail "$node: the plan's audit record does not name the bundle"
  if on "$node" sh -c 'command -v git' >/dev/null 2>&1; then fail "$node: git is still on the node"; fi
done

step "push delivery: a bundle changed on the way is refused, and nothing of it stays"
printf '# a second pushed commit\n' >>"$TMP/repo/compose.yaml"
commit "$(users alice bob)" "a second pushed commit"
to_client
TAMPERED=$(git_repo rev-parse HEAD)
# Between sshd and the relay on app-2: one character of the first data
# frame's payload is changed, as a broken or hostile hop would.
cat >"$TMP/tamper.pl" <<'PERL'
use strict;
binmode(STDIN); binmode(STDOUT); $| = 1;
my $changed = 0;
while (read(STDIN, my $head, 4) == 4) {
  my $length = unpack("N", $head);
  my $frame = "";
  while (length($frame) < $length) {
    my $count = read(STDIN, my $part, $length - length($frame));
    last unless $count;
    $frame .= $part;
  }
  if (!$changed && $frame =~ /"type":"data"/ && $frame =~ /"bytes":"([A-Za-z0-9+\/]{64})/) {
    my $at = $-[1] + 40;
    substr($frame, $at, 1) = substr($frame, $at, 1) eq "A" ? "B" : "A";
    $changed = 1;
  }
  print $head, $frame;
}
PERL
docker cp "$TMP/tamper.pl" "$(container app-2):/usr/libexec/mm-tamper.pl"
on app-2 sh -c 'cp -p /usr/libexec/mm-relay-shell /usr/libexec/mm-relay-shell.real
  printf "#!/bin/sh\nperl /usr/libexec/mm-tamper.pl | /usr/bin/mm relay serve --stdio --socket /run/mm/mm.sock --daemon-uid 0\n" >/usr/libexec/mm-relay-shell'
if mm_as alice deploy plan >"$TMP/push-tampered.log" 2>&1; then fail "a bundle that was changed on the way was planned"; fi
on app-2 mv /usr/libexec/mm-relay-shell.real /usr/libexec/mm-relay-shell
has "$TMP/push-tampered.log" "app-2" "hashes to" "nothing was kept"
on app-2 test ! -e "/srv/mm/projects/$PROJECT_ID/production/${TAMPERED:0:12}" || fail "app-2 kept a checkout of the refused bundle"
[ -z "$(on app-2 ls "/var/lib/mm/projects/$PROJECT_ID/incoming")" ] || fail "app-2 kept a file of the refused bundle"
[ "$(mm_as alice --output json deploy status | json 'sorted(set(n["status"]["current"]["commit"] for n in value["nodes"]))')" = "['$PUSH_COMMIT']" ] \
  || fail "the refused bundle changed what a node runs"
# The same commit over an honest connection: only app-2 still needs it.
mm_as alice deploy plan | tee "$TMP/push-plan-again.log"
has "$TMP/push-plan-again.log" "sent to app-2)"
deploy >/dev/null
[ "$(mm_as alice --output json deploy status | json 'sorted(set(n["status"]["current"]["commit"] for n in value["nodes"]))')" = "['$TAMPERED']" ] \
  || fail "the second pushed commit is not what the nodes run"

step "push delivery: a rollback and an access sync need no git host and no upload"
mm_as alice deploy rollback --yes 2>&1 | tee "$TMP/push-rollback.log"
has "$TMP/push-rollback.log" "returns app-1, app-2, app-3 to commit $PUSH_COMMIT" "every node already held it" "Deployment "
[ "$(mm_as alice --output json deploy status | json 'sorted(set(n["status"]["current"]["commit"] for n in value["nodes"]))')" = "['$PUSH_COMMIT']" ] \
  || fail "the rollback did not return the nodes to the first pushed commit"
SYNC=$(mm_as ops --output json team sync | json 'value["plan_hash"]')
mm_as ops team sync --plan "$SYNC" --yes >/dev/null

printf '\nPASS: cluster deployment, placement, status and logs, config files, the terminal UI, networks, the mesh guard, names, rolling deploys, shared volumes, hardening, runtimes, restarts, jobs (also with a deployment), ingress, Cloudflare Tunnel ingress, release facts, the managed secret store, and push delivery across three %s nodes\n' "$ENGINE"
