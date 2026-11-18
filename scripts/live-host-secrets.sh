#!/usr/bin/env bash
# Live host secrets: a remote host reads an environment's secrets from an
# OpenBao it reaches itself, with an AppRole only the host holds. The client
# never sees a value. One secret reaches a service as an environment variable
# and one as a mounted file. Rotating either in the store and deploying again
# replaces the service that uses it and leaves the others running, and a
# deployment that fails returns to the file it mounted before.
#
# Requires Docker and openssl. The target container runs privileged so it can
# host its own Docker engine. The OpenBao server is a throwaway container on
# the test network with no published port, initialized here with a root token
# that exists only for this run. Set MM_LIVE_PREBUILT_DIR to a directory
# holding Debian bookworm builds of `mm` and `mmd` to skip the build.
#
# Set MM_LIVE_ENGINE=podman to run the host on rootful Podman (Debian 13,
# the node image of scripts/live-cluster.sh), where every service has user
# ids of its own: the file secret is then the worker's user 1000's, mode
# 0400, and its 0600 config file its root's.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TMP=$(mktemp -d)
NETWORK="mm-host-secrets-$RANDOM-$RANDOM"
TARGET="${NETWORK}-target"
CLIENT="${NETWORK}-client"
BAO="${NETWORK}-bao"
PROJECT_ID=019c527b-6242-7b43-bfd4-bd60e27c1de5
WORKLOAD_IMAGE=${MM_LIVE_WORKLOAD_IMAGE:-busybox:1.36.1}
OPENBAO_IMAGE=${MM_LIVE_OPENBAO_IMAGE:-ghcr.io/openbao/openbao@sha256:595c83b42614a4d2b044608e4593c05b019c5db25bc9c185d8fff3ac96c03ddd}
ENGINE=${MM_LIVE_ENGINE:-docker}
# What the worker's file secret asks for: on Docker Engine the only owner and
# mode it takes, on Podman user 1000 of the worker's own ids, for it only.
case $ENGINE in
  docker) RUNTIME=docker-compose ENGINE_DATA=/var/lib/docker PIDS=512 SECRET_OWNER='"0"' SECRET_MODE=292 READER=0:0 ;;
  # Podman's vfs storage copies an image for each service's own ids.
  podman) RUNTIME=podman-compose ENGINE_DATA=/var/lib/containers PIDS=2048 SECRET_OWNER='"1000"' SECRET_MODE=256 READER=1000:1000
    MM_LIVE_TARGET_IMAGE=${MM_LIVE_TARGET_IMAGE:-mm-live-cluster-node:trixie-podman} ;;
  *) echo "MM_LIVE_ENGINE is docker or podman, not '$ENGINE'" >&2; exit 2 ;;
esac

cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    docker exec "$TARGET" sh -c 'tail -n 50 /tmp/mmd.log /tmp/sshd.log 2>/dev/null' >&2 || true
    docker logs --tail 30 "$BAO" >&2 2>/dev/null || true
  fi
  # With its anonymous volume: the target keeps its engine's data in one.
  docker rm -f -v "$CLIENT" "$TARGET" "$BAO" >/dev/null 2>&1 || true
  docker network rm "$NETWORK" >/dev/null 2>&1 || true
  rm -rf "$TMP"
  return "$status"
}
trap cleanup EXIT

step() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
json() { python3 -I -c "import json,sys; value=json.load(sys.stdin); print($1)"; }
# A value nobody chose, for a secret, a token pattern, or a probe.
random_value() { head -c 24 /dev/urandom | base64 | tr -d '/+=\n'; }
digest() { sha256sum | cut -d' ' -f1; }

mkdir -p "$TMP/build" "$TMP/keys" "$TMP/repo" "$TMP/bao"
chmod 700 "$TMP"
if [ -n "${MM_LIVE_PREBUILT_DIR:-}" ]; then
  BIN_DIR=$MM_LIVE_PREBUILT_DIR
else
  step "building mm and mmd in the resource-limited Rust image"
  docker run --rm --cpus=1 --cpu-shares=256 --memory=4g --pids-limit=256 \
    --user "$(id -u):$(id -g)" -e HOME=/tmp -e CARGO_HOME=/tmp/cargo \
    -e CARGO_TARGET_DIR=/build -e CARGO_BUILD_JOBS=1 -e CARGO_PROFILE_DEV_DEBUG=0 \
    -v "$ROOT:/workspace:ro" -v "$TMP/build:/build" -w /workspace \
    mm-rust:1.98.0 cargo build --locked --all-features --bin mm --bin mmd
  BIN_DIR="$TMP/build/debug"
fi

step "starting OpenBao with a certificate of its own, on the test network only"
openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj "/CN=mm host secrets live CA" \
  -keyout "$TMP/bao/ca-key.pem" -out "$TMP/bao/ca.pem" >/dev/null 2>&1
openssl req -newkey rsa:2048 -nodes -subj "/CN=$BAO" \
  -keyout "$TMP/bao/server-key.pem" -out "$TMP/bao/server.csr" >/dev/null 2>&1
printf 'subjectAltName=DNS:%s,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n' "$BAO" >"$TMP/bao/server.ext"
openssl x509 -req -days 1 -in "$TMP/bao/server.csr" -CA "$TMP/bao/ca.pem" -CAkey "$TMP/bao/ca-key.pem" \
  -CAcreateserial -extfile "$TMP/bao/server.ext" -out "$TMP/bao/server.pem" >/dev/null 2>&1
cat >"$TMP/bao/config.hcl" <<EOF
ui = false
disable_mlock = true
api_addr = "https://$BAO:8200"
storage "inmem" {}
listener "tcp" {
  address = "0.0.0.0:8200"
  tls_cert_file = "/config/server.pem"
  tls_key_file = "/config/server-key.pem"
}
EOF
docker network create "$NETWORK" >/dev/null
docker create --name "$BAO" --network "$NETWORK" --user 0:0 \
  --cpus=0.5 --cpu-shares=256 --memory=256m --pids-limit=128 \
  --entrypoint bao "$OPENBAO_IMAGE" server -config=/config/config.hcl >/dev/null
docker cp "$TMP/bao/." "$BAO:/config"
docker start "$BAO" >/dev/null
# The store's own command line, inside its container. The root token reaches
# it through the environment of `docker exec`, never through an argument.
bao() { docker exec -i -e BAO_ADDR=https://127.0.0.1:8200 -e BAO_CACERT=/config/ca.pem -e BAO_TOKEN "$BAO" bao "$@"; }
for _ in $(seq 1 60); do
  bao status >/dev/null 2>&1 && break
  # Sealed and not initialized answers with status 2 once it listens.
  [ "$(bao status -format=json 2>/dev/null | json 'value["initialized"]' 2>/dev/null || true)" = False ] && break
  sleep 0.5
done
INIT=$(bao operator init -key-shares=1 -key-threshold=1 -format=json)
bao operator unseal "$(printf '%s' "$INIT" | json 'value["unseal_keys_b64"][0]')" >/dev/null
BAO_TOKEN=$(printf '%s' "$INIT" | json 'value["root_token"]')
export BAO_TOKEN
unset INIT
bao secrets enable -path=mm-kv -version=2 kv >/dev/null
bao auth enable -path=mm-approle approle >/dev/null
bao policy write mm-host-staging - >/dev/null <<POLICY
path "mm-kv/data/mm/$PROJECT_ID/staging/*"     { capabilities = ["read"] }
path "mm-kv/metadata/mm/$PROJECT_ID/staging/*" { capabilities = ["read"] }
POLICY
bao write auth/mm-approle/role/mm-host-staging \
  token_policies=mm-host-staging token_ttl=15m token_max_ttl=30m >/dev/null
FIRST=$(random_value)
SECOND=$(random_value)
# store_secret LOCATION VALUE: a new version of the secret in the store.
store_secret() { printf '%s' "$2" | bao kv put -mount=mm-kv "mm/$PROJECT_ID/staging/$1" value=- >/dev/null; }
store_secret database/password "$FIRST"
TOKEN_FIRST=$(random_value)
TOKEN_SECOND=$(random_value)
TOKEN_THIRD=$(random_value)
store_secret api/token "$TOKEN_FIRST"

step "starting the target (sshd, mmd, $ENGINE) and the client"
docker run -d --privileged --name "$TARGET" --hostname remote-target --network "$NETWORK" \
  --volume "$ENGINE_DATA" \
  --cpus=1 --cpu-shares=256 --memory=1g --pids-limit="$PIDS" "${MM_LIVE_TARGET_IMAGE:-debian:bookworm-slim}" sleep infinity >/dev/null
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
  DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends docker-ce docker-ce-cli containerd.io docker-compose-plugin >/dev/null
  rm -rf /var/lib/apt/lists/*'
[ -n "${MM_LIVE_CLIENT_IMAGE:-}" ] || docker exec "$CLIENT" sh -c 'apt-get update >/dev/null && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends openssh-client git ca-certificates python3 >/dev/null && rm -rf /var/lib/apt/lists/* && git config --global --add safe.directory "*"'
if [ "$ENGINE" = podman ]; then
  # As scripts/live-cluster.sh prepares a Podman node: a host of its own for
  # Podman's control groups and storage, then what `mm target setup` writes
  # (userns auto for other containers, the range of the services' own ids).
  # Podman's API socket is what Docker Compose talks to, and with no
  # systemd a loop runs the health checks.
  docker exec "$TARGET" sh -c 'set -e
mkdir -p /sys/fs/cgroup/init /run/podman /etc/containers/containers.conf.d
xargs -rn1 </sys/fs/cgroup/cgroup.procs >/sys/fs/cgroup/init/cgroup.procs 2>/dev/null || true
sed -e "s/ / +/g" -e "s/^/+/" </sys/fs/cgroup/cgroup.controllers >/sys/fs/cgroup/cgroup.subtree_control 2>/dev/null || true
printf "[storage]\ndriver = \"vfs\"\nrunroot = \"/run/containers/storage\"\ngraphroot = \"/var/lib/containers/storage\"\n" >/etc/containers/storage.conf
printf "[containers]\nlog_driver = \"k8s-file\"\n[engine]\ncgroup_manager = \"cgroupfs\"\nevents_logger = \"file\"\n" >/etc/containers/containers.conf.d/90-live.conf
printf "[containers]\nuserns = \"auto\"\n" >/etc/containers/containers.conf.d/50-mm.conf
for file in /etc/subuid /etc/subgid; do
  printf "containers:2147483647:1073741825\nmm-services:3221225472:1073676288\n" >>"$file"
done'
  docker exec -d "$TARGET" sh -c 'podman system service --time=0 unix:///run/podman/podman.sock >/tmp/podman.log 2>&1'
  docker exec -d "$TARGET" sh -c 'while :; do for c in $(podman ps -q); do podman healthcheck run "$c" >/dev/null 2>&1 || true; done; sleep 1; done'
else
  # The inner engine cannot stack overlayfs on the container's own root.
  docker exec -d "$TARGET" sh -c 'dockerd --storage-driver=vfs >/tmp/dockerd.log 2>&1'
fi
# `engine` is the target's engine command for what this script does there.
docker exec "$TARGET" sh -c "ln -sf \"\$(command -v $ENGINE)\" /usr/local/bin/engine"
for _ in $(seq 1 60); do
  docker exec "$TARGET" engine info >/dev/null 2>&1 && break
  sleep 1
done
docker exec "$TARGET" engine info >/dev/null 2>&1 || fail "target $ENGINE engine did not start"
if [ "$ENGINE" = podman ]; then
  for _ in $(seq 1 30); do docker exec "$TARGET" test -S /run/podman/podman.sock && break; sleep 1; done
  docker exec "$TARGET" test -S /run/podman/podman.sock || fail "Podman's API socket did not open"
fi
if docker image inspect "$WORKLOAD_IMAGE" >/dev/null 2>&1; then
  docker save "$WORKLOAD_IMAGE" | docker exec -i "$TARGET" engine load >/dev/null
else
  docker exec "$TARGET" engine pull -q "$WORKLOAD_IMAGE" >/dev/null
fi

docker cp "$BIN_DIR/mm" "$TARGET:/usr/bin/mm"
docker cp "$BIN_DIR/mmd" "$TARGET:/usr/libexec/mmd"
docker cp "$BIN_DIR/mm" "$CLIENT:/usr/local/bin/mm"
docker cp "$ROOT/packaging/mm-relay-shell" "$TARGET:/usr/libexec/mm-relay-shell"
docker cp "$ROOT/packaging/sshd/90-mm-relay.conf" "$TARGET:/etc/ssh/sshd_config.d/90-mm-relay.conf"
docker exec "$TARGET" sh -c '
  set -e
  chmod 755 /usr/bin/mm /usr/libexec/mmd /usr/libexec/mm-relay-shell
  getent group mm >/dev/null || groupadd --system --gid 991 mm
  getent passwd mm >/dev/null || useradd --system --uid 991 --gid 991 --home-dir /nonexistent --no-create-home --shell /usr/libexec/mm-relay-shell mm
  grep -qx /usr/libexec/mm-relay-shell /etc/shells || echo /usr/libexec/mm-relay-shell >> /etc/shells
  mkdir -p /run/sshd /run/mm /etc/mm /var/lib/mm/ssh /var/lib/mm-home /srv/mm/projects /srv/git
  chown root:991 /run/mm && chmod 750 /run/mm
  chmod 700 /etc/mm
  sshd -t'

step "writing the host policy and the project"
ssh-keygen -q -t ed25519 -N '' -C ops@offline -f "$TMP/keys/ops"
OPS_KEY=$(cut -d' ' -f1,2 "$TMP/keys/ops.pub")
cat >"$TMP/host.yaml" <<YAML
schema-version: 2
break-glass:
  - name: ops
    key: $OPS_KEY
projects:
  $PROJECT_ID:
    name: host-secrets-live
    origin: /srv/git/host-secrets-live.git
    environments: [staging]
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
docker cp "$TMP/host.yaml" "$TARGET:/etc/mm/host.yaml"
docker cp "$TMP/mmd.json" "$TARGET:/etc/mm/mmd.json"
docker cp "$TMP/bao/ca.pem" "$TARGET:/etc/mm/bao-ca.pem"
docker exec "$TARGET" sh -c 'chown root:root /etc/mm/* && chmod 600 /etc/mm/*'
docker exec "$TARGET" /usr/libexec/mmd access validate
DAEMON_FP=$(docker exec "$TARGET" /usr/libexec/mmd --print-identity)
HOST_KEY=$(docker exec "$TARGET" sh -c 'cut -d" " -f1,2 /etc/ssh/ssh_host_ed25519_key.pub')

mkdir -p "$TMP/repo/.config" "$TMP/repo/api" "$TMP/repo/web"
touch "$TMP/repo/web/.keep"
cp "$TMP/bao/ca.pem" "$TMP/repo/bao-ca.pem"
printf 'DB_PASSWORD= # mm:from=DB.PASSWORD required\n' >"$TMP/repo/api/.env.example"
mkdir -p "$TMP/repo/worker"
# A module with a secret file has an environment template, here with a
# plain value. Its config file is for its root only.
printf 'WORKER_NAME=worker\n' >"$TMP/repo/worker/.env.example"
printf 'worker settings\n' >"$TMP/repo/worker/worker.conf"
# write_compose CHECK: the Compose file, with the command that says whether
# the worker is healthy.
write_compose() {
  cat >"$TMP/repo/compose.yaml" <<YAML
services:
  api:
    image: $WORKLOAD_IMAGE
    command: ["sh", "-c", "echo api-started && exec tail -f /dev/null"]
  web:
    image: $WORKLOAD_IMAGE
    command: ["sh", "-c", "echo web-started && exec tail -f /dev/null"]
  worker:
    image: $WORKLOAD_IMAGE
    command: ["sh", "-c", "echo worker-started && exec tail -f /dev/null"]
    healthcheck:
      test: ["CMD", "$1"]
      interval: 1s
      timeout: 1s
      retries: 2
YAML
}
write_compose true
cat >"$TMP/repo/.config/mm.yaml" <<YAML
schema-version: 1
default-environment: staging
project:
  id: $PROJECT_ID
  name: host-secrets-live
target: local
runtime:
  driver: $RUNTIME
stacks:
  app:
    project-name: host-secrets-live-{environment}-{stack}
    files: [compose.yaml]
modules:
  api:
    path: api
    env:
      template: .env.example
    services:
      api:
        stack: app
  web:
    path: web
    services:
      web:
        stack: app
  worker:
    path: worker
    env:
      template: .env.example
    services:
      worker:
        stack: app
        secret-files:
          api-token:
            secret: api.token
            uid: $SECRET_OWNER
            gid: $SECRET_OWNER
            mode: $SECRET_MODE
        config-files:
          /etc/worker.conf: {file: worker.conf, mode: 0o600}
providers:
  team-secrets:
    driver: openbao
    scope: project
    mode: external
    address: https://bao.example.test:8200
    storage-mount: mm-kv
    automation-auth-mount: mm-approle
    trust-bundle: bao-ca.pem
    expected-trust-sha256: sha256:$(digest <"$TMP/bao/ca.pem")
secrets:
  database.password:
    storage:
      provider: team-secrets
      location: database/password
  api.token:
    storage:
      provider: team-secrets
      location: api/token
targets:
  remote:
    host: $TARGET
    host-key: $HOST_KEY
    daemon-fingerprint: $DAEMON_FP
    providers:
      team-secrets: {address: "https://$BAO:8200"}
environments:
  staging:
    target: remote
    deploy-from: [main]
    bindings:
      DB.PASSWORD:
        secret: database.password
YAML
git_repo() { git -C "$TMP/repo" -c user.name=live -c user.email=live@example.test "$@"; }
git_repo init --quiet --initial-branch=main
git_repo add .
git_repo commit --quiet -m "the host reads the database password from the store"
git clone --quiet --bare "$TMP/repo" "$TMP/origin.git"
# publish: the commit reaches the origin and the client's checkout.
publish() {
  git -C "$TMP/repo" push --quiet "$TMP/origin.git" main
  # Stand-in for a git host: the origin is a root-owned bare repository.
  docker exec "$TARGET" rm -rf /srv/git/host-secrets-live.git
  docker cp "$TMP/origin.git" "$TARGET:/srv/git/host-secrets-live.git"
  docker exec "$TARGET" chown -R root:root /srv/git/host-secrets-live.git
  docker exec "$CLIENT" rm -rf /project
  docker cp "$TMP/repo" "$CLIENT:/project"
}
docker exec "$CLIENT" sh -c 'mkdir -p /home-ops && git config --global --add safe.directory "*"'
publish

docker exec -d "$TARGET" sh -c '/usr/libexec/mmd >/tmp/mmd.log 2>&1'
docker exec -d "$TARGET" sh -c '/usr/sbin/sshd -D -e >/tmp/sshd.log 2>&1'
for _ in $(seq 1 30); do
  docker exec "$TARGET" test -S /run/mm/mm.sock && break
  sleep 0.5
done

# The break-glass key is this script's own. Nothing here uses a keychain.
OPS_PRIVATE=$(cat "$TMP/keys/ops")
as_ops() { docker exec -e MM_SSH_KEY="$OPS_PRIVATE" -e MM_KEY_VAULT=file "$CLIENT" mm --root /project --mm-home /home-ops --runtime "$RUNTIME" --environment staging "$@"; }
plan_hash() { json 'value["binding"]["plan_hash"]'; }
container_of() { docker exec "$TARGET" engine ps --quiet --no-trunc --filter "label=com.docker.compose.service=$1"; }
# The digest of what the service's container holds as DB_PASSWORD; the value
# itself is never printed.
delivered() {
  docker exec "$TARGET" sh -c "engine inspect --format '{{range .Config.Env}}{{println .}}{{end}}' $(container_of "$1") | sed -n 's/^DB_PASSWORD=//p' | tr -d '\n' | sha256sum | cut -d' ' -f1"
}
# The digest of the file the worker's container mounts as its token, read
# as the user the secret file is for.
mounted() {
  docker exec "$TARGET" sh -c "engine exec --user $READER $(container_of worker) cat /run/secrets/api-token | sha256sum | cut -d' ' -f1"
}
# in_worker COMMAND: what COMMAND prints in the worker's container.
in_worker() { docker exec "$TARGET" engine exec "$(container_of worker)" sh -c "$1"; }
# The files the host keeps for file secrets, one per revision.
REVISIONS=/var/lib/mm-home/run/runtime-secrets-files
revisions() { docker exec "$TARGET" sh -c "find $REVISIONS -type f -name 'mm-*' | wc -l"; }
deploy() {
  as_ops --output json deploy plan >"$TMP/plan-$1.json"
  as_ops deploy apply --plan "$(plan_hash <"$TMP/plan-$1.json")" --yes >"$TMP/apply-$1.log" 2>&1 \
    || { cat "$TMP/apply-$1.log" >&2; fail "deployment '$1' failed"; }
}

step "a host with no connection to the store refuses the plan and names the command"
as_ops team whoami >/dev/null
if as_ops --output json deploy plan >"$TMP/refused.json" 2>"$TMP/refused.log"; then
  fail "the host planned without a connection to the store"
fi
grep -q "mmd secrets connect --project $PROJECT_ID --environment staging --provider team-secrets" "$TMP/refused.log" "$TMP/refused.json" \
  || { cat "$TMP/refused.log" >&2; fail "the refusal did not name the command to run on the host"; }

step "a wrong secret ID is refused and nothing is stored"
ROLE_ID=$(bao read -field=role_id auth/mm-approle/role/mm-host-staging/role-id)
SECRET_ID=$(bao write -f -field=secret_id auth/mm-approle/role/mm-host-staging/secret-id)
connect() {
  docker exec -i "$TARGET" /usr/libexec/mmd secrets connect --project "$PROJECT_ID" --environment staging \
    --provider team-secrets --address "https://$BAO:8200" --ca-file /etc/mm/bao-ca.pem
}
if printf '%s\n%s\n' "$ROLE_ID" "$(random_value)" | connect >"$TMP/denied.json" 2>"$TMP/denied.log"; then
  fail "the host stored a credential the store refuses"
fi
removed() { docker exec "$TARGET" /usr/libexec/mmd secrets disconnect --project "$PROJECT_ID" --provider team-secrets | json 'value["removed"]'; }
[ "$(removed)" = False ] || fail "a refused connection was kept"

step "the host connects with its own AppRole"
printf '%s\n%s\n' "$ROLE_ID" "$SECRET_ID" | connect >"$TMP/connect.json"
[ "$(json 'value["authenticated"]' <"$TMP/connect.json")" = True ] || fail "the host did not log in"
[ "$(docker exec "$TARGET" stat -c %a /var/lib/mm-home/secrets/openbao-connections.age)" = 600 ] \
  || fail "the connection store is not for root only"

step "the deployment delivers the value on the host"
deploy first
API=$(container_of api)
WEB=$(container_of web)
WORKER=$(container_of worker)
[ -n "$API" ] && [ -n "$WEB" ] && [ -n "$WORKER" ] || fail "the services are not running"
if [ "$ENGINE" = podman ]; then
  # The worker's own ids: its root is OWN on the host, its user 1000 is
  # OWN + 1000. The file secret is user 1000's alone, bind-mounted as the
  # host keeps it, and the config file its root's.
  OWN=$(in_worker 'cat /proc/self/uid_map' | awk '$1 == 0 && $3 == 65536 {print $2}')
  [ "${OWN:-0}" -ge 3221225472 ] || fail "worker has no user ids of its own from mm's range: $(in_worker 'cat /proc/self/uid_map')"
  [ -z "$(docker exec "$TARGET" sh -c "find $REVISIONS -type d ! -perm 700; find $REVISIONS -type f -name 'mm-*' \\( ! -perm 400 -o ! -uid $((OWN + 1000)) -o ! -gid $((OWN + 1000)) \\)")" ] \
    || fail "the secret file is not worker's user 1000's alone ($((OWN + 1000))), in directories for root only: $(docker exec "$TARGET" find "$REVISIONS" -printf '%m %U:%G %p\n' | tr '\n' ';') plan: $(grep -o '"first_host_ids":{[^}]*}' "$TMP/plan-first.json" || echo 'no first_host_ids')"
  [ "$(in_worker 'stat -c "%u %g %a" /run/secrets/api-token /etc/worker.conf' | tr '\n' ' ')" = "1000 1000 400 0 0 600 " ] \
    || fail "worker sees $(in_worker 'stat -c "%n %u %g %a" /run/secrets/api-token /etc/worker.conf; grep -e secrets -e worker.conf /proc/mounts' | tr '\n' ' ')"
  if docker exec "$TARGET" engine exec --user 1001 "$(container_of worker)" cat /run/secrets/api-token >/dev/null 2>&1; then
    fail "another user of the worker reads the token"
  fi
fi
if [ "$ENGINE" = docker ]; then
  [ -z "$(docker exec "$TARGET" sh -c "find $REVISIONS \\( -type f ! -perm 600 \\) -o \\( -type d ! -perm 700 \\)")" ] \
    || fail "a secret revision file or its directory is not for root only"
fi
[ "$(delivered api)" = "$(printf '%s' "$FIRST" | digest)" ] || fail "api did not receive the stored value"
[ "$(delivered web)" = "$(printf '' | digest)" ] || fail "web received a secret it does not use"
[ "$(mounted)" = "$(printf '%s' "$TOKEN_FIRST" | digest)" ] || fail "worker does not mount the stored token"
[ "$(revisions)" -eq 1 ] || fail "the host should keep one file for the one revision of the token"
[ "$(in_worker 'cat /etc/worker.conf')" = "worker settings" ] || fail "worker cannot read its 0600 config file"

step "deploying again with nothing rotated replaces nothing"
deploy unchanged
[ "$(container_of api)" = "$API" ] && [ "$(container_of web)" = "$WEB" ] && [ "$(container_of worker)" = "$WORKER" ] \
  || fail "a deployment with the same secret versions replaced a container"

step "a secret rotated in the store rolls only the service that uses it"
store_secret database/password "$SECOND"
deploy rotated
[ "$(container_of web)" = "$WEB" ] || fail "rotating the secret replaced web, which does not use it"
[ "$(container_of worker)" = "$WORKER" ] || fail "rotating the secret replaced worker, which does not use it"
[ "$(container_of api)" != "$API" ] || fail "rotating the secret did not replace api"
[ "$(delivered api)" = "$(printf '%s' "$SECOND" | digest)" ] || fail "api did not receive the rotated value"
API=$(container_of api)

step "a file secret rotated in the store rolls only the service that mounts it"
store_secret api/token "$TOKEN_SECOND"
deploy file-rotated
[ "$(container_of web)" = "$WEB" ] || fail "rotating the file secret replaced web, which does not mount it"
[ "$(container_of api)" = "$API" ] || fail "rotating the file secret replaced api, which does not mount it"
[ "$(container_of worker)" != "$WORKER" ] || fail "rotating the file secret did not replace worker"
[ "$(mounted)" = "$(printf '%s' "$TOKEN_SECOND" | digest)" ] || fail "worker does not mount the rotated token"
[ "$(revisions)" -eq 1 ] || fail "the superseded revision was not removed after the deployment succeeded"
WORKER=$(container_of worker)
deploy file-unchanged
[ "$(container_of worker)" = "$WORKER" ] || fail "a deployment with the same token version replaced worker"

step "a deployment that fails returns to the revision the host ran"
store_secret api/token "$TOKEN_THIRD"
write_compose false
git_repo commit --quiet -am "a worker that never becomes healthy"
publish
as_ops --output json deploy plan >"$TMP/plan-broken.json"
if as_ops deploy apply --plan "$(plan_hash <"$TMP/plan-broken.json")" --yes >"$TMP/apply-broken.log" 2>&1; then
  fail "a deployment whose worker is unhealthy succeeded"
fi
[ -n "$(container_of worker)" ] || { cat "$TMP/apply-broken.log" >&2; fail "the worker is not running after the rollback"; }
[ "$(mounted)" = "$(printf '%s' "$TOKEN_SECOND" | digest)" ] || fail "the rollback did not mount the revision that ran before"
[ "$(container_of api)" = "$API" ] && [ "$(container_of web)" = "$WEB" ] \
  || fail "the failed deployment replaced a service it did not change"
write_compose true
git_repo commit --quiet -am "a worker that is healthy"
publish
deploy file-third
[ "$(mounted)" = "$(printf '%s' "$TOKEN_THIRD" | digest)" ] || fail "worker does not mount the third token"
[ "$(revisions)" -eq 1 ] || fail "revisions the host no longer runs were not removed"

step "no value and no credential is in what mm wrote or printed"
printf '%s\n' "$FIRST" "$SECOND" "$TOKEN_FIRST" "$TOKEN_SECOND" "$TOKEN_THIRD" "$ROLE_ID" "$SECRET_ID" >"$TMP/patterns"
chmod 600 "$TMP/patterns"
# The scan finds a value that is there.
printf 'canary %s\n' "$FIRST" >"$TMP/canary.log"
grep -qFf "$TMP/patterns" "$TMP/canary.log" || fail "the scan does not find a value that is present"
rm "$TMP/canary.log"
LEAKED=$(grep -lFf "$TMP/patterns" "$TMP"/*.json "$TMP"/*.log || true)
[ -z "$LEAKED" ] || fail "a value or a credential reached the client's output: $LEAKED"
docker cp "$TMP/patterns" "$TARGET:/root/patterns"
# The one place a value is kept is the file a container mounts, and no name
# under it holds a value either.
LEAKED=$(docker exec "$TARGET" sh -c 'grep -rlFf /root/patterns --exclude-dir=runtime-secrets-files /var/lib/mm /var/lib/mm-home /srv/mm /run/mm /etc/mm /tmp/mmd.log 2>/dev/null || true')
NAMED=$(docker exec "$TARGET" sh -c "find $REVISIONS | grep -Ff /root/patterns || true")
docker exec "$TARGET" rm /root/patterns
[ -z "$LEAKED" ] || fail "a value or a credential is in a file mm keeps on the host: $LEAKED"
[ -z "$NAMED" ] || fail "a file name holds a value"

step "a forgotten connection is refused again"
[ "$(removed)" = True ] || fail "the connection was not removed"
if as_ops --output json deploy plan >"$TMP/after.json" 2>"$TMP/after.log"; then
  fail "the host planned after its connection was removed"
fi

printf '\nPASS: the host resolved, delivered and rotated secrets from the store\n'
