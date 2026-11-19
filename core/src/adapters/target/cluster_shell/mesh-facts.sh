# Print this node's CPU (uname -m) on the first line, then what it holds for
# the mesh as JSON, without changing anything: its WireGuard public key and
# the interface state mmd applied last.
set -eu
uname -m
if [ -x /usr/libexec/mmd ] && [ -f /etc/mm/mmd.json ] &&
    /usr/libexec/mmd mesh --help >/dev/null 2>&1; then
    exec /usr/libexec/mmd mesh show
fi
# mmd is missing or predates the mesh commands. A wg-quick mesh that an
# earlier mm version made is replaced at apply, and its key is kept.
legacy=/etc/wireguard/wg0.conf
if [ -f "$legacy" ] && head -n 1 "$legacy" | grep -q '^# Managed by mm target setup' &&
    [ -s /etc/wireguard/wg0.key ] && command -v wg >/dev/null 2>&1; then
    printf '{"public-key":"%s","legacy-interface":"wg0"}\n' "$(wg pubkey </etc/wireguard/wg0.key)"
else
    printf '{}\n'
fi
