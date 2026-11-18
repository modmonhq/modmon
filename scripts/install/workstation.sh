#!/usr/bin/env bash
# Workstation setup is separate from compilation so it can be retried cheaply.
main() {
set -euo pipefail
binary=${1:?installed mm binary required}
runtime=${MM_SETUP_RUNTIME:-docker-compose}
scripts=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
fail() { printf 'mm workstation: %s\n' "$*" >&2; exit 1; }

case $runtime in docker-compose|podman-compose) ;; *) fail 'MM_SETUP_RUNTIME must be docker-compose or podman-compose' ;; esac
# Run outside a project: workstation setup must not inherit a checkout's target.
setup_services() {
    local scratch
    scratch=$(mktemp -d)
    "$binary" --root "$scratch" --runtime "$runtime" dev setup --services || { rm -rf "$scratch"; return 1; }
    rm -rf "$scratch"
}
finish() {
    printf '\nWorkstation ready. Local service names use {service}.{project-name}.test, for example web.store.test and api.store.test.\n'
    printf 'Project images, environment values, and deployment are prepared separately by mm.\n'
}
if [[ $(uname -s) == Darwin ]]; then
    # Ingress, services, and the dashboard run as launchd agents of this user.
    # sudo is used only for scoped .test DNS.
    [[ $runtime == docker-compose ]] || fail 'macOS setup supports Docker Desktop; unset MM_SETUP_RUNTIME or set it to docker-compose'
    command -v brew >/dev/null || fail 'Homebrew is required for HAProxy and dnsmasq. Install it from https://brew.sh, then rerun ./install.sh --setup-only'
    prefix=$(brew --prefix)
    for formula in haproxy dnsmasq; do
        if [[ ! -d $prefix/opt/$formula ]]; then brew install "$formula"; fi
    done
    command -v docker >/dev/null || fail 'Install and start Docker Desktop, then rerun ./install.sh --setup-only'
    endpoint=${DOCKER_HOST:-$(docker context inspect --format '{{.Endpoints.docker.Host}}' 2>/dev/null || true)}
    desktop=unix://$HOME/.docker/run/docker.sock
    if [[ $endpoint != "$desktop" ]] && ! [[ $endpoint == unix:///var/run/docker.sock && $(readlink /var/run/docker.sock 2>/dev/null) == "$HOME/.docker/run/docker.sock" ]]; then
        fail "local HTTPS on macOS requires Docker Desktop, but Docker uses ${endpoint:-no endpoint}. Run: docker context use desktop-linux"
    fi
    docker info >/dev/null 2>&1 || fail 'Docker Desktop is not running. Start it, then rerun ./install.sh --setup-only'
    command -v sudo >/dev/null || fail 'sudo is required to install scoped .test DNS'
    printf 'Workstation setup uses sudo only for scoped .test DNS; ingress runs as your user.\n'
    sudo -v || fail 'Run ./install.sh --setup-only in a terminal to authorize DNS setup'
    bash "$scripts/local-dns-macos.sh"
    setup_services
    finish
    exit 0
fi
[[ $(uname -s) == Linux ]] || fail 'Automatic workstation setup requires Linux with systemd or macOS with Docker Desktop.'
command -v systemctl >/dev/null || fail 'systemd is required for local HTTPS ingress'
systemctl is-active --quiet systemd-resolved || fail 'Enable systemd-resolved before local .test DNS setup'
command -v sudo >/dev/null || fail 'sudo is required to install DNS, trust, and ingress prerequisites'
printf 'Workstation setup uses sudo for local DNS, certificate trust, and ingress.\n'
sudo -v || fail 'Run ./install.sh --setup-only in a terminal to authorize workstation setup'

# The interactive CLI remains user-owned. Only this protected copy is elevated
# for fixed, reviewed ingress operations.
helper=/usr/local/libexec/mm/mm-ingress-helper
helper_source=$(dirname "$binary")/mm-ingress-helper
[[ -x $helper_source ]] || fail 'The ingress helper is missing from this build. Run ./install.sh --no-setup once to rebuild the CLI and helper, then rerun --setup-only.'
for location in /usr/local/libexec /usr/local/libexec/mm "$helper"; do
    [[ ! -L $location ]] || fail "refusing symlinked privileged helper path: $location"
done
sudo install -d -o root -g root -m 755 /usr/local/libexec/mm
if ! cmp -s "$helper_source" "$helper"; then
    sudo install -o root -g root -m 755 "$helper_source" "$helper.new"
    sudo mv -f "$helper.new" "$helper"
fi
printf 'Protected ingress helper: %s\n' "$helper"

missing=()
for package in dnsmasq haproxy; do command -v "$package" >/dev/null || missing+=("$package"); done
if [[ -e /sys/fs/selinux/enforce ]] && ! command -v semanage >/dev/null; then
    missing+=(policycoreutils-python-utils)
fi
if (( ${#missing[@]} )); then
    if command -v dnf >/dev/null; then sudo dnf install -y "${missing[@]}"
    elif command -v apt-get >/dev/null; then sudo apt-get install -y "${missing[@]}"
    elif command -v pacman >/dev/null; then sudo pacman -S --needed --noconfirm "${missing[@]}"
    else fail "Install these packages, then rerun setup: ${missing[*]}"; fi
fi

bash "$scripts/local-dns.sh"
setup_services
finish
}
main "$@"; exit "$?"
