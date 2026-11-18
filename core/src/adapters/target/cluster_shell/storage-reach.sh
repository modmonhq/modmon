# Prove that this node can mount mm's shared storage from the storage node:
# mount its root read-only at a directory of our own and take it off again.
# Needs no package: the kernel mounts NFSv4.2 by itself when it is given
# the server's address. Leaves nothing behind.
# Arguments: STORAGE_ADDRESS
set -eu
address=$1
point=$(mktemp -d /run/mm-storage-reach.XXXXXX)
cleanup() {
    umount -l "$point" 2>/dev/null || true
    rmdir "$point" 2>/dev/null || true
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
if ! problem=$(timeout 60 mount -i -t nfs4 \
    -o "ro,vers=4.2,addr=$address,timeo=100,retrans=2" "$address:/" "$point" 2>&1); then
    printf 'mm: this node cannot mount shared storage from %s: %s\n' "$address" "$problem" >&2
    case $problem in
        *"unknown filesystem type"*)
            printf 'mm: the kernel has no NFS client; install the kernel modules package that carries nfs.ko and nfsv4.ko for this kernel\n' >&2
            ;;
    esac
    exit 1
fi
printf 'storage=reachable address=%s\n' "$address"
