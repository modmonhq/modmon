# Serve mm's shared storage from this host's kernel NFS server: NFSv4.2 only,
# listening on the node's private address only, the storage root exported
# (fsid=0) to the cluster's private addresses.
# Arguments: STORAGE_ROOT LISTEN_ADDRESS CLIENT_ADDRESS...
set -eu
root=$1 listen=$2
shift 2
if ! dpkg -s nfs-kernel-server >/dev/null 2>&1; then
    # NFSv4 needs no port mapper. Keep the one this install brings from ever
    # listening; a host that already runs one keeps it.
    if ! dpkg -s rpcbind >/dev/null 2>&1; then
        systemctl mask --quiet rpcbind.service rpcbind.socket
    fi
    export DEBIAN_FRONTEND=noninteractive
    apt-get -o DPkg::Lock::Timeout=600 update -qq
    apt-get -o DPkg::Lock::Timeout=600 install -y -qq nfs-kernel-server >/dev/null
fi
install -d -m 0755 "$root" /etc/nfs.conf.d /etc/exports.d
replace() {
    if [ -f "$2" ] && cmp -s "$1" "$2"; then
        rm -f "$1"
        return 1
    fi
    mv "$1" "$2"
}
restart=no
next=$(mktemp)
printf '# Managed by mm target setup: NFSv4.2 only, on the private network only.\n[nfsd]\nhost=%s\nvers2=n\nvers3=n\nvers4.0=n\nvers4.1=n\nvers4.2=y\n' "$listen" >"$next"
if replace "$next" /etc/nfs.conf.d/mm.conf; then restart=yes; fi
line=$root
for client in "$@"; do
    line="$line $client(rw,fsid=0,no_subtree_check,no_root_squash,sync)"
done
next=$(mktemp)
printf '# Managed by mm target setup.\n%s\n' "$line" >"$next"
changed=no
if replace "$next" /etc/exports.d/mm-storage.exports; then changed=yes; fi
chmod 0644 /etc/nfs.conf.d/mm.conf /etc/exports.d/mm-storage.exports
systemctl enable --quiet nfs-server
if [ "$restart" = yes ]; then
    # rpc.mountd reads the same settings: with NFSv2 and NFSv3 off it opens
    # no socket, so restart it too or it keeps the ones it had.
    systemctl restart nfs-mountd nfs-server
elif ! systemctl is-active --quiet nfs-server; then
    systemctl start nfs-server
fi
exportfs -ra
printf 'storage=%s listen=%s exports-changed=%s\n' "$root" "$listen" "$changed"
