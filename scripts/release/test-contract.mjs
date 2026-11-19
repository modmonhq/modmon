#!/usr/bin/env node
import { execFileSync, spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { copyFile, mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { canonicalJson, fileRecord, manifestSchema, minimumInstallerSchema, releaseBinaries, targets } from "./lib.mjs";

const require = createRequire(path.resolve("site/package.json"));
const Ajv2020 = require("ajv/dist/2020").default;
const addFormats = new Ajv2020({ strict: false });
addFormats.addFormat("date-time", /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
const schema = JSON.parse(await readFile("packaging/release/manifest.schema.json", "utf8"));
const validate = addFormats.compile(schema);
const temporary = await mkdtemp(path.join(tmpdir(), "mm-contract-test-"));
try {
  const packages = path.join(temporary, "packages");
  await mkdir(packages);
  for (const target of targets) {
    const format = target.includes("windows") ? "zip" : "tar.gz";
    const name = `mm-v0.1.0-${target}.${format}`;
    await writeFile(path.join(packages, name), `native fixture ${target}\n`);
    const record = await fileRecord(path.join(packages, name), name);
    await writeFile(path.join(packages, `${target}.metadata.json`), JSON.stringify({ ...record, target, format, binaries: releaseBinaries(target) }));
  }
  const source = path.join(temporary, "source.tar.gz");
  await writeFile(source, "source fixture\n");
  const metadata = path.join(temporary, "cargo-metadata.json");
  await writeFile(metadata, JSON.stringify({ packages: [{ name: "mm", version: "0.1.0", license: "MIT" }] }));
  const first = path.join(temporary, "first");
  const second = path.join(temporary, "second");
  assemble(packages, source, metadata, first);
  assemble(packages, source, metadata, second);
  const firstFiles = await walk(first);
  const secondFiles = await walk(second);
  if (JSON.stringify(firstFiles) !== JSON.stringify(secondFiles)) throw new Error("deterministic assembly inventory drift");
  for (const file of firstFiles) {
    const left = await readFile(path.join(first, file));
    const right = await readFile(path.join(second, file));
    if (!left.equals(right)) throw new Error(`deterministic assembly bytes drift: ${file}`);
  }
  const manifestBytes = await readFile(path.join(first, "manifest.json"));
  if (!manifestBytes.equals(await readFile(path.join(first, "releases/v0.1.0/manifest.json")))) throw new Error("root and historical manifests differ");
  const fixture = "core/tests/fixtures/release-manifest.json";
  if (process.env.MM_RELEASE_UPDATE_FIXTURE === "1") await writeFile(fixture, manifestBytes);
  if (!manifestBytes.equals(await readFile(fixture))) throw new Error(`the assembled manifest differs from ${fixture}, which the update check's tests parse; if the contract changed on purpose, rerun with MM_RELEASE_UPDATE_FIXTURE=1`);
  const manifest = JSON.parse(manifestBytes);
  if (!validate(manifest)) throw new Error(`manifest schema failed: ${JSON.stringify(validate.errors)}`);
  if (manifest.schema_version !== manifestSchema || manifest.minimum_installer_schema !== minimumInstallerSchema || minimumInstallerSchema !== 1) throw new Error("the manifest raises the schema that every mm since 0.1.1 reads; readers of schema 1 would refuse this release");
  const later = structuredClone(manifest);
  later.schema_version = manifestSchema + 1;
  later.signatures = { "x86_64-unknown-linux-gnu": "a field this release does not know" };
  later.artifacts["x86_64-unknown-linux-musl"] = { ...later.artifacts["x86_64-unknown-linux-gnu"], path: "releases/v0.1.0/mm-v0.1.0-x86_64-unknown-linux-musl.tar.gz", binaries: ["mm", "mm-future-tool"] };
  later.artifacts["aarch64-unknown-linux-gnu"].binaries.push("mm-future-tool");
  if (!validate(later)) throw new Error(`the schema refuses a later release that adds a field, a target, and a binary: ${JSON.stringify(validate.errors)}`);
  const order = Object.keys(manifest.artifacts);
  if (JSON.stringify(order) !== JSON.stringify([...targets].sort())) throw new Error("manifest target keys are not deterministic");

  if (JSON.stringify(manifest.artifacts["aarch64-unknown-linux-gnu"].binaries) !== JSON.stringify(["mm", "mmd", "mm-ingress-helper"])) throw new Error("a Linux artifact does not declare the ingress helper");
  if (JSON.stringify(manifest.artifacts["aarch64-apple-darwin"].binaries) !== JSON.stringify(["mm", "mmd"])) throw new Error("a macOS artifact declares more than mm and mmd");
  const linuxMetadata = path.join(packages, "x86_64-unknown-linux-gnu.metadata.json");
  const linuxRecord = JSON.parse(await readFile(linuxMetadata, "utf8"));
  await writeFile(linuxMetadata, JSON.stringify({ ...linuxRecord, binaries: ["mm", "mmd"] }));
  expectAssemblyFailure(packages, source, metadata, path.join(temporary, "no-helper"), "invalid binary inventory for x86_64-unknown-linux-gnu");
  await writeFile(linuxMetadata, JSON.stringify(linuxRecord));
  const macosMetadata = path.join(packages, "aarch64-apple-darwin.metadata.json");
  const macosRecord = JSON.parse(await readFile(macosMetadata, "utf8"));
  await writeFile(macosMetadata, JSON.stringify({ ...macosRecord, binaries: ["mm", "mmd", "mm-ingress-helper"] }));
  expectAssemblyFailure(packages, source, metadata, path.join(temporary, "macos-helper"), "invalid binary inventory for aarch64-apple-darwin");
  await writeFile(macosMetadata, JSON.stringify(macosRecord));
  await rm(path.join(packages, `${targets[0]}.metadata.json`));
  expectAssemblyFailure(packages, source, metadata, path.join(temporary, "missing"), `expected ${targets.length} package metadata files`);
  process.stdout.write("release contract passed schema, a later release's additions, the update check's fixture, exact target, per-target binary inventory, deterministic serialization/assembly, byte-identical manifest, and missing-target tests\n");
} finally {
  await rm(temporary, { recursive: true, force: true });
}

function assemble(packages, source, metadata, output) {
  const { GITHUB_REPOSITORY: _repository, GITHUB_RUN_ID: _run, ...env } = process.env;
  execFileSync(process.execPath, ["scripts/release/assemble.mjs", "--tag", "v0.1.0", "--commit", "1".repeat(40), "--epoch", "1787529600", "--packages", packages, "--source", source, "--cargo-metadata", metadata, "--output", output], { env });
}

function expectAssemblyFailure(packages, source, metadata, output, text) {
  const result = spawnSync(process.execPath, ["scripts/release/assemble.mjs", "--tag", "v0.1.0", "--commit", "1".repeat(40), "--epoch", "1787529600", "--packages", packages, "--source", source, "--cargo-metadata", metadata, "--output", output], { encoding: "utf8" });
  if (result.status === 0 || !result.stderr.includes(text)) throw new Error(`expected assembly failure '${text}', got ${result.stderr}`);
}

async function walk(directory, prefix = "") {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) files.push(...(await walk(path.join(directory, entry.name), relative)));
    else files.push(relative);
  }
  return files.sort();
}
