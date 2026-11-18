---
title: Security
description: Supported revisions and private vulnerability reporting for mm.
---

Security fixes target the latest release and the `master` branch. Releases
are listed at <https://github.com/modmonhq/modmon/releases>; check a download
as in [install a release from GitHub](../start/install.md#install-a-release-from-github).

Do not open a public issue for a suspected vulnerability. Report it privately
through the repository's security advisory form,
<https://github.com/modmonhq/modmon/security/advisories/new>. Include the
affected release or revision, impact, and a reproduction that contains no live
secrets.

Reports involving secret disclosure, unmanaged-resource ownership, certificate
identity or trust, and reviewed-plan hash bypasses are in scope. Vulnerabilities
in Docker, Podman, or Ansible themselves should be reported upstream.

Remote transport reports should include the protocol version and redacted
request ID, but never private keys, host keys not already public, daemon identity
files, payload frames, or target audit records containing private topology.
