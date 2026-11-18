# Security policy

## Supported versions

Security fixes target the latest release and the `master` branch. Releases
are listed at <https://github.com/modmonhq/modmon/releases>, each with
`SHA256SUMS` and build provenance attestations; see
[install a release from GitHub](docs/start/install.md#install-a-release-from-github)
to check them. To run a fix before it is released, update your checkout and
rebuild with `./install.sh`.

## Reporting a vulnerability

Do not open a public issue for security defects.

Report privately through the repository's security advisory form:
<https://github.com/modmonhq/modmon/security/advisories/new>.

Please include:

- affected revision
- impact (secret leakage, ownership bypass, TLS identity, plan-hash bypass)
- reproduction that does not include live secrets

## Scope

In scope:

- secret values appearing in plans, logs, JSON, SQLite, or process arguments
- adopting or deleting unmanaged runtime resources
- certificate identity / trust-store mistakes
- reviewed-plan hash bypasses

Out of scope:

- using `mm` against an untrusted project checkout
- vulnerabilities in Docker, Podman, or Ansible themselves

## Response

Maintainers will acknowledge a valid private report when possible and publish a
fix or advisory before discussing the defect in public issues.

## Release credentials

Releases are built and published by the release workflow on a tag push, with
the workflow's own short-lived token: no publishing credential is stored in the
repository. Each archive and `SHA256SUMS` carry a GitHub build provenance
attestation, which `gh attestation verify` checks. The maintainer may also sign
`SHA256SUMS` with an SSH key kept off the repository and off CI;
`packaging/release/allowed_signers` lists its public half. See the
[release process](docs/contribute/releases.md).
