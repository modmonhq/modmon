# Examples

Start with Node to try the full local workflow. The other examples show how the
same setup works with different frameworks.

| Example | What runs |
| --- | --- |
| [Node full stack](./node-fullstack) | Frontend, API, MongoDB, and development email |
| [.NET web API](./dotnet-web-api) | ASP.NET Core with live reload, PostgreSQL, and development email |
| [Laravel + Rust](./laravel-rust) | Laravel web and queue, a Rust API, PostgreSQL, Valkey, and scheduled jobs |

## Before running

- [Install mm](../docs/start/install.md).
- Use Linux with systemd and HAProxy for the full local HTTPS and Mailpit setup.
- Install Docker Compose or Podman Compose, plus Docker Buildx for image builds.
- Replace `ghcr.io/example/...` image repositories with ones you can push to.
- Commit the changes; publishing needs a clean checkout with full history and tags.

The [first local app tutorial](../docs/tutorials/first-local-app.md)
walks through all of these steps with the Node example. Each example's README
includes its endpoints and verification commands. Examples are for development.

## How the examples are configured

`.config/mm.yaml` contains build repositories, environment bindings, secret
references, managed networks, and routes. Compose files contain container
commands, health checks, third-party images, and volumes.

Local source mounts let you edit code while preserving dependencies in managed
volumes. Mailpit receives development email. DNS or hosts-file entries are still
needed to open generated `.test` URLs in your browser.

## Check an example without deploying it

From its directory, run `mm config validate`. To validate the Compose model with
Docker, include its `compose.validation.yaml` file after the source Compose
file(s). The validation file supplies placeholder images and must not be used
to run the application.

The repository's `scripts/smoke-example.sh` tests built images in disposable
containers. Language dependencies use the lockfiles in each example. The Rust
pricing service has its own Cargo workspace and lockfile.
