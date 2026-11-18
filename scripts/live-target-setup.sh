#!/usr/bin/env bash
# Live `mm target setup` with the remote access layer: mm and Ansible set up a
# systemd Debian host over SSH as a sudo admin, then the break-glass key
# reaches mmd through the relay account that setup created.
#
# Requires Docker. The target container runs privileged so systemd can run
# the sandboxed mmd.service. Set MM_LIVE_PREBUILT_DIR to a directory holding
# Debian bookworm builds of `mm` and `mmd` to skip the build. The project's
# bootstrap playbook is a stub, so no container runtime is installed.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TMP=$(mktemp -d)
NETWORK="mm-target-setup-$RANDOM-$RANDOM"
TARGET="${NETWORK}-target"
CLIENT="${NETWORK}-client"
PROJECT_ID=019c527b-6242-7b43-bfd4-bd60e27c1de5
# GitHub's published ed25519 host key; setup only pins it, nothing fetches.
ORIGIN_HOST_KEY='ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl'

cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    docker exec "$TARGET" sh -c 'journalctl -u mmd -u ssh --no-pager -n 60' >&2 || true
  fi
  if [ -n "${MM_LIVE_KEEP:-}" ]; then
    echo "kept $TARGET and $CLIENT (MM_LIVE_KEEP); remove them and network $NETWORK when done" >&2
    return "$status"
  fi
  docker rm -f "$CLIENT" "$TARGET" >/dev/null 2>&1 || true
  docker network rm "$NETWORK" >/dev/null 2>&1 || true
  rm -rf "$TMP"
  return "$status"
}
trap cleanup EXIT

step() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

mkdir -p "$TMP/build" "$TMP/release" "$TMP/keys" "$TMP/project/.config/ansible"
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
# Setup copies both binaries to the host; stripped copies keep that quick.
docker run --rm --user "$(id -u):$(id -g)" -v "$BIN_DIR:/in:ro" -v "$TMP/release:/out" \
  mm-rust:1.99.1 sh -c 'strip -o /out/mm /in/mm && strip -o /out/mmd /in/mmd'

step "building the host (systemd, sshd, a sudo admin) and the client (Ansible)"
docker build -q -t mm-live-setup-target:bookworm - >/dev/null <<'DOCKERFILE'
FROM debian:bookworm-slim
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      systemd systemd-sysv openssh-server sudo python3 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --create-home --shell /bin/bash admin \
 && echo 'admin ALL=(ALL) NOPASSWD:ALL' >/etc/sudoers.d/admin \
 && chmod 0440 /etc/sudoers.d/admin
CMD ["/sbin/init"]
DOCKERFILE
docker build -q -t mm-live-setup-client:2.18.6 - >/dev/null <<'DOCKERFILE'
FROM python:3.13-slim
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends openssh-client git \
 && rm -rf /var/lib/apt/lists/* \
 && pip install --no-cache-dir ansible-core==2.18.6
DOCKERFILE
docker network create "$NETWORK" >/dev/null
docker run -d --privileged --cgroupns=private --name "$TARGET" --hostname setup-target \
  --network "$NETWORK" --cpus=1 --cpu-shares=256 --memory=1g --pids-limit=512 \
  mm-live-setup-target:bookworm >/dev/null
docker run -d --name "$CLIENT" --network "$NETWORK" \
  --cpus=0.5 --cpu-shares=256 --memory=768m --pids-limit=256 \
  mm-live-setup-client:2.18.6 sleep infinity >/dev/null
for _ in $(seq 1 60); do
  state=$(docker exec "$TARGET" systemctl is-system-running 2>/dev/null || true)
  case $state in running|degraded) break ;; esac
  sleep 1
done
docker exec "$TARGET" systemctl is-active --quiet ssh.service || fail "the host's sshd did not start"

step "keys: the admin's login key and an offline break-glass key"
ssh-keygen -q -t ed25519 -N '' -C admin@laptop -f "$TMP/keys/admin"
ssh-keygen -q -t ed25519 -N '' -C ops@offline -f "$TMP/keys/ops"
docker cp "$TMP/keys/admin.pub" "$TARGET:/tmp/admin.pub"
docker exec "$TARGET" sh -c 'install -d -m 0700 -o admin -g admin ~admin/.ssh &&
  install -m 0600 -o admin -g admin /tmp/admin.pub ~admin/.ssh/authorized_keys'
docker cp "$TMP/keys" "$CLIENT:/keys"
docker cp "$TMP/release" "$CLIENT:/release"
docker cp "$TMP/release/mm" "$CLIENT:/usr/local/bin/mm"
docker exec "$CLIENT" sh -c 'chown -R root:root /keys /release && chmod 600 /keys/admin /keys/ops && ssh-agent -a /tmp/agent.sock >/dev/null && SSH_AUTH_SOCK=/tmp/agent.sock ssh-add -q /keys/admin'

step "a project whose bootstrap playbook installs nothing"
write_config() {
  cat >"$TMP/project/.config/mm.yaml" <<YAML
schema-version: 1
default-environment: production
project:
  id: $PROJECT_ID
  name: setup-live
target: local
runtime:
  driver: docker-compose
stacks:
  app:
    files: [compose.yaml]
environments:
  local: {}
  production:
    target: ${1:-local}
YAML
  if [ -n "${2:-}" ]; then
    printf '%s\n' "$2" >>"$TMP/project/.config/mm.yaml"
  fi
  docker exec "$CLIENT" rm -rf /project
  docker cp "$TMP/project" "$CLIENT:/project"
  # mm refuses a project the current user (root here) does not own.
  docker exec "$CLIENT" chown -R root:root /project
}
printf 'services: {}\n' >"$TMP/project/compose.yaml"
cp "$ROOT/core/templates/ansible/ansible.cfg" "$TMP/project/.config/ansible/ansible.cfg"
printf -- '---\n- name: Stub bootstrap\n  hosts: mm_targets\n  tasks: []\n' \
  >"$TMP/project/.config/ansible/bootstrap.yml"
write_config

as_admin() {
  docker exec -e SSH_AUTH_SOCK=/tmp/agent.sock "$CLIENT" \
    mm --root /project --mm-home /home-admin --environment production "$@"
}
FINGERPRINT=$(docker exec "$CLIENT" sh -c "ssh-keyscan -t ed25519 $TARGET 2>/dev/null | ssh-keygen -lf - | cut -d' ' -f2")
as_admin target add setup-target --host "$TARGET" --user admin --host-key-fingerprint "$FINGERPRINT" >/dev/null
OPS_PUBLIC=$(cat "$TMP/keys/ops.pub")
ACCESS=(--origin git@github.com:acme/setup-live.git --origin-host-key "$ORIGIN_HOST_KEY"
  --break-glass "ops=$OPS_PUBLIC" --host-binaries /release)

setup() {
  local plan
  plan=$(as_admin --output json --target setup-target --runtime docker-compose \
    target setup plan "${ACCESS[@]}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["plan_hash"])')
  as_admin --output json --target setup-target --runtime docker-compose \
    target setup apply --plan "$plan" --yes "${ACCESS[@]}"
}

step "setup installs and verifies the access layer"
setup | tee "$TMP/setup.json"
python3 - "$TMP/setup.json" "$OPS_PUBLIC" <<'PY' || fail "setup did not report the host facts"
import json, sys
result = json.load(open(sys.argv[1]))
access = result["access"]
assert "remote_access" in result["verified_capabilities"], result
assert access["host-key"].startswith("ssh-ed25519 "), access
assert len(access["daemon-fingerprint"]) == 64, access
assert access["deploy-key"].startswith("ssh-ed25519 "), access
assert result["next_command"] == "mm --environment production team sync", result
PY
HOST_KEY=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["access"]["host-key"])' "$TMP/setup.json")
DAEMON_FP=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["access"]["daemon-fingerprint"])' "$TMP/setup.json")
[ "$HOST_KEY" = "$(docker exec "$TARGET" cut -d' ' -f1,2 /etc/ssh/ssh_host_ed25519_key.pub)" ] \
  || fail "the reported host key is not the host's"

step "the host matches the plan"
docker exec "$TARGET" systemctl is-active --quiet mmd.service || fail "mmd is not running under systemd"
[ "$(docker exec "$TARGET" id -Gn mm)" = mm-relay ] || fail "the mm account has extra groups"
[ "$(docker exec "$TARGET" getent passwd mm | cut -d: -f7)" = /usr/libexec/mm-relay-shell ] \
  || fail "the mm account's shell is not the relay"
docker exec "$TARGET" /usr/libexec/mmd access show | tee "$TMP/show.json"
grep -q '"ops"' "$TMP/show.json" || fail "the break-glass key is not in the host policy"
grep -q '"origin": "git@github.com:acme/setup-live.git"' "$TMP/show.json" || fail "the origin is not enrolled"
docker exec "$TARGET" grep -q "origin-host-key: $ORIGIN_HOST_KEY" /etc/mm/host.yaml \
  || fail "the origin host key is not pinned"
docker exec "$TARGET" test -f "/var/lib/mm/projects/$PROJECT_ID/deploy-key" || fail "no deploy key"
docker exec "$TARGET" sh -c 'grep -c "^restrict," /etc/ssh/mm_authorized_keys' | grep -qx 1 \
  || fail "authorized_keys should hold only the break-glass key"

step "the break-glass key reaches mmd through the relay, and nothing else"
write_config remote "targets:
  remote:
    host: $TARGET
    host-key: $HOST_KEY
    daemon-fingerprint: $DAEMON_FP"
OPS_PRIVATE=$(cat "$TMP/keys/ops")
docker exec -e MM_SSH_KEY="$OPS_PRIVATE" "$CLIENT" \
  mm --root /project --mm-home /home-ops --runtime docker-compose --environment production team whoami \
  | tee "$TMP/whoami.log"
grep -q ops "$TMP/whoami.log" || fail "whoami did not identify the break-glass key"
SSH_OPS=(-F /dev/null -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o BatchMode=yes -i /keys/ops)
if docker exec "$CLIENT" ssh "${SSH_OPS[@]}" "mm@$TARGET" 'touch /tmp/escape' </dev/null; then
  fail "the relay account ran a command"
fi
docker exec "$TARGET" test ! -e /tmp/escape || fail "the relay account left a file"

step "a second setup changes nothing and keeps the daemon identity"
write_config
setup >"$TMP/again.json"
[ "$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["access"]["daemon-fingerprint"])' "$TMP/again.json")" = "$DAEMON_FP" ] \
  || fail "the daemon identity changed"
docker exec "$TARGET" sh -c "grep -c 'name: ops' /etc/mm/host.yaml" | grep -qx 1 \
  || fail "the break-glass key was added twice"

step "a reconcile from the repo rewrites the relay's keys under systemd"
# Stand-in for a git host: a root-owned bare repository on the host.
ACCESS=(--origin /srv/git/setup-live.git --break-glass "ops=$OPS_PUBLIC" --host-binaries /release)
setup >/dev/null
docker exec "$CLIENT" mm --mm-home /home-alice key create >/dev/null
ALICE_KEY=$(docker exec "$CLIENT" cut -d' ' -f1,2 /home-alice/identities/ssh/id_ed25519.pub)
write_config remote "targets:
  remote:
    host: $TARGET
    host-key: $HOST_KEY
    daemon-fingerprint: $DAEMON_FP
users:
  alice:
    keys: [\"$ALICE_KEY\"]
    access:
      production: view"
git -C "$TMP/project" init --quiet --initial-branch=main
git -C "$TMP/project" add .
git -C "$TMP/project" -c user.name=live -c user.email=live@example.test commit --quiet -m "alice may view production"
git clone --quiet --bare "$TMP/project" "$TMP/origin.git"
docker exec "$TARGET" mkdir -p /srv/git
docker cp "$TMP/origin.git" "$TARGET:/srv/git/setup-live.git"
docker exec "$TARGET" chown -R root:root /srv/git/setup-live.git
docker exec "$CLIENT" rm -rf /project
docker cp "$TMP/project" "$CLIENT:/project"
docker exec "$CLIENT" chown -R root:root /project
as_ops() {
  docker exec -e MM_SSH_KEY="$OPS_PRIVATE" "$CLIENT" \
    mm --root /project --mm-home /home-ops --runtime docker-compose --environment production "$@"
}
SYNC=$(as_ops --output json team sync | python3 -c 'import json,sys; print(json.load(sys.stdin)["binding"]["plan_hash"])')
as_ops team sync --plan "$SYNC" --yes
docker exec "$TARGET" sh -c 'grep -c "^restrict," /etc/ssh/mm_authorized_keys' | grep -qx 2 \
  || fail "mmd did not add alice's key to authorized_keys"
docker exec "$CLIENT" mm --root /project --mm-home /home-alice --runtime docker-compose --environment production team whoami \
  | tee "$TMP/alice.log"
grep -q '"level":"view"' "$TMP/alice.log" || fail "alice does not have view on production"

step "live target setup passed"
