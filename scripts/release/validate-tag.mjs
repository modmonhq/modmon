#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import process from "node:process";
import { parseReleaseTag, releaseNotes } from "./lib.mjs";

const tag = process.argv[2] ?? process.env.GITHUB_REF_NAME ?? "";
const releaseBranch = process.env.RELEASE_BRANCH ?? "master";
const { version, prerelease } = parseReleaseTag(tag);
const cargo = await readFile("core/Cargo.toml", "utf8");
const packageVersion = /^version = "([^"]+)"$/m.exec(cargo)?.[1];
if (packageVersion !== version) throw new Error(`tag ${tag} does not match core package ${packageVersion}`);
const lock = await readFile("Cargo.lock", "utf8");
if (!lock.includes('name = "mm"') || !lock.includes(`version = "${version}"`)) {
  throw new Error("Cargo.lock does not contain the tagged mm package version");
}
await releaseNotes(process.cwd(), version, tag);
git(["diff", "--quiet"]);
git(["diff", "--cached", "--quiet"]);
const untracked = git(["ls-files", "--others", "--exclude-standard"], true).trim();
if (untracked) throw new Error(`working tree has untracked files: ${untracked}`);
const tagCommit = git(["rev-list", "-n", "1", tag], true).trim();
const head = git(["rev-parse", "HEAD"], true).trim();
if (head !== tagCommit) throw new Error(`HEAD is ${head}, not ${tag} (${tagCommit}); check out the tag first: git checkout ${tag}`);
const branchRef = process.env.GITHUB_ACTIONS ? `origin/${releaseBranch}` : releaseBranch;
try {
  git(["merge-base", "--is-ancestor", tagCommit, branchRef]);
} catch {
  throw new Error(`${tag} is not on ${branchRef}; tag a commit of ${branchRef}, or set RELEASE_BRANCH to the branch you release from`);
}
const epoch = git(["show", "-s", "--format=%ct", tagCommit], true).trim();
if (!/^[1-9][0-9]*$/.test(epoch)) throw new Error("tag commit has no valid source epoch");
process.stdout.write(JSON.stringify({ tag, version, prerelease, commit: tagCommit, source_date_epoch: Number(epoch) }));

function git(args, capture = false) {
  return execFileSync("git", args, { encoding: capture ? "utf8" : undefined, stdio: capture ? undefined : "inherit" });
}
