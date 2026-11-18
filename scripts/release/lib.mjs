import { createHash } from "node:crypto";
import { readFile, stat } from "node:fs/promises";
import path from "node:path";

export const stableTag = /^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/;
export const releaseTag = /^v((?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*))(-rc\.(?:0|[1-9][0-9]*))?$/;
export const targets = Object.freeze([
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
]);

export function releaseBinaries(target) {
  return target.endsWith("-unknown-linux-gnu") ? ["mm", "mmd", "mm-ingress-helper"] : ["mm", "mmd"];
}

export function parseReleaseTag(tag) {
  const match = releaseTag.exec(tag);
  if (!match) throw new Error(`tag is not vMAJOR.MINOR.PATCH or vMAJOR.MINOR.PATCH-rc.N: ${tag}`);
  return { tag, version: match[1], prerelease: Boolean(match[2]) };
}

export function versionFromTag(tag) {
  const match = stableTag.exec(tag);
  if (!match) throw new Error(`tag is not exact stable SemVer: ${tag}`);
  return tag.slice(1);
}

export async function sha256(file) {
  return createHash("sha256").update(await readFile(file)).digest("hex");
}

export async function fileRecord(file, objectPath) {
  const details = await stat(file);
  if (!details.isFile() || details.size === 0) throw new Error(`empty or missing release file: ${file}`);
  return { path: objectPath, sha256: await sha256(file), size: details.size };
}

export function canonicalJson(value) {
  return `${JSON.stringify(sort(value), null, 2)}\n`;
}

export function sort(value) {
  if (Array.isArray(value)) return value.map(sort);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => left.localeCompare(right, "en"))
        .map(([key, child]) => [key, sort(child)]),
    );
  }
  return value;
}

export const repository = "https://github.com/modmonhq/modmon";
export const githubReleaseNotesLimit = 125000;

export function changelogSection(changelog, version) {
  const heading = version === "Unreleased" ? "## [Unreleased]" : `## [${version}] -`;
  if (!changelog.includes(heading)) throw new Error(`CHANGELOG.md has no ${version === "Unreleased" ? "[Unreleased]" : "release"} section for ${version}`);
  const after = changelog.split(heading)[1];
  const newline = after.indexOf("\n");
  return (newline < 0 ? "" : after.slice(newline + 1)).split(/^## /m)[0].trim();
}

export function protocolVersion(remoteSource) {
  const match = /^pub const PROTOCOL_VERSION: u16 = ([0-9]+);$/m.exec(remoteSource);
  if (!match) throw new Error("core/src/domain/remote.rs does not define PROTOCOL_VERSION");
  return Number(match[1]);
}

export function requireRemoteProtocol(notes, protocol, label) {
  const stated = [...notes.matchAll(/^Remote protocol: ([0-9]+)\b/gm)].map((match) => Number(match[1]));
  if (stated.length === 0) throw new Error(`${label} do not state "Remote protocol: ${protocol}"`);
  if (stated.length > 1) throw new Error(`${label} state the remote protocol ${stated.length} times; keep one "Remote protocol: ${protocol}" line`);
  if (stated[0] !== protocol) throw new Error(`${label} state "Remote protocol: ${stated[0]}", and PROTOCOL_VERSION in core/src/domain/remote.rs is ${protocol}`);
}

export async function releaseNotes(root, version, tag = `v${version}`) {
  const changelog = await readFile(path.join(root, "CHANGELOG.md"), "utf8");
  const notes = changelogSection(changelog, version);
  const protocol = protocolVersion(await readFile(path.join(root, "core/src/domain/remote.rs"), "utf8"));
  const label = version === "Unreleased" ? "the [Unreleased] notes" : `the release notes of ${version}`;
  let published = notes;
  if (version === "Unreleased") {
    if (/^Remote protocol:/m.test(notes)) requireRemoteProtocol(notes, protocol, label);
  } else {
    if (!notes || /\bUnreleased\b/i.test(notes)) throw new Error("release notes are empty or include Unreleased content");
    requireRemoteProtocol(notes, protocol, label);
    published = notes.replace(/\]\((?![a-z][a-z0-9+.-]*:|#|\/)([^)\s]+)\)/gi, `](${repository}/blob/${tag}/$1)`);
  }
  if (published.length > githubReleaseNotesLimit) {
    throw new Error(`${label} are ${published.length} characters; a GitHub release takes at most ${githubReleaseNotesLimit}`);
  }
  return `${published}\n`;
}
