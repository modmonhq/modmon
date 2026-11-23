#!/usr/bin/env bash
# Live remote access: one locked `mm` SSH account, keys rendered by mmd from
# the repo's users file, deploys fetched from a pinned git origin, and no
# shell, forwarding, or PTY for anyone. The developer's keys are encrypted
# with passphrases mm keeps in a keyring: the client runs a Secret Service of
# its own (GNOME Keyring on a private session bus), never the machine's.
#
# Requires Docker. The target container runs privileged so it can host its
# own Docker engine. A prebuilt client image (MM_LIVE_CLIENT_IMAGE) needs
# dbus, gnome-keyring, libsecret-tools and procps besides the rest. Set MM_LIVE_PREBUILT_DIR to a directory holding Debian
# bookworm builds of `mm` and `mmd` to skip the build.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TMP=$(mktemp -d)
NETWORK="mm-remote-access-$RANDOM-$RANDOM"
TARGET="${NETWORK}-target"
CLIENT="${NETWORK}-client"
PROJECT_ID=019c527b-6242-7b43-bfd4-bd60e27c1de4
WORKLOAD_IMAGE=${MM_LIVE_WORKLOAD_IMAGE:-busybox:1.36.1}
REGISTRY_IMAGE=${MM_LIVE_REGISTRY_IMAGE:-registry:2.8.3}

cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    docker exec "$TARGET" sh -c 'tail -n 50 /tmp/mmd.log /tmp/sshd.log /var/lib/mm/audit.jsonl 2>/dev/null' >&2 || true
    docker exec "$CLIENT" sh -c 'tail -n 20 /tmp/forward*.log 2>/dev/null' >&2 || true
  fi
  if [ -n "${MM_LIVE_KEEP:-}" ]; then
    echo "kept $TARGET and $CLIENT (MM_LIVE_KEEP); remove them and network $NETWORK when done" >&2
    return "$status"
  fi
  # With its anonymous volume: the target keeps its engine's data in one.
  docker rm -f -v "$CLIENT" "$TARGET" >/dev/null 2>&1 || true
  docker network rm "$NETWORK" >/dev/null 2>&1 || true
  rm -rf "$TMP"
  return "$status"
}
trap cleanup EXIT

step() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

mkdir -p "$TMP/build" "$TMP/keys" "$TMP/repo"
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

step "starting the target (sshd, mmd, Docker) and the client"
docker network create "$NETWORK" >/dev/null
# MM_LIVE_TARGET_IMAGE / MM_LIVE_CLIENT_IMAGE: prebuilt images with the
# packages below already installed, to skip the installs between runs.
docker run -d --privileged --name "$TARGET" --hostname remote-target --network "$NETWORK" \
  --volume /var/lib/docker \
  --cpus=1 --cpu-shares=256 --memory=1g --pids-limit=512 "${MM_LIVE_TARGET_IMAGE:-debian:bookworm-slim}" sleep infinity >/dev/null
# With an init process: mm keeps its SSH connections open in background
# `ssh` processes, and something has to reap each one when it closes.
docker run -d --init --name "$CLIENT" --network "$NETWORK" \
  --cpus=0.5 --cpu-shares=256 --memory=512m --pids-limit=128 "${MM_LIVE_CLIENT_IMAGE:-debian:bookworm-slim}" sleep infinity >/dev/null
[ -n "${MM_LIVE_TARGET_IMAGE:-}" ] || docker exec "$TARGET" sh -c '
  set -e
  apt-get update >/dev/null
  DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends openssh-server git ca-certificates curl gnupg iptables >/dev/null
  install -m 0755 -d /etc/apt/keyrings
  curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc
  echo "deb [signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian bookworm stable" >/etc/apt/sources.list.d/docker.list
  apt-get update >/dev/null
  DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends docker-ce docker-ce-cli containerd.io docker-compose-plugin docker-buildx-plugin >/dev/null
  rm -rf /var/lib/apt/lists/*'
[ -n "${MM_LIVE_CLIENT_IMAGE:-}" ] || docker exec "$CLIENT" sh -c 'apt-get update >/dev/null && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends openssh-client git ca-certificates python3 procps dbus gnome-keyring libsecret-tools >/dev/null && rm -rf /var/lib/apt/lists/* && git config --global --add safe.directory "*"'
# The client's keyring: a session bus of its own and a login keyring
# unlocked with a password nobody keeps. Only commands given KEYRING reach it.
docker exec "$CLIENT" sh -c '
  set -e
  mkdir -p /run/keyring /home-keyring && chmod 700 /run/keyring
  export HOME=/home-keyring XDG_RUNTIME_DIR=/run/keyring
  dbus-daemon --session --address=unix:path=/run/keyring/bus --fork --nopidfile
  head -c 18 /dev/urandom | base64 | DBUS_SESSION_BUS_ADDRESS=unix:path=/run/keyring/bus \
    gnome-keyring-daemon --unlock --components=secrets --daemonize >/dev/null 2>&1'
KEYRING=(-e DBUS_SESSION_BUS_ADDRESS=unix:path=/run/keyring/bus)
for _ in $(seq 1 20); do
  # A lookup that finds nothing says nothing; with no keyring it complains.
  docker exec "${KEYRING[@]}" "$CLIENT" sh -c '[ -z "$(secret-tool lookup service mm-ssh-key fingerprint probe 2>&1)" ]' && break
  sleep 0.5
done
# The inner engine cannot stack overlayfs on the container's own root.
docker exec -d "$TARGET" sh -c 'dockerd --storage-driver=vfs >/tmp/dockerd.log 2>&1'
for _ in $(seq 1 60); do
  docker exec "$TARGET" docker info >/dev/null 2>&1 && break
  sleep 1
done
docker exec "$TARGET" docker info >/dev/null 2>&1 || fail "target Docker engine did not start"
if docker image inspect "$WORKLOAD_IMAGE" >/dev/null 2>&1; then
  docker save "$WORKLOAD_IMAGE" | docker exec -i "$TARGET" docker load >/dev/null
else
  docker exec "$TARGET" docker pull -q "$WORKLOAD_IMAGE" >/dev/null
fi

docker cp "$BIN_DIR/mm" "$TARGET:/usr/bin/mm"
docker cp "$BIN_DIR/mmd" "$TARGET:/usr/libexec/mmd"
docker cp "$BIN_DIR/mm" "$CLIENT:/usr/local/bin/mm"
docker cp "$ROOT/packaging/mm-relay-shell" "$TARGET:/usr/libexec/mm-relay-shell"
docker cp "$ROOT/packaging/sshd/90-mm-relay.conf" "$TARGET:/etc/ssh/sshd_config.d/90-mm-relay.conf"

# The one SSH account. Its shell and every authorized_keys line run only the
# relay; it is in no group but the socket's.
docker exec "$TARGET" sh -c '
  set -e
  chmod 755 /usr/bin/mm /usr/libexec/mmd /usr/libexec/mm-relay-shell
  groupadd --system --gid 991 mm
  useradd --system --uid 991 --gid 991 --home-dir /nonexistent --no-create-home --shell /usr/libexec/mm-relay-shell mm
  echo /usr/libexec/mm-relay-shell >> /etc/shells
  mkdir -p /run/sshd /run/mm /etc/mm /var/lib/mm/ssh /var/lib/mm-home /srv/mm/projects /srv/git
  chown root:991 /run/mm && chmod 750 /run/mm
  chmod 700 /etc/mm
  sshd -t'

step "creating keys: a break-glass key, and a developer key as an earlier mm made it"
ssh-keygen -q -t ed25519 -N '' -C ops@offline -f "$TMP/keys/ops"
# The break-glass key is this script's own, so the script may use it with
# plain ssh below. The developer's keys are mm's: only mm uses them.
docker exec "$CLIENT" sh -c 'mkdir -p /home-alice /home-ops /project /keys && chmod 700 /keys'
docker cp "$TMP/keys/ops" "$CLIENT:/keys/ops"
docker exec "$CLIENT" sh -c 'chown root:root /keys/ops && chmod 600 /keys/ops'
# Before mm encrypted its keys, the device key was a plain file. Alice starts
# with one, so the migration below is of a key that already has access.
ALICE_PRIVATE=/home-alice/identities/ssh/id_ed25519
docker exec "$CLIENT" sh -c "mkdir -p /home-alice/identities/ssh && chmod -R 700 /home-alice && ssh-keygen -q -t ed25519 -N '' -C mm:alice@client -f $ALICE_PRIVATE"
ALICE_KEY=$(docker exec "$CLIENT" sh -c "cat $ALICE_PRIVATE.pub")
OPS_KEY=$(cut -d' ' -f1,2 "$TMP/keys/ops.pub")
# Whether a key file loads with no passphrase and nobody to ask, which is
# the first thing `ssh -o BatchMode=yes -i KEY` does.
loads_alone() { docker exec "$CLIENT" ssh-keygen -y -P '' -f "$1" >/dev/null 2>&1; }
keyring_entries() { docker exec "${KEYRING[@]}" "$CLIENT" sh -c 'secret-tool search --all service mm-ssh-key 2>&1 | grep -c "^attribute.fingerprint" || true'; }

step "writing the host policy"
cat >"$TMP/host.yaml" <<YAML
schema-version: 1
break-glass:
  - name: ops
    key: $OPS_KEY
projects:
  $PROJECT_ID:
    name: access-live
    origin: /srv/git/access-live.git
    environments: [staging]
YAML
cat >"$TMP/mmd.json" <<'JSON'
{
  "schema_version": 1,
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
docker cp "$TMP/host.yaml" "$TARGET:/etc/mm/host.yaml"
docker cp "$TMP/mmd.json" "$TARGET:/etc/mm/mmd.json"
docker exec "$TARGET" sh -c 'chown root:root /etc/mm/* && chmod 600 /etc/mm/*'
docker exec "$TARGET" /usr/libexec/mmd access validate
DAEMON_FP=$(docker exec "$TARGET" /usr/libexec/mmd --print-identity)
HOST_KEY=$(docker exec "$TARGET" sh -c 'cut -d" " -f1,2 /etc/ssh/ssh_host_ed25519_key.pub')

step "writing the project: users and deploy-from live in the repo"
mkdir -p "$TMP/repo/.config" "$TMP/repo/api"
touch "$TMP/repo/api/.keep"
cat >"$TMP/repo/compose.yaml" <<YAML
services:
  api:
    image: $WORKLOAD_IMAGE
    command: ["sh", "-c", "mkdir -p /www && echo forwarded-hello >/www/index.html && echo access-live-started && exec httpd -f -p 8080 -h /www"]
    expose: ["8080"]
YAML
write_config() {
  cat >"$TMP/repo/.config/mm.yaml" <<YAML
schema-version: 1
default-environment: staging
project:
  id: $PROJECT_ID
  name: access-live
target: local
runtime:
  driver: docker-compose
build:
  platforms: [linux/amd64]
stacks:
  app:
    project-name: access-live-{environment}-{stack}
    files: [compose.yaml]
modules:
  api:
    path: api
    services:
      api:
        stack: app
${WEB_MODULE:-}${AGENTS_BLOCK:-}targets:
  remote:
    host: $TARGET
    host-key: $HOST_KEY
    daemon-fingerprint: ${2:-$DAEMON_FP}
environments:
  local: {}
  staging:
    target: remote
    deploy-from: [main]
YAML
  if [ -n "${1:-}" ]; then
    cat >>"$TMP/repo/.config/mm.yaml" <<YAML
users:
  alice:
    keys: ["$ALICE_KEY"${AGENT_KEY_ENTRY:-}]
    access:
      staging: $1
YAML
  fi
}
git_repo() { git -C "$TMP/repo" -c user.name=live -c user.email=live@example.test "$@"; }
write_config "{level: deploy, forward: [api]}"
git_repo init --quiet --initial-branch=main
git_repo add .
git_repo commit --quiet -m "alice may deploy staging"
git clone --quiet --bare "$TMP/repo" "$TMP/origin.git"
publish() {
  git -C "$TMP/repo" push --quiet "$TMP/origin.git" "$@"
  # Stand-in for a git host: the origin is a root-owned bare repository.
  docker exec "$TARGET" rm -rf /srv/git/access-live.git
  docker cp "$TMP/origin.git" "$TARGET:/srv/git/access-live.git"
  docker exec "$TARGET" chown -R root:root /srv/git/access-live.git
  docker exec "$CLIENT" rm -rf /project
  docker cp "$TMP/repo" "$CLIENT:/project"
}
publish main

docker exec -d "$TARGET" sh -c '/usr/libexec/mmd >/tmp/mmd.log 2>&1'
docker exec -d "$TARGET" sh -c '/usr/sbin/sshd -D -e >/tmp/sshd.log 2>&1'
for _ in $(seq 1 30); do
  docker exec "$TARGET" test -S /run/mm/mm.sock && break
  sleep 0.5
done
docker exec "$TARGET" sh -c 'grep -c "^restrict," /etc/ssh/mm_authorized_keys' | grep -x 1 >/dev/null \
  || fail "authorized_keys should hold only the break-glass key before any reconcile"

OPS_PRIVATE=$(cat "$TMP/keys/ops")
as_alice() { docker exec "${KEYRING[@]}" "$CLIENT" mm --root /project --mm-home /home-alice --runtime docker-compose --environment staging "$@"; }
as_ops() { docker exec -e MM_SSH_KEY="$OPS_PRIVATE" "$CLIENT" mm --root /project --mm-home /home-ops --runtime docker-compose --environment staging "$@"; }
plan_hash() { python3 -c 'import json,sys; print(json.load(sys.stdin)["binding"]["plan_hash"])'; }

step "before any reconcile, sshd refuses the developer's key"
if as_alice team whoami >"$TMP/denied.log" 2>&1; then fail "alice reached the host before the users file was applied"; fi

step "break-glass reconciles staging from the repo"
as_ops team whoami
SYNC=$(as_ops --output json team sync | plan_hash)
as_ops team sync --plan "$SYNC" --yes
docker exec "$TARGET" sh -c 'grep -c "^restrict," /etc/ssh/mm_authorized_keys' | grep -x 2 >/dev/null \
  || fail "the reconcile should add alice's key to authorized_keys"

step "the developer deploys a commit the host fetched itself"
as_alice team whoami | tee "$TMP/whoami.log"
grep -q '"level":"deploy"' "$TMP/whoami.log" || fail "alice should have deploy on staging"
PLAN=$(as_alice --output json deploy plan | plan_hash)
as_alice deploy apply --plan "$PLAN" --yes
as_alice service status | tee "$TMP/status.log"
as_alice service logs api | tee "$TMP/logs.log"
grep -q access-live-started "$TMP/logs.log" || fail "logs did not reach the client"
if as_alice service exec api -- echo escaped >"$TMP/exec.log" 2>&1; then
  fail "deploy level unexpectedly allowed exec"
fi
grep -q MM_SECURITY "$TMP/exec.log" || fail "exec denial was not a security error"

step "the plain key is reported, and protected in place without changing who alice is"
loads_alone "$ALICE_PRIVATE" || fail "the plain key should load without a passphrase before it is protected"
# For the timing comparison at the end: connecting with the plain key.
PLAIN_TIMING=$(docker exec -e MM_SSH_REUSE=off "$CLIENT" sh -c 'for _ in 1 2 3 4 5; do s=$(date +%s%N); mm --root /project --mm-home /home-alice --runtime docker-compose --environment staging team whoami >/dev/null 2>&1 || printf "failed:"; e=$(date +%s%N); printf "%s " $(( (e - s) / 1000000 )); done')
as_alice --output json doctor >"$TMP/doctor-plain.json" 2>/dev/null || true
grep -q MM_KEY_UNPROTECTED "$TMP/doctor-plain.json" || fail "mm doctor did not report the plain key"
grep -q 'Run `mm key protect`' "$TMP/doctor-plain.json" || fail "mm doctor did not name the command that protects the key"
as_alice key protect | tee "$TMP/protect-plan.log"
grep -q "The public key stays the same" "$TMP/protect-plan.log" || fail "the review did not say the public key stays"
loads_alone "$ALICE_PRIVATE" || fail "reviewing the step changed the key"
PROTECT=$(as_alice --output json key protect | python3 -c 'import json,sys; print(json.load(sys.stdin)["plan_hash"])')
as_alice key protect --plan "$PROTECT" --yes | tee "$TMP/protect.log"
grep -q "passphrase in the system keychain" "$TMP/protect.log" || fail "the key was not protected with the keyring"
[ "$(docker exec "$CLIENT" sh -c "cat $ALICE_PRIVATE.pub")" = "$ALICE_KEY" ] || fail "protecting the key changed its public half"
if loads_alone "$ALICE_PRIVATE"; then fail "the protected key still loads without a passphrase"; fi
docker exec "$CLIENT" test ! -e "$ALICE_PRIVATE.passphrase" || fail "the passphrase was written beside the key although a keyring answers"
[ "$(keyring_entries)" -eq 1 ] || fail "the keyring should hold one passphrase, for alice's key"
[ "$(docker exec "$CLIENT" sh -c 'find /home-alice/run/ssh -type s 2>/dev/null | wc -l')" -eq 0 ] || fail "protecting the key left a connection open"
# Same key, so the host needs to learn nothing: access is as it was, and no
# command prompts (there is no terminal here to prompt on).
as_alice team whoami >"$TMP/whoami-protected.log" </dev/null
grep -q '"level":"deploy"' "$TMP/whoami-protected.log" || fail "alice lost access when her key was protected"
as_alice service logs api </dev/null | grep access-live-started >/dev/null || fail "logs did not reach the client with the protected key"
as_alice --output json doctor >"$TMP/doctor-protected.json" 2>/dev/null || true
if grep -q 'MM_KEY_' "$TMP/doctor-protected.json"; then fail "mm doctor still reports a key problem"; fi

step "the key file alone authenticates nowhere"
# What an agent taking a shortcut would try. sshd never sees a signature.
ALICE_SSH=(-F /dev/null -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o BatchMode=yes -o IdentitiesOnly=yes -o IdentityAgent=none -i "$ALICE_PRIVATE")
if docker exec "$CLIENT" ssh "${ALICE_SSH[@]}" "mm@$TARGET" </dev/null >"$TMP/direct.log" 2>&1; then
  fail "plain ssh authenticated with alice's key file"
fi
grep -q "Permission denied" "$TMP/direct.log" || fail "plain ssh with alice's key did not end in a refused login"
# Copied to another place (another machine has no keyring entry either).
docker exec "$CLIENT" sh -c "cp $ALICE_PRIVATE /tmp/copied-key && chmod 600 /tmp/copied-key"
if loads_alone /tmp/copied-key; then fail "a copy of the key file loads without a passphrase"; fi
docker exec "$CLIENT" rm -f /tmp/copied-key

step "the developer's connection stays open between commands, for her alone"
# How many connections a home keeps, and the trace of one more command.
kept() { docker exec "$CLIENT" sh -c "find $1/run/ssh -type s 2>/dev/null | wc -l"; }
traced() { docker exec "${KEYRING[@]}" -e MM_TIMING=1 "$CLIENT" mm --root /project --mm-home /home-alice --runtime docker-compose --environment staging team whoami 2>&1 >/dev/null; }
[ "$(kept /home-alice)" -eq 1 ] || fail "alice's connection to the host was not kept"
docker exec "$CLIENT" stat -c '%a %u' /home-alice/run/ssh | grep -x '700 0' >/dev/null || fail "the socket directory is open to others"
docker exec "$CLIENT" sh -c 'stat -c %a $(find /home-alice/run/ssh -type s)' | grep -x 600 >/dev/null || fail "the socket is open to others"
traced >"$TMP/kept-trace.log"
grep -q 'ssh.connect .*, kept' "$TMP/kept-trace.log" || fail "the next command did not use the kept connection"
# A command on a kept connection authenticates nothing, so it asks no vault.
if grep -q 'key.unlock' "$TMP/kept-trace.log"; then fail "a command on a kept connection read the passphrase"; fi
# The connection is listed with its host and key, and can be closed.
as_alice target connections | tee "$TMP/connections.log"
grep -q "mm@$TARGET:22  key $ALICE_PRIVATE" "$TMP/connections.log" || fail "mm target connections did not list the kept connection"
if grep -q "no longer answering" "$TMP/connections.log"; then fail "the kept connection is listed as dead"; fi
as_alice --output json target connections | grep '"open": true' >/dev/null || fail "the JSON list does not show an open connection"
as_alice target connections --close | grep "^Closed:" >/dev/null || fail "mm target connections --close did not say what it closed"
[ "$(kept /home-alice)" -eq 0 ] || fail "mm target connections --close left a connection open"
as_alice target connections | grep "No connection is kept open." >/dev/null || fail "the list is not empty after closing"
traced >"$TMP/anew-trace.log"
grep -q 'key.unlock' "$TMP/anew-trace.log" || fail "a new connection did not read the passphrase from the vault"
if grep -q ', kept' "$TMP/anew-trace.log"; then fail "a connection was used after it was closed"; fi
[ "$(kept /home-alice)" -eq 1 ] || fail "the connection was not kept again after closing"
# A key passed in the environment is never kept.
[ "$(kept /home-ops)" -eq 0 ] || fail "a connection made with MM_SSH_KEY was kept"
# Turning reuse off closes what was kept, and connects anew.
docker exec "${KEYRING[@]}" -e MM_SSH_REUSE=off -e MM_TIMING=1 "$CLIENT" mm --root /project --mm-home /home-alice --runtime docker-compose --environment staging team whoami >"$TMP/reuse-off.log" 2>&1
if grep -q ', kept' "$TMP/reuse-off.log"; then fail "MM_SSH_REUSE=off still used a kept connection"; fi
[ "$(kept /home-alice)" -eq 0 ] || fail "MM_SSH_REUSE=off left a connection open"
sleep 1
if docker exec "$CLIENT" sh -c 'grep -l "\[mux\]" /proc/[0-9]*/cmdline 2>/dev/null | grep -q .'; then
  fail "MM_SSH_REUSE=off left a background ssh running"
fi
# A directory someone else can enter keeps nothing.
docker exec "$CLIENT" chmod 755 /home-alice/run/ssh
as_alice team whoami >/dev/null
[ "$(kept /home-alice)" -eq 0 ] || fail "a connection was kept in a directory open to others"
docker exec "$CLIENT" chmod 700 /home-alice/run/ssh
as_alice team whoami >/dev/null
[ "$(kept /home-alice)" -eq 1 ] || fail "the connection was not kept again"

step "port forwarding reaches the service, never the host"
fetch() { docker exec "$CLIENT" python3 -c "import urllib.request,sys; print(urllib.request.urlopen('http://127.0.0.1:'+sys.argv[1]+'/', timeout=20).read().decode().strip())" "$1"; }
wait_port() { for _ in $(seq 1 60); do docker exec "$CLIENT" python3 -c "import socket,sys; socket.create_connection(('127.0.0.1', int(sys.argv[1])), 1)" "$1" 2>/dev/null && return 0; sleep 0.5; done; return 1; }
docker exec -d "${KEYRING[@]}" "$CLIENT" sh -c 'mm --root /project --mm-home /home-alice --runtime docker-compose --environment staging forward api --local 18080 >/tmp/forward.log 2>&1'
wait_port 18080 || fail "the forward never listened"
[ "$(fetch 18080)" = forwarded-hello ] || fail "the forward did not reach the service"
docker exec "$CLIENT" mm --root /project --mm-home /home-alice --environment staging forward save web api:8080 --local 18081 --auto
docker exec -d "${KEYRING[@]}" "$CLIENT" sh -c 'mm --root /project --mm-home /home-alice --runtime docker-compose forward up >/tmp/forward-up.log 2>&1'
wait_port 18081 || fail "the saved forward never listened"
[ "$(fetch 18081)" = forwarded-hello ] || fail "the saved forward did not reach the service"
docker exec -d "${KEYRING[@]}" "$CLIENT" sh -c 'mm --root /project --mm-home /home-alice --runtime docker-compose --environment staging forward escape:80 --local 18082 >/tmp/forward-denied.log 2>&1'
wait_port 18082 || fail "the denied forward never listened"
if fetch 18082 >/dev/null 2>&1; then fail "a forward to an ungranted service connected"; fi
sleep 1
docker exec "$CLIENT" grep -q 'security policy rejected the operation' /tmp/forward-denied.log || fail "the ungranted forward was not a security denial"
docker exec "$CLIENT" pkill -f 'mm --root /project' || true

step "the SSH account grants nothing but the relay"
# With the break-glass key, which sshd accepts: a key of mm's cannot be used
# with plain ssh at all.
SSH_BASE=(-F /dev/null -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o BatchMode=yes -i /keys/ops)
if docker exec "$CLIENT" ssh "${SSH_BASE[@]}" "mm@$TARGET" 'touch /tmp/escape' </dev/null; then fail "arbitrary command ran"; fi
docker exec "$TARGET" test ! -e /tmp/escape || fail "arbitrary command left a file"
if docker exec "$CLIENT" ssh "${SSH_BASE[@]}" -tt "mm@$TARGET" </dev/null; then fail "PTY session opened"; fi
if docker exec "$CLIENT" timeout 5 ssh "${SSH_BASE[@]}" -N -L 127.0.0.1:19090:127.0.0.1:22 "mm@$TARGET"; then
  fail "port forwarding was allowed"
fi
if docker exec "$CLIENT" timeout 5 sh -c "echo ls | sftp -b - ${SSH_BASE[*]} mm@$TARGET" >/dev/null 2>&1; then
  fail "sftp was allowed"
fi
docker exec "$TARGET" sh -c '! id -nG mm | grep -Eq "(^| )(sudo|wheel|docker|podman)( |$)"' || fail "mm account is in a privileged group"

step "a commit cannot silently repoint clients at another daemon"
write_config "{level: deploy, forward: [api]}" "$(printf 'f%.0s' $(seq 64))"
git_repo commit --quiet -am "repoint the target"
publish main
if as_alice team whoami >"$TMP/pin.log" 2>&1; then fail "a changed daemon identity was accepted"; fi
grep -q "mm target trust remote" "$TMP/pin.log" || fail "pin refusal did not point at mm target trust"
git_repo revert --quiet --no-edit HEAD
publish main
as_alice team whoami >/dev/null

step "a feature-branch commit cannot reach staging (deploy-from: [main])"
git_repo checkout --quiet -b feature
echo "# feature" >>"$TMP/repo/compose.yaml"
git_repo commit --quiet -am "feature work"
publish feature
if as_alice deploy plan >"$TMP/feature.log" 2>&1; then fail "a feature commit was planned for staging"; fi
grep -q "not on an allowed branch" "$TMP/feature.log" || fail "feature refusal did not explain the branch rule"
git_repo checkout --quiet main
publish main

step "a deploy cannot take the host: privileged Compose is refused"
cat >>"$TMP/repo/compose.yaml" <<'YAML'
  escape:
    image: busybox:1.36.1
    privileged: true
    pid: host
YAML
git_repo commit --quiet -am "try to escape"
publish main
if as_alice deploy plan >"$TMP/guardrail.log" 2>&1; then fail "a privileged service was planned"; fi
grep -q "refuses Compose features that reach the host" "$TMP/guardrail.log" || fail "guardrail refusal was not explained"
git_repo revert --quiet --no-edit HEAD
publish main

step "a build made in CI deploys from its receipt; the host never builds"
# A registry and a BuildKit builder on the target's engine stand in for CI and
# its registry. The deploy still pulls the image by digest, like any host.
docker exec "$TARGET" sh -c 'docker buildx version >/dev/null 2>&1 || { apt-get update >/dev/null && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends docker-buildx-plugin >/dev/null; }'
docker exec "$TARGET" docker run -d --name registry -p 127.0.0.1:5000:5000 "$REGISTRY_IMAGE" >/dev/null
docker exec "$TARGET" sh -c 'printf "[registry.\"localhost:5000\"]\n  http = true\n" >/tmp/buildkitd.toml
  docker buildx create --name ci --driver docker-container --driver-opt network=host --buildkitd-config /tmp/buildkitd.toml --use >/dev/null'
WEB_MODULE='  web:
    path: web
    build:
      image: localhost:5000/access-live/web
    services:
      web:
        stack: app
'
mkdir -p "$TMP/repo/web"
cat >"$TMP/repo/web/Dockerfile" <<DOCKERFILE
FROM $WORKLOAD_IMAGE
CMD ["sh", "-c", "echo receipt-web-started && exec sleep 3600"]
DOCKERFILE
cat >>"$TMP/repo/compose.yaml" <<'YAML'
  web: {}
YAML
write_config "{level: deploy, forward: [api]}"
git_repo add .
git_repo commit --quiet -m "add a module mm builds"
publish main
BUILT_COMMIT=$(git_repo rev-parse HEAD)
if as_alice deploy plan >"$TMP/no-receipt.log" 2>&1; then fail "a built module was planned without a build"; fi
grep -q "deploy plan --receipt" "$TMP/no-receipt.log" || fail "the refusal did not point at --receipt"

# CI: a clean clone of the commit publishes the image and writes the receipt.
docker exec "$TARGET" sh -c 'rm -rf /ci && mkdir -p /ci/home && git clone --quiet /srv/git/access-live.git /ci/project'
ci() { docker exec "$TARGET" mm --root /ci/project --mm-home /ci/home --environment staging "$@"; }
ci build --publish --yes
ci build receipt /ci/receipt.json
docker cp "$TARGET:/ci/receipt.json" "$TMP/receipt.json"
docker cp "$TMP/receipt.json" "$CLIENT:/receipt.json"
DIGEST=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["result"]["images"][0]["digest"])' "$TMP/receipt.json")

PLAN=$(as_alice --output json deploy plan --receipt /receipt.json | plan_hash)
as_alice deploy apply --plan "$PLAN" --yes
WEB_IMAGE=$(docker exec "$TARGET" sh -c 'docker inspect --format "{{.Config.Image}}" $(docker ps -q --filter label=com.docker.compose.service=web)')
[ "$WEB_IMAGE" = "localhost:5000/access-live/web@$DIGEST" ] || fail "web runs '$WEB_IMAGE', not the receipt's digest"
for _ in $(seq 1 20); do
  as_alice service logs web >"$TMP/web.log" 2>&1 && grep -q receipt-web-started "$TMP/web.log" && break
  sleep 0.5
done
grep -q receipt-web-started "$TMP/web.log" || fail "the deployed build's logs did not reach the client"
docker exec "$TARGET" grep '"result":"planned"' /var/lib/mm/audit.jsonl | grep '"build_receipt"' | grep "$DIGEST" >/dev/null \
  || fail "the plan's audit record lacks the receipt's digest"
# The host recorded the receipt as the commit's build.
as_alice deploy plan >/dev/null

step "the host remembers its deployments: status, and a rollback that needs no checkout"
as_alice deploy status | tee "$TMP/deploy-status.log"
grep -q "runs:     commit ${BUILT_COMMIT:0:12}" "$TMP/deploy-status.log" || fail "deploy status does not show the deployed commit"
grep -q "before:   commit" "$TMP/deploy-status.log" || fail "deploy status does not show the deployment before"
grep -q "No rollout is in progress." "$TMP/deploy-status.log" || fail "deploy status reports a rollout on an idle host"
# Back to the deployment before the build: the host plans that commit itself.
as_alice deploy rollback --yes | tee "$TMP/rollback.log"
grep -q "Rollback: undoes the deployment of commit ${BUILT_COMMIT:0:12}" "$TMP/rollback.log" || fail "the rollback does not say what it undoes"
[ -z "$(docker exec "$TARGET" docker ps -q --filter label=com.docker.compose.service=web)" ] \
  || fail "web, which the earlier commit does not have, still runs after the rollback"
# And forward again: the host kept the build receipt with the deployment.
as_alice deploy rollback --yes >"$TMP/rollforward.log"
WEB_IMAGE=$(docker exec "$TARGET" sh -c 'docker inspect --format "{{.Config.Image}}" $(docker ps -q --filter label=com.docker.compose.service=web)')
[ "$WEB_IMAGE" = "localhost:5000/access-live/web@$DIGEST" ] || fail "after rolling forward web runs '$WEB_IMAGE', not the receipt's digest"
as_alice deploy status >"$TMP/deploy-status-2.log"
grep -q "runs:     commit ${BUILT_COMMIT:0:12}" "$TMP/deploy-status-2.log" || fail "deploy status does not show the build again"

# A damaged receipt, or one built from another commit, is refused.
python3 - "$TMP/receipt.json" "$TMP/tampered.json" <<'PY'
import json, sys
receipt = json.load(open(sys.argv[1]))
receipt["result"]["images"][0]["digest"] = "sha256:" + "0" * 64
json.dump(receipt, open(sys.argv[2], "w"))
PY
docker cp "$TMP/tampered.json" "$CLIENT:/tampered.json"
if as_alice deploy plan --receipt /tampered.json >"$TMP/tampered.log" 2>&1; then fail "a damaged receipt was planned"; fi
grep -q "does not match" "$TMP/tampered.log" || fail "the damaged receipt was not explained"
echo "# later" >>"$TMP/repo/compose.yaml"
git_repo commit --quiet -am "a later commit"
publish main
if as_alice deploy plan --receipt /receipt.json >"$TMP/stale.log" 2>&1; then fail "another commit's receipt was planned"; fi
grep -q "$BUILT_COMMIT" "$TMP/stale.log" || fail "the stale receipt refusal did not name its commit"

step "two environments build two images from one stage, and a receipt keeps to its variant"
# A second project, built in CI only: staging selects a variant of web's
# production stage with its own SITE_URL.
docker exec -i "$TARGET" sh -c 'rm -rf /ci/variants /ci/variants-home && mkdir -p /ci/variants/.config /ci/variants/web /ci/variants-home && cat >/ci/variants/.config/mm.yaml' <<'YAML'
schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de9
  name: variants-live
default-environment: production
target: local
stacks:
  app:
    files: [compose.yaml]
modules:
  web:
    path: web
    services:
      web:
        stack: app
    build:
      image: localhost:5000/variants-live/web
      args:
        SITE_URL: https://example.com
      targets:
        production: ""
        production-staging:
          target: production
          suffix: staging
          args:
            SITE_URL: https://staging.example.com
environments:
  production:
    build-variant: production
  staging:
    build-variant: production-staging
YAML
docker exec -i "$TARGET" sh -c 'cat >/ci/variants/web/Dockerfile' <<DOCKERFILE
FROM $WORKLOAD_IMAGE AS production
ARG SITE_URL
RUN echo "\$SITE_URL" >/site-url
DOCKERFILE
docker exec "$TARGET" sh -c 'cd /ci/variants && printf "services:\n  web:\n    image: localhost:5000/variants-live/web\n" >compose.yaml \
  && git init --quiet && git add . && git -c user.name=ci -c user.email=ci@example.test commit --quiet -m variants'
variants() { docker exec "$TARGET" mm --root /ci/variants --mm-home /ci/variants-home --environment "$@"; }
variants staging build --publish plan >"$TMP/variants-plan.log"
grep -q "target: production-staging (Dockerfile target production)" "$TMP/variants-plan.log" || fail "the staging plan does not name its variant and stage"
grep -q "args: SITE_URL=https://staging.example.com" "$TMP/variants-plan.log" || fail "the staging plan does not list its build arguments"
if grep -q "target: production \[" "$TMP/variants-plan.log"; then fail "staging plans production's variant too"; fi
variants production build --publish --yes >/dev/null
variants staging build --publish --yes >/dev/null
variants production build receipt /ci/production-receipt.json >/dev/null
variants staging build receipt /ci/staging-receipt.json | tee "$TMP/variants-receipt.log"
grep -q "args: SITE_URL=https://staging.example.com" "$TMP/variants-receipt.log" || fail "the receipt does not list its build arguments"
docker cp "$TARGET:/ci/production-receipt.json" "$TMP/production-receipt.json"
docker cp "$TARGET:/ci/staging-receipt.json" "$TMP/staging-receipt.json"
image_of() { python3 -c 'import json,sys; image=json.load(open(sys.argv[1]))["result"]["images"][0]; print(image["repository"] + "@" + image["digest"])' "$1"; }
PRODUCTION_IMAGE=$(image_of "$TMP/production-receipt.json")
STAGING_IMAGE=$(image_of "$TMP/staging-receipt.json")
[ "$PRODUCTION_IMAGE" != "$STAGING_IMAGE" ] || fail "both environments got the same image"
[ "$(docker exec "$TARGET" docker run --rm "$PRODUCTION_IMAGE" cat /site-url)" = https://example.com ] || fail "production's image was not built with its SITE_URL"
[ "$(docker exec "$TARGET" docker run --rm "$STAGING_IMAGE" cat /site-url)" = https://staging.example.com ] || fail "staging's image was not built with its SITE_URL"
python3 -c 'import json,sys; tags=json.load(open(sys.argv[1]))["result"]["images"][0]["tags"]; sys.exit(0 if tags and all(t.endswith("-staging") for t in tags) else 1)' "$TMP/staging-receipt.json" \
  || fail "staging's image is not tagged with its suffix"
if variants production build import /ci/staging-receipt.json >"$TMP/variants-refused.log" 2>&1; then fail "staging's receipt was accepted for production"; fi
grep -q "build variant production here, production-staging in the receipt" "$TMP/variants-refused.log" || fail "the refusal does not name the variants"
grep -q "mm -e production build --publish" "$TMP/variants-refused.log" || fail "the refusal does not name the command that builds for production"

step "an AI agent's key gets what agents: allows, whatever its user or its checkout says"
# Created encrypted: its passphrase goes to the keyring beside alice's.
docker exec "${KEYRING[@]}" "$CLIENT" mm --mm-home /home-alice key create --agent | tee "$TMP/agent-key.log"
grep -q "passphrase in the system keychain" "$TMP/agent-key.log" || fail "the agent key was not created under a passphrase in the keyring"
AGENT_PRIVATE=/home-alice/identities/ssh/agent_ed25519
AGENT_KEY=$(docker exec "$CLIENT" sh -c "cat $AGENT_PRIVATE.pub")
if loads_alone "$AGENT_PRIVATE"; then fail "the agent key loads without a passphrase"; fi
[ "$(keyring_entries)" -eq 2 ] || fail "the keyring should hold a passphrase for each of alice's two keys"
# Alice is an admin; agents may only view staging.
AGENT_KEY_ENTRY=", {key: \"$AGENT_KEY\", agent: true}" AGENTS_BLOCK=$'agents:\n  staging: view\n' write_config admin
git_repo commit --quiet -am "alice is admin; her agent may view staging"
publish main
as_ops team sync | tee "$TMP/agent-plan.log"
grep -q 'alice (agent): none -> view' "$TMP/agent-plan.log" || fail "the access plan did not show the agent on its own line"
AGENT_SYNC=$(as_ops --output json team sync | plan_hash)
as_ops team sync --plan "$AGENT_SYNC" --yes
as_alice --output json team whoami | tee "$TMP/alice-whoami.log"
grep -q '"level": "admin"' "$TMP/alice-whoami.log" || fail "alice should be admin on staging"
as_alice service exec api -- echo alice-ran-this | grep alice-ran-this >/dev/null || fail "an admin could not exec"

# The agent's limits, through the MCP server, which is the only thing that
# opens the agent key: the client speaks JSON-RPC to `mm mcp serve`, and the
# server reaches the host with the agent key only, taking its passphrase
# from the keyring with nobody to prompt.
mcp() {
  docker exec -i "$CLIENT" python3 - "$@" <<'PY'
import json, subprocess, sys
server = subprocess.Popen(
    ["mm", "--root", "/project", "--mm-home", "/home-alice", "mcp", "serve"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
    # A coding tool starts the server inside the agent's environment.
    env={"PATH": "/usr/local/bin:/usr/bin:/bin", "HOME": "/root", "CLAUDECODE": "1",
         "DBUS_SESSION_BUS_ADDRESS": "unix:path=/run/keyring/bus"},
)
def request(identifier, method, params):
    server.stdin.write(json.dumps({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params}) + "\n")
    server.stdin.flush()
    while True:
        message = json.loads(server.stdout.readline())
        if message.get("id") == identifier:
            return message["result"]
request(1, "initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "live", "version": "0"}})
server.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
server.stdin.flush()
result = request(2, "tools/call", {"name": sys.argv[1], "arguments": json.loads(sys.argv[2])})
print(json.dumps(result["structuredContent"]))
server.kill()
PY
}
STAGING='"scope": {"environment": "staging"}'
TRUST=$(as_alice --output json agent trust | python3 -c 'import json,sys; print(json.load(sys.stdin)["policy_hash"])')
as_alice agent trust --plan "$TRUST" --yes >/dev/null
if docker exec "${KEYRING[@]}" -e CLAUDECODE=1 "$CLIENT" mm --root /project --mm-home /home-alice --environment staging service status >"$TMP/guard.log" 2>&1; then
  fail "mm ran directly for an agent"
fi
grep -q mm_access "$TMP/guard.log" || fail "the refusal did not point the agent at the MCP tools"
# Nor does any command that touches a key or a kept connection.
for command in "key show" "key show --agent" "key protect" "key rotate" "key rotate --agent" "key remove --agent" "target connections" "target connections --close" "--environment staging team whoami"; do
  # shellcheck disable=SC2086 # the words of one mm command
  if docker exec "${KEYRING[@]}" -e CLAUDECODE=1 "$CLIENT" mm --root /project --mm-home /home-alice $command >"$TMP/guard-key.log" 2>&1; then
    fail "mm $command ran for an agent"
  fi
  grep -q "mm does not run for an AI agent" "$TMP/guard-key.log" || fail "mm $command was not refused as an agent's"
done
[ "$(keyring_entries)" -eq 2 ] || fail "a refused command changed the keyring"
mcp mm_access '{}' | tee "$TMP/mcp-access.log"
grep -q '"access": "view", "environment": "staging"' "$TMP/mcp-access.log" || fail "mm_access should report view on staging"
mcp mm_logs "{$STAGING, \"service\": \"api\"}" | grep access-live-started >/dev/null || fail "the agent could not read logs through MCP"
mcp mm_exec "{$STAGING, \"service\": \"api\", \"argv\": [\"echo\", \"escaped\"]}" | tee "$TMP/mcp-exec.log"
grep -q MM_AGENT_HANDOFF "$TMP/mcp-exec.log" || fail "exec outside the grant was not handed to the user"
grep -q 'mm --environment staging service exec api -- echo escaped' "$TMP/mcp-exec.log" || fail "the handoff did not carry the user's command"
# The agent rewrites the block in its checkout. Untrusted, it grants nothing.
docker exec "$CLIENT" sed -i 's/^  staging: view$/  staging: {level: deploy, exec: true}/' /project/.config/mm.yaml
mcp mm_exec "{$STAGING, \"service\": \"api\", \"argv\": [\"echo\", \"escaped\"]}" | tee "$TMP/mcp-untrusted.log"
grep -q policy_not_trusted "$TMP/mcp-untrusted.log" || fail "an edited agents block applied without trust"
# Even if the user trusts the edited block, the host still applies the block
# of the commit it deployed.
RETRUST=$(as_alice --output json agent trust | python3 -c 'import json,sys; print(json.load(sys.stdin)["policy_hash"])')
as_alice agent trust --plan "$RETRUST" --yes >/dev/null
mcp mm_exec "{$STAGING, \"service\": \"api\", \"argv\": [\"echo\", \"escaped\"]}" | tee "$TMP/mcp-host.log"
grep -q '"code": "MM_SECURITY"' "$TMP/mcp-host.log" || fail "the host did not refuse an exec its deployed agents block does not allow"
if grep -q escaped "$TMP/mcp-host.log"; then fail "the agent's command ran"; fi
# The MCP server made every one of those calls on a connection of its own:
# the only one kept under this home is the one alice's commands made.
[ "$(kept /home-alice)" -le 1 ] || fail "the MCP server kept a connection made with the agent key"
if docker exec "$CLIENT" sh -c 'grep -l agent_ed25519 /home-alice/run/ssh/*.owner 2>/dev/null | grep -q .'; then
  fail "a kept connection names the agent key"
fi
docker exec "$TARGET" grep -q '"principal":"alice (agent)"' /var/lib/mm/audit.jsonl || fail "the host audit log does not name the agent"

step "the passphrase is in no argument, no environment, and no file on either machine"
# The test keyring hands alice's passphrase to the checks inside the client.
# It is never printed, and it leaves the client only as a pattern for grep
# on the target.
ALICE_FP=$(as_alice --output json key show | python3 -c 'import json,sys; print(json.load(sys.stdin)["fingerprint"])')
docker exec "${KEYRING[@]}" "$CLIENT" sh -c "umask 077; secret-tool lookup service mm-ssh-key fingerprint '$ALICE_FP' >/tmp/passphrase; [ \$(wc -c </tmp/passphrase) -ge 43 ]" \
  || fail "the keyring has no passphrase for alice's key"
# Every process's arguments and environment, for as long as mm connects and
# authenticates over and over. `scan PATTERN_FILE` prints what holds it.
docker exec -i "$CLIENT" sh -c 'cat >/usr/local/bin/scan && chmod 755 /usr/local/bin/scan' <<'SCAN'
#!/bin/sh
for file in /proc/[0-9]*/cmdline /proc/[0-9]*/environ; do
  # A process may end while it is looked at.
  if tr '\0' '\n' 2>/dev/null <"$file" | grep -qF -f "$1"; then echo "$file"; fi
done
SCAN
# The scan finds a value that is there: a canary in a process's environment.
docker exec "$CLIENT" sh -c 'echo canary-7f3a9c51d2 >/tmp/canary; CANARY=canary-7f3a9c51d2 sleep 3 & scan /tmp/canary | grep -q environ' \
  || fail "the scan of process arguments and environments finds nothing at all"
docker exec "$CLIENT" rm -f /tmp/connections-done
docker exec "$CLIENT" rm -f /tmp/connections.log
docker exec -d "${KEYRING[@]}" -e MM_SSH_REUSE=off "$CLIENT" sh -c 'for _ in 1 2 3 4 5 6 7 8; do mm --root /project --mm-home /home-alice --runtime docker-compose --environment staging team whoami >/dev/null 2>&1 && echo authenticated >>/tmp/connections.log; done; touch /tmp/connections-done'
docker exec "$CLIENT" sh -c 'while [ ! -e /tmp/connections-done ]; do scan /tmp/passphrase; done' >"$TMP/leaks.log"
[ ! -s "$TMP/leaks.log" ] || fail "the passphrase was in a process's arguments or environment: $(sort -u "$TMP/leaks.log" | head -3)"
[ "$(docker exec "$CLIENT" grep -c authenticated /tmp/connections.log)" -eq 8 ] || fail "the connections made during the scan did not all authenticate"
# Every file mm wrote on the client, and on the host everything mmd keeps:
# plans, receipts, the audit log, its home, and the daemons' logs.
docker exec "$CLIENT" sh -c 'grep -rlF -f /tmp/passphrase /home-alice /home-ops /project /root /tmp 2>/dev/null | grep -v "^/tmp/passphrase$"' >"$TMP/client-files.log" || true
[ ! -s "$TMP/client-files.log" ] || fail "the passphrase is in a file on the client: $(head -3 "$TMP/client-files.log")"
docker exec "$CLIENT" cat /tmp/passphrase | docker exec -i "$TARGET" sh -c 'umask 077; cat >/tmp/pattern; grep -rlF -f /tmp/pattern /var/lib/mm /var/lib/mm-home /srv/mm /etc/mm /etc/ssh /tmp/mmd.log /tmp/sshd.log 2>/dev/null; rm -f /tmp/pattern' >"$TMP/host-files.log" || true
[ ! -s "$TMP/host-files.log" ] || fail "the passphrase reached the host: $(head -3 "$TMP/host-files.log")"
docker exec "$TARGET" test -s /var/lib/mm/audit.jsonl || fail "the host has no audit log to have looked in"
docker exec "$CLIENT" rm -f /tmp/passphrase /tmp/canary

step "timing: what a command waits for, with and without a kept connection"
# Milliseconds for `mm team whoami`, five times. Both machines are containers
# on one bridge, so this is the cost of the work, not of a network.
wall() {
  docker exec "${KEYRING[@]}" "$@" "$CLIENT" sh -c 'for _ in 1 2 3 4 5; do s=$(date +%s%N); mm --root /project --mm-home /home-alice --runtime docker-compose --environment staging team whoami >/dev/null 2>&1 || printf "failed:"; e=$(date +%s%N); printf "%s " $(( (e - s) / 1000000 )); done'
}
as_alice team whoami >/dev/null
echo "protected key, kept connection (ms):            $(wall)"
echo "protected key, new connection every time (ms):  $(wall -e MM_SSH_REUSE=off)"
echo "plain key before it was protected, new connection every time (ms): $PLAIN_TIMING"
docker exec "${KEYRING[@]}" -e MM_SSH_REUSE=off -e MM_TIMING=1 "$CLIENT" mm --root /project --mm-home /home-alice --runtime docker-compose --environment staging team whoami 2>&1 >/dev/null | grep -E ' (key\.unlock|ssh\.connect|rpc)( |$)' || true
as_alice team whoami >/dev/null
traced | grep -E ' (key\.unlock|ssh\.connect|rpc)( |$)' || true

step "a lost keyring entry is reported, and a new key recovers"
as_alice team whoami >/dev/null
[ "$(kept /home-alice)" -eq 1 ] || fail "alice has no kept connection before her keyring entry goes"
docker exec "${KEYRING[@]}" "$CLIENT" secret-tool clear service mm-ssh-key fingerprint "$ALICE_FP"
# A connection kept from before authenticated while the passphrase was
# there, and carries requests until it is closed.
as_alice team whoami >/dev/null || fail "the kept connection stopped carrying requests when the keyring entry went"
as_alice target connections --close >/dev/null
if as_alice team whoami >"$TMP/lost.log" 2>&1 </dev/null; then fail "alice connected without her key's passphrase"; fi
grep -q "its passphrase is not in the system keychain" "$TMP/lost.log" || fail "the missing passphrase was not named"
grep -q '`mm key rotate`' "$TMP/lost.log" || fail "the missing passphrase did not point at mm key rotate"
as_alice key show | grep "passphrase missing from the system keychain" >/dev/null || fail "mm key show does not say the passphrase is missing"
as_alice --output json doctor >"$TMP/doctor-lost.json" 2>/dev/null || true
grep -q MM_KEY_PASSPHRASE_MISSING "$TMP/doctor-lost.json" || fail "mm doctor did not report the missing passphrase"
as_alice key rotate | tee "$TMP/rotate-plan.log"
grep -q "until an admin registers the new public key" "$TMP/rotate-plan.log" || fail "the review of a rotation did not say access is lost"
ROTATE=$(as_alice --output json key rotate | python3 -c 'import json,sys; print(json.load(sys.stdin)["plan_hash"])')
as_alice key rotate --plan "$ROTATE" --yes >"$TMP/rotate.log"
NEW_ALICE_KEY=$(docker exec "$CLIENT" sh -c "cat $ALICE_PRIVATE.pub")
[ "$NEW_ALICE_KEY" != "$ALICE_KEY" ] || fail "rotating did not make a new key"
if loads_alone "$ALICE_PRIVATE"; then fail "the new key loads without a passphrase"; fi
[ "$(keyring_entries)" -eq 2 ] || fail "after rotating, the keyring should hold the new key's passphrase and the agent key's"
# The host has never seen the new key.
if as_alice team whoami >"$TMP/unregistered.log" 2>&1; then fail "the host accepted a key nobody registered"; fi
grep -q "refused this device's key" "$TMP/unregistered.log" || fail "the refusal of the new key was not explained"
# Re-enrol: the new public key replaces the old one in the users file.
ALICE_KEY=$NEW_ALICE_KEY
AGENT_KEY_ENTRY=", {key: \"$AGENT_KEY\", agent: true}" AGENTS_BLOCK=$'agents:\n  staging: view\n' write_config admin
git_repo commit --quiet -am "alice's new key"
publish main
REENROL=$(as_ops --output json team sync | plan_hash)
as_ops team sync --plan "$REENROL" --yes
as_alice --output json team whoami | grep '"level": "admin"' >/dev/null || fail "alice has no access with her new key after it was registered"

step "removing the agent key takes its passphrase with it"
REMOVE=$(docker exec "${KEYRING[@]}" "$CLIENT" mm --mm-home /home-alice --output json key remove --agent | python3 -c 'import json,sys; print(json.load(sys.stdin)["plan_hash"])')
docker exec "${KEYRING[@]}" "$CLIENT" mm --mm-home /home-alice key remove --agent --plan "$REMOVE" --yes
docker exec "$CLIENT" sh -c "test ! -e $AGENT_PRIVATE && test ! -e $AGENT_PRIVATE.pub" || fail "the agent key files were not deleted"
[ "$(keyring_entries)" -eq 1 ] || fail "the agent key's passphrase stayed in the keyring"

step "a machine without a keyring keeps the passphrase in a file, and says so"
# No session bus is given to this command, as on a server or in CI.
docker exec "$CLIENT" mm --mm-home /home-headless key create | tee "$TMP/headless.log"
grep -q "passphrase in a file beside the key" "$TMP/headless.log" || fail "the headless key does not say where its passphrase is"
grep -q "does not stop a program that reads both files" "$TMP/headless.log" || fail "mm did not say what a passphrase file is worth"
docker exec "$CLIENT" stat -c %a /home-headless/identities/ssh/id_ed25519.passphrase | grep -x 600 >/dev/null || fail "the passphrase file is open to others"
if loads_alone /home-headless/identities/ssh/id_ed25519; then fail "the headless key loads without a passphrase"; fi
[ "$(keyring_entries)" -eq 1 ] || fail "a command without the session bus reached the keyring"

step "removing the developer from the repo revokes access at the next sync"
# Her connection is still open when her access goes: the host refuses the
# next request on it all the same.
as_alice team whoami >/dev/null
[ "$(kept /home-alice)" -eq 1 ] || fail "alice has no kept connection to revoke behind"
write_config
git_repo commit --quiet -am "remove alice"
publish main
REVOKE=$(as_ops --output json team sync | plan_hash)
as_ops team sync --plan "$REVOKE" --yes
if as_alice team whoami >"$TMP/revoked.log" 2>&1; then fail "alice kept access after removal"; fi
docker exec "$TARGET" sh -c 'grep -c "^restrict," /etc/ssh/mm_authorized_keys' | grep -x 1 >/dev/null \
  || fail "alice's key should leave authorized_keys"
docker exec "$TARGET" grep -q '"result":"denied"' /var/lib/mm/audit.jsonl || true

echo
echo "live remote access: relay-only account, repo-declared users and targets, key pinning, git-fetched deploys, port forwarding, kept connections, deploy-from, guardrails, CI build receipts, build variants with their own arguments, agent keys and MCP limits, encrypted keys with passphrases in a keyring (migration, direct use refused, rotation, recovery, removal), revocation: pass"
