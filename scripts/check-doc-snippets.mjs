import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const require = createRequire(path.join(root, "site/package.json"));
const { parseDocument } = require("yaml");
const Ajv2020 = require("ajv/dist/2020").default;
const inputs = [path.join(root, "docs"), path.join(root, "examples")];
const failures = [];
const projectSchema = JSON.parse(
  await readFile(path.join(root, "site/public/schema/mm.schema.json"), "utf8"),
);
const ajv = new Ajv2020({ strict: false });
for (const format of ["uuid", "int64", "uint", "uint16", "uint32", "uint64"]) {
  ajv.addFormat(format, true);
}
const validateProject = ajv.compile(projectSchema);
const cliReference = await readFile(
  path.join(root, "docs/reference/cli.md"),
  "utf8",
);
// Commands that take either a subcommand or a positional argument.
const acceptsPositionalOrSubcommand = new Set(["mm forward"]);
const commandPaths = new Set(
  [...cliReference.matchAll(/^#{1,6} `([^`]+)`$/gm)].map((match) => match[1]),
);

for (const input of inputs) {
  for (const file of await markdownFiles(input)) {
    const source = await readFile(file, "utf8");
    for (const block of fencedBlocks(source)) {
      const location = `${path.relative(root, file)}:${block.line}`;
      if (["yaml", "yml"].includes(block.language)) {
        const document = parseDocument(block.body, { uniqueKeys: true });
        for (const error of document.errors) failures.push(`${location} YAML: ${error.message}`);
        if (document.errors.length === 0) {
          validateSchemaOne(document.toJSON(), location);
        }
      } else if (block.language === "json") {
        try {
          JSON.parse(block.body);
        } catch (error) {
          failures.push(`${location} JSON: ${error.message}`);
        }
      } else if (["sh", "bash", "shell"].includes(block.language)) {
        const checked = spawnSync("bash", ["-n"], { input: block.body, encoding: "utf8" });
        if (checked.status !== 0) failures.push(`${location} shell: ${checked.stderr.trim()}`);
        validateMmCommands(block.body, location);
      }
    }
  }
}

for (const example of ["node-fullstack", "dotnet-web-api", "laravel-rust"]) {
  const file = path.join(root, "examples", example, ".config/mm.yaml");
  const document = parseDocument(await readFile(file, "utf8"), { uniqueKeys: true });
  const location = path.relative(root, file);
  for (const error of document.errors) failures.push(`${location} YAML: ${error.message}`);
  if (document.errors.length === 0) validateSchemaOne(document.toJSON(), location, true);
}

if (failures.length > 0) {
  console.error("Invalid documentation snippets:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exitCode = 1;
} else {
  console.log(
    "Documentation snippets passed syntax, schema 1, and generated Clap command checks.",
  );
}

function validateSchemaOne(value, location, required = false) {
  if (!value || typeof value !== "object" || Array.isArray(value)) return;
  if (!("schema-version" in value)) {
    if (required) failures.push(`${location} schema: schema-version is required`);
    return;
  }
  // A host policy (/etc/mm/host.yaml) also has a schema-version, but it is
  // not project configuration; mmd validates it with `mmd access validate`.
  if ("break-glass" in value) return;
  if (!validateProject(value)) {
    for (const error of validateProject.errors ?? []) {
      failures.push(`${location} schema: ${error.instancePath || "/"} ${error.message}`);
    }
  }
}

function validateMmCommands(source, location) {
  const logical = source.replace(/\\\n/g, " ");
  for (const match of logical.matchAll(/(?:^|[;&|]\s*)mm[ \t]+([^\n;|]*)/gm)) {
    const tokens = shellTokens(match[1]);
    let current = "mm";
    for (let index = 0; index < tokens.length; index += 1) {
      const token = tokens[index];
      if (token.startsWith("#")) break;
      if (token.startsWith("-")) {
        if (optionTakesValue(token)) index += 1;
        continue;
      }
      const candidate = `${current} ${token}`;
      if (commandPaths.has(candidate)) {
        current = candidate;
        continue;
      }
      const hasSubcommands = [...commandPaths].some((path) => path.startsWith(`${current} `));
      if (acceptsPositionalOrSubcommand.has(current)) break;
      if (current === "mm" || hasSubcommands) {
        failures.push(`${location} CLI: unknown command path '${candidate}'`);
      }
      break;
    }
  }
}

function shellTokens(source) {
  return [...source.matchAll(/"[^"]*"|'[^']*'|\S+/g)].map((match) =>
    match[0].replace(/^(['"])(.*)\1$/, "$2"),
  );
}

function optionTakesValue(option) {
  if (option.includes("=")) return false;
  return new Set([
    "-e",
    "--environment",
    "--root",
    "--runtime",
    "--target",
    "--mm-home",
    "--output",
    "--plan",
    "--deploy-plan",
    "--name",
    "--definition",
    "--operation",
    "--smtp-port",
    "--limit",
    "--outcome",
    "--since",
    "--before",
    "--host",
    "--user",
    "--host-key-fingerprint",
    "--host-key",
    "--identity-file",
    "--remote-root",
    "--daemon-fingerprint",
    "--driver",
    "--mode",
  ]).has(option);
}

function fencedBlocks(source) {
  const blocks = [];
  const pattern = /^```([A-Za-z0-9_-]+)[^\n]*\n([\s\S]*?)^```\s*$/gm;
  for (const match of source.matchAll(pattern)) {
    blocks.push({
      language: match[1].toLowerCase(),
      body: match[2],
      line: source.slice(0, match.index).split("\n").length,
    });
  }
  return blocks;
}

async function markdownFiles(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (["node_modules", "target"].includes(entry.name)) continue;
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await markdownFiles(entryPath)));
    else if (entry.isFile() && entry.name.endsWith(".md")) files.push(entryPath);
  }
  return files;
}
