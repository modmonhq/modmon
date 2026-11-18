#!/usr/bin/env bash
# A dedicated, host-only DNS interface. Resolved rejects per-link DNS on lo.
main() {
set -euo pipefail
probe=mm-install-$$.test
resolved() { getent ahostsv4 "$probe" | awk '$1 != "127.0.0.1" { bad=1 } END { exit (NR == 0 || bad) }'; }
if resolved; then
    printf 'Existing wildcard .test DNS resolves to 127.0.0.1; reusing it.\n'
    exit 0
fi

dnsmasq=$(command -v dnsmasq)
resolvectl=$(command -v resolvectl)
ip=$(command -v ip)
marker='# Managed by mm local setup'
config=/etc/mm/dnsmasq.conf
unit=/etc/systemd/system/mm-test-dns.service
nm_config=/etc/NetworkManager/conf.d/90-mm-test-dns.conf
[[ ! -L /etc/mm && ! -L $config && ! -L $unit ]] || { printf 'Refusing symlinked mm DNS configuration.\n' >&2; exit 1; }
for file in "$config" "$unit"; do
    if [[ -e $file ]] && ! grep -Fqx "$marker" "$file"; then
        printf 'Refusing to overwrite unmanaged DNS configuration: %s\n' "$file" >&2
        exit 1
    fi
done
if [[ ! -f $unit ]] && "$ip" link show mm-test-dns >/dev/null 2>&1; then
    printf 'The mm-test-dns interface already exists without an mm-owned service. Refusing to replace it.\n' >&2
    exit 1
fi
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
# Keep this infrastructure interface out of NetworkManager's connection
# activation and dispatcher hooks. Other DNS managers must not rewrite it.
if command -v nmcli >/dev/null && systemctl is-active --quiet NetworkManager; then
    [[ ! -L $nm_config ]] || { printf 'Refusing symlinked mm NetworkManager configuration.\n' >&2; exit 1; }
    if [[ -e $nm_config ]] && ! grep -Fqx "$marker" "$nm_config"; then
        printf 'Refusing to overwrite unmanaged configuration: %s\n' "$nm_config" >&2
        exit 1
    fi
    cat > "$scratch/90-mm-test-dns.conf" <<EOF
$marker
[device-mm-test-dns]
match-device=interface-name:mm-test-dns
managed=0
EOF
    sudo install -d -m 755 /etc/NetworkManager/conf.d
    sudo install -m 644 "$scratch/90-mm-test-dns.conf" "$nm_config"
    sudo nmcli general reload conf
fi
cat > "$scratch/dnsmasq.conf" <<EOF
$marker
interface=mm-test-dns
except-interface=lo
listen-address=169.254.77.53
bind-interfaces
no-resolv
no-hosts
local=/test/
address=/test/127.0.0.1
EOF
cat > "$scratch/mm-test-dns.service" <<EOF
$marker
[Unit]
Description=mm local .test DNS
Requires=systemd-resolved.service
After=systemd-resolved.service NetworkManager.service network-online.target
StartLimitIntervalSec=30
StartLimitBurst=3

[Service]
Type=simple
ExecStartPre=$ip link add mm-test-dns type dummy
ExecStartPre=$ip address add 169.254.77.53/32 dev mm-test-dns
ExecStartPre=$ip link set mm-test-dns up
ExecStart=$dnsmasq --keep-in-foreground --conf-file=$config
ExecStartPost=$resolvectl dns mm-test-dns 169.254.77.53
ExecStartPost=$resolvectl domain mm-test-dns ~test
ExecStartPost=$resolvectl default-route mm-test-dns no
ExecStopPost=-$ip link delete mm-test-dns
Restart=on-failure
RestartSec=2

[Install]
WantedBy=multi-user.target
EOF
"$dnsmasq" --test --conf-file="$scratch/dnsmasq.conf"
sudo install -d -m 755 /etc/mm
sudo install -m 644 "$scratch/dnsmasq.conf" "$config"
sudo install -m 644 "$scratch/mm-test-dns.service" "$unit"
sudo systemctl daemon-reload
sudo systemctl enable mm-test-dns.service
sudo systemctl reset-failed mm-test-dns.service
sudo systemctl restart mm-test-dns.service
sudo "$resolvectl" flush-caches
for _ in 1 2 3 4 5; do
    if resolved; then printf 'Wildcard .test DNS is ready.\n'; exit 0; fi
    sleep 1
done
printf 'Local .test DNS verification failed. Inspect resolvectl status mm-test-dns: it must use 169.254.77.53 with domain ~test, not another resolver or ~.\n' >&2
"$resolvectl" status mm-test-dns >&2 || true
exit 1
}
main "$@"
