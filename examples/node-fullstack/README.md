# Node full stack

```text
browser -> frontend:8080 -> backend:3000 -> MongoDB
                              |
                              +-> shared Mailpit SMTP
```

This example runs a frontend, API, and MongoDB with local HTTPS and development
email. Follow the [first local app tutorial](../../docs/tutorials/first-local-app.md)
for the complete walkthrough.

## Prerequisites and setup

Use Linux with systemd and HAProxy, mm, Docker Buildx, and Docker or Podman
Compose. Replace both `ghcr.io/example/...` repositories with ones you can push
to, commit your changes, and use a full checkout with tags. From this directory:

```sh
mm --runtime docker-compose dev setup --services
mm target preference set-runtime docker-compose
mm config validate
mm secret init age
mm secret generate --missing
mm env review
mm build --publish plan
mm build --publish apply --plan BUILD_PLAN_HASH --yes
mm deploy plan
mm deploy apply --plan DEPLOY_PLAN_HASH --yes
```

Use `podman-compose` consistently instead if Podman owns the local target.
Local uses reviewed `env-file` delivery; production uses direct process
delivery. Generated environment and secret files are private operation
artifacts and are never committed.

## Local URLs

- Frontend: `https://frontend.node-fullstack.test`
- API: `https://backend.node-fullstack.test`
- Installation dashboard: `https://mm.test`
- Mailpit: `https://mail.mm.test`

Find the project dashboard from `https://mm.test`. Browser access requires DNS
or hosts-file entries for the `.test` names. Replace each `*_PLAN_HASH` above
with the hash printed by the corresponding plan command.

Send a real development message, then inspect it in Mailpit:

```sh
curl --resolve backend.node-fullstack.test:443:127.0.0.1 \
  https://backend.node-fullstack.test/health
curl --resolve backend.node-fullstack.test:443:127.0.0.1 \
  -H 'content-type: application/json' \
  -d '{"to":"developer@example.test"}' \
  https://backend.node-fullstack.test/api/mail-preview
```

The health request must return `{"status":"ok"}` and the mail request must
return HTTP 202 with `"accepted":true` before the message appears in Mailpit.

The Node presets mount each source directory at `/app` while named volumes
preserve `node_modules`. `mm service logs backend --follow`, `mm service exec backend -- node
--version`, and the dashboard help you check the running services.

## Data and cleanup

This example is for development. MongoDB uses the initial
password only for an empty data volume. Coordinate password changes with the
database rather than rotating only the mm secret.

Public bindings such as URLs and database names belong in `mm.yaml`; the MongoDB
password is only a secret declaration there, while its value stays in the Age
store. Stop this local deployment and optionally remove shared services with:

```sh
mm dev down --yes
mm dev services plan --operation remove
mm dev services remove --plan SERVICES_PLAN_HASH --yes
```

Add `--purge` to both service commands only when Mailpit and access data may be
destroyed.
