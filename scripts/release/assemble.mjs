#!/usr/bin/env node
import { copyFile, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { canonicalJson, fileRecord, releaseBinaries, repository, targets, versionFromTag } from "./lib.mjs";

const options = parse(process.argv.slice(2));
const version = versionFromTag(options.tag);
if (!/^[0-9a-f]{40}$/.test(options.commit)) throw new Error("assembly requires a full lowercase commit ID");
if (!/^[1-9][0-9]*$/.test(options.epoch)) throw new Error("assembly requires a positive source epoch");
const epoch = Number(options.epoch);
const releasePrefix = `releases/${options.tag}`;
await rm(options.output, { recursive: true, force: true });
await mkdir(path.join(options.output, releasePrefix, "provenance"), { recursive: true });

const entries = await readdir(options.packages);
const metadataNames = entries.filter((name) => name.endsWith(".metadata.json")).sort();
if (metadataNames.length !== targets.length) throw new Error(`expected ${targets.length} package metadata files, found ${metadataNames.length}`);
const metadata = [];
for (const name of metadataNames) {
  const item = JSON.parse(await readFile(path.join(options.packages, name), "utf8"));
  validateMetadata(item, version);
  const source = path.join(options.packages, item.path);
  const recomputed = await fileRecord(source, item.path);
  if (recomputed.sha256 !== item.sha256 || recomputed.size !== item.size) throw new Error(`package metadata drift: ${item.target}`);
  const destinationKey = `${releasePrefix}/${item.path}`;
  await copyFile(source, path.join(options.output, destinationKey));
  metadata.push({ ...item, path: destinationKey });
}
const seen = metadata.map((item) => item.target).sort();
if (JSON.stringify(seen) !== JSON.stringify([...targets].sort())) throw new Error(`target set mismatch: ${seen.join(",")}`);

const sourceName = `mm-${options.tag}-source.tar.gz`;
await copyFile(options.source, path.join(options.output, releasePrefix, sourceName));

const cargoMetadata = JSON.parse(await readFile(options.cargoMetadata, "utf8"));
const sbom = spdx(cargoMetadata, options.tag, options.commit, epoch);
await writeFile(path.join(options.output, releasePrefix, "sbom.spdx.json"), canonicalJson(sbom));

const subjects = [];
for (const item of metadata) subjects.push({ name: item.path, digest: { sha256: item.sha256 }, size: item.size });
subjects.push(await objectSubject(options.output, `${releasePrefix}/${sourceName}`));
const provenanceKey = `${releasePrefix}/provenance/build.intoto.jsonl`;
const provenance = {
  _type: "https://in-toto.io/Statement/v1",
  subject: subjects.map(({ name, digest }) => ({ name, digest })),
  predicateType: "https://slsa.dev/provenance/v1",
  predicate: {
    buildDefinition: {
      buildType: `${repository}/blob/${options.tag}/.github/workflows/release.yml`,
      externalParameters: { tag: options.tag, targets },
      internalParameters: { rust_toolchain: "1.98.0", source_date_epoch: epoch },
      resolvedDependencies: [{ uri: `git+${process.env.GITHUB_REPOSITORY ? `https://github.com/${process.env.GITHUB_REPOSITORY}` : repository}@${options.commit}`, digest: { gitCommit: options.commit } }],
    },
    runDetails: {
      builder: { id: "https://github.com/actions/runner" },
      metadata: { invocationId: process.env.GITHUB_RUN_ID ?? "local-fake", startedOn: new Date(epoch * 1000).toISOString() },
    },
  },
};
await writeFile(path.join(options.output, provenanceKey), `${JSON.stringify(provenance)}\n`);

const certificateKey = `${releasePrefix}/release-certificate.json`;
const certificate = {
  schema_version: 1,
  tag: options.tag,
  version,
  commit: options.commit,
  source_date_epoch: epoch,
  generated_at: new Date(epoch * 1000).toISOString(),
  rust_toolchain: "1.98.0",
  supported_targets: targets,
  reproducibility: {
    archives: "normalized order, permissions, source-commit timestamps, locale, and owner/group metadata",
    linux_binaries: "built by scripts/release/build-linux.sh in pinned containers with fixed paths, so a rebuild of the tag can be compared byte for byte",
    macos_binaries: "built on GitHub's macOS runners and not claimed reproducible across runner-image changes",
  },
  security_claims: {
    signed: false,
    notarized: false,
    provenance_kind: "GitHub build provenance attestations over every archive, SHA256SUMS, and this manifest, made by the release workflow",
  },
  subjects,
};
await writeFile(path.join(options.output, certificateKey), canonicalJson(certificate));

const source = await objectRecord(options.output, `${releasePrefix}/${sourceName}`);
const sbomRecord = await objectRecord(options.output, `${releasePrefix}/sbom.spdx.json`);
const certificateRecord = await objectRecord(options.output, certificateKey);
const provenanceRecord = await objectRecord(options.output, provenanceKey);
const artifacts = Object.fromEntries(
  metadata.map((item) => [item.target, { path: item.path, format: item.format, sha256: item.sha256, size: item.size, binaries: item.binaries }]),
);
const manifest = {
  schema_version: 1,
  product: "mm",
  channel: "stable",
  version,
  tag: options.tag,
  commit: options.commit,
  published_at: new Date(epoch * 1000).toISOString().replace(".000Z", "Z"),
  source_date_epoch: epoch,
  minimum_installer_schema: 1,
  artifacts,
  source,
  sbom: sbomRecord,
  release_certificate: certificateRecord,
  provenance: [provenanceRecord],
};
const manifestBytes = canonicalJson(manifest);
await writeFile(path.join(options.output, releasePrefix, "manifest.json"), manifestBytes);
await writeFile(path.join(options.output, "manifest.json"), manifestBytes);

process.stdout.write(`assembled ${options.tag}: ${metadata.length} native archives and the release manifest\n`);

function parse(arguments_) {
  const values = {};
  for (let index = 0; index < arguments_.length; index += 2) {
    const name = arguments_[index];
    const value = arguments_[index + 1];
    if (!name?.startsWith("--") || value === undefined) throw new Error("assembly arguments must be --name value pairs");
    values[name.slice(2).replaceAll("-", "_")] = value;
  }
  for (const required of ["tag", "commit", "epoch", "packages", "source", "cargo_metadata", "output"]) {
    if (!values[required]) throw new Error(`missing --${required.replaceAll("_", "-")}`);
  }
  return { ...values, cargoMetadata: values.cargo_metadata };
}

function validateMetadata(item, version) {
  if (!targets.includes(item.target)) throw new Error(`unsupported package target: ${item.target}`);
  const extension = item.target.includes("windows") ? "zip" : "tar.gz";
  const expected = `mm-v${version}-${item.target}.${extension}`;
  if (item.path !== expected || item.format !== extension) throw new Error(`package name/format mismatch for ${item.target}`);
  if (!/^[0-9a-f]{64}$/.test(item.sha256) || !Number.isSafeInteger(item.size) || item.size < 1) throw new Error(`invalid package checksum/size for ${item.target}`);
  if (JSON.stringify(item.binaries) !== JSON.stringify(releaseBinaries(item.target))) throw new Error(`invalid binary inventory for ${item.target}: ${JSON.stringify(item.binaries)}, expected ${JSON.stringify(releaseBinaries(item.target))}`);
}

async function objectRecord(root, key) {
  return fileRecord(path.join(root, key), key);
}

async function objectSubject(root, key) {
  const record = await objectRecord(root, key);
  return { name: key, digest: { sha256: record.sha256 }, size: record.size };
}

function spdx(metadata, tag, commit, epoch) {
  const packages = [...metadata.packages]
    .sort((left, right) => `${left.name}@${left.version}`.localeCompare(`${right.name}@${right.version}`, "en"))
    .map((pkg, index) => ({
      SPDXID: `SPDXRef-Package-${index}`,
      name: pkg.name,
      versionInfo: pkg.version,
      downloadLocation: "NOASSERTION",
      filesAnalyzed: false,
      licenseConcluded: "NOASSERTION",
      licenseDeclared: pkg.license ?? "NOASSERTION",
      copyrightText: "NOASSERTION",
      externalRefs: [{ referenceCategory: "PACKAGE-MANAGER", referenceType: "purl", referenceLocator: `pkg:cargo/${encodeURIComponent(pkg.name)}@${pkg.version}` }],
    }));
  return {
    spdxVersion: "SPDX-2.3",
    dataLicense: "CC0-1.0",
    SPDXID: "SPDXRef-DOCUMENT",
    name: `mm-${tag}`,
    documentNamespace: `${repository}/releases/${tag}/sbom/${commit}`,
    creationInfo: { created: new Date(epoch * 1000).toISOString(), creators: ["Tool: scripts/release/assemble.mjs"] },
    packages,
  };
}
