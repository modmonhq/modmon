# Check that every peer answers on the private network and, when MTU is not
# 0, that a packet as large as the mesh MTU arrives unfragmented.
# Arguments: MTU PEER_ADDRESS...
set -eu
mtu=$1
shift
for peer in "$@"; do
    attempt=0
    until ping -c 1 -W 2 "$peer" >/dev/null 2>&1; do
        attempt=$((attempt + 1))
        if [ "$attempt" -ge 15 ]; then
            printf 'mm-unreachable: %s\n' "$peer" >&2
            exit 1
        fi
        sleep 2
    done
    if [ "$mtu" -gt 0 ] && ! ping -c 3 -W 3 -M do -s "$((mtu - 28))" "$peer" >/dev/null 2>&1; then
        printf 'mm-mtu: %s\n' "$peer" >&2
        exit 1
    fi
done
printf 'reachable=%s\n' "$#"
