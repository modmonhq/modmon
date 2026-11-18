# Install a pinned OpenBao release, its account and directories. Idempotent.
# Arguments: STORE_ID VERSION, then MACHINE URL SHA256 for each machine.
set -eu
id=$1 version=$2
shift 2
machine=$(uname -m)
url= sha256=
while [ $# -ge 3 ]; do
    if [ "$1" = "$machine" ]; then
        url=$2 sha256=$3
    fi
    shift 3
done
if [ -z "$url" ]; then
    printf 'mm: OpenBao %s has no release mm installs on %s\n' "$version" "$machine" >&2
    exit 1
fi
if ! getent group mm-openbao >/dev/null; then
    groupadd --system mm-openbao
fi
if ! getent passwd mm-openbao >/dev/null; then
    useradd --system --gid mm-openbao --home-dir /nonexistent --no-create-home \
        --shell /usr/sbin/nologin mm-openbao
fi
install -d -m 0755 -o root -g root /etc/mm-openbao
install -d -m 2750 -o root -g mm-openbao "/etc/mm-openbao/$id"
install -d -m 0755 -o root -g root /var/lib/mm-openbao
install -d -m 0700 -o root -g root /var/lib/mm-openbao/archives
install -d -m 0700 -o mm-openbao -g mm-openbao "/var/lib/mm-openbao/$id" "/var/lib/mm-openbao/$id/data"
binary=/usr/libexec/mm-openbao/$version/bao
if [ -x "$binary" ]; then
    exit 0
fi
archive=/var/lib/mm-openbao/archives/${url##*/}
if [ ! -f "$archive" ] || [ "$(sha256sum "$archive" | cut -d ' ' -f 1)" != "$sha256" ]; then
    rm -f "$archive.part"
    if command -v curl >/dev/null 2>&1; then
        curl --fail --silent --show-error --location --proto '=https' --max-time 600 \
            --output "$archive.part" "$url"
    else
        wget --quiet --https-only --timeout=600 --output-document="$archive.part" "$url"
    fi
    actual=$(sha256sum "$archive.part" | cut -d ' ' -f 1)
    if [ "$actual" != "$sha256" ]; then
        rm -f "$archive.part"
        printf 'mm: %s has SHA-256 %s, not the pinned %s; nothing was installed\n' \
            "$url" "$actual" "$sha256" >&2
        exit 1
    fi
    mv "$archive.part" "$archive"
fi
staging=$(mktemp -d /usr/libexec/.mm-openbao.XXXXXX 2>/dev/null || {
    install -d -m 0755 /usr/libexec
    mktemp -d /usr/libexec/.mm-openbao.XXXXXX
})
trap 'rm -rf "$staging"' EXIT
tar -xzf "$archive" -C "$staging" bao
chmod 0755 "$staging/bao"
install -d -m 0755 /usr/libexec/mm-openbao
install -d -m 0755 "/usr/libexec/mm-openbao/$version"
mv "$staging/bao" "$binary.new"
mv "$binary.new" "$binary"
