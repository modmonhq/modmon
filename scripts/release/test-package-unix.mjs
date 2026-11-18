#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { writeFile } from "node:fs/promises";

const temporary = await mkdtemp(path.join(tmpdir(), "mm-package-test-"));
try {
  const target = "x86_64-unknown-linux-gnu";
  const binaries = path.join(temporary, "target", target, "release");
  const macos = path.join(temporary, "target", "aarch64-apple-darwin", "release");
  for (const directory of [binaries, macos]) {
    await mkdir(directory, { recursive: true });
    for (const name of ["mm", "mmd", "mm-ingress-helper"]) {
      await writeFile(path.join(directory, name), `#!/bin/sh\necho '${name} 0.1.0'\n`);
      await chmod(path.join(directory, name), 0o755);
    }
  }
  const first = path.join(temporary, "first");
  const second = path.join(temporary, "second");
  packageOnce(target, binaries, first);
  packageOnce(target, binaries, second);
  for (const name of [`mm-v0.1.0-${target}.tar.gz`, `${target}.metadata.json`]) {
    if (!(await readFile(path.join(first, name))).equals(await readFile(path.join(second, name)))) throw new Error(`package is not deterministic: ${name}`);
  }
  const archive = path.join(first, `mm-v0.1.0-${target}.tar.gz`);
  const listing = spawnSync("tar", ["-tzf", archive], { encoding: "utf8" });
  if (listing.status !== 0) throw new Error(listing.stderr);
  for (const required of ["/mm", "/mmd", "/mm-ingress-helper", "/LICENSE", "/README.md", "/completions/mm.bash", "/man/mm.1"]) {
    if (!listing.stdout.includes(required)) throw new Error(`package inventory missing ${required}`);
  }
  const declared = JSON.parse(await readFile(path.join(first, `${target}.metadata.json`), "utf8")).binaries;
  if (JSON.stringify(declared) !== JSON.stringify(["mm", "mmd", "mm-ingress-helper"])) throw new Error(`Linux package declares ${JSON.stringify(declared)}`);
  packageOnce("aarch64-apple-darwin", macos, path.join(temporary, "macos"));
  const macosListing = spawnSync("tar", ["-tzf", path.join(temporary, "macos", "mm-v0.1.0-aarch64-apple-darwin.tar.gz")], { encoding: "utf8" });
  if (macosListing.status !== 0 || macosListing.stdout.includes("mm-ingress-helper")) throw new Error("a macOS package carries the Linux ingress helper");
  const top = new Set(listing.stdout.trim().split("\n").map((line) => line.split("/")[0]));
  if (top.size !== 1) throw new Error("package does not have exactly one top-level directory");
  process.stdout.write("Unix package passed deterministic bytes, naming, one-root, binary (ingress helper on Linux only), license, completion, and manpage tests\n");
} finally {
  await rm(temporary, { recursive: true, force: true });
}

function packageOnce(target, binaries, output) {
  const result = spawnSync("bash", ["scripts/release/package-unix.sh", "v0.1.0", target, output], {
    cwd: process.cwd(), encoding: "utf8",
    env: { ...process.env, SOURCE_DATE_EPOCH: "1787529600", MM_RELEASE_BINARY_ROOT: path.resolve(binaries, "../..") },
  });
  if (result.status !== 0) throw new Error(result.stderr);
}
