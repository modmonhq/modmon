#!/bin/sh
set -eu

origin=https://github.com/modmonhq/modmon
if [ "${MM_RELEASE_ALLOW_INSECURE_TEST_ORIGIN:-}" = 1 ] && [ -n "${MM_RELEASE_TEST_ORIGIN:-}" ]; then
    origin=${MM_RELEASE_TEST_ORIGIN%/}
fi
requested_version=
install_dir=${HOME:+$HOME/.local/bin}
with_daemon=0
force=0
no_modify_path=0

usage() {
    cat <<'EOF'
Usage: install.sh [--version vMAJOR.MINOR.PATCH] [--install-dir PATH]
                  [--with-daemon] [--force] [--no-modify-path]
Installs mm from its GitHub release without sudo, with mm-ingress-helper on
Linux. mmd is installed only with --with-daemon.
EOF
}

fail() { printf 'mm installer: %s\n' "$*" >&2; exit 1; }
while [ "$#" -gt 0 ]; do
    case "$1" in
        --version) [ "$#" -ge 2 ] || fail "--version requires a value"; requested_version=$2; shift 2 ;;
        --install-dir) [ "$#" -ge 2 ] || fail "--install-dir requires a value"; install_dir=$2; shift 2 ;;
        --with-daemon) with_daemon=1; shift ;;
        --force) force=1; shift ;;
        --no-modify-path) no_modify_path=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) fail "unknown argument: $1" ;;
    esac
done
[ -n "${install_dir:-}" ] || fail "HOME is unset; pass --install-dir"
case "$install_dir" in *'\n'*|*'\r'*) fail "install directory contains a line break" ;; esac
tag_expression='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-rc\.(0|[1-9][0-9]*))?$'
if [ -z "$requested_version" ]; then
    base="$origin/releases/latest/download"
else
    printf '%s' "$requested_version" | grep -Eq "$tag_expression" || fail "version must be vMAJOR.MINOR.PATCH or vMAJOR.MINOR.PATCH-rc.N"
    base="$origin/releases/download/$requested_version"
fi

system=$(uname -s 2>/dev/null || true)
machine=$(uname -m 2>/dev/null || true)
case "$machine" in x86_64|amd64|AMD64) architecture=x86_64 ;; arm64|aarch64|ARM64) architecture=aarch64 ;; *) fail "unsupported architecture: $machine" ;; esac
case "$system" in
    Linux) target="$architecture-unknown-linux-gnu" ;;
    Darwin) target="$architecture-apple-darwin" ;;
    *) fail "unsupported operating system: $system; mm runs on Linux and macOS" ;;
esac

temporary=$(mktemp -d 2>/dev/null || mktemp -d -t mm-install) || fail "could not create a private temporary directory"
chmod 700 "$temporary" || fail "could not protect temporary directory"
cleanup() { rm -rf "$temporary"; }
trap cleanup EXIT HUP INT TERM
fetch() {
    url=$1 destination=$2
    if command -v curl >/dev/null 2>&1; then
        if [ "${MM_RELEASE_ALLOW_INSECURE_TEST_ORIGIN:-}" = 1 ]; then
            curl -LsSf --retry 3 --output "$destination" "$url" || fail "download failed: $url (check TLS/proxy settings)"
        else
            curl --proto '=https' --tlsv1.2 -LsSf --retry 3 --output "$destination" "$url" || fail "download failed: $url (check TLS/proxy settings)"
        fi
    elif command -v wget >/dev/null 2>&1; then
        wget -q --https-only -O "$destination" "$url" || fail "download failed: $url (check TLS/proxy settings)"
    else
        fail "curl or wget is required"
    fi
}
sums="$temporary/SHA256SUMS"
fetch "$base/SHA256SUMS" "$sums"

entries=$(grep -E "^[0-9a-f]{64}  mm-v[0-9A-Za-z.-]+-$target\.tar\.gz\$" "$sums" || true)
[ -n "$entries" ] || fail "the release has no archive for $target in SHA256SUMS"
[ "$(printf '%s\n' "$entries" | wc -l | tr -d ' ')" = 1 ] || fail "SHA256SUMS lists more than one archive for $target"
expected_sha=${entries%% *}
archive_name=${entries##* }
tag=${archive_name#mm-}
tag=${tag%-"$target".tar.gz}
printf '%s' "$tag" | grep -Eq "$tag_expression" || fail "SHA256SUMS names an archive with an invalid version: $archive_name"
[ -z "$requested_version" ] || [ "$tag" = "$requested_version" ] || fail "release $requested_version lists the archive of $tag"
version=${tag#v}
version=${version%%-*}

archive="$temporary/archive"
fetch "$base/$archive_name" "$archive"
actual_sha=
if command -v sha256sum >/dev/null 2>&1; then actual_sha=$(sha256sum "$archive" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then actual_sha=$(shasum -a 256 "$archive" | awk '{print $1}')
elif command -v openssl >/dev/null 2>&1; then actual_sha=$(openssl dgst -sha256 "$archive" | awk '{print $NF}')
else fail "sha256sum, shasum, or openssl is required"; fi
[ "$actual_sha" = "$expected_sha" ] || fail "checksum mismatch for $archive_name: the archive does not match SHA256SUMS; nothing was installed"

extract="$temporary/extract"
mkdir "$extract"
command -v tar >/dev/null 2>&1 || fail "tar is required"
top="mm-$tag-$target"
listing=$(tar -tzf "$archive") || fail "archive is truncated or invalid"
verbose=$(tar -tvzf "$archive") || fail "archive metadata is invalid"
printf '%s\n' "$verbose" | grep -Eq '^l| -> ' && fail "archive contains a symlink"
printf '%s\n' "$listing" | grep -Eq '(^/|(^|/)\.\.(/|$)|\\)' && fail "archive contains an unsafe path"
printf '%s\n' "$listing" | awk -F/ -v top="$top" 'NF && $1 != top { exit 1 }' || fail "archive has another top-level directory than $top"
printf '%s\n' "$listing" | grep -Ev "^$top/?$|^$top/(mm|mmd|mm-ingress-helper|LICENSE|README\.md|completions/?|completions/mm\.(bash|fish|zsh|elvish|ps1)|man/?|man/mm\.1)$" >/dev/null && fail "archive contains an unexpected file"
tar -xzf "$archive" -C "$extract" || fail "archive extraction failed"
with_helper=0
[ ! -f "$extract/$top/mm-ingress-helper" ] || with_helper=1
[ "$with_daemon" -eq 0 ] || [ -f "$extract/$top/mmd" ] || fail "the archive has no mmd"

current="$install_dir/mm"
if [ -x "$current" ] || [ -f "$current" ]; then
    installed=$($current --version 2>/dev/null | awk '{print $2}' || true)
    if printf '%s' "$installed" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
        if awk -v current="$installed" -v candidate="$version" 'BEGIN { split(current,c,"."); split(candidate,n,"."); for(i=1;i<=3;i++){ if(c[i]+0>n[i]+0)exit 0; if(c[i]+0<n[i]+0)exit 1 } exit 1 }'; then
            [ "$force" -eq 1 ] || \
            fail "refusing to downgrade mm $installed to $version without --force"
        fi
    elif [ "$force" -ne 1 ]; then
        fail "existing mm version is unknown; use --force to replace it"
    fi
fi

mkdir -p "$install_dir" || fail "cannot create install directory: $install_dir"
[ -w "$install_dir" ] || fail "install directory is not writable: $install_dir"
install_one() {
    name=$1
    source="$extract/$top/$name"
    destination="$install_dir/$name"
    if [ ! -f "$source" ] || [ -L "$source" ]; then fail "archive is missing $name"; fi
    replacement="$destination.mm-new-$$"
    backup="$destination.mm-old-$$"
    [ "${MM_INSTALL_INJECT_FAILURE:-}" != "$name" ] || return 1
    cp "$source" "$replacement" || fail "could not stage $name"
    chmod 755 "$replacement" || { rm -f "$replacement"; fail "could not make $name executable"; }
    if [ -e "$destination" ]; then mv "$destination" "$backup" || { rm -f "$replacement"; fail "could not replace $destination"; }; fi
    if ! mv "$replacement" "$destination"; then [ ! -e "$backup" ] || mv "$backup" "$destination"; fail "atomic replacement failed for $name; previous binary restored"; fi
}
rollback() {
    for name in mm mm-ingress-helper mmd; do
        destination="$install_dir/$name"
        backup="$destination.mm-old-$$"
        if [ -e "$backup" ]; then rm -f "$destination"; mv "$backup" "$destination"; fi
    done
}
trap 'rollback; cleanup' EXIT HUP INT TERM
install_one mm || fail "injected or filesystem failure while staging mm"
if [ "$with_helper" -eq 1 ]; then install_one mm-ingress-helper || fail "injected or filesystem failure while staging mm-ingress-helper"; fi
if [ "$with_daemon" -eq 1 ]; then install_one mmd || fail "injected or filesystem failure while staging mmd"; fi
rm -f "$install_dir/mm.mm-old-$$" "$install_dir/mm-ingress-helper.mm-old-$$" "$install_dir/mmd.mm-old-$$"
trap cleanup EXIT HUP INT TERM
printf 'Installed mm %s (%s) to %s\n' "$version" "$tag" "$install_dir/mm"
if [ "$no_modify_path" -eq 0 ]; then
    case ":${PATH:-}:" in *":$install_dir:"*) ;; *) printf 'Add %s to PATH; the installer did not edit shell startup files.\n' "$install_dir" ;; esac
fi
