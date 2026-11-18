#!/usr/bin/env node
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { arch } from "node:os";
import { chmod, cp, mkdir, mkdtemp, readFile, rename, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { sha256 } from "./lib.mjs";

const target = `${arch() === "arm64" ? "aarch64" : "x86_64"}-${process.platform === "darwin" ? "apple-darwin" : "unknown-linux-gnu"}`;
const linux = target.endsWith("-unknown-linux-gnu");
const temporary = await mkdtemp(path.join(tmpdir(), "mm-installer-test-"));
let server;
try {
  const web = path.join(temporary, "web");
  await release(web, "v1.2.2", ["mm", "mmd"]);
  await release(web, "v1.2.3-rc.1", ["mm", "mmd", "mm-ingress-helper"]);
  await release(web, "v1.2.3", ["mm", "mmd", "mm-ingress-helper"]);
  await cp(path.join(web, "releases/download/v1.2.3"), path.join(web, "releases/latest/download"), { recursive: true });
  server = spawn("python3", ["-u", "-m", "http.server", "0", "--bind", "127.0.0.1", "--directory", web], { stdio: ["ignore", "pipe", "pipe"] });
  const port = await new Promise((resolve, reject) => {
    let output = "";
    const timer = setTimeout(() => reject(new Error("fixture HTTP server did not start")), 5000);
    const observe = (chunk) => {
      output += chunk;
      const match = /port ([0-9]+)/.exec(output);
      if (match) { clearTimeout(timer); resolve(match[1]); }
    };
    server.stdout.on("data", observe);
    server.stderr.on("data", observe);
    server.on("error", reject);
  });
  const origin = `http://127.0.0.1:${port}`;
  const install = path.resolve("packaging/release/install.sh");

  const bin = path.join(temporary, "bin");
  const latest = run(install, origin, ["--install-dir", bin, "--no-modify-path"]);
  if (!latest.includes("Installed mm 1.2.3 (v1.2.3)")) throw new Error(`latest install said: ${latest}`);
  await executable(path.join(bin, "mm"), "mm 1.2.3");
  if (linux) await executable(path.join(bin, "mm-ingress-helper"), "mm-ingress-helper 1.2.3");
  else await missing(path.join(bin, "mm-ingress-helper"));
  await missing(path.join(bin, "mmd"));
  run(install, origin, ["--install-dir", bin, "--no-modify-path"]);

  const daemon = path.join(temporary, "daemon");
  run(install, origin, ["--install-dir", daemon, "--with-daemon", "--no-modify-path"]);
  await executable(path.join(daemon, "mm"), "mm 1.2.3");
  await executable(path.join(daemon, "mmd"), "mmd 1.2.3");

  const pinned = path.join(temporary, "pinned");
  run(install, origin, ["--version", "v1.2.2", "--install-dir", pinned, "--with-daemon", "--no-modify-path"]);
  await executable(path.join(pinned, "mm"), "mm 1.2.2");
  await executable(path.join(pinned, "mmd"), "mmd 1.2.2");
  await missing(path.join(pinned, "mm-ingress-helper"));
  const candidate = path.join(temporary, "candidate");
  run(install, origin, ["--version", "v1.2.3-rc.1", "--install-dir", candidate, "--no-modify-path"]);
  await executable(path.join(candidate, "mm"), "mm 1.2.3");

  expectFailure(install, origin, ["--install-dir", bin, "--version", "v1.2.2"], "refusing to downgrade mm 1.2.3 to 1.2.2 without --force");
  run(install, origin, ["--version", "v1.2.2", "--install-dir", bin, "--force", "--no-modify-path"]);
  await executable(path.join(bin, "mm"), "mm 1.2.2");
  expectFailure(install, origin, ["--install-dir", bin, "--version", "1.2.3"], "version must be vMAJOR.MINOR.PATCH");
  expectFailure(install, origin, ["--install-dir", bin, "--version", "v9.9.9"], "download failed");

  const old = await readFile(path.join(bin, "mm"));
  expectFailure(install, origin, ["--install-dir", bin, "--with-daemon"], "filesystem failure", { MM_INSTALL_INJECT_FAILURE: "mmd" });
  if (!old.equals(await readFile(path.join(bin, "mm")))) throw new Error("an interrupted install did not roll back mm");
  if (linux) {
    expectFailure(install, origin, ["--install-dir", bin], "staging mm-ingress-helper", { MM_INSTALL_INJECT_FAILURE: "mm-ingress-helper" });
    if (!old.equals(await readFile(path.join(bin, "mm")))) throw new Error("a failed helper install did not roll back mm");
  }

  const directory = path.join(web, "releases/latest/download");
  const sumsPath = path.join(directory, "SHA256SUMS");
  const sums = await readFile(sumsPath, "utf8");
  const archiveName = `mm-v1.2.3-${target}.tar.gz`;
  const archivePath = path.join(directory, archiveName);
  const archiveBytes = await readFile(archivePath);
  const fresh = path.join(temporary, "fresh");

  await writeFile(archivePath, Buffer.concat([archiveBytes, Buffer.from("tampered")]));
  expectFailure(install, origin, ["--install-dir", fresh], "checksum mismatch");
  await missing(path.join(fresh, "mm"));
  await writeFile(archivePath, archiveBytes.subarray(0, Math.floor(archiveBytes.length / 2)));
  expectFailure(install, origin, ["--install-dir", fresh], "checksum mismatch");
  await writeFile(archivePath, archiveBytes);

  await writeFile(sumsPath, sums.split("\n").filter((line) => !line.endsWith(archiveName)).join("\n"));
  expectFailure(install, origin, ["--install-dir", fresh], `has no archive for ${target}`);
  await writeFile(sumsPath, sums);
  await rename(archivePath, `${archivePath}.missing`);
  expectFailure(install, origin, ["--install-dir", fresh], "download failed");
  await rename(`${archivePath}.missing`, archivePath);
  await rm(sumsPath);
  expectFailure(install, origin, ["--install-dir", fresh], "download failed");
  await writeFile(sumsPath, sums);

  const malicious = path.join(temporary, "malicious");
  const top = `mm-v1.2.3-${target}`;
  await mkdir(path.join(malicious, top), { recursive: true });
  await writeFile(path.join(malicious, "outside"), "bad");
  execFileSync("ln", ["-s", "../outside", path.join(malicious, top, "mm")]);
  execFileSync("tar", ["-czf", archivePath, "-C", malicious, top]);
  const maliciousDigest = await sha256(archivePath);
  await writeFile(sumsPath, sums.split("\n").map((line) => (line.endsWith(`  ${archiveName}`) ? `${maliciousDigest}  ${archiveName}` : line)).join("\n"));
  expectFailure(install, origin, ["--install-dir", fresh], "symlink");
  await missing(path.join(fresh, "mm"));
  process.stdout.write("POSIX installer passed latest, pinned, release candidate, --with-daemon, ingress helper, downgrade, rollback, checksum mismatch, truncated, missing target, missing download, and symlink tests against a local GitHub-style release\n");
} finally {
  if (server) { server.kill("SIGTERM"); await new Promise((resolve) => server.once("exit", resolve)); }
  await rm(temporary, { recursive: true, force: true });
}

async function release(web, tag, binaries) {
  const version = tag.slice(1).replace(/-.*$/, "");
  const directory = path.join(web, "releases/download", tag);
  await mkdir(directory, { recursive: true });
  const lines = [];
  for (const each of [target, linux ? "aarch64-apple-darwin" : "x86_64-unknown-linux-gnu"]) {
    const top = `mm-${tag}-${each}`;
    const stage = path.join(web, "stage", tag, top);
    await mkdir(path.join(stage, "completions"), { recursive: true });
    await mkdir(path.join(stage, "man"), { recursive: true });
    for (const name of each.endsWith("-unknown-linux-gnu") ? binaries : binaries.filter((name) => name !== "mm-ingress-helper")) {
      await writeFile(path.join(stage, name), `#!/bin/sh\nprintf '${name} ${version}\\n'\n`);
      await chmod(path.join(stage, name), 0o755);
    }
    await writeFile(path.join(stage, "LICENSE"), "fixture\n");
    await writeFile(path.join(stage, "README.md"), "fixture\n");
    await writeFile(path.join(stage, "completions/mm.bash"), "# fixture\n");
    await writeFile(path.join(stage, "completions/mm.elvish"), "# fixture\n");
    await writeFile(path.join(stage, "man/mm.1"), ".TH MM 1\n");
    const archive = path.join(directory, `${top}.tar.gz`);
    execFileSync("tar", ["-czf", archive, "-C", path.dirname(stage), top]);
    lines.push(`${await sha256(archive)}  ${top}.tar.gz`);
  }
  await writeFile(path.join(directory, `mm-${tag}-source.tar.gz`), "source\n");
  lines.push(`${await sha256(path.join(directory, `mm-${tag}-source.tar.gz`))}  mm-${tag}-source.tar.gz`);
  await writeFile(path.join(directory, "SHA256SUMS"), `${lines.sort((a, b) => a.slice(66).localeCompare(b.slice(66))).join("\n")}\n`);
}

function run(install, origin, args, extra = {}) {
  const result = spawnSync("sh", [install, ...args], { encoding: "utf8", env: { ...process.env, MM_RELEASE_TEST_ORIGIN: origin, MM_RELEASE_ALLOW_INSECURE_TEST_ORIGIN: "1", ...extra } });
  if (result.status !== 0) throw new Error(`installer failed: ${result.stderr}`);
  return result.stdout;
}

function expectFailure(install, origin, args, text, extra = {}) {
  const result = spawnSync("sh", [install, ...args], { encoding: "utf8", env: { ...process.env, MM_RELEASE_TEST_ORIGIN: origin, MM_RELEASE_ALLOW_INSECURE_TEST_ORIGIN: "1", ...extra } });
  if (result.status === 0 || !result.stderr.includes(text)) throw new Error(`expected installer failure '${text}', got ${result.status}: ${result.stderr}`);
}

async function executable(file, expected) {
  const result = spawnSync(file, ["--version"], { encoding: "utf8" });
  if (result.status !== 0 || result.stdout.trim() !== expected) throw new Error(`bad installed binary ${file}: ${result.stdout}`);
}

async function missing(file) {
  try { await stat(file); throw new Error(`unexpected installed file: ${file}`); } catch (error) { if (error.code !== "ENOENT") throw error; }
}
