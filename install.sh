#!/usr/bin/env bash
# Build and install this checkout. See --help for prerequisites and options.
# Re-exec under full Bash: dash has no BASH_VERSION, and macOS `sh` is Bash in
# POSIX mode, which rejects the process substitution used below.
case ${BASH_VERSION:-}:${SHELLOPTS:-} in
    :*|*posix*) [ -n "${MM_INSTALL_REEXEC:-}" ] || MM_INSTALL_REEXEC=1 exec bash "$0" "$@" ;;
esac
# Parse the complete installer before a long build. Editing this checkout while
# installation runs must not change the shell program under its open file cursor.
main() {
set -euo pipefail
umask 022
root=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
install_dir=${HOME:?HOME must be set}/.local/bin
cache=${XDG_CACHE_HOME:-$HOME/.cache}/mm-installer
with_daemon=0
modify_path=1
build_only=0
setup_workstation=1
setup_only=0
build_profile=local
profile_directory=local
keep_work=1
force_build=0
jobs=1
usage() {
    cat <<'EOF'
Usage: ./install.sh [OPTIONS]
Build this checkout (including local edits) and install a native mm.
Run git pull, then rerun this command to update, even at the same package version.

  --with-daemon         Also build/install mmd (does not start a service)
  --install-dir PATH    Binary destination (default: ~/.local/bin)
  --no-modify-path      Do not update shell startup files
  --build-only          Write binaries to target/local-install/<target>/ only
  --no-setup            Rebuild/install binaries without workstation changes
  --setup-only          Set up TLS, local .test DNS, services, and coding tools without rebuilding
  --dev                 Use the fast incremental local profile (default)
  --release             Use the slower optimized release profile
  --rebuild             Build even when inputs match the cached binary
  --jobs NUMBER         Parallel Rust compilation jobs (default: 1)
  -h, --help            Show this help

Linux x86-64/ARM64: Docker Compose, systemd, systemd-resolved, and sudo for setup.
macOS Intel/Apple Silicon: Apple Command Line Tools to build; setup needs Docker
Desktop, Homebrew (installs HAProxy and dnsmasq), and sudo for .test DNS only.
Pinned Rust/Node are downloaded into a private user cache on macOS.
Internet access and several GB of free disk space are needed for the first build.
Linux builds use a container limited to 1 CPU / 4 GiB RAM, with no extra swap.

Default cache: ${XDG_CACHE_HOME:-~/.cache}/mm-installer (safe to delete when idle).
Build logs: .modmon/install/ in this checkout. Build as your normal user; setup requests sudo.
Setup also registers the mm MCP server with Claude Code and OpenCode when they are on PATH.
Supported PATH setup: Bash, Zsh, Fish. Open a new terminal after installation.
Uninstall: remove mm, optional mmd, and install-receipt.txt from the destination.
EOF
}
fail() { printf 'mm installer: %s\n' "$*" >&2; exit 1; }
say() { printf '\n==> %s\n' "$*"; }
while (( $# )); do
    case $1 in
        --with-daemon) with_daemon=1; shift ;;
        --no-modify-path) modify_path=0; shift ;;
        --build-only) build_only=1; shift ;;
        --no-setup) setup_workstation=0; shift ;;
        --setup-only) setup_only=1; shift ;;
        --dev) build_profile=local; profile_directory=local; keep_work=1; shift ;;
        --release) build_profile=release; profile_directory=release; keep_work=0; shift ;;
        --rebuild) force_build=1; shift ;;
        --install-dir|--jobs)
            if (( $# < 2 )) || [[ -z $2 || $2 == --* ]]; then fail "$1 requires a value"; fi
            if [[ $1 == --install-dir ]]; then install_dir=$2; else jobs=$2; fi
            shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) fail "unknown option: $1 (see --help)" ;;
    esac
done
# Registers the mm MCP server with the AI coding tools on PATH, for this user.
# Temporary: it covers Claude Code and OpenCode without asking. A guided
# installer will offer each tool and its options.
connect_agents() {
    local mm=$1 clients=()
    if command -v claude >/dev/null 2>&1; then clients+=(--client claude-code); fi
    if command -v opencode >/dev/null 2>&1; then clients+=(--client opencode); fi
    if (( ${#clients[@]} == 0 )); then
        printf 'Claude Code and OpenCode were not found. To connect a coding tool later: mm agent setup --user --yes\n'
        return 0
    fi
    say "Connecting AI coding tools to the mm MCP server"
    "$mm" agent setup --user "${clients[@]}" --yes || printf 'Could not connect the coding tools. Run later from a terminal: mm agent setup --user --yes\n' >&2
}
if (( setup_only )); then
    (( setup_workstation && ! build_only )) || fail '--setup-only cannot be combined with --no-setup or --build-only'
    [[ -x $install_dir/mm ]] || fail "mm is not installed at $install_dir/mm; run ./install.sh first"
    bash "$root/scripts/install/workstation.sh" "$install_dir/mm" || exit "$?"
    connect_agents "$install_dir/mm"
    exit 0
fi
[[ $jobs =~ ^[1-9][0-9]*$ ]] || fail '--jobs must be a positive integer'
case $root$install_dir$cache in *$'\n'*|*$'\r'*|*:*) fail 'paths must not contain line breaks or colons' ;; esac
system=$(uname -s)
case $(uname -m) in x86_64) arch=x86_64 ;; arm64|aarch64) arch=aarch64 ;; *) fail 'only x86-64 and ARM64 are supported' ;; esac
case $system in
    Linux)
        target=$arch-unknown-linux-musl
        command -v docker >/dev/null || fail 'install Docker, start its daemon, then rerun ./install.sh'
        docker info >/dev/null 2>&1 || fail 'Docker is unavailable. Start Docker and ensure your user can access it (do not run this installer with sudo).'
        docker_arch=$(docker info --format '{{.Architecture}}')
        case $docker_arch in arm64) docker_arch=aarch64 ;; amd64) docker_arch=x86_64 ;; esac
        [[ $docker_arch == "$arch" ]] || fail 'Docker must run locally on the same CPU architecture as this host'
        ;;
    Darwin)
        target=$arch-apple-darwin
        xcrun --find clang >/dev/null 2>&1 || fail 'Apple Command Line Tools are required. Run xcode-select --install, finish installation, then rerun ./install.sh'
        command -v curl >/dev/null || fail 'curl is required to download private build toolchains'
        ;;
    *) fail "unsupported OS: $system (Linux and macOS only)" ;;
esac
[[ -f $root/core/Cargo.toml && -f $root/Cargo.lock ]] || fail 'run install.sh from a complete repository checkout'
if (( build_only )); then install_dir=$root/target/local-install/$target; fi
case $install_dir in /*) ;; *) install_dir=$PWD/$install_dir ;; esac
mkdir -p "$install_dir" "$cache" "$root/.modmon/install" || fail 'cannot create destination/cache/log directories; check permissions'
install_dir=$(CDPATH='' cd -- "$install_dir" && pwd -P)
cache=$(CDPATH='' cd -- "$cache" && pwd -P)
[[ -w $install_dir ]] || fail "destination is not writable: $install_dir"
log=$(mktemp "$root/.modmon/install/build.XXXXXX")
work=
stage=
container=
cache_locked=0
destination_locked=0
recovery_lock=
committed=0
changed=()
changed_count=0

record_lock_owner() { printf '%s %s\n' "$$" "${HOSTNAME:-unknown}" > "$1/owner"; }
lock_owner() {
    owner_pid='' owner_host=''
    [[ -d $1 && ! -L $1 && -O $1 && -f $1/owner && ! -L $1/owner && -O $1/owner ]] || return 1
    read -r owner_pid owner_host < "$1/owner" || return 1
    [[ $owner_pid =~ ^[1-9][0-9]*$ && -n $owner_host ]]
}
# Invoked indirectly by the EXIT trap's cleanup.
# shellcheck disable=SC2317,SC2329
release_lock() {
    if lock_owner "$1" && [[ $owner_pid == "$$" && $owner_host == "${HOSTNAME:-unknown}" ]]; then
        rm -f "$1/owner"
        rmdir "$1"
    fi
}
acquire_lock() {
    local path=$1 description=$2
    if mkdir -m 700 "$path" 2>/dev/null; then record_lock_owner "$path"; return; fi
    if lock_owner "$path" && [[ $owner_host == "${HOSTNAME:-unknown}" ]] && ! kill -0 "$owner_pid" 2>/dev/null; then
        # Serialize stale-lock recovery and recheck after taking the guard.
        mkdir -m 700 "$path.recovery" 2>/dev/null || fail "another installer is recovering $description; retry shortly"
        recovery_lock=$path.recovery
        if lock_owner "$path" && [[ $owner_host == "${HOSTNAME:-unknown}" ]] && ! kill -0 "$owner_pid" 2>/dev/null; then
            printf 'Recovering stale installer lock for exited PID %s: %s\n' "$owner_pid" "$path"
            rm -f "$path/owner"
            rmdir "$path"
        fi
        if mkdir -m 700 "$path" 2>/dev/null; then
            record_lock_owner "$path"
            rmdir "$recovery_lock"; recovery_lock=
            return
        fi
        rmdir "$recovery_lock"; recovery_lock=
    fi
    if lock_owner "$path"; then
        fail "another installer is using $description (PID $owner_pid on $owner_host)"
    fi
    fail "another installer is using $description (lock has no owner metadata; check running installers before removing $path)"
}
# Invoked indirectly by the EXIT trap below.
# shellcheck disable=SC2317,SC2329
cleanup() {
    status=$?
    set +e
    restore_failed=0
    trap - EXIT
    if [[ -n $container ]]; then docker rm -f "$container" >/dev/null 2>&1 || true; fi
    if (( ! committed && changed_count > 0 )) && [[ -n $stage ]]; then
        for name in "${changed[@]}"; do
            if [[ -f $stage/old-$name ]]; then
                mv -f "$stage/old-$name" "$install_dir/$name" || { restore_failed=1; printf 'Restore %s manually from %s\n' "$name" "$stage" >&2; }
            else rm -f "$install_dir/$name"; fi
        done
    fi
    [[ -z $work || $keep_work == 1 ]] || rm -rf "$work"
    [[ -z $stage || $restore_failed == 1 ]] || rm -rf "$stage"
    [[ -z $recovery_lock ]] || rmdir "$recovery_lock"
    (( ! destination_locked )) || release_lock "$install_dir/.mm-install.lock"
    (( ! cache_locked )) || release_lock "$cache/install.lock"
    if (( status != 0 )); then printf 'Installation failed. Build log: %s\n' "$log" >&2; fi
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP
acquire_lock "$cache/install.lock" "$cache"
cache_locked=1
acquire_lock "$install_dir/.mm-install.lock" "$install_dir"
destination_locked=1
names=(mm mm-ingress-helper)
if (( with_daemon )); then names+=(mmd); fi
for name in "${names[@]}" install-receipt.txt; do
    [[ ! -L $install_dir/$name && ( ! -e $install_dir/$name || -f $install_dir/$name ) ]] || fail "refusing to replace non-regular file: $install_dir/$name"
done
if (( keep_work )); then
    # Stable paths let Cargo reuse incremental codegen between local rebuilds.
    work=$cache/workspaces/$target
    mkdir -p "$work"
    rm -rf "$work/core" "$work/docs" "$work/scripts" "$work/packaging"
else
    work=$(mktemp -d "$cache/source.XXXXXX")
fi
stage=$(mktemp -d "$install_dir/.mm-stage.XXXXXX")
source_commit=$(git -C "$root" rev-parse HEAD 2>/dev/null || printf unknown)
source_dirty=unknown
if source_status=$(git -C "$root" status --porcelain --untracked-files=normal 2>/dev/null); then
    source_dirty=false
    [[ -z $source_status ]] || source_dirty=true
fi
say "Building $target from $root"
printf 'Build log: %s\n' "$log"
# Snapshot the working tree, excluding dependency trees and embedded build output.
# The archive also fingerprints the build recipe and toolchain inputs.
tar -C "$root" --exclude=node_modules --exclude=core/target --exclude=core/ui/dist --exclude=.env \
    --exclude='.env.*' --exclude=coverage --exclude=test-results --exclude=playwright-report --exclude='*.tsbuildinfo' --exclude=.mm \
    -cf "$work/source.tar" Cargo.toml Cargo.lock rust-toolchain.toml core docs/mcp.md packages/ui install.sh packaging/local/Dockerfile packaging/mm-relay-shell packaging/sshd/90-mm-relay.conf packaging/systemd/mmd.service packaging/systemd/mm-mesh.service packaging/systemd/mm-network.service scripts/install/build.sh scripts/install/workstation.sh scripts/install/local-dns.sh scripts/install/local-dns-macos.sh
tar -C "$work" -xf "$work/source.tar"
cp "$work/scripts/install/build.sh" "$work/build.sh"
if command -v sha256sum >/dev/null; then source_hash=$(sha256sum "$work/source.tar"); else source_hash=$(shasum -a 256 "$work/source.tar"); fi
build_key="$target:$build_profile:$with_daemon:${source_hash%% *}"
output_directory=$cache/target/$target/$target/$profile_directory
cached=0
if (( ! force_build )) && [[ -f $output_directory/build-inputs.txt ]] && [[ $(< "$output_directory/build-inputs.txt") == "$build_key" ]]; then
    cached=1
    for name in "${names[@]}"; do [[ -x $output_directory/$name ]] || cached=0; done
    [[ -f $output_directory/build-toolchains.txt ]] || cached=0
fi
mkdir -p "$cache/cargo" "$cache/target/$target" "$cache/npm"
fetch() { curl --proto '=https' --tlsv1.2 --fail --location --retry 3 --connect-timeout 30 --output "$2" "$1"; }
verify() {
    local file=$1 expected=$2 actual
    actual=$(shasum -a 256 "$file")
    [[ ${actual%% *} == "$expected" && $expected =~ ^[0-9a-f]{64}$ ]] || fail "checksum mismatch: $file"
}
mac_toolchains() {
    local node_version=v24.10.0 node_arch=x64 archive base expected rust_version
    [[ $arch != aarch64 ]] || node_arch=arm64
    export CARGO_HOME="$cache/cargo" RUSTUP_HOME="$cache/rustup"
    rust_version=$(sed -n 's/^channel = "\([^"]*\)"/\1/p' "$root/rust-toolchain.toml")
    [[ $rust_version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail 'invalid pinned Rust version'
    if [[ ! -x $CARGO_HOME/bin/rustup ]]; then
        base=https://static.rust-lang.org/rustup/archive/1.28.2/$target
        fetch "$base/rustup-init" "$work/rustup-init"
        fetch "$base/rustup-init.sha256" "$work/rustup.sha256"
        read -r expected _ < "$work/rustup.sha256"
        verify "$work/rustup-init" "$expected"
        chmod 755 "$work/rustup-init"
        "$work/rustup-init" -y --no-modify-path --profile minimal --default-toolchain none
    fi
    export PATH="$CARGO_HOME/bin:$PATH" RUSTUP_TOOLCHAIN="$rust_version"
    rustup toolchain install "$rust_version" --profile minimal --no-self-update
    rustup target add "$target" --toolchain "$rust_version"
    archive=node-$node_version-darwin-$node_arch
    if [[ ! -x $cache/$archive/bin/node ]]; then
        base=https://nodejs.org/dist/$node_version
        fetch "$base/$archive.tar.gz" "$work/node.tar.gz"
        fetch "$base/SHASUMS256.txt" "$work/node.sha256"
        expected=$(awk -v name="$archive.tar.gz" '$2 == name {print $1}' "$work/node.sha256")
        verify "$work/node.tar.gz" "$expected"
        tar -xzf "$work/node.tar.gz" -C "$work"
        rm -rf "${cache:?}/${archive:?}"
        mv "$work/$archive" "$cache/$archive"
    fi
    export PATH="$cache/$archive/bin:$PATH"
}
build() {
    if [[ $system == Linux ]]; then
        local image=mm-local-builder:rust-1.99.1-$arch
        docker build -t "$image" -f "$work/packaging/local/Dockerfile" "$work/packaging/local"
        container=mm-install-$(id -u)-$$
        docker run --rm --name "$container" --cpus=1 --memory=4g --memory-swap=4g --pids-limit=512 \
            --user "$(id -u):$(id -g)" -e HOME=/cache -e CARGO_HOME=/cache/cargo \
            -e CARGO_TARGET_DIR=/cache/target -e npm_config_cache=/cache/npm \
            -e CARGO_BUILD_JOBS="$jobs" -e CC=musl-gcc \
            -e MM_BUILD_PROFILE="$build_profile" \
            -v "$work:/workspace" -v "$cache/cargo:/cache/cargo" \
            -v "$cache/target/$target:/cache/target" -v "$cache/npm:/cache/npm" \
            -w /workspace "$image" bash ./build.sh "$target" "$with_daemon"
    else
        mac_toolchains
        export CARGO_TARGET_DIR="$cache/target/$target" CARGO_BUILD_JOBS="$jobs" npm_config_cache="$cache/npm" MM_BUILD_PROFILE="$build_profile"
        (cd "$work" && bash ./build.sh "$target" "$with_daemon")
    fi
}
# Avoid a pipeline here: toolchain environment and container cleanup state must
# remain in the parent shell. tee receives both build output streams.
if (( cached )); then
    say 'Build inputs unchanged; reusing cached binaries'
else
    build > >(tee -a "$log") 2>&1
    printf '%s\n' "$build_key" > "$output_directory/build-inputs.txt"
fi
for name in "${names[@]}"; do
    cp "$cache/target/$target/$target/$profile_directory/$name" "$stage/$name"
    chmod 755 "$stage/$name"
    "$stage/$name" --version
    "$stage/$name" --help >/dev/null
done
{
    printf 'target=%s\nsource=%s\nbuilt_at=%s\n' "$target" "$root" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'commit=%s\ndirty=%s\nbinaries=%s\n' "$source_commit" "$source_dirty" "${names[*]}"
    printf 'profile=%s\n' "$build_profile"
    cat "$cache/target/$target/$target/$profile_directory/build-toolchains.txt"
    "$stage/mm" --version
} > "$stage/install-receipt.txt"
say "Installing into $install_dir"
for name in "${names[@]}" install-receipt.txt; do
    if [[ -f $install_dir/$name ]]; then cp -p "$install_dir/$name" "$stage/old-$name"; fi
    changed+=("$name")
    changed_count=$((changed_count + 1))
    mv -f "$stage/$name" "$install_dir/$name"
done
committed=1
# Bash and Zsh print aliases differently. Only retire the old checkout alias;
# keep user-defined aliases targeting other commands intact.
alias_cleanup="case \"\$(alias mm 2>/dev/null)\" in \"alias mm='./mm'\"|\"mm='./mm'\"|\"mm=./mm\") unalias mm ;; esac"
if (( modify_path && ! build_only )); then
    # Shell-quote paths rather than interpolating them as executable shell code.
    quoted=$(printf '%s' "$install_dir" | sed "s/'/'\\\\''/g")
    line="case \":\$PATH:\" in *:'$quoted':*) ;; *) export PATH='$quoted':\$PATH ;; esac"
    shell_name=${SHELL:-}
    case ${shell_name##*/} in
        zsh) profile=${ZDOTDIR:-$HOME}/.zshrc ;;
        bash)
            if [[ $system == Darwin ]]; then
                profile=$HOME/.bash_profile
                for candidate in "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile"; do
                    if [[ -f $candidate ]]; then profile=$candidate; break; fi
                done
            else profile=$HOME/.bashrc; fi ;;
        fish)
            profile=${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d/mm-path.fish
            # Fish supports backslash and single-quote escaping inside quotes.
            quoted=$(printf '%s' "$install_dir" | sed "s/\\\\/\\\\\\\\/g; s/'/\\\\'/g")
            line="fish_add_path -- '$quoted'" ;;
        *) profile= ;;
    esac
    if [[ -n $profile ]]; then
        if mkdir -p "$(dirname "$profile")" && { [[ -f $profile ]] && grep -Fqx "$line" "$profile" || printf '\n# mm local install\n%s\n' "$line" >> "$profile"; }; then
            case ${shell_name##*/} in
                bash|zsh)
                    if ! grep -Fqx "$alias_cleanup" "$profile"; then
                        printf '\n# mm: retire the legacy checkout alias\n%s\n' "$alias_cleanup" >> "$profile" || printf 'Could not configure legacy alias cleanup in %s.\n' "$profile" >&2
                    fi ;;
            esac
            printf 'PATH configured in %s. Open a new terminal.\n' "$profile"
        else printf 'Could not configure PATH in %s; add %s manually.\n' "$profile" "$install_dir" >&2; fi
    fi
fi
if (( setup_workstation && ! build_only )); then
    say "Setting up local TLS, .test DNS, and shared services"
    bash "$work/scripts/install/workstation.sh" "$install_dir/mm" || fail 'Binary installed, but workstation setup is incomplete. Rerun ./install.sh --setup-only from a terminal; no rebuild is needed.'
    connect_agents "$install_dir/mm"
fi
say "Installed: $install_dir/mm"
printf 'Run now: %q --help\n' "$install_dir/mm"
if (( ! build_only )); then
    # shellcheck disable=SC2016
    printf 'For this Bash/Zsh session: %s; export PATH=%q:"$PATH"; hash -r\n' "$alias_cleanup" "$install_dir"
    resolved=$(command -v mm || true)
    if [[ -n $resolved && $resolved != "$install_dir/mm" ]]; then
        printf 'Note: your current PATH resolves mm to %s. Use the PATH command above.\n' "$resolved"
    fi
fi
}
main "$@"; exit "$?"
