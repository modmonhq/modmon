#!/usr/bin/env node
import { readFile } from "node:fs/promises";

const source = await readFile(".github/workflows/release.yml", "utf8");
const requiredJobs = ["validate", "build-linux", "build-macos", "assemble", "publish"];
for (const job of requiredJobs) {
  if (!new RegExp(`^  ${job}:`, "m").test(source)) throw new Error(`release workflow is missing job ${job}`);
}
for (const line of source.split("\n").filter((value) => /^\s*- uses:/.test(value))) {
  if (!/@[0-9a-f]{40}(?:\s+#\s+\S+)?$/.test(line)) throw new Error(`action is not pinned by commit: ${line.trim()}`);
}
for (const prohibited of ["a" + "ws-actions", "a" + "ws cli", "s3." + "amazonaws.com", "ubuntu-latest", "macos-latest", "windows-", "cancel-in-progress: true", "R2_", "publish-r2", "release-certify", "get." + "mod" + "mon.org", "git push"]) {
  if (source.toLowerCase().includes(prohibited.toLowerCase())) throw new Error(`release workflow contains prohibited text: ${prohibited}`);
}
const job = (name) => source.split(new RegExp(`^  ${name}:`, "m"))[1]?.split(/^  [a-z0-9-]+:$/m)[0] ?? "";
if (/unknown-linux-gnu/.test(job("build-macos"))) throw new Error("the macOS job builds a Linux target; Linux packages come from scripts/release/build-linux.sh");
if (!/\.\/scripts\/release\/build-linux\.sh "\$RELEASE_TAG" "target\/dist\/\$RELEASE_TAG"/.test(job("build-linux"))) throw new Error("the build-linux job does not run scripts/release/build-linux.sh");
if (!/scripts\/release\/package-unix\.sh/.test(job("build-macos"))) throw new Error("the macOS job does not package with scripts/release/package-unix.sh");
if (/(?:^|\s)packages(?:\/|\s|$)/m.test(job("build-linux") + job("build-macos"))) throw new Error("a build writes into packages/, which holds the repository's own packages");
for (const name of ["validate", "build-linux", "build-macos", "assemble"]) {
  if (/contents: write|id-token: write|attestations: write/.test(job(name))) throw new Error(`job ${name} can write; only publish may`);
}
const publish = job("publish");
if (!/if: needs\.validate\.outputs\.publish == 'true'/.test(publish)) throw new Error("publish runs without a pushed tag");
if (!/PUBLISH: \$\{\{ github\.event_name == 'push' \}\}/.test(job("validate")) || !/elif \[ "\$PUBLISH" = true \]; then[\s\S]*?exit 1/.test(job("validate"))) throw new Error("a pushed tag that is missing could be built as a dry run, or a dry run could publish");
for (const name of ["build-linux", "build-macos", "assemble", "publish"]) {
  if (!/ref: \$\{\{ needs\.validate\.outputs\.commit \}\}/.test(job(name))) throw new Error(`job ${name} does not build the commit validate checked`);
}
for (const permission of ["contents: write", "id-token: write", "attestations: write"]) {
  if (!publish.includes(permission)) throw new Error(`publish lacks ${permission}`);
}
if (!/actions\/attest-build-provenance@[0-9a-f]{40}/.test(publish) || !/release\/SHA256SUMS/.test(publish) || !/release\/manifest\.json/.test(publish)) throw new Error("publish does not attest the archives, SHA256SUMS, and the manifest");
if (!/if: needs\.validate\.outputs\.prerelease == 'false'[\s\S]*scripts\/release\/assemble\.mjs/.test(job("assemble"))) throw new Error("the manifest is not assembled for stable releases only");
if (!/--prerelease --latest=false/.test(publish)) throw new Error("a release candidate would be published as the latest release");
if (/needs: \[?[^\n]*(certify|workflow_run)/.test(source) || /actions: read/.test(source)) throw new Error("the release depends on other workflow runs");
process.stdout.write("release workflow policy passed jobs, Linux and macOS build scripts, write permissions, attestation, stable-only manifest, prerelease, independence, action pin, and runner checks\n");
