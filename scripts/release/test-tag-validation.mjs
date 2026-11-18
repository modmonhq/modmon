#!/usr/bin/env node
import { execFileSync, spawnSync } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

const temporary = await mkdtemp(path.join(tmpdir(), "mm-tag-test-"));
try {
  await mkdir(path.join(temporary, "core"));
  await writeFile(path.join(temporary, "core/Cargo.toml"), '[package]\nname = "mm"\nversion = "1.2.3"\n');
  await writeFile(path.join(temporary, "Cargo.lock"), '[[package]]\nname = "mm"\nversion = "1.2.3"\n');
  await mkdir(path.join(temporary, "core/src/domain"), { recursive: true });
  await writeFile(path.join(temporary, "core/src/domain/remote.rs"), "pub const PROTOCOL_VERSION: u16 = 7;\n");
  await writeFile(path.join(temporary, "CHANGELOG.md"), changelog("Remote protocol: 7. See [the upgrade guide](docs/how-to/upgrade.md#steps), [the site](https://example.com/docs) and [below](#added).\n\n"));
  git(["init", "-b", "master"]);
  git(["add", "."]);
  git(["-c", "user.name=Release Test", "-c", "user.email=release@example.invalid", "commit", "-m", "release"]);
  git(["tag", "v1.2.3"]);
  const valid = run("v1.2.3");
  const identity = JSON.parse(valid.stdout);
  if (identity.version !== "1.2.3" || !/^[0-9a-f]{40}$/.test(identity.commit)) throw new Error("valid tag identity is wrong");
  if (identity.prerelease !== false) throw new Error("a stable tag was read as a prerelease");
  git(["tag", "v1.2.3-rc.1"]);
  const candidate = JSON.parse(run("v1.2.3-rc.1").stdout || "{}");
  if (candidate.version !== "1.2.3" || candidate.prerelease !== true || candidate.commit !== identity.commit) throw new Error(`a release candidate tag was not read as 1.2.3: ${JSON.stringify(candidate)}`);
  const candidateNotes = spawnSync(process.execPath, [path.resolve("scripts/release/release-notes.mjs"), "v1.2.3-rc.1"], { cwd: temporary, encoding: "utf8" });
  if (candidateNotes.status !== 0 || !candidateNotes.stdout.includes("/blob/v1.2.3-rc.1/docs/how-to/upgrade.md")) throw new Error(`release candidate notes do not link the candidate tag: ${candidateNotes.stderr}${candidateNotes.stdout}`);
  for (const invalid of ["1.2.3", "v01.2.3", "v1.2", "v1.2.3-rc", "v1.2.3-rc.01", "v1.2.3-beta.1", "v1.2.3+build", "v1.2.3.4"]) {
    const result = run(invalid);
    if (result.status === 0 || !result.stderr.includes("is not vMAJOR.MINOR.PATCH or vMAJOR.MINOR.PATCH-rc.N")) throw new Error(`invalid tag was accepted: ${invalid}`);
  }
  const notes = spawnSync(process.execPath, [path.resolve("scripts/release/release-notes.mjs"), "v1.2.3"], { cwd: temporary, encoding: "utf8" });
  if (notes.status !== 0 || notes.stdout !== "Remote protocol: 7. See [the upgrade guide](https://github.com/modmonhq/modmon/blob/v1.2.3/docs/how-to/upgrade.md#steps), [the site](https://example.com/docs) and [below](#added).\n\n- Stable release.\n") throw new Error(`release notes were not extracted: ${notes.stderr}${notes.stdout}`);
  for (const [stated, expected] of [
    ["", 'do not state "Remote protocol: 7"'],
    ["Remote protocol: 6\n\n", 'state "Remote protocol: 6", and PROTOCOL_VERSION in core/src/domain/remote.rs is 7'],
    ["Remote protocol: 7\n\nRemote protocol: 7\n\n", "state the remote protocol 2 times"],
  ]) {
    await writeFile(path.join(temporary, "CHANGELOG.md"), changelog(stated));
    for (const result of [run("v1.2.3"), spawnSync(process.execPath, [path.resolve("scripts/release/release-notes.mjs"), "1.2.3"], { cwd: temporary, encoding: "utf8" })]) {
      if (result.status === 0 || !result.stderr.includes(expected)) throw new Error(`release notes stating '${stated.trim()}' were accepted: ${result.stderr}`);
    }
  }
  await writeFile(path.join(temporary, "CHANGELOG.md"), changelog("Remote protocol: 7\n\n", "Remote protocol: 8\n\n- Next.\n\n"));
  const unreleased = spawnSync(process.execPath, [path.resolve("scripts/release/release-notes.mjs"), "Unreleased"], { cwd: temporary, encoding: "utf8" });
  if (unreleased.status === 0 || !unreleased.stderr.includes('the [Unreleased] notes state "Remote protocol: 8"')) throw new Error(`unreleased notes with another protocol were accepted: ${unreleased.stderr}`);
  await writeFile(path.join(temporary, "CHANGELOG.md"), changelog("Remote protocol: 7\n\n", "- Next.\n\n"));
  const unstated = spawnSync(process.execPath, [path.resolve("scripts/release/release-notes.mjs"), "Unreleased"], { cwd: temporary, encoding: "utf8" });
  if (unstated.status !== 0 || unstated.stdout !== "- Next.\n") throw new Error(`unreleased notes without a protocol line were refused: ${unstated.stderr}`);
  await writeFile(path.join(temporary, "CHANGELOG.md"), changelog(`Remote protocol: 7\n\n${"x".repeat(125000)}\n`));
  const long = run("v1.2.3");
  if (long.status === 0 || !long.stderr.includes("a GitHub release takes at most 125000")) throw new Error("release notes over the GitHub limit were accepted");
  await writeFile(path.join(temporary, "CHANGELOG.md"), changelog("Remote protocol: 7. See the upgrade guide.\n\n"));
  await writeFile(path.join(temporary, "core/Cargo.toml"), '[package]\nname = "mm"\nversion = "1.2.4"\n');
  const dirty = run("v1.2.3");
  if (dirty.status === 0 || !dirty.stderr.includes("does not match core package")) throw new Error("tag/package mismatch was accepted");
  await writeFile(path.join(temporary, "core/Cargo.toml"), '[package]\nname = "mm"\nversion = "1.2.3"\n');
  await writeFile(path.join(temporary, "later.txt"), "after the tag\n");
  git(["add", "."]);
  git(["-c", "user.name=Release Test", "-c", "user.email=release@example.invalid", "commit", "-m", "later"]);
  const moved = run("v1.2.3");
  if (moved.status === 0 || !moved.stderr.includes("check out the tag first: git checkout v1.2.3")) throw new Error(`a checkout away from the tag was accepted: ${moved.stderr}`);
  git(["checkout", "-q", "v1.2.3"]);
  const elsewhere = spawnSync(process.execPath, [path.resolve("scripts/release/validate-tag.mjs"), "v1.2.3"], { cwd: temporary, encoding: "utf8", env: { ...process.env, RELEASE_BRANCH: "missing" } });
  if (elsewhere.status === 0 || !elsewhere.stderr.includes("v1.2.3 is not on missing")) throw new Error(`a tag off the release branch was accepted: ${elsewhere.stderr}`);
  process.stdout.write("tag validation passed stable and release candidate tags, package/lock/changelog, remote protocol notes, checked-out tag, clean source, and release-branch ancestry tests\n");
} finally {
  await rm(temporary, { recursive: true, force: true });
}

function changelog(stated, unreleased = "") {
  return `# Changelog\n\n## [Unreleased]\n\n${unreleased}## [1.2.3] - 2026-08-25\n\n${stated}- Stable release.\n`;
}

function git(arguments_) {
  execFileSync("git", arguments_, { cwd: temporary, stdio: "ignore" });
}

function run(tag) {
  return spawnSync(process.execPath, [path.resolve("scripts/release/validate-tag.mjs"), tag], {
    cwd: temporary,
    encoding: "utf8",
    env: { ...process.env, RELEASE_BRANCH: "master" },
  });
}
