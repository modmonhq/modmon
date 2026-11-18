# Contributing to mm

The repository is <https://github.com/modmonhq/modmon>. Start with a small change, run the checks for the area you touched, and include
the results in your pull request. Public command or configuration changes need
updated docs in the same PR.

## Set up

Run commands from the repository root. You need Docker for the repository's
build/check scripts, and Node.js 24 with npm 11 for the website and dashboard.
Rust 1.98.0 is pinned in `rust-toolchain.toml`.

Build the Rust toolchain image once:

```sh
docker build -f Dockerfile.cargo -t mm-rust:1.98.0 .
```

The `mm` binary embeds the web dashboard from `core/ui/dist`, which is not
tracked. Build it before compiling, and again after dashboard changes:

```sh
make dashboard-assets
```

With Rust installed locally, you can then run the CLI directly:

```sh
cargo run -p mm --features unstable -- --help
```

The `unstable` feature is required to build the binaries. See
[`core/README.md`](core/README.md) if you use the Rust library.

## Check your change

| Area | Run from the repository root |
| --- | --- |
| Documentation or website | `make docs links` |
| Rust code | `make rust deny` |
| Dashboard | `make ui` |
| Generated reference or schemas | `make generated` |
| Node examples | `make examples` |
| Release tooling | `make release release-source` |
| Public files | `make public-content` |
| Full repository check | `make quality` |

The [Makefile](Makefile) contains the commands behind each target. `make rust`
runs formatting, Clippy, compilation, tests, and Rustdoc in a bounded container.
`make docs` (or `make site`) installs locked dependencies, type-checks the
website, checks snippets and links, and builds the static site into
`site/.output/public`.

Compiling the `mm` crate's unit tests is the step closest to that container's
4 GB limit; `scripts/test-build-memory.sh` prints its peak. Put tests that need
only the public API in `core/tests/`, where they are compiled separately.

The documentation is Markdown in `docs/`. The website in `site/` renders it
under `/docs`, next to the landing page. To preview both:

```sh
npm --prefix site ci
npm --prefix site run dev
```

Then open <http://localhost:4317>.

`mm` refuses to run when a coding agent's environment variables are set (see
the [agent guide](docs/mcp.md)). Tests that start the binary
use `temp_dirs::mm()`, which removes those variables; use it for new tests too.
It also sets `MM_KEY_VAULT=file`, so a key a test creates keeps its passphrase
in the test's own directory and never in your keychain. Set it yourself when
you run a built `mm` against a throwaway `MM_HOME`.

Generate CLI reference, schemas, completions, and manpages with
`./scripts/generate-artifacts.sh`. Edit their sources, not the generated files.

Changes to runtime, deployment, or domain behavior also need the applicable
live runtime checks. These create and remove test resources; use a test machine:

```sh
./scripts/live-runtime-conformance.sh docker-compose
./scripts/live-runtime-conformance.sh podman-compose
```

On test hosts with exhausted default address pools, set `MM_CONFORMANCE_SUBNET`
and `MM_CONFORMANCE_COLLISION_SUBNET` to two unused, non-overlapping CIDRs.

Service environment filters also have a live removal/re-enable regression. With
the selected engine running, run:

```sh
MM_TEST_RUNTIME=docker-compose cargo test --features unstable --locked \
  --test live_service_environments -- --ignored
```

Repeat with `MM_TEST_RUNTIME=podman-compose`.
The test uses uniquely named, resource-limited workloads and removes them afterward.

Changes to cluster setup (`mm target setup` on a target with `nodes:`) need a
run against real hosts. Set up a disposable cluster with `mm target setup
apply`, then check every node, the mesh between them, and the storage export:

```sh
MM_CLUSTER_NODES='app-1=203.0.113.11=10.90.0.1 app-2=203.0.113.12=10.90.0.2' \
  MM_CLUSTER_SSH_KEY=~/.ssh/id_ed25519 ./scripts/live-cluster-setup.sh
```

Each entry is a node's name, public host, and private address. The script only
reads the nodes; its header lists the other variables.

Changes to deploying across the nodes of a cluster target (plans, apply,
`--nodes`, `mm target nodes`, the mesh `mmd` owns, `mm target mesh status`,
the networks that span the nodes and their firewall rules, the remote
protocol) need the three-node check. It starts three small
Docker-in-Docker hosts with sshd, the relay, and `mmd`, and removes them
afterward. The hosts are privileged containers that create WireGuard
interfaces and load iptables rules, so the machine's kernel needs WireGuard:

```sh
./scripts/live-cluster.sh
```

Changes to rootful Podman on remote hosts need the same check with Podman
nodes (Debian 13 containers with Podman and Docker Compose as its provider):

```sh
MM_LIVE_ENGINE=podman ./scripts/live-cluster.sh
```

Each [example](examples/README.md) lists its own additional requirements and
validation commands.

## Keep it fast

Speed is checked, not assumed. The bars, for a release build on a
workstation:

| What | Bar |
| --- | --- |
| `mm --help`, `mm --version`, shell completion | under 50 ms |
| First frame of the terminal UI, any environment | under 150 ms |
| A read (`mm service status`) on a kept connection | the host's own time plus four round trips |

`core/tests/startup_time.rs` holds the first two with budgets wide enough for
an unoptimized build on one shared CPU: it fails when a printing command
looks for a project, state, or an engine, and when the terminal UI's first
frame waits for an engine. Do not add a remote call, an engine call, or a
file walk to startup or to a read without need; ask the nodes of a cluster at
the same time when order does not matter.

To see where a command spends its time, set `MM_TIMING=1`. Every phase is
printed on standard error when it ends: when it began, counted from the start
of the process, how long it took, and what it was.

```sh
MM_TIMING=1 mm --environment production service status >/dev/null
```

```text
timing  +     2.2 ms      1.8 ms  config.load
timing  +     5.1 ms     97.9 ms  ssh.connect  node 'app-1' (203.0.113.11:22), kept
timing  +   103.0 ms    473.0 ms  rpc  status node 'app-1' (203.0.113.11:22)
timing  +   772.4 ms               command finished
```

`ssh.connect` ends when the host's `mmd` has answered the hello (`kept` marks
a connection that was already open), `rpc` is one request, `process` is one
command of an engine or tool, and the `tui.*` phases are the parts the
terminal UI reads after its first frame. The terminal UI draws on the
terminal, so send the trace to a file: `MM_TIMING=1 mm 2>timing.log`. `mmd`
prints the same for each request it serves when `MM_TIMING=1` is in its
environment; a systemd drop-in with `Environment=MM_TIMING=1` puts it in the
journal. To add a phase, hold `crate::app::timing::phase("name")` for as long
as it lasts; a phase costs nothing while the trace is off.

`scripts/time-terminal-ui.py` starts the terminal UI on a pseudo-terminal and
reports when the first frame was drawn and when a given text first appeared:

```sh
scripts/time-terminal-ui.py --mark '3/3 running' -- mm --environment production
```

Changes to the remote client or to what a read asks of a host need a timing
run against real hosts with real latency; a Docker-in-Docker host on the same
machine hides every round trip. From a project whose remote environment is
set up and deployed, with a release build:

```sh
MM_BIN=target/release/mm ./scripts/live-timing.sh --environment production
```

It only reads. It prints the median of each command and fails when
`--help`, `--version`, or completion take over 50 ms, the terminal UI's first
frame takes over 150 ms, or a status on kept connections takes over a second
or is not faster than the first one; its header lists the variables that
change the budgets.

## Find the code

| Directory | What belongs here |
| --- | --- |
| `core/src/domain` | Data types and rules without infrastructure access; compiled as the crate `mm-domain` (`core/domain`) |
| `core/src/app` | Workflows and interfaces for external operations |
| `core/src/adapters` | Docker, databases, files, and other integrations |
| `core/src/presentation` | CLI, terminal UI, HTTP, and MCP interfaces |
| `core/src/composition` | Wiring the application and integrations together |
| `core/ui` | Web dashboard |
| `docs` | Documentation, as Markdown |
| `site` | Website: landing page and rendered documentation |
| `packages/ui` | Shared Vue components and design tokens |
| `brand` | The mm mark and icon as SVG and PNG, generated by `scripts/generate-brand.mjs` |

Keep domain and application code independent of concrete adapters. Follow nearby
code conventions. Product code forbids `unsafe`, `unwrap`, `expect`, and `panic`.

## Open a pull request

- Explain the user-visible change and how you checked it.
- For runtime, ownership, or TLS changes, include the failure cases you tested.
- Keep secret values out of code, examples, logs, and reports.
- Preserve resource ownership checks and review-before-apply behavior.
- Describe the approach in the PR for significant changes to public APIs,
  configuration, state formats, secrets, or deployment behavior.

For publishing, follow the [release guide](docs/contribute/releases.md).
Report vulnerabilities privately using [SECURITY.md](SECURITY.md).
