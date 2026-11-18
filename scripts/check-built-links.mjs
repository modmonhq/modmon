import { access, readdir, readFile, stat } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const dist = path.resolve(process.argv[2] ?? "site/.output/public");
const referencePattern = /\s(?:href|src)=["']([^"']+)["']/g;
const failures = [];

for (const file of await htmlFiles(dist)) {
  const html = await readFile(file, "utf8");
  for (const match of html.matchAll(referencePattern)) {
    const reference = match[1];
    if (
      reference.startsWith("#") ||
      reference.startsWith("//") ||
      /^[a-z][a-z\d+.-]*:/i.test(reference)
    ) {
      continue;
    }

    const pathname = decodeURIComponent(reference.split(/[?#]/, 1)[0]);
    if (!pathname) continue;

    const target = pathname.startsWith("/")
      ? path.join(dist, pathname)
      : path.resolve(baseDirectory(file), pathname);
    if (!(await existsAsBuiltTarget(target))) {
      failures.push(`${path.relative(dist, file)} -> ${reference}`);
    }
  }
}

if (failures.length > 0) {
  console.error("Broken references in built site:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exitCode = 1;
} else {
  console.log("Built site link check passed.");
}

// The site serves docs/guide/index.html as /docs/guide, without a trailing
// slash, so a relative reference on that page resolves from /docs/.
function baseDirectory(file) {
  const directory = path.dirname(file);
  if (path.basename(file) !== "index.html" || directory === dist) {
    return directory;
  }
  return path.dirname(directory);
}

async function existsAsBuiltTarget(target) {
  try {
    const details = await stat(target);
    if (details.isFile()) return true;
    if (details.isDirectory()) {
      await access(path.join(target, "index.html"));
      return true;
    }
  } catch {
    if (!path.extname(target)) {
      try {
        await access(path.join(target, "index.html"));
        return true;
      } catch {
        return false;
      }
    }
  }
  return false;
}

async function htmlFiles(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await htmlFiles(entryPath)));
    else if (entry.isFile() && entry.name.endsWith(".html"))
      files.push(entryPath);
  }
  return files;
}
