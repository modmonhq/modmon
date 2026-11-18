# mm

Turn a list of servers into one cluster. `mm` is a Docker Swarm replacement:
it runs your Docker Compose project across every host you list, with a private
network between them, service names that resolve on every node, rolling
deploys that roll back, HTTPS routes into every node, autoscaling, shared
volumes, and one merged log:

```yaml
# .config/mm.yaml
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
      - {name: db-1, host: 203.0.113.13}
```

That is a complete cluster target. [Run a cluster](docs/how-to/cluster.md)
takes it to a deployment, and [Coming from Docker Swarm](docs/how-to/from-docker-swarm.md)
maps what you already know.

The same configuration runs the project on your own machine with Docker
Compose or Podman Compose: container builds, environments, secrets, and
trusted local HTTPS. Coding agents work on it through
[limits you set](docs/mcp.md).

The full local HTTPS, dashboard, and Mailpit setup requires Linux with systemd,
or macOS with Docker Desktop and Homebrew. See [supported features and platforms](docs/reference/capability-matrix.md).

## Get started

On Linux or macOS:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://modmon.dev/install.sh | sh
mm dev setup
```

The installer verifies the release and puts `mm` in `~/.local/bin` without sudo.
`mm dev setup` prepares local trust and shared services, and asks before it
changes anything. To build from this checkout instead, run `./install.sh`.

1. [Install mm](docs/start/install.md) for requirements,
   options, and updates.
2. [Run your first local app](docs/tutorials/first-local-app.md).
   The Node example includes a frontend, API, and MongoDB.
3. [Add mm to your project](docs/onboarding.md) when you are ready.

Already working in an mm project? Check its configuration and running services:

```sh
mm config validate
mm doctor
mm service status
```

Run `mm` without arguments in a terminal for the guided interface.

## How it fits your project

Keep container commands, health checks, and volumes in Compose. Put image
repositories, environments, secret references, and routes in `.config/mm.yaml`.
Secret values stay in a separate store.

Before a deployment, `mm` shows what will change. Review the plan, then pass its
printed hash to apply it:

```sh
mm deploy plan
mm deploy apply --plan PLAN_HASH --yes
```

Replace `PLAN_HASH` with the hash from the first command. If the configuration
changes, create a fresh plan. Projects that build their own images need a
published build first; the [build guide](docs/concepts/builds-and-deployments.md)
walks through that step.

## Documentation

- [Getting started](docs/getting-started.md)
- [Run a cluster](docs/how-to/cluster.md)
- [Examples: Node, .NET, Laravel + Rust](examples/README.md)
- [Configuration reference](docs/reference/configuration.md)
- [CLI reference](docs/reference/cli.md)
- [Let an AI agent work on your project](docs/mcp.md)
- [Troubleshooting](docs/help/troubleshooting.md)
- [Known limitations](docs/reference/known-issues.md)

## Contribute

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, the code map, and checks.
The Rust application is in `core/` (Rust 1.98.0, pinned in
`rust-toolchain.toml`) and the web dashboard in `core/ui/`. The documentation
is Markdown in `docs/`; the website in `site/` renders it, using the UI
components in `packages/ui/`.

[Changelog](CHANGELOG.md) · [Security](SECURITY.md) ·
[Code of conduct](CODE_OF_CONDUCT.md) · [MIT license](LICENSE)
