#!/usr/bin/env bash
set -euo pipefail

MM_BIN=${MM_BIN:-target/debug/mm}
if [[ ! -x "${MM_BIN}" && -x "${MM_BIN}.exe" ]]; then
  MM_BIN="${MM_BIN}.exe"
fi
MM_TLS_PORT=${MM_TLS_PORT:-18443}
TEMP_ROOT=${RUNNER_TEMP:-/tmp}
CURL=/usr/bin/curl
case "$(uname -s)" in
MSYS* | MINGW* | CYGWIN*)
  TEMP_ROOT=$(cygpath -u "${TEMP_ROOT}")
  CURL=/c/Windows/System32/curl.exe
  ;;
Darwin*) CURL=/usr/bin/curl ;;
esac
MM_TLS_HOME=${MM_TLS_HOME:-"${TEMP_ROOT}/mm-live-local-tls-${RANDOM}"}
MM_TLS_PROJECT=${MM_TLS_PROJECT:-"${TEMP_ROOT}/mm-live-local-ingress-${RANDOM}"}
GENERATED_HOST=web.live-evidence.test
SERVER_PID=
CURL_TRUST=()

cleanup() {
  if [[ -n "${SERVER_PID}" ]]; then
    kill "${SERVER_PID}" 2>/dev/null || true
    wait "${SERVER_PID}" 2>/dev/null || true
  fi
  "${MM_BIN}" --mm-home "${MM_TLS_HOME}" dev tls uninstall >/dev/null 2>&1 || true
  rm -rf -- "${MM_TLS_HOME}" "${MM_TLS_PROJECT}"
}
trap cleanup EXIT INT TERM

if [[ "${MM_NO_SYSTEM_TRUST:-0}" == 1 ]]; then
  "${MM_BIN}" --mm-home "${MM_TLS_HOME}" dev setup --no-trust
  CURL_TRUST=(--cacert "${MM_TLS_HOME}/ca/ca.crt")
else
  "${MM_BIN}" --mm-home "${MM_TLS_HOME}" dev setup
fi
mkdir -p "${MM_TLS_PROJECT}/.config" "${MM_TLS_PROJECT}/app"
"${MM_BIN}" --root "${MM_TLS_PROJECT}" --mm-home "${MM_TLS_HOME}" \
  init --name live-evidence >/dev/null
read -r _ PROJECT_ID < <(grep '^  id: ' "${MM_TLS_PROJECT}/.config/mm.yaml")
cat >"${MM_TLS_PROJECT}/compose.yaml" <<'YAML'
services:
  web:
    image: docker.io/library/alpine:3.22
    expose: [8080]
YAML
cat >"${MM_TLS_PROJECT}/.config/mm.yaml" <<YAML
project:
  id: ${PROJECT_ID}
  name: live-evidence
  development-environment: local
default-environment: local
target: local
runtime:
  driver: podman-compose
stacks:
  app:
    files: [compose.yaml]
modules:
  app:
    path: app
    services:
      web:
        stack: app
environments:
  local:
    bindings: {}
YAML
"${MM_BIN}" --root "${MM_TLS_PROJECT}" --mm-home "${MM_TLS_HOME}" \
  --output json config ingress explain web >"${MM_TLS_HOME}/ingress.json"
grep -q "https://${GENERATED_HOST}" "${MM_TLS_HOME}/ingress.json"
grep -q '"provenance": "automatic_convention"' "${MM_TLS_HOME}/ingress.json"
"${MM_BIN}" --root "${MM_TLS_PROJECT}" --mm-home "${MM_TLS_HOME}" \
  --output json deploy plan >"${MM_TLS_HOME}/plan.json"
test -f "${MM_TLS_HOME}/ca/issued/${GENERATED_HOST}.crt"
test -f "${MM_TLS_HOME}/ca/issued/${GENERATED_HOST}.key"
"${MM_BIN}" --mm-home "${MM_TLS_HOME}" --output json dev tls status >"${MM_TLS_HOME}/status.json"

if [[ "${MM_NO_SYSTEM_TRUST:-0}" == 1 ]]; then
  grep -q '"trust_disabled": true' "${MM_TLS_HOME}/status.json"
else
  test "$(grep -c '"installed": true' "${MM_TLS_HOME}/status.json")" -ge 2
  grep -q '"mismatch": false' "${MM_TLS_HOME}/status.json"
fi
grep -q '"mm.test"' "${MM_TLS_HOME}/status.json"
grep -q "\"${GENERATED_HOST}\"" "${MM_TLS_HOME}/status.json"
if grep -q 'PRIVATE KEY' "${MM_TLS_HOME}/status.json"; then
  exit 3
fi

openssl s_server \
  -accept "127.0.0.1:${MM_TLS_PORT}" \
  -cert "${MM_TLS_HOME}/ca/issued/${GENERATED_HOST}.crt" \
  -key "${MM_TLS_HOME}/ca/issued/${GENERATED_HOST}.key" \
  -www >"${MM_TLS_HOME}/server.log" 2>&1 &
SERVER_PID=$!

for _ in $(seq 1 30); do
  if "${CURL}" --fail --silent --show-error --noproxy '*' \
    "${CURL_TRUST[@]}" \
    --resolve "${GENERATED_HOST}:${MM_TLS_PORT}:127.0.0.1" \
    "https://${GENERATED_HOST}:${MM_TLS_PORT}/" >"${MM_TLS_HOME}/response.txt"; then
    break
  fi
  sleep 1
done

grep -q 's_server' "${MM_TLS_HOME}/response.txt"
"${MM_BIN}" --mm-home "${MM_TLS_HOME}" dev tls uninstall
"${MM_BIN}" --mm-home "${MM_TLS_HOME}" --output json dev tls status >"${MM_TLS_HOME}/removed.json"
grep -q '"installed": false' "${MM_TLS_HOME}/removed.json"

printf 'generated local ingress name, leaf issuance, system curl trust, exact uninstall, and cleanup passed\n'
