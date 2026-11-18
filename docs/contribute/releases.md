---
title: Publish a release
description: Tag a release, let the release workflow build, attest, and publish it, and check the result.
---

A release is a tag on `master`: `vMAJOR.MINOR.PATCH`, such as `v0.1.1`, or
`vMAJOR.MINOR.PATCH-rc.N` for a release candidate. Pushing the tag starts
`.github/workflows/release.yml`, which builds every archive, attests it, and
publishes a GitHub release. Nothing is uploaded by hand.

## Before you start

- Permission to push tags to `github.com/modmonhq/modmon`. The repository must
  be public: Actions minutes on GitHub-hosted runners, including the macOS
  ones, are free only for public repositories, and artifact attestations need
  a public repository unless the organization pays for GitHub Enterprise
  Cloud.
- Node.js 24 and git to prepare the release commit. Docker only to reproduce a
  Linux build on your machine.

## Prepare the release commit

1. Set the new version in `core/Cargo.toml` and `core/domain/Cargo.toml`, and
   bring `Cargo.lock` along with `cargo update --workspace`.
2. In `CHANGELOG.md`, rename `## [Unreleased]` to `## [0.1.1] - YYYY-MM-DD`
   and add an empty `## [Unreleased]` above it. The release section starts
   with the line `Remote protocol: N`, where `N` is `PROTOCOL_VERSION` in
   `core/src/domain/remote.rs`; the release workflow refuses notes without
   that line or with another number.
3. Check the notes and run the checks:

   ```sh
   node scripts/release/release-notes.mjs v0.1.1
   make quality
   ```

4. Commit on `master` and push it.

## Try a release candidate

```sh
git tag -a v0.1.1-rc.1 -m "mm 0.1.1-rc.1"
git push origin v0.1.1-rc.1
```

A candidate goes through every step below and is published as a GitHub
prerelease that is not marked latest, so the installer's default skips it and
the website is not deployed. It is built from a commit that already has the
release's version, so its binaries report `mm 0.1.1`, and its notes are the
`[0.1.1]` section. Install it with `--version v0.1.1-rc.1` to try the
installer. Push `-rc.2` and so on after fixes; never move a tag.

## Publish

```sh
git tag -a v0.1.1 -m "mm 0.1.1"
git push origin v0.1.1
```

The workflow depends on no earlier workflow run. Its jobs:

1. **validate**: the tag's form, the package version and `Cargo.lock`, the
   notes and their remote protocol line, a clean checkout of the tag, and the
   tag on `master`; then the public content check and the workflow's own
   policy (`scripts/release/check-workflow.mjs`).
2. **build-linux**: `scripts/release/build-linux.sh`, which builds `mm`, `mmd`,
   and `mm-ingress-helper` for `x86_64-unknown-linux-gnu` and
   `aarch64-unknown-linux-gnu` from `git archive` of the tag in pinned
   containers (the Rust 1.98.0 image pinned by digest, with the ARM64 cross
   compiler and QEMU from a dated Debian snapshot), checks every binary's
   `--version`, packages them with every file dated from the tagged commit,
   and makes the source archive. Then the installer's tests run.
3. **build-macos**: `mm` and `mmd` built on GitHub's Intel and Apple Silicon
   macOS runners, their `--version` checked, and packaged with
   `scripts/release/package-unix.sh`.
4. **assemble**: `SHA256SUMS` over the five archives, checked.
   For a stable release it also assembles `manifest.json`
   (`scripts/release/assemble.mjs`), which `mm doctor --check-updates` reads
   from the latest release.
5. **publish** (pushed tags only): a build provenance attestation for every
   archive, for `SHA256SUMS`, and for `manifest.json`, then the GitHub release
   with those files and the notes, as the latest release or, for a candidate,
   as a prerelease without a manifest.

After a stable release is published, `.github/workflows/site-deploy.yml`
deploys the website, which serves the installer at `https://modmon.dev/install.sh`.

Expected result: the release page lists
`mm-v0.1.1-x86_64-unknown-linux-gnu.tar.gz`,
`mm-v0.1.1-aarch64-unknown-linux-gnu.tar.gz`,
`mm-v0.1.1-x86_64-apple-darwin.tar.gz`, `mm-v0.1.1-aarch64-apple-darwin.tar.gz`,
`mm-v0.1.1-source.tar.gz`, `SHA256SUMS`, and `manifest.json`, and its notes
start with `Remote protocol: N`.

To build everything without publishing, run the workflow by hand. With a tag
that does not exist yet, it builds the head of the branch you choose as that
tag, so you can try a release before you push its tag:

```sh
gh workflow run release.yml --ref master -f tag=v0.1.1
gh run watch "$(gh run list --workflow release.yml --limit 1 --json databaseId --jq '.[0].databaseId')"
```

Every job but **publish** runs, and the files are left as the run's `release`
artifact. Run by hand with an existing tag, it builds that tag the same way.

## Check the release

From a clone, in an empty directory:

```sh
gh release download v0.1.1 --dir mm-v0.1.1
scripts/release/verify.sh --checksums-only mm-v0.1.1
gh attestation verify mm-v0.1.1/SHA256SUMS --owner modmonhq \
  --signer-workflow github.com/modmonhq/modmon/.github/workflows/release.yml
curl --proto '=https' --tlsv1.2 -LsSf https://modmon.dev/install.sh | sh
mm --version
```

`verify.sh` checks every file against `SHA256SUMS` and that each Linux archive
holds `mm`, `mmd`, and `mm-ingress-helper`. `gh attestation verify` prints
`Verification succeeded!` when GitHub's attestation says the file was built
by this repository's release workflow; run it on any archive too. Users check
a download as in [install a release from
GitHub](../start/install.md#install-a-release-from-github).

## Sign with your own key

Optional, after the workflow has published the release: sign `SHA256SUMS`
with an SSH key that only you hold, and add the signature to the release.

```sh
gh release download v0.1.1 --dir mm-v0.1.1
scripts/release/sign.sh mm-v0.1.1 ~/.ssh/RELEASE_KEY
gh release upload v0.1.1 mm-v0.1.1/SHA256SUMS.sig
```

`sign.sh` checks the files first, runs `ssh-keygen -Y sign -n mm-release` over
`SHA256SUMS`, and checks the signature against
`packaging/release/allowed_signers`. Pass the key's `.pub` file when the key is
in `ssh-agent` or on a security key. The key's line must be in
`allowed_signers` on `master` before you upload the signature, because users
take that file from the repository:

```sh
printf 'release@example.com namespaces="mm-release" %s\n' \
  "$(cut -d ' ' -f 1,2 RELEASE_KEY.pub)" >>packaging/release/allowed_signers
```

## Reproduce a Linux build

The Linux archives are built to be reproducible: pinned images, fixed paths
inside the containers, and every date taken from the tagged commit. On an
x86-64 Linux machine with Docker, Node.js 24, and git, from a clean checkout
of the tag:

```sh
git checkout v0.1.1
scripts/release/build-linux.sh v0.1.1
grep linux-gnu target/dist/v0.1.1/SHA256SUMS
```

The two Linux lines should match the release's `SHA256SUMS`. A full build
takes hours on one CPU. `MM_RELEASE_CPUS` and `MM_RELEASE_MEMORY` limit each
build container, and `MM_RELEASE_CARGO_CACHE` and `MM_RELEASE_NPM_CACHE` name
the Docker volumes that keep downloads between builds. To check the cross
toolchain alone, `scripts/release/build-linux.sh --check-toolchain` builds a
small crate with the same C dependencies as `mm` twice for both targets,
compares the bytes, and runs both binaries.

## If the workflow fails

Nothing is published until every build has passed: the release is created in
the last step. Fix the cause, then re-run the failed jobs of the same run, or
push a new tag (the next candidate, or the next patch release) if the source
must change. Never move or reuse a published tag. If creating the release
failed halfway, delete that release (`gh release delete v0.1.1`, keeping the
tag) and re-run the publish job.

## What a release carries

- One archive per platform: Linux x86-64 and ARM64 with `mm`, `mmd`, and
  `mm-ingress-helper`; macOS Intel and Apple Silicon with `mm` and `mmd`.
  Each holds the licence, the README, shell completions, and the manual page.
- `mm-vX.Y.Z-source.tar.gz`, the tagged source without build output.
- `SHA256SUMS`, with the SHA-256 of each of those files. The installer checks
  an archive against it before it unpacks anything.
- `manifest.json`, for a stable release: its version, tag, commit, and each
  archive's SHA-256, size, and binaries. `mm doctor --check-updates` reads it
  from `releases/latest/download/manifest.json`; a candidate has none, so it
  is never offered as an update.
- A build provenance attestation for each of those files, kept by GitHub and
  linked from the workflow run.
- `SHA256SUMS.sig`, when the maintainer added an SSH signature.

The binaries carry no platform code signature, and the macOS binaries are not
notarized.
