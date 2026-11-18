# Prepare one cluster node: hostname, node facts, and the tools its mesh needs.
# Arguments: NAME PRIVATE_INTERFACE PRIVATE_ADDRESS WIREGUARD(yes|no) RUNTIME
# With WireGuard, mmd creates the interface later and makes the node's key
# itself; this script only installs the tools. RUNTIME is the engine the
# provisioning installs next (docker-compose or podman-compose); mmd reads it
# from the node facts. Idempotent.
set -eu
name=$1 interface=$2 address=$3 wireguard=$4 runtime=${5:-docker-compose}
if [ "$(hostnamectl --static 2>/dev/null || hostname)" != "$name" ]; then
    hostnamectl set-hostname "$name"
fi
if grep -qx "127\.0\.1\.1 $name" /etc/hosts; then
    :
elif grep -q '^127\.0\.1\.1[[:space:]]' /etc/hosts; then
    sed -i "s/^127\.0\.1\.1[[:space:]].*/127.0.1.1 $name/" /etc/hosts
else
    printf '127.0.1.1 %s\n' "$name" >>/etc/hosts
fi
# Writes stdin to FILE unless it already holds exactly that.
keep() {
    cat >"$1.tmp"
    if [ -f "$1" ] && cmp -s "$1.tmp" "$1"; then
        rm -f "$1.tmp"
    else
        mv "$1.tmp" "$1"
    fi
}
umask 022
if [ -d /etc/cloud/cloud.cfg.d ]; then
    printf 'preserve_hostname: true\n' | keep /etc/cloud/cloud.cfg.d/99-mm-hostname.cfg
fi
install -d -m 0755 /etc/mm
printf 'MM_NODE_NAME=%s\nMM_PRIVATE_INTERFACE=%s\nMM_PRIVATE_ADDRESS=%s\nMM_RUNTIME=%s\n' \
    "$name" "$interface" "$address" "$runtime" | keep /etc/mm/node.env
packages=
# iptables: mmd loads the chains that keep the mesh between the hosts
# together with the interface, before a container engine is installed.
if [ "$wireguard" = yes ] && { ! command -v wg >/dev/null 2>&1 || ! command -v ip >/dev/null 2>&1 ||
    ! PATH="/usr/sbin:/sbin:$PATH" command -v iptables >/dev/null 2>&1; }; then
    packages="wireguard-tools iproute2 iptables"
fi
# Podman mounts a shared volume with mount(8), which needs the NFS helper;
# Docker Engine mounts it itself.
if [ "$runtime" = podman-compose ] && ! command -v mount.nfs >/dev/null 2>&1; then
    packages="$packages nfs-common"
fi
if [ -n "$packages" ]; then
    export DEBIAN_FRONTEND=noninteractive
    apt-get -o DPkg::Lock::Timeout=600 update -qq
    # shellcheck disable=SC2086 # a list of package names
    apt-get -o DPkg::Lock::Timeout=600 install -y -qq $packages >/dev/null
fi
if [ "$wireguard" != yes ]; then
    ip -4 -o addr show dev "$interface" | grep -q "inet $address/" || {
        printf 'mm: %s does not hold %s\n' "$interface" "$address" >&2
        exit 1
    }
fi
