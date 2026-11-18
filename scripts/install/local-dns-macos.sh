#!/usr/bin/env bash
# Scoped .test DNS for macOS: a root-owned copy of Homebrew's dnsmasq answers on a
# dedicated loopback alias, and /etc/resolver/test sends only .test queries there.
# Usage: local-dns-macos.sh [--remove]. Compatible with macOS Bash 3.2.
main() {
set -euo pipefail
marker='Managed by mm local setup'
address=127.0.77.53
helper=/Library/PrivilegedHelperTools/local.mm.dnsmasq
state=/Library/Application\ Support/mm/dns
config=$state/dnsmasq.conf
resolver=/etc/resolver/test
dns_label=local.mm.dns
alias_label=local.mm.loopback-alias
dns_plist=/Library/LaunchDaemons/$dns_label.plist
alias_plist=/Library/LaunchDaemons/$alias_label.plist
fail() { printf 'mm .test DNS: %s\n' "$*" >&2; exit 1; }
managed() { [[ -f $1 && ! -L $1 ]] && grep -Fq "$marker" "$1"; }
refuse_unmanaged() {
    local file
    for file in "$@"; do
        [[ ! -L $file ]] || fail "refusing symlinked DNS configuration: $file"
        if [[ -e $file ]] && ! managed "$file"; then
            fail "refusing to overwrite unmanaged DNS configuration: $file"
        fi
    done
    # The helper binary cannot carry a marker; it belongs to mm only with its daemon.
    if [[ -e $helper ]] && ! managed "$dns_plist"; then
        fail "refusing to replace $helper, which no mm-owned DNS daemon uses"
    fi
}
# System name resolution, as browsers use it; dig bypasses /etc/resolver.
resolves() {
    dscacheutil -q host -a name "$1" | awk '/^ip_address:/ { found = 1; if ($2 != "127.0.0.1") bad = 1 } END { exit !(found && !bad) }'
}

if [[ ${1:-} == --remove ]]; then
    refuse_unmanaged "$resolver" "$config" "$dns_plist" "$alias_plist"
    sudo launchctl bootout "system/$dns_label" 2>/dev/null || true
    sudo launchctl bootout "system/$alias_label" 2>/dev/null || true
    if [[ -f $alias_plist ]]; then sudo /sbin/ifconfig lo0 -alias "$address" 2>/dev/null || true; fi
    sudo rm -f "$resolver" "$dns_plist" "$alias_plist" "$config" "$helper"
    sudo rmdir "$state" 2>/dev/null || true
    sudo dscacheutil -flushcache
    sudo killall -HUP mDNSResponder 2>/dev/null || true
    printf 'Removed mm .test DNS.\n'
    exit 0
fi

probe=mm-install-$$.test
if resolves "$probe" && [[ ! -f $dns_plist ]]; then
    printf 'Existing wildcard .test DNS resolves to 127.0.0.1; reusing it.\n'
    exit 0
fi
refuse_unmanaged "$resolver" "$config" "$dns_plist" "$alias_plist"
if [[ ! -f $alias_plist ]] && ifconfig lo0 | grep -Fq "inet $address "; then
    fail "loopback alias $address already exists without an mm-owned daemon; refusing to reuse it"
fi
source_binary=
for prefix in "$(brew --prefix 2>/dev/null || true)" /opt/homebrew /usr/local; do
    if [[ -n $prefix && -x $prefix/opt/dnsmasq/sbin/dnsmasq ]]; then source_binary=$prefix/opt/dnsmasq/sbin/dnsmasq; break; fi
done
[[ -n $source_binary ]] || fail 'dnsmasq is missing; run "brew install dnsmasq", then retry'
# Root must never execute a user-writable binary. dnsmasq links only libSystem.
if otool -L "$source_binary" | tail -n +2 | grep -v '^[[:space:]]*/usr/lib/' | grep -q .; then
    fail "$source_binary links non-system libraries; a root-owned copy would not be self-contained"
fi

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
cat > "$scratch/dnsmasq.conf" <<EOF
# $marker
listen-address=$address
bind-interfaces
port=53
no-resolv
no-hosts
no-poll
pid-file=
local=/test/
address=/test/127.0.0.1
user=nobody
group=nobody
EOF
cat > "$scratch/test" <<EOF
# $marker
nameserver $address
EOF
plist() {
    local label=$1 keep_alive=$2
    shift 2
    printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>' \
        '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
        '<plist version="1.0">' "<!-- $marker -->" '<dict>' \
        "  <key>Label</key><string>$label</string>" '  <key>ProgramArguments</key><array>'
    local argument
    for argument in "$@"; do printf '    <string>%s</string>\n' "$argument"; done
    printf '%s\n' '  </array>' '  <key>RunAtLoad</key><true/>' "  <key>KeepAlive</key>$keep_alive" \
        '  <key>ThrottleInterval</key><integer>5</integer>' '</dict>' '</plist>'
}
plist "$alias_label" '<false/>' /sbin/ifconfig lo0 alias "$address" netmask 255.255.255.255 > "$scratch/alias.plist"
plist "$dns_label" '<true/>' "$helper" --keep-in-foreground "--conf-file=$config" > "$scratch/dns.plist"
"$source_binary" --test "--conf-file=$scratch/dnsmasq.conf" >/dev/null
plutil -lint "$scratch/alias.plist" "$scratch/dns.plist" >/dev/null

printf 'Installing scoped .test DNS (sudo): %s, %s\n' "$resolver" "$dns_plist"
sudo install -d -o root -g wheel -m 755 /Library/PrivilegedHelperTools "$state" /etc/resolver
sudo install -o root -g wheel -m 755 "$source_binary" "$helper"
sudo install -o root -g wheel -m 644 "$scratch/dnsmasq.conf" "$config"
sudo install -o root -g wheel -m 644 "$scratch/alias.plist" "$alias_plist"
sudo install -o root -g wheel -m 644 "$scratch/dns.plist" "$dns_plist"
sudo install -o root -g wheel -m 644 "$scratch/test" "$resolver"
for label in "$alias_label" "$dns_label"; do
    sudo launchctl bootout "system/$label" 2>/dev/null || true
done
sudo launchctl bootstrap system "$alias_plist"
# The alias job is one-shot; wait for it so dnsmasq can bind the address.
for _ in 1 2 3 4 5 6 7 8 9 10; do
    if ifconfig lo0 | grep -Fq "inet $address "; then break; fi
    sleep 0.5
done
sudo launchctl bootstrap system "$dns_plist"
sudo dscacheutil -flushcache
sudo killall -HUP mDNSResponder 2>/dev/null || true
for _ in 1 2 3 4 5 6 7 8 9 10; do
    if resolves "$probe" && resolves mm.test && resolves mail.mm.test; then
        printf 'Wildcard .test DNS is ready.\n'
        exit 0
    fi
    sleep 1
done
printf 'Local .test DNS verification failed. Check "scutil --dns" for a test resolver using %s, and "sudo launchctl print system/%s".\n' "$address" "$dns_label" >&2
exit 1
}
main "$@"
