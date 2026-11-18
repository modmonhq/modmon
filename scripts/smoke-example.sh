#!/bin/sh
set -eu

example=${1:-}
case "$example" in
  node-fullstack|dotnet-web-api|laravel-rust) ;;
  *)
    printf '%s\n' "usage: $0 {node-fullstack|dotnet-web-api|laravel-rust}" >&2
    exit 2
    ;;
esac

MM_BIN=${MM_BIN:-target/debug/mm}
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
source_root=$root/examples/$example
temporary=$(mktemp -d "${TMPDIR:-/tmp}/mm-example-smoke.XXXXXX")
home=$temporary/home
overlay=$temporary/smoke.yaml
project=mm-smoke-$example-$$
mailpit_image=docker.io/axllent/mailpit:v1.27.8@sha256:6abc8e633df15eaf785cfcf38bae48e66f64beecdc03121e249d0f9ec15f0707
curl_image=docker.io/curlimages/curl:8.16.0@sha256:463eaf6072688fe96ac64fa623fe73e1dbe25d8ad6c34404a669ad3ce1f104b6
images=
compose_files=

cleanup() {
  if [ -n "$compose_files" ]; then
    # shellcheck disable=SC2086
    docker compose $compose_files -f "$overlay" -p "$project" down --volumes --remove-orphans >/dev/null 2>&1 || true
  fi
  for image in $images; do docker image rm "$image" >/dev/null 2>&1 || true; done
  "$MM_BIN" --mm-home "$home" dev tls uninstall >/dev/null 2>&1 || true
  rm -rf -- "$temporary"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$home"
chmod 700 "$home"
"$MM_BIN" --mm-home "$home" dev setup --no-trust >/dev/null
"$MM_BIN" --root "$source_root" --mm-home "$home" --runtime docker-compose config validate >/dev/null
"$MM_BIN" --root "$source_root" --mm-home "$home" --runtime docker-compose \
  --environment production config validate >/dev/null

build_image() {
  tag=$1
  context=$2
  shift 2
  docker build --memory=4g --cpu-period=100000 --cpu-quota=200000 --ulimit nproc=512 \
    "$@" -t "$tag" "$context" >/dev/null
  images="$images $tag"
}

case "$example" in
  node-fullstack)
    expected_host=frontend.node-fullstack.test
    "$MM_BIN" --root "$source_root" --mm-home "$home" --runtime docker-compose \
      --output json dev mounts plan frontend | grep -Fq '"preset": "node"'
    "$MM_BIN" --root "$source_root" --mm-home "$home" --runtime docker-compose \
      --output json dev mounts plan backend | grep -Fq '"/app/node_modules"'
    build_image mm-smoke-node-frontend:latest "$source_root/frontend"
    build_image mm-smoke-node-backend:latest "$source_root/backend"
    compose_files="-f $source_root/compose.yaml"
    cat >"$overlay" <<YAML
services:
  frontend:
    image: mm-smoke-node-frontend:latest
    environment:
      PUBLIC_API_URL: http://backend:3000
  backend:
    image: mm-smoke-node-backend:latest
    environment:
      FRONTEND_ORIGIN: http://frontend:8080
      MONGO_HOST: mongo
      MONGO_PORT: "27017"
      MONGO_DATABASE: notes
      MONGO_USERNAME: notes
      MONGO_PASSWORD: smoke-password
      MONGO_AUTH_SOURCE: admin
      MM_MAIL_SMTP_HOST: mailpit
      MM_MAIL_SMTP_PORT: "1025"
      MM_MAIL_UI_URL: http://mailpit:8025
  mongo:
    environment:
      MONGO_INITDB_ROOT_USERNAME: notes
      MONGO_INITDB_ROOT_PASSWORD: smoke-password
      MONGO_INITDB_DATABASE: notes
  mailpit:
    image: $mailpit_image
    cpus: "0.5"
    mem_limit: 128m
    pids_limit: 64
YAML
    ;;
  dotnet-web-api)
    expected_host=api.dotnet-web-api.test
    "$MM_BIN" --root "$source_root" --mm-home "$home" --runtime docker-compose \
      --output json dev mounts plan api | grep -Fq '"/source/obj"'
    build_image mm-smoke-dotnet-api:latest "$source_root/src/Example.Api"
    compose_files="-f $source_root/compose.yaml"
    cat >"$overlay" <<YAML
services:
  api:
    image: mm-smoke-dotnet-api:latest
    environment:
      ASPNETCORE_URLS: http://+:8080
      Database__Host: postgres
      Database__Port: "5432"
      Database__Name: widgets
      Database__Username: widgets
      Database__Password: smoke-password
      MM_MAIL_SMTP_HOST: mailpit
      MM_MAIL_SMTP_PORT: "1025"
      MM_MAIL_UI_URL: http://mailpit:8025
  postgres:
    environment:
      POSTGRES_DB: widgets
      POSTGRES_USER: widgets
      POSTGRES_PASSWORD: smoke-password
  mailpit:
    image: $mailpit_image
    cpus: "0.5"
    mem_limit: 128m
    pids_limit: 64
YAML
    ;;
  laravel-rust)
    expected_host=web.laravel-rust.test
    "$MM_BIN" --root "$source_root" --mm-home "$home" --runtime docker-compose \
      --output json dev mounts plan laravel | grep -Fq '"/app/vendor"'
    build_image mm-smoke-laravel-app:latest "$source_root/app"
    build_image mm-smoke-pricing:latest "$source_root/services/pricing"
    compose_files="-f $source_root/compose.data.yaml -f $source_root/compose.app.yaml"
    cat >"$overlay" <<YAML
services:
  web:
    image: mm-smoke-laravel-app:latest
    environment: &laravel-environment
      APP_NAME: Example Store
      APP_ENV: local
      APP_KEY: base64:MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=
      APP_DEBUG: "true"
      APP_URL: http://web:8080
      APP_ROLE: web
      LOG_CHANNEL: stderr
      SESSION_DRIVER: cookie
      DB_CONNECTION: pgsql
      DB_HOST: postgres
      DB_PORT: "5432"
      DB_DATABASE: application
      DB_USERNAME: application
      DB_PASSWORD: smoke-password
      CACHE_STORE: redis
      QUEUE_CONNECTION: redis
      REDIS_CLIENT: phpredis
      REDIS_HOST: valkey
      REDIS_PORT: "6379"
      PRICING_SERVICE_URL: http://pricing:8080
      MAIL_MAILER: smtp
      MAIL_HOST: mailpit
      MAIL_PORT: "1025"
      MAIL_FROM_ADDRESS: preview@laravel-rust.test
      MAILPIT_UI_URL: http://mailpit:8025
  queue:
    image: mm-smoke-laravel-app:latest
    depends_on:
      postgres:
        condition: service_healthy
      valkey:
        condition: service_healthy
    environment:
      <<: *laravel-environment
      APP_ROLE: queue
  pricing:
    image: mm-smoke-pricing:latest
    environment:
      PORT: "8080"
      VALKEY_HOST: valkey
      VALKEY_PORT: "6379"
  postgres:
    environment:
      POSTGRES_DB: application
      POSTGRES_USER: application
      POSTGRES_PASSWORD: smoke-password
  mailpit:
    image: $mailpit_image
    cpus: "0.5"
    mem_limit: 128m
    pids_limit: 64
YAML
    ;;
esac

# shellcheck disable=SC2086
docker compose $compose_files -f "$overlay" -p "$project" up -d --wait --wait-timeout 180 >/dev/null
network=${project}_default

case "$example" in
  node-fullstack)
    endpoint=http://backend:3000
    ;;
  dotnet-web-api)
    endpoint=http://api:8080
    ;;
  laravel-rust)
    endpoint=http://web:8080
    ;;
esac

docker run --rm --cpus=0.5 --memory=64m --pids-limit=32 --network "$network" \
  "$curl_image" --fail --silent --show-error "$endpoint/health" >/dev/null 2>&1 || \
docker run --rm --cpus=0.5 --memory=64m --pids-limit=32 --network "$network" \
  "$curl_image" --fail --silent --show-error "$endpoint/up" >/dev/null

if ! docker run --rm --cpus=0.5 --memory=64m --pids-limit=32 --network "$network" \
  "$curl_image" --fail --silent --show-error -H 'accept: application/json' -H 'content-type: application/json' \
  -d '{"to":"developer@example.test"}' "$endpoint/api/mail-preview" >/dev/null; then
  docker run --rm --cpus=0.5 --memory=64m --pids-limit=32 --network "$network" \
    "$curl_image" --include --silent --show-error -H 'accept: application/json' -H 'content-type: application/json' \
    -d '{"to":"developer@example.test"}' "$endpoint/api/mail-preview" || true
  # shellcheck disable=SC2086
  docker compose $compose_files -f "$overlay" -p "$project" logs --no-color
  exit 1
fi
messages=$(docker run --rm --cpus=0.5 --memory=64m --pids-limit=32 --network "$network" \
  "$curl_image" --fail --silent --show-error http://mailpit:8025/api/v1/messages)
case "$messages" in
  *"developer@example.test"*) ;;
  *) printf '%s\n' "Mailpit did not retain the example message" >&2; exit 1 ;;
esac

printf 'example=%s schema=1 ingress=%s mounts=reviewed health=ok mailpit=delivered cleanup=armed\n' \
  "$example" "$expected_host"
