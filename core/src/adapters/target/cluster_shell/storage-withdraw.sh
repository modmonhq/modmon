# Stop serving mm's shared storage from this node: another node serves it
# now. The storage directory and whatever is in it stay where they are.
set -eu
rm -f /etc/exports.d/mm-storage.exports /etc/nfs.conf.d/mm.conf
if command -v exportfs >/dev/null 2>&1; then
    exportfs -ra 2>/dev/null || true
    # A host that exports something of its own keeps its server.
    if [ -z "$(exportfs -s 2>/dev/null)" ]; then
        systemctl disable --now --quiet nfs-server 2>/dev/null || true
        systemctl stop --quiet nfs-mountd 2>/dev/null || true
    else
        systemctl restart nfs-mountd nfs-server
    fi
fi
printf 'storage=withdrawn\n'
