---
title: Install mm
description: Install mm with one command on Linux or macOS, or build it from a checkout.
---

Run the installer on Linux or macOS:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://modmon.dev/install.sh | sh
```

The installer downloads the latest release for your platform from
[GitHub](https://github.com/modmonhq/modmon/releases), checks the archive's
SHA-256 against the release's `SHA256SUMS` before it unpacks anything, and
installs `mm` in `~/.local/bin`, on Linux with `mm-ingress-helper` beside it. It
does not use sudo and does not edit shell startup files. If the install
directory is not on your `PATH`, the installer prints the directory to add.

It needs `curl` or `wget`, `tar`, and one of `sha256sum`, `shasum`, or `openssl`.
Releases are built for Linux (glibc) and macOS, each on x86-64 and ARM64. See
[supported platforms](../reference/capability-matrix.md) for what each runtime
and operating system supports.

Check the result, then set up local trust and shared services:

```sh
mm --version
mm dev setup
```

`mm dev setup` shows its plan and asks before it changes anything. Run `mm`
without arguments for the guided interface.

## Installer options

Pass options after `sh -s --`:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://modmon.dev/install.sh | sh -s -- --with-daemon
```

| Option                         | Effect                                                                                   |
| ------------------------------ | ---------------------------------------------------------------------------------------- |
| `--version vMAJOR.MINOR.PATCH` | Install that release instead of the latest; a candidate is `vMAJOR.MINOR.PATCH-rc.N`     |
| `--install-dir PATH`           | Choose a different binary directory                                                      |
| `--with-daemon`                | Also install `mmd`; does not start a service                                             |
| `--force`                      | Allow a downgrade, or replace an `mm` whose version is unknown                           |
| `--no-modify-path`             | Do not print the `PATH` reminder                                                         |

## Update mm

Update an installation from a release with `mm update`:

```sh
mm update --check
mm update
```

`--check` only says whether a newer stable release exists. `mm update` reads
the latest release's `manifest.json`, prints what it replaces, such as
`mm 0.1.1 → 0.1.2 in ~/.local/bin: mm, mmd`, and asks first. It downloads the
archive for your platform, checks its SHA-256 and size against the manifest,
runs the new `mm --version`, and then replaces `mm`, and `mmd` and
`mm-ingress-helper` when they sit beside it, all at once: if one cannot be
replaced, every one is put back.

The same plan and confirmation cover the rest of the update:

- **Local services.** The local dashboard control API runs `mm` as a user
  service (`mm-dashboard.service` under systemd on Linux, the launchd agent
  `local.mm.dashboard` on macOS). `mm update` restarts it so it runs the new
  release, and checks it is still running a few seconds later. Local ingress
  (`mm-ingress.service`) starts Traefik through Compose, not `mm`, so it keeps
  running. Without a new release, `mm update` restarts only a service whose
  process started before the `mm` on disk was written.
- **MCP servers.** `mm mcp serve` processes belong to the agent client that
  started them. `mm update` lists each one still on the old `mm` by process
  ID; reload the client's MCP servers, or restart the client.
- **Hosts.** Run in a project, `mm update` asks the nodes of each remote
  environment which `mmd` they run (`--environment NAME` checks one), and for
  each environment with an older node prints the command that plans the
  upgrade, such as `mm --environment production target setup plan --upgrade`.
  That plan prints the apply command with every flag it needs. Hosts are not
  changed by `mm update`.

It ends with what runs which release:

```text
Updated mm 0.1.1 to 0.1.2 in ~/.local/bin: mm, mmd.

Now running:
  mm on this machine    0.1.2
  mm-dashboard.service  0.1.2, restarted
  mm-ingress.service    runs /usr/bin/podman, not mm; left running
Environment production (cluster fleet):
  app-1  mmd 0.1.1, older
  app-2  mmd 0.1.1, older
Upgrade the hosts of production:
  mm --environment production target setup plan --upgrade
  then run the apply command that plan prints
```

Expected result: `mm --version` prints the new release, and every local
service in the summary shows it. A service that did not come back is named
with the commands that show why, such as `systemctl --user status
mm-dashboard.service`, and `mm update` exits with status 1. A node that did not
answer is listed with the reason; `mm --environment NAME target nodes` asks
again.

| Option             | Effect                                                                                 |
| ------------------ | -------------------------------------------------------------------------------------- |
| `--version vX.Y.Z` | Install that release; a candidate (`vX.Y.Z-rc.N`) is checked against its `SHA256SUMS` |
| `--force`          | Install an older release named with `--version`, the same release again, or replace an `mm` built from a checkout |
| `--yes`            | Do not ask before replacing and restarting; needed when standard input is not a terminal |

`mm update` refuses, and names what to run instead, when the directory of
`mm` is not writable (run `sudo mm update`, or install into a directory you
own with the installer's `--install-dir`), when `mm` was built from a checkout
(update the checkout, see below), and when no release is built for your
platform. On Linux, if local ingress runs a root-owned copy of
`mm-ingress-helper` that differs from the new one, it prints the `sudo install`
command that refreshes it. Hosts keep the release they run until you upgrade
them: see [upgrade mm and your hosts](../how-to/upgrade.md).

Running the installer again also updates `mm`: it replaces the existing
binaries only after the new archive is verified, and refuses to downgrade
without `--force`. Use it when `mm update` says a release needs a newer `mm`.

A release candidate prints the version it leads to, so `mm --version` cannot
tell `0.1.2-rc.1` from `0.1.2`. After a candidate, install the final release
with `mm update --version v0.1.2 --force`.

## Install a release from GitHub

Each release on [GitHub](https://github.com/modmonhq/modmon/releases) carries
an archive for each platform, the source archive, `SHA256SUMS`, and a build
provenance attestation for every file. The Linux archives hold `mm`, `mmd`, and
`mm-ingress-helper`; the macOS archives hold `mm` and `mmd`. Set `target` to
`aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, or `aarch64-apple-darwin`
for another platform:

```sh
version=v0.1.1
target=x86_64-unknown-linux-gnu
base=https://github.com/modmonhq/modmon/releases/download/$version
curl --proto '=https' --tlsv1.2 -LsSfO "$base/mm-$version-$target.tar.gz"
curl --proto '=https' --tlsv1.2 -LsSfO "$base/SHA256SUMS"
sha256sum --check --ignore-missing SHA256SUMS
```

Expected result: `mm-v0.1.1-x86_64-unknown-linux-gnu.tar.gz: OK`. On macOS, use
`shasum -a 256 --check --ignore-missing SHA256SUMS`. Stop if the check fails.

To check that the archive was built by the release workflow of this
repository, use the [GitHub CLI](https://cli.github.com):

```sh
gh attestation verify "mm-$version-$target.tar.gz" --owner modmonhq   --signer-workflow github.com/modmonhq/modmon/.github/workflows/release.yml
```

Expected result: `Verification succeeded!`. Then install the binaries into one
directory:

```sh
tar -xzf "mm-$version-$target.tar.gz"
install -m 0755 "mm-$version-$target/mm" "mm-$version-$target/mmd" ~/.local/bin/
install -m 0755 "mm-$version-$target/mm-ingress-helper" ~/.local/bin/
mm --version
```

Skip the second `install` on macOS. Keep `mm-ingress-helper` beside `mm`: for
local HTTPS, `mm ingress start` copies it with sudo to `/usr/local/libexec/mm/`,
the one place local ingress runs a root-owned program from. The archive also
holds shell completions in `completions/` and the manual page in `man/`.

A release may also carry `SHA256SUMS.sig`, the maintainer's SSH signature over
`SHA256SUMS`. To check it (OpenSSH 8.2 or newer), take `allowed_signers` from
the repository, never from the release download, since a signature checked
against a key that came with the same download proves nothing:

```sh
curl --proto '=https' --tlsv1.2 -LsSfO "$base/SHA256SUMS.sig"
curl --proto '=https' --tlsv1.2 -LsSf -o allowed_signers \
  https://github.com/modmonhq/modmon/raw/master/packaging/release/allowed_signers
ssh-keygen -Y verify -f allowed_signers -n mm-release -s SHA256SUMS.sig \
  -I "$(ssh-keygen -Y find-principals -f allowed_signers -s SHA256SUMS.sig)" <SHA256SUMS
```

Expected result: `Good "mm-release" signature for …`. In a clone,
`scripts/release/verify.sh --checksums-only DIRECTORY` checks every file of a
downloaded release; without `--checksums-only` it also requires
`SHA256SUMS.sig` and checks it.

## Build from a checkout

To build mm yourself, clone the repository and run this from its root:

```sh
git clone https://github.com/modmonhq/modmon.git mm
cd mm
./install.sh
```

The installer builds mm and its dashboard, installs the binary in
`~/.local/bin`, sets up your shell's `PATH`, and runs workstation setup: wildcard
`.test` DNS, a trusted local certificate authority, and a reviewed shared ingress,
dashboard, and Mailpit service plan. Setup prompts for sudo and plan approval.
You do not need to install Rust or Node.js yourself.

### Requirements

| Platform                      | What you need                                                                                                                  |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Linux, x86-64 or ARM64        | Local Docker with Compose, systemd, active systemd-resolved, and sudo access                                                   |
| macOS, Intel or Apple Silicon | Apple Command Line Tools; for setup, Docker Desktop as the active Docker context, [Homebrew](https://brew.sh), and sudo access |

On macOS, install the command-line tools first if needed:

```sh
xcode-select --install
```

Wait for that installation to finish, then run `./install.sh` again.

Linux setup installs missing `dnsmasq` and `haproxy` packages using DNF, APT, or
Pacman. Other distributions need those packages installed beforehand. Existing
working wildcard `.test` resolution is reused. Otherwise setup installs the
dedicated `mm-test-dns` service and host-only DNS interface, routing only `.test`
queries there. It does not add per-project entries to `/etc/hosts`.

macOS setup installs missing `haproxy` and `dnsmasq` formulae with Homebrew as
your user. Local HTTPS on macOS is supported with Docker Desktop only: if
`docker context show` names another engine, such as Colima, run
`docker context use desktop-linux` before setup and when running mm. Existing
working wildcard `.test` resolution is reused. Otherwise setup adds
`/etc/resolver/test`, which sends only `.test` queries to a dnsmasq instance on
the dedicated loopback address `127.0.77.53`. That dnsmasq runs from a
root-owned copy in `/Library/PrivilegedHelperTools/`, started by the
`local.mm.dns` and `local.mm.loopback-alias` launch daemons.

The default setup runtime is Docker Compose. If an existing installation uses
Podman Compose, run setup with `MM_SETUP_RUNTIME=podman-compose` instead; do not
switch the runtime owning shared services implicitly.

#### Privileges and binary ownership

The normal `~/.local/bin/mm` remains owned by you and runs as your user. Setup
installs a separate, protected `mm-ingress-helper` under `/usr/local/libexec/mm/`.
That helper accepts only fixed, reviewed ingress installation/removal operations;
it is neither setuid nor a persistent root mm process. Setup does not install a
passwordless sudo rule.

On SELinux-enabled hosts, the helper installs a fixed `mm_ingress` policy and
labels only the two target-specific backend TCP ports. This permits HAProxy's
domain transition under the hardened systemd unit and its connections to mm's
loopback backend. SELinux stays enforcing; setup does not enable the global
`haproxy_connect_any` boolean. Fedora may require the
`policycoreutils-python-utils` package, which workstation setup installs when
missing.

On macOS, setup uses sudo only for `.test` DNS and, if you choose it, system-wide
certificate trust. macOS lets any user listen on ports 80 and 443 on all
addresses, so HAProxy, the Compose autostart, and the dashboard run as your user
in launchd agents under `~/Library/LaunchAgents/` (`local.mm.ingress-forwarder`,
`local.mm.ingress`, and `local.mm.dashboard`). mm writes their configuration
to `~/Library/Application Support/mm/` and their output to `~/Library/Logs/mm/`.
At login, `local.mm.ingress` retries every 15 seconds until Docker Desktop is
running, then starts the ingress containers. The dashboard listens only on
`127.0.0.1`. No `mm-ingress-helper` is installed on macOS.

Once the public-port forwarder is installed, ordinary site routing changes reuse
it when its ownership, fixed configuration, and availability still match. They
do not elevate the CLI. Changing or removing the host-level forwarding setup
requires sudo again. Rebuilds keep the CLI user-owned; `--setup-only` refreshes
the protected helper when necessary.

As its last step, setup registers the mm MCP server with Claude Code and
OpenCode when they are on `PATH`, so their agents can
[work on your projects](../mcp.md). It changes only each tool's own user
configuration. `--no-setup` skips it; run `mm agent setup --user --yes` to do
it later, or for Codex and Cursor.

Use `mm ingress stop` to free public ports 80/443 for another development stack,
and `mm ingress start` to resume mm's public forwarding. These global commands
leave site containers and data running.
`start` also installs or repairs missing ingress prerequisites idempotently,
using the existing shared-services runtime and reviewed plans when needed.

The first build needs internet access, several GB of free disk space, and time
to download and compile dependencies. Linux builds run in a Docker container
limited to 1 CPU and 4 GiB RAM, without extra container swap. Docker must run on the same machine and CPU
architecture; a remote Docker daemon is not supported.

On macOS, the installer downloads its own Rust and Node.js toolchains into a
private user cache.

### Check it works

Open a new terminal, then run:

```sh
mm --version
mm --help
```

The installer configures Bash, Zsh, and Fish. For another shell, add the printed
install directory to `PATH` yourself. To use mm immediately without opening a
new terminal:

```sh
"$HOME/.local/bin/mm" --help
```

If your shell finds an older mm, use the `PATH` command printed at the end of
installation. `command -v mm` shows which copy your shell is using.

For Bash and Zsh, the installer also disables the legacy `alias mm=./mm` when
you reload the configured startup file or open a new terminal. To clear that
alias in an already-open session, run `unalias mm`, or use the current-session
command printed by the installer. Other custom aliases are preserved.

### Update

`mm update` refuses to replace an `mm` built from a checkout, since the
checkout's `install-receipt.txt` sits beside it. From your checkout, get the
latest changes and rerun the installer:

```sh
git pull
./install.sh
```

It builds the current checkout, including local edits, even when the package
version has not changed. Downloaded dependencies and compilation results are
cached to speed up later builds.

Normal installs use a fast incremental local build. For code-edit/rebuild cycles
after workstation setup:

```sh
./install.sh --no-setup
```

The local profile disables link-time optimization and debug-info generation,
retains incremental compilation, and caches optimized dependencies. Linux uses
the Rust toolchain's LLD linker with one linking thread. The first build populates
the cache; later builds reuse it. Unchanged build inputs reuse the cached binaries
without rerunning npm or Cargo. Do not delete the cache to resolve an installer
lock—it makes the next build cold again.

Use `./install.sh --release` when you specifically want the slower, fully optimized
release build. Use `--rebuild` to force a build without deleting dependencies.
Linux builds remain limited to 1 CPU and 4 GiB RAM in either mode, and cannot use
extra swap. The default Cargo job count is one.

To finish or retry workstation setup without compiling anything:

```sh
./install.sh --setup-only
```

A setup failure leaves the successfully installed binary in place and reports
that workstation setup is incomplete. Fix the reported prerequisite and retry
`--setup-only`, rather than rebuilding.

If a previous shared-services operation left an interrupted journal, setup first
offers a reviewed repair, then a fresh convergence plan. Neither setup plan
purges data. Cancelling or leaving the service stack unhealthy keeps setup in an
incomplete state instead of reporting success.

### Options

| Option               | Effect                                                                                                                             |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `--install-dir PATH` | Choose a different binary directory                                                                                                |
| `--no-modify-path`   | Leave shell startup files alone                                                                                                    |
| `--no-setup`         | Install the binary without changing workstation DNS, trust, or services                                                            |
| `--setup-only`       | Set up the workstation and coding tools using the installed binary; skip compilation                                               |
| `--dev`              | Select the fast incremental local build, which is the default                                                                      |
| `--release`          | Build the slower optimized release binary                                                                                          |
| `--rebuild`          | Force a build instead of reusing an unchanged cached binary                                                                        |
| `--with-daemon`      | Also build and install `mmd`; does not start a service                                                                             |
| `--build-only`       | Write binaries under `target/local-install/<target>/` without installing them into your normal binary directory or changing `PATH` |
| `--jobs NUMBER`      | Set parallel Rust compilation jobs; default is `1`                                                                                 |
| `--help`             | Show usage and requirements                                                                                                        |

For example:

```sh
./install.sh --install-dir "$HOME/bin" --no-modify-path
```

Use a user-writable destination and run the installer without `sudo`.

### If installation fails

- **Docker unavailable:** start Docker and check that `docker info` works as your
  normal user, then rerun the installer.
- **Command Line Tools missing:** complete `xcode-select --install` on macOS.
- **Build uses too much memory:** use the default local profile and one job.
  The Linux container is limited to 4 GiB with no extra swap.
- **Another installer is running:** the message identifies its PID and host.
  Locks with an exited local owner are recovered automatically. An ownerless
  lock is left alone until running installers have been checked; do not delete
  the dependency cache to clear it.
- **Build error:** inspect the build log path printed by the installer.
- **DNS, trust, or ingress setup failed:** rerun `./install.sh --setup-only` from
  a terminal so sudo and the reviewed services plan can be approved. On Linux,
  inspect `systemctl status mm-test-dns.service` and
  `journalctl -u mm-test-dns.service` for DNS service failures. On macOS, check
  that `scutil --dns` lists a `test` resolver using `127.0.77.53`, and inspect
  `sudo launchctl print system/local.mm.dns`.
- **macOS reports another Docker endpoint:** local HTTPS on macOS needs Docker
  Desktop. Run `docker context use desktop-linux`, or unset `DOCKER_HOST`.

The installer builds and checks the new binaries before replacing your installed
copy. It also writes `install-receipt.txt` beside the binaries with the source
revision and build toolchain versions.

The cache is `${XDG_CACHE_HOME:-$HOME/.cache}/mm-installer`. You can delete it
when no installation is running; the next build will download dependencies again.

### Uninstall

For the default destination:

```sh
rm "$HOME/.local/bin/mm"
```

If you installed the optional daemon, remove `mmd` from the same directory too.
You can also remove `install-receipt.txt`. For a custom destination, remove the
files there instead.

Removing binaries leaves running services, certificates, and private state in
place. Use [local service removal](../local-services.md) and
[TLS removal](../local-https.md#uninstall) first if you want to remove those too.

If the installer created mm's dedicated DNS service, remove it separately:

```sh
sudo systemctl disable --now mm-test-dns.service
sudo rm /etc/systemd/system/mm-test-dns.service /etc/mm/dnsmasq.conf
sudo systemctl daemon-reload
```

Stopping that service removes its dedicated DNS interface. DNS provided by an
existing resolver that setup merely reused is not changed.

On macOS, remove mm's DNS integration from the checkout with:

```sh
./scripts/install/local-dns-macos.sh --remove
```

It unloads `local.mm.dns` and `local.mm.loopback-alias` and removes only the
files mm created: `/etc/resolver/test`, the two launch daemons, the dnsmasq
configuration, and the dnsmasq copy. Removing the local services also removes
the ingress and dashboard launch agents.

On NetworkManager hosts, setup also installs
`/etc/NetworkManager/conf.d/90-mm-test-dns.conf` to keep other connection
dispatcher hooks from rewriting mm's DNS interface. Remove that mm-owned file
and run `sudo nmcli general reload conf` when removing mm's DNS integration.
After removing ingress, the protected helper can be removed from
`/usr/local/libexec/mm/mm-ingress-helper`; its user-owned build copy is beside
the CLI in the install directory.

## Next step

Workstation setup prepares DNS, trust, and shared services. Project images,
environment values, and deployments are still project-specific. Bare `mm` guides
you through missing prerequisites. [Run your first local app](../tutorials/first-local-app.md)
for a complete example.
