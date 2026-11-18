#!/usr/bin/env bash
# Checks the result of `mm target setup apply` on a cluster target's real
# hosts: every node runs Docker Engine with Compose and mmd, mmd owns the
# WireGuard interface of the private mesh and restores it at boot, the mesh
# connects every pair of nodes, the storage node serves NFSv4.2 on the mesh
# only, every other node can mount it, and every node holds the rules that
# keep containers away from the storage port. It reads the nodes and changes
# nothing but a scratch file in the storage root.
#
# Usage:
#   MM_CLUSTER_NODES='app-1=203.0.113.11=10.90.0.1 app-2=203.0.113.12=10.90.0.2' \
#   MM_CLUSTER_SSH_KEY=~/.ssh/id_ed25519 scripts/live-cluster-setup.sh
#
# MM_CLUSTER_NODES         NAME=HOST=PRIVATE_ADDRESS for every node, in the
#                          target's order, separated by spaces.
# MM_CLUSTER_SSH_KEY       private key of an account that may become root.
# MM_CLUSTER_SSH_USER      that account (default: root).
# MM_CLUSTER_KNOWN_HOSTS   known_hosts file to verify the nodes against
#                          (default: ~/.ssh/known_hosts).
# MM_CLUSTER_STORAGE       name of the storage node (default: the first node).
# MM_CLUSTER_INTERFACE     private interface on the nodes (default: mm0, the
#                          WireGuard interface mmd owns; name another one for
#                          a target with cluster.mesh.interface).
# MM_CLUSTER_MTU           MTU the mesh interface must have (default: 1420).
set -euo pipefail

: "${MM_CLUSTER_NODES:?set MM_CLUSTER_NODES to NAME=HOST=PRIVATE_ADDRESS entries}"
: "${MM_CLUSTER_SSH_KEY:?set MM_CLUSTER_SSH_KEY to a private key file}"
USER_NAME=${MM_CLUSTER_SSH_USER:-root}
KNOWN_HOSTS=${MM_CLUSTER_KNOWN_HOSTS:-$HOME/.ssh/known_hosts}
INTERFACE=${MM_CLUSTER_INTERFACE:-mm0}
MTU=${MM_CLUSTER_MTU:-1420}
STORAGE_ROOT=/var/lib/mm/storage

names=() hosts=() addresses=()
for entry in $MM_CLUSTER_NODES; do
  IFS='=' read -r name host address <<<"$entry"
  [ -n "$name" ] && [ -n "$host" ] && [ -n "$address" ] ||
    { echo "MM_CLUSTER_NODES entry '$entry' is not NAME=HOST=PRIVATE_ADDRESS" >&2; exit 2; }
  names+=("$name") hosts+=("$host") addresses+=("$address")
done
storage=${MM_CLUSTER_STORAGE:-${names[0]}}

step() { printf '== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

# Runs a root shell script, read from stdin, on node INDEX.
on() {
  local index=$1 become=()
  [ "$USER_NAME" = root ] || become=(sudo -n)
  ssh -i "$MM_CLUSTER_SSH_KEY" -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=15 \
    -o StrictHostKeyChecking=yes -o UserKnownHostsFile="$KNOWN_HOSTS" -o LogLevel=ERROR \
    "$USER_NAME@${hosts[$index]}" "${become[@]}" /bin/sh -seu
}

storage_index=-1
for index in "${!names[@]}"; do
  name=${names[$index]} address=${addresses[$index]}
  [ "$name" = "$storage" ] && storage_index=$index
  step "$name"
  on "$index" <<EOF || fail "$name is not set up as a cluster node"
[ "\$(hostname)" = '$name' ] || { echo 'hostname is not $name' >&2; exit 1; }
grep -qx 'MM_NODE_NAME=$name' /etc/mm/node.env || { echo '/etc/mm/node.env does not name $name' >&2; exit 1; }
grep -qx 'MM_PRIVATE_ADDRESS=$address' /etc/mm/node.env || { echo '/etc/mm/node.env does not hold $address' >&2; exit 1; }
docker info --format '{{.ServerVersion}}' >/dev/null || { echo 'Docker Engine does not answer' >&2; exit 1; }
docker compose version >/dev/null || { echo 'the Compose plugin is missing' >&2; exit 1; }
systemctl is-active --quiet mmd || { echo 'mmd is not running' >&2; exit 1; }
ip -4 -o addr show dev '$INTERFACE' | grep -q 'inet $address/' || { echo '$INTERFACE does not hold $address' >&2; exit 1; }
for peer in ${addresses[*]}; do
  ping -c 2 -W 3 -q "\$peer" >/dev/null || { echo "\$peer does not answer over $INTERFACE" >&2; exit 1; }
done
[ '$INTERFACE' = mm0 ] || exit 0
# The mesh mmd owns: its state and key under mmd's own directory, the unit
# that restores it at boot, and nothing left of an earlier wg-quick mesh.
ip link show dev mm0 | grep -q 'mtu $MTU ' || { echo 'mm0 does not have MTU $MTU' >&2; exit 1; }
[ "\$(stat -c '%U %a' /var/lib/mm/mesh/private.key)" = 'root 600' ] || { echo 'the mesh key is not a root-only file' >&2; exit 1; }
[ -s /var/lib/mm/mesh/mesh.json ] || { echo 'mmd holds no mesh state' >&2; exit 1; }
systemctl is-enabled --quiet mm-mesh.service || { echo 'mm-mesh.service is not enabled: the mesh would not come back after a reboot' >&2; exit 1; }
systemctl is-active --quiet mm-mesh.service || { echo 'mm-mesh.service is not active' >&2; exit 1; }
systemctl is-enabled --quiet mm-network.service || { echo 'mm-network.service is not enabled: networks that span the nodes would not come back after a reboot' >&2; exit 1; }
if ip link show dev wg0 >/dev/null 2>&1 || [ -e /etc/wireguard/wg0.conf ] || systemctl is-enabled --quiet wg-quick@wg0 2>/dev/null; then
  echo 'the wg-quick mesh of an earlier mm version is still there' >&2; exit 1
fi
# Every peer has shaken hands, and a packet of the full MTU gets through.
[ "\$(wg show mm0 peers | wc -l)" -eq $((${#names[@]} - 1)) ] || { echo 'mm0 does not have one peer per other node' >&2; exit 1; }
if wg show mm0 latest-handshakes | awk '\$2 == 0 { found = 1 } END { exit !found }'; then
  echo 'a peer of mm0 never completed a handshake' >&2; exit 1
fi
for peer in ${addresses[*]}; do
  [ "\$peer" = '$address' ] || ping -c 2 -W 3 -q -M do -s $((MTU - 28)) "\$peer" >/dev/null ||
    { echo "a packet of $MTU bytes does not reach \$peer" >&2; exit 1; }
done
/usr/libexec/mmd mesh status | grep -q '"up": true' || { echo 'mmd does not report mm0 as up' >&2; exit 1; }
# Containers are kept away from shared storage: what one sends to the
# storage port through the mesh is dropped, and the node's mesh address
# takes storage traffic from the other nodes' mesh addresses only.
iptables -S MM-STORAGE 2>/dev/null | grep -q -- '-o mm0 -p tcp -m tcp --dport 2049 .*-j DROP' ||
  { echo 'the chain MM-STORAGE does not drop what containers send to the storage port' >&2; exit 1; }
iptables -S MM-STORAGE-IN 2>/dev/null | grep -q -- '-d $address/32 -p tcp -m tcp --dport 2049 .*-j DROP' ||
  { echo 'the chain MM-STORAGE-IN does not guard the storage port of $address' >&2; exit 1; }
iptables -C DOCKER-USER -j MM-STORAGE 2>/dev/null || { echo 'DOCKER-USER does not enter MM-STORAGE' >&2; exit 1; }
iptables -C INPUT -j MM-STORAGE-IN 2>/dev/null || { echo 'INPUT does not enter MM-STORAGE-IN' >&2; exit 1; }
EOF
done
[ "$storage_index" -ge 0 ] || fail "MM_CLUSTER_STORAGE names '$storage', which MM_CLUSTER_NODES does not list"

storage_address=${addresses[$storage_index]}
step "storage on $storage"
on "$storage_index" <<EOF || fail "$storage does not serve storage on the mesh only"
exportfs -v | grep -q '^$STORAGE_ROOT' || { echo 'no export of $STORAGE_ROOT' >&2; exit 1; }
for client in ${addresses[*]}; do
  exportfs -v | grep -A1 '^$STORAGE_ROOT' | grep -q "\$client(" || { echo "the export does not allow \$client" >&2; exit 1; }
done
[ "\$(cat /proc/fs/nfsd/versions)" = '-3 +4 -4.0 -4.1 +4.2' ] || { echo 'nfsd serves more than NFSv4.2' >&2; exit 1; }
[ "\$(ss -Hltn 'sport = :2049' | awk '{print \$4}')" = '$storage_address:2049' ] ||
  { echo 'nfsd does not listen on $storage_address only' >&2; exit 1; }
if ss -Hltnup | grep -E 'rpc\.(mountd|statd)|rpcbind'; then
  echo 'an NFS helper daemon listens on the network' >&2; exit 1
fi
EOF

# Without an NFS client package the kernel mounts directly; -i skips the helper.
for index in "${!names[@]}"; do
  [ "$index" -ne "$storage_index" ] || continue
  on "$index" <<EOF || fail "${names[$index]} cannot use the storage over the mesh"
directory=\$(mktemp -d)
trap 'umount "\$directory" 2>/dev/null || true; rmdir "\$directory"' EXIT
mount -i -t nfs4 -o vers=4.2,addr=$storage_address,clientaddr=${addresses[$index]} '$storage_address:/' "\$directory"
touch "\$directory/.mm-live-check"
rm "\$directory/.mm-live-check"
EOF
done

step 'storage does not answer on a public address'
if timeout 10 bash -c "exec 3<>/dev/tcp/${hosts[$storage_index]}/2049" 2>/dev/null; then
  fail "NFS answers on ${hosts[$storage_index]}"
fi
step 'cluster setup checks passed'
