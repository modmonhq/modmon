#!/usr/bin/env bash
set -euo pipefail

mode=${1:-all}
case "$mode" in external|managed|all) ;; *) printf 'usage: %s external|managed|all\n' "$0" >&2; exit 2 ;; esac
runtime=${2:-docker-compose}
case "$runtime" in
    docker-compose) engine=docker ;;
    podman-compose) engine=podman ;;
    *) printf 'usage: %s external|managed|all [docker-compose|podman-compose]\n' "$0" >&2; exit 2 ;;
esac
provider=${3:-vault}
case "$provider" in
    vault)
        product=Vault
        binary=vault
        image_old='docker.io/hashicorp/vault@sha256:750bb37c1638fa194ab37053a81618c61bb0491ddec6fccac87c07a8e6cd8166'
        image_new='docker.io/hashicorp/vault@sha256:c4298db7f9b2ea8cab452cbff5877749087913aa035fcae62026cf16132929f5'
        version_old=1.18.5
        version_new=1.19.5
        container_root=/vault
        ;;
    openbao)
        product=OpenBao
        binary=bao
        image_old='ghcr.io/openbao/openbao@sha256:afec9f65a7067a22acf24b29f07d2f028353c02cc0fa9c80dfa4e7fab158b373'
        image_new='ghcr.io/openbao/openbao@sha256:595c83b42614a4d2b044608e4593c05b019c5db25bc9c185d8fff3ac96c03ddd'
        version_old=2.3
        version_new=2.4
        container_root=/openbao
        ;;
    *) printf 'usage: %s external|managed|all [docker-compose|podman-compose] [vault|openbao]\n' "$0" >&2; exit 2 ;;
esac

docker() { command "$engine" "$@"; }

compose() {
    if [[ $runtime == docker-compose ]]; then
        command docker compose "$@"
    else
        command podman compose "$@"
    fi
}

run_id="mm-${provider}-live-$$"
network="$run_id"
tmp=$(mktemp -d "/tmp/$run_id.XXXXXX")
containers=()
volumes=()
podman_service_pid=

cleanup() {
    local status=$?
    if (( status != 0 )); then
        for container in "${containers[@]}"; do docker logs "$container" >&2 2>/dev/null || true; done
    fi
    for container in "${containers[@]}"; do docker rm --force "$container" >/dev/null 2>&1 || true; done
    for volume in "${volumes[@]}"; do docker volume rm "$volume" >/dev/null 2>&1 || true; done
    docker network rm "$network" >/dev/null 2>&1 || true
    if [[ -n $podman_service_pid ]]; then kill "$podman_service_pid" >/dev/null 2>&1 || true; fi
    rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

if [[ $runtime == podman-compose && ! -S ${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/podman/podman.sock ]]; then
    podman system service --time=0 >/dev/null 2>&1 &
    podman_service_pid=$!
    for _ in $(seq 1 40); do
        [[ -S ${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/podman/podman.sock ]] && break
        sleep 0.1
    done
fi

docker network create --label io.mm.test=vault-provider "$network" >/dev/null

make_tls() {
    local directory=$1
    mkdir -p "$directory"
    openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj "/CN=mm $product live CA" -keyout "$directory/ca-key.pem" -out "$directory/ca.pem" >/dev/null 2>&1
    openssl req -newkey rsa:2048 -nodes -subj '/CN=127.0.0.1' -keyout "$directory/server-key.pem" -out "$directory/server.csr" >/dev/null 2>&1
    printf 'subjectAltName=IP:127.0.0.1,DNS:localhost\nextendedKeyUsage=serverAuth\n' >"$directory/server.ext"
    openssl x509 -req -days 1 -in "$directory/server.csr" -CA "$directory/ca.pem" -CAkey "$directory/ca-key.pem" -CAcreateserial -extfile "$directory/server.ext" -out "$directory/server.pem" >/dev/null 2>&1
    chmod 644 "$directory/ca.pem" "$directory/server.pem"
    chmod 600 "$directory/server-key.pem"
}

write_config() {
    local directory=$1 node=$2
    cat >"$directory/config.hcl" <<EOF
ui = false
disable_mlock = true
api_addr = "https://127.0.0.1:8200"
cluster_addr = "https://127.0.0.1:8201"
storage "raft" { path = "$container_root/file" node_id = "$node" }
listener "tcp" {
  address = "0.0.0.0:8200"
  cluster_address = "0.0.0.0:8201"
  tls_cert_file = "$container_root/config/server.pem"
  tls_key_file = "$container_root/config/server-key.pem"
  tls_min_version = "tls13"
}
EOF
    chmod 600 "$directory/config.hcl"
}

prepare_volumes() {
    local volume=$1 config_volume=$2 image=$3 source=$4
    docker volume create --label io.mm.test=vault-provider "$volume" >/dev/null
    docker volume create --label io.mm.test=vault-provider "$config_volume" >/dev/null
    volumes+=("$volume" "$config_volume")
    docker run --rm --network none --user 0:0 --cpus=0.25 --memory=64m --pids-limit=32 --read-only --cap-drop=ALL --cap-add=CHOWN --cap-add=DAC_OVERRIDE --security-opt no-new-privileges:true --entrypoint /bin/sh --volume "$volume:/data" --volume "$config_volume:/config" --volume "$source:/source:ro,Z" "$image" -c 'cp /source/config.hcl /source/ca.pem /source/server.pem /source/server-key.pem /config/ && chmod 700 /data /config && chmod 600 /config/config.hcl /config/server-key.pem && chmod 644 /config/ca.pem /config/server.pem && chown -R 100:1000 /data /config'
}

start_vault() {
    local name=$1 image=$2 port=$3 volume=$4 config_volume=$5
    containers+=("$name")
    local compose_file="$tmp/$name.compose.yaml"
    cat >"$compose_file" <<EOF
services:
  vault:
    image: "$image"
    container_name: "$name"
    user: "100:1000"
    entrypoint: ["$binary"]
    command: ["server", "-config=$container_root/config/config.hcl"]
    restart: unless-stopped
    read_only: true
    cap_drop: ["ALL"]
    security_opt: ["no-new-privileges:true"]
    cpus: 2
    mem_limit: 4g
    pids_limit: 512
    tmpfs: ["/tmp:rw,noexec,nosuid,nodev,size=64m"]
    ports: ["127.0.0.1:$port:8200"]
    volumes:
      - "config:$container_root/config:ro"
      - "data:$container_root/file"
    networks: ["provider"]
    labels: {io.mm.test: vault-provider}
volumes:
  data: {external: true, name: "$volume"}
  config: {external: true, name: "$config_volume"}
networks:
  provider: {external: true, name: "$network"}
EOF
    compose --project-name "$name" --file "$compose_file" up --detach >/dev/null
}

wait_health() {
    local port=$1 ca=$2
    for _ in $(seq 1 80); do
        if curl --silent --show-error --cacert "$ca" --output /dev/null "https://127.0.0.1:$port/v1/sys/health"; then return; fi
        sleep 0.25
    done
    return 1
}

initialize() {
    local port=$1 ca=$2 output=$3
    for _ in $(seq 1 20); do
        if curl --silent --show-error --fail --cacert "$ca" --request POST --data '{"secret_shares":1,"secret_threshold":1}' "https://127.0.0.1:$port/v1/sys/init" >"$output"; then return; fi
        sleep 0.25
    done
    return 1
}

unseal() {
    local port=$1 ca=$2 key=$3
    curl --silent --show-error --fail --cacert "$ca" --request POST --data "{\"key\":\"$key\"}" "https://127.0.0.1:$port/v1/sys/unseal" >/dev/null
}

api() {
    local method=$1 port=$2 ca=$3 token=$4 path=$5 data=${6-}
    local args=(--silent --show-error --fail --cacert "$ca" --header "X-Vault-Token: $token" --request "$method")
    if [[ -n $data ]]; then args+=(--data "$data"); fi
    curl "${args[@]}" "https://127.0.0.1:$port/v1/$path"
}

inspect_security() {
    local name=$1
    local inspection
    inspection=$(docker inspect "$name")
    if [[ $runtime == docker-compose ]]; then
        jq -e '.[0].Config.User == "100:1000" and .[0].HostConfig.ReadonlyRootfs == true and .[0].HostConfig.CapDrop == ["ALL"] and (.[0].HostConfig.SecurityOpt | index("no-new-privileges:true")) != null and .[0].HostConfig.PidsLimit == 512 and .[0].HostConfig.Memory == 4294967296 and .[0].HostConfig.NanoCpus == 2000000000 and .[0].HostConfig.RestartPolicy.Name == "unless-stopped" and .[0].HostConfig.PortBindings["8200/tcp"][0].HostIp == "127.0.0.1"' <<<"$inspection" >/dev/null
    else
        jq -e '.[0].Config.User == "100:1000" and .[0].HostConfig.ReadonlyRootfs == true and ((.[0].HostConfig.CapAdd // []) | length) == 0 and (.[0].HostConfig.CapDrop | length) == 11 and (.[0].HostConfig.SecurityOpt | index("no-new-privileges")) != null and .[0].HostConfig.PidsLimit == 512 and .[0].HostConfig.Memory == 4294967296 and .[0].HostConfig.RestartPolicy.Name == "unless-stopped" and .[0].HostConfig.PortBindings["8200/tcp"][0].HostIp == "127.0.0.1"' <<<"$inspection" >/dev/null || {
            jq '.[0] | {User: .Config.User, ReadonlyRootfs: .HostConfig.ReadonlyRootfs, CapDrop: .HostConfig.CapDrop, SecurityOpt: .HostConfig.SecurityOpt, PidsLimit: .HostConfig.PidsLimit, Memory: .HostConfig.Memory, RestartPolicy: .HostConfig.RestartPolicy, PortBindings: .HostConfig.PortBindings}' <<<"$inspection" >&2
            return 1
        }
    fi
}

external_case() {
    local line=$1 image=$2 port=$3
    local name="$run_id-external-${line//./-}" volume="$run_id-external-${line//./-}" config_volume="$run_id-external-config-${line//./-}" config="$tmp/external-${line//./-}"
    make_tls "$config"; write_config "$config" "$name"; prepare_volumes "$volume" "$config_volume" "$image" "$config"; start_vault "$name" "$image" "$port" "$volume" "$config_volume"; wait_health "$port" "$config/ca.pem"
    initialize "$port" "$config/ca.pem" "$tmp/init.json"
    local root key
    root=$(jq -r .root_token "$tmp/init.json"); key=$(jq -r .keys_base64[0] "$tmp/init.json"); unseal "$port" "$config/ca.pem" "$key"
    api POST "$port" "$config/ca.pem" "$root" sys/mounts/mm-kv '{"type":"kv","options":{"version":"2"}}' >/dev/null
    api POST "$port" "$config/ca.pem" "$root" sys/auth/mm-approle '{"type":"approle"}' >/dev/null
    api PUT "$port" "$config/ca.pem" "$root" sys/policies/acl/mm-live '{"policy":"path \"mm-kv/data/mm/project-a/integration/database/password\" { capabilities = [\"create\",\"read\",\"update\"] }\npath \"mm-kv/metadata/mm/project-a/integration/database/password\" { capabilities = [\"read\"] }"}' >/dev/null
    api POST "$port" "$config/ca.pem" "$root" auth/mm-approle/role/mm-live '{"token_policies":["mm-live"],"token_ttl":"15m","token_max_ttl":"30m","token_type":"service"}' >/dev/null
    local role secret token sentinel="vault-live-sentinel-$line"
    role=$(api GET "$port" "$config/ca.pem" "$root" auth/mm-approle/role/mm-live/role-id | jq -r .data.role_id)
    secret=$(api POST "$port" "$config/ca.pem" "$root" auth/mm-approle/role/mm-live/secret-id '{}' | jq -r .data.secret_id)
    token=$(curl --silent --show-error --fail --cacert "$config/ca.pem" --request POST --data "{\"role_id\":\"$role\",\"secret_id\":\"$secret\"}" "https://127.0.0.1:$port/v1/auth/mm-approle/login" | jq -r .auth.client_token)
    api POST "$port" "$config/ca.pem" "$token" mm-kv/data/mm/project-a/integration/database/password "{\"options\":{\"cas\":0},\"data\":{\"value\":\"$sentinel\"}}" >/dev/null
    test "$(api GET "$port" "$config/ca.pem" "$token" mm-kv/data/mm/project-a/integration/database/password | jq -r .data.data.value)" = "$sentinel"
    api POST "$port" "$config/ca.pem" "$token" mm-kv/data/mm/project-a/integration/database/password '{"options":{"cas":1},"data":{"value":"rotated"}}' >/dev/null
    if api POST "$port" "$config/ca.pem" "$token" mm-kv/data/mm/project-a/integration/database/password '{"options":{"cas":1},"data":{"value":"stale"}}' >/dev/null 2>&1; then return 1; fi
    if api GET "$port" "$config/ca.pem" "$token" mm-kv/data/mm/project-b/integration/database/password >/dev/null 2>&1; then return 1; fi
    if api GET "$port" "$config/ca.pem" "$token" mm-kv/data/mm/project-a/uat/database/password >/dev/null 2>&1; then return 1; fi
    make_tls "$tmp/wrong-ca"
    if curl --silent --cacert "$tmp/wrong-ca/ca.pem" "https://127.0.0.1:$port/v1/sys/health" >/dev/null 2>&1; then return 1; fi
    inspect_security "$name"
    if docker inspect "$name" | grep -F "$sentinel" >/dev/null; then return 1; fi
    if docker logs "$name" 2>&1 | grep -F "$sentinel" >/dev/null; then return 1; fi
    printf 'LIVE external %s %s (%s): TLS+pin PASS; AppRole PASS; KVv2 CAS conflict PASS; project/env sibling deny PASS; hardening PASS; sentinel PASS\n' "$product" "$line" "$runtime"
    docker rm --force "$name" >/dev/null; docker volume rm "$volume" "$config_volume" >/dev/null
}

managed_case() {
    local port=19220 name="$run_id-managed" volume="$run_id-managed" config_volume="$run_id-managed-config" config="$tmp/managed"
    make_tls "$config"; write_config "$config" "$name"; prepare_volumes "$volume" "$config_volume" "$image_old" "$config"; start_vault "$name" "$image_old" "$port" "$volume" "$config_volume"; wait_health "$port" "$config/ca.pem"
    initialize "$port" "$config/ca.pem" "$tmp/managed-init.json"
    local root key sentinel='vault-managed-live-sentinel-4f00'
    root=$(jq -r .root_token "$tmp/managed-init.json"); key=$(jq -r .keys_base64[0] "$tmp/managed-init.json"); unseal "$port" "$config/ca.pem" "$key"
    api POST "$port" "$config/ca.pem" "$root" sys/mounts/mm-kv '{"type":"kv","options":{"version":"2"}}' >/dev/null
    api POST "$port" "$config/ca.pem" "$root" mm-kv/data/mm/project-a/integration/value "{\"options\":{\"cas\":0},\"data\":{\"value\":\"$sentinel\"}}" >/dev/null
    api GET "$port" "$config/ca.pem" "$root" sys/storage/raft/snapshot >"$tmp/snapshot.snap"
    api POST "$port" "$config/ca.pem" "$root" sys/seal >/dev/null; unseal "$port" "$config/ca.pem" "$key"
    docker restart "$name" >/dev/null; wait_health "$port" "$config/ca.pem"; unseal "$port" "$config/ca.pem" "$key"
    docker rm --force "$name" >/dev/null; start_vault "$name" "$image_new" "$port" "$volume" "$config_volume"; wait_health "$port" "$config/ca.pem"; unseal "$port" "$config/ca.pem" "$key"
    test "$(api GET "$port" "$config/ca.pem" "$root" mm-kv/data/mm/project-a/integration/value | jq -r .data.data.value)" = "$sentinel"
    inspect_security "$name"
    local fresh="$run_id-restored" fresh_volume="$run_id-restored" fresh_config_volume="$run_id-restored-config" fresh_port=19221 fresh_config="$tmp/restored"
    make_tls "$fresh_config"; write_config "$fresh_config" "$fresh"; prepare_volumes "$fresh_volume" "$fresh_config_volume" "$image_new" "$fresh_config"; start_vault "$fresh" "$image_new" "$fresh_port" "$fresh_volume" "$fresh_config_volume"; wait_health "$fresh_port" "$fresh_config/ca.pem"
    initialize "$fresh_port" "$fresh_config/ca.pem" "$tmp/fresh-init.json"; local fresh_root fresh_key
    fresh_root=$(jq -r .root_token "$tmp/fresh-init.json"); fresh_key=$(jq -r .keys_base64[0] "$tmp/fresh-init.json"); unseal "$fresh_port" "$fresh_config/ca.pem" "$fresh_key"
    curl --silent --show-error --fail --cacert "$fresh_config/ca.pem" --header "X-Vault-Token: $fresh_root" --request POST --data-binary "@$tmp/snapshot.snap" "https://127.0.0.1:$fresh_port/v1/sys/storage/raft/snapshot-force" >/dev/null
    sleep 1; unseal "$fresh_port" "$fresh_config/ca.pem" "$key"
    test "$(api GET "$fresh_port" "$fresh_config/ca.pem" "$root" mm-kv/data/mm/project-a/integration/value | jq -r .data.data.value)" = "$sentinel"
    inspect_security "$fresh"
    if docker inspect "$name" "$fresh" | grep -F "$sentinel" >/dev/null; then return 1; fi
    if docker logs "$name" 2>&1 | grep -F "$sentinel" >/dev/null; then return 1; fi
    if grep -R -F "$sentinel" "$config" "$fresh_config" >/dev/null; then return 1; fi
    printf 'LIVE managed %s %s->%s (%s): init/restart/seal/unseal PASS; Raft persistence PASS; upgrade PASS; snapshot fresh-restore PASS; hardening PASS; sentinel PASS\n' "$product" "$version_old" "$version_new" "$runtime"
}

if [[ $mode == external || $mode == all ]]; then external_case "$version_old" "$image_old" 19218; external_case "$version_new" "$image_new" 19219; fi
if [[ $mode == managed || $mode == all ]]; then managed_case; fi
printf '%s live resources cleaned by trap: %s\n' "$product" "$run_id"
