import { createHash } from "node:crypto";
import { lstat, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const [command, ...args] = process.argv.slice(2);

if (command === "create" && args.length === 6) {
  const [tree, commit, sourceDateEpoch, archive, inventoryPath, manifestPath] = args;
  const files = await inventory(tree);
  const archiveBytes = await readFile(archive);
  const archiveSha256 = digest(archiveBytes);
  await writeFile(
    inventoryPath,
    `${files.map((file) => `${file.sha256}  ${file.mode}  ${file.size}  ${file.path}`).join("\n")}\n`,
  );
  await writeFile(
    manifestPath,
    `${JSON.stringify(
      {
        schema_version: 1,
        source_commit: commit,
        source_date_epoch: Number(sourceDateEpoch),
        archive: {
          name: path.basename(archive),
          sha256: archiveSha256,
          size: archiveBytes.length,
        },
        files,
      },
      null,
      2,
    )}\n`,
  );
  process.stdout.write(`${archiveSha256}  ${path.basename(archive)}\n`);
} else if (command === "compare" && args.length === 2) {
  const [left, right] = args;
  const leftFiles = await inventory(left);
  const rightFiles = await inventory(right);
  if (JSON.stringify(leftFiles) !== JSON.stringify(rightFiles)) {
    throw new Error("extracted archive inventory differs from the staged source tree");
  }
} else {
  process.stderr.write(
    "Usage: release-manifest.mjs create TREE COMMIT EPOCH ARCHIVE INVENTORY MANIFEST | compare LEFT RIGHT\n",
  );
  process.exitCode = 2;
}

async function inventory(root) {
  const files = [];
  await walk(root, "", files);
  return files.sort((left, right) => left.path.localeCompare(right.path, "en"));
}

async function walk(root, relative, files) {
  const directory = path.join(root, relative);
  const entries = await readdir(directory, { withFileTypes: true });
  entries.sort((left, right) => left.name.localeCompare(right.name, "en"));
  for (const entry of entries) {
    const child = path.join(relative, entry.name);
    const absolute = path.join(root, child);
    const stats = await lstat(absolute);
    if (stats.isSymbolicLink()) {
      throw new Error(`release source must not contain symlinks: ${child}`);
    }
    if (stats.isDirectory()) {
      await walk(root, child, files);
      continue;
    }
    if (!stats.isFile()) throw new Error(`unsupported archive entry: ${child}`);
    const bytes = await readFile(absolute);
    files.push({
      path: child.split(path.sep).join("/"),
      mode: (stats.mode & 0o777).toString(8).padStart(4, "0"),
      size: stats.size,
      sha256: digest(bytes),
    });
  }
}

function digest(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}
