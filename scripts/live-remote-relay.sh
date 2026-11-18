#!/usr/bin/env bash
set -euo pipefail

# Remote access certification: relay-only SSH account, repo-declared users,
# git-fetched deploys, deploy-from, and revocation.
exec "$(dirname "${BASH_SOURCE[0]}")/live-remote-access.sh" "$@"
