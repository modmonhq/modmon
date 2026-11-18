import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const scripts = path.dirname(fileURLToPath(import.meta.url));
const temporary = await mkdtemp(path.join(tmpdir(), "mm-doc-checks-"));

try {
  const source = path.join(temporary, "source");
  await mkdir(source);
  await writeFile(path.join(source, "guide.md"), "# Guide\n");
  await writeFile(path.join(source, "README.md"), "[Guide](guide.md)\n");
  expectPass("source Markdown links", "check-links.mjs", source);

  await mkdir(path.join(source, ".modmon"));
  await writeFile(path.join(source, ".modmon", "notes.md"), "[Draft](missing.md)\n");
  expectPass("private working notes", "check-links.mjs", source);
  await writeFile(path.join(source, "README.md"), "[Notes](.modmon/notes.md)\n");
  expectFailure("private documentation dependency", "check-links.mjs", source,
    "README.md -> private workspace");
  await writeFile(path.join(source, "README.md"), "[Notes](%2Emodmon/notes.md)\n");
  expectFailure("encoded private documentation dependency", "check-links.mjs", source,
    "README.md -> private workspace");
  for (const markdown of [
    "[Notes](/.modmon/notes.md)\n",
    "[Notes][private]\n\n[private]: .modmon/notes.md\n",
    '<a href=".modmon/notes.md">Notes</a>\n',
  ]) {
    await writeFile(path.join(source, "README.md"), markdown);
    expectFailure("private link forms", "check-links.mjs", source,
      "README.md -> private workspace");
  }

  await writeFile(path.join(source, "README.md"), "[Missing](missing.md)\n");
  expectFailure(
    "source Markdown links",
    "check-links.mjs",
    source,
    "README.md -> missing.md",
  );

  // The generated site: each route is a directory with an index.html, served
  // without a trailing slash, next to the 200.html and 404.html fallbacks.
  const built = path.join(temporary, "public");
  await mkdir(path.join(built, "docs", "guide"), { recursive: true });
  await mkdir(path.join(built, "docs", "reference"));
  await mkdir(path.join(built, "_nuxt"));
  await mkdir(path.join(built, "schema"));
  await writeFile(path.join(built, "_nuxt", "app.js"), "export {};\n");
  await writeFile(path.join(built, "schema", "mm.schema.json"), "{}\n");
  await writeFile(path.join(built, "docs", "index.html"), "<h1>Docs</h1>\n");
  await writeFile(
    path.join(built, "docs", "guide", "index.html"),
    '<a href="reference#options">Reference</a><a href="../schema/mm.schema.json">Schema</a>\n',
  );
  await writeFile(
    path.join(built, "docs", "reference", "index.html"),
    '<a href="/docs/guide">Guide</a><a href="/docs">Docs</a>\n',
  );
  for (const fallback of ["200.html", "404.html"]) {
    await writeFile(
      path.join(built, fallback),
      '<a href="/">Home</a><script src="/_nuxt/app.js"></script>\n',
    );
  }
  await writeFile(
    path.join(built, "index.html"),
    '<a href="/docs/guide">Guide</a><a href="docs/reference">Reference</a><script src="/_nuxt/app.js"></script><img src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==" alt="">\n',
  );
  expectPass("built links and assets", "check-built-links.mjs", scripts, built);

  await writeFile(
    path.join(built, "index.html"),
    '<script src="/_nuxt/missing.js"></script>\n',
  );
  expectFailure(
    "built asset references",
    "check-built-links.mjs",
    scripts,
    "index.html -> /_nuxt/missing.js",
    built,
  );

  await writeFile(path.join(built, "index.html"), "<h1>Home</h1>\n");
  await writeFile(
    path.join(built, "docs", "guide", "index.html"),
    '<a href="/docs/missing">Missing</a>\n',
  );
  expectFailure(
    "built route references",
    "check-built-links.mjs",
    scripts,
    "docs/guide/index.html -> /docs/missing",
    built,
  );

  // A relative link on /docs/guide resolves from /docs/, not /docs/guide/.
  await writeFile(
    path.join(built, "docs", "guide", "index.html"),
    '<a href="../reference">Reference</a>\n',
  );
  expectFailure(
    "built relative references",
    "check-built-links.mjs",
    scripts,
    "docs/guide/index.html -> ../reference",
    built,
  );
} finally {
  await rm(temporary, { recursive: true, force: true });
}

console.log("Documentation checker tests passed.");

function run(script, cwd, argument) {
  return spawnSync(
    process.execPath,
    [path.join(scripts, script), ...(argument ? [argument] : [])],
    {
      cwd,
      encoding: "utf8",
    },
  );
}

function expectPass(name, script, cwd, argument) {
  const result = run(script, cwd, argument);
  assert.equal(
    result.status,
    0,
    `${name} should pass:\n${result.stderr}${result.stdout}`,
  );
}

function expectFailure(name, script, cwd, expected, argument) {
  const result = run(script, cwd, argument);
  assert.notEqual(result.status, 0, `${name} should fail`);
  assert.match(
    `${result.stderr}${result.stdout}`,
    new RegExp(escapeRegExp(expected)),
  );
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
