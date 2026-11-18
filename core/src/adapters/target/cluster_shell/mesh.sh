# Bring this node's mesh to the interface state on stdin (JSON). mmd owns the
# interface mm0: it stores the state, changes only what differs, and
# mm-mesh.service restores it at boot. A host that an earlier mm version set
# up with wg-quick hands its interface over first; mmd keeps that key.
set -eu
legacy=/etc/wireguard/wg0.conf
if [ -f "$legacy" ] && head -n 1 "$legacy" | grep -q '^# Managed by mm target setup'; then
    # Frees the port and the address for mm0.
    systemctl disable --quiet --now wg-quick@wg0.service 2>/dev/null || true
fi
# Standard output carries mmd's result and nothing else.
/usr/libexec/mmd mesh apply
# mm-network.service restores the networks that span the cluster after the
# container engine starts; it has nothing to do until a deployment has given
# a network its addresses.
systemctl enable --quiet mm-mesh.service mm-network.service >&2
# Runs the boot path now, in the unit's sandbox, so a host where it could
# not restore the mesh after a reboot fails here instead.
if systemctl is-active --quiet mm-mesh.service; then
    systemctl reload mm-mesh.service >&2
else
    systemctl start mm-mesh.service >&2
fi
# A node that already holds networks spanning the cluster brings them to the
# state mmd stored, with the mmd this setup installed.
if [ -f /var/lib/mm/networks/networks.json ]; then
    systemctl restart mm-network.service >&2
fi
