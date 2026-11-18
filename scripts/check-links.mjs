import { access, readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';

const root = process.cwd();
const privateWorkspace = '.modmon';
const excludedDirectories = new Set([
  '.git',
  '.nuxt',
  '.output',
  'coverage',
  'dist',
  'node_modules',
  'target',
  'tasks',
]);
const markdownLinks = /!?\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g;
const referenceLinks = /^\s{0,3}\[[^\]]+\]:\s*<?([^\s>]+)>?/gm;
const htmlLinks = /\b(?:href|src)=["']([^"']+)["']/g;
const failures = [];

for (const file of await markdownFiles(root)) {
  const content = await readFile(file, 'utf8');
  for (const match of [
    ...content.matchAll(markdownLinks),
    ...content.matchAll(referenceLinks),
    ...content.matchAll(htmlLinks),
  ]) {
    const link = match[1];
    let target;
    try {
      target = decodeURIComponent(link.split(/[?#]/, 1)[0]);
    } catch {
      failures.push(`${path.relative(root, file)} -> invalid URL: ${link}`);
      continue;
    }
    if (target.split(/[\\/]/).includes(privateWorkspace)) {
      failures.push(`${path.relative(root, file)} -> private workspace: ${link}`);
      continue;
    }
    if (
      link.startsWith('#') ||
      link.startsWith('/') ||
      /^[a-z][a-z\d+.-]*:/i.test(link)
    ) {
      continue;
    }

    if (!target) continue;

    try {
      await access(path.resolve(path.dirname(file), target));
    } catch {
      failures.push(`${path.relative(root, file)} -> ${link}`);
    }
  }
}

if (failures.length > 0) {
  console.error('Broken relative Markdown links:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exitCode = 1;
} else {
  console.log('Relative Markdown link check passed.');
}

async function markdownFiles(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.name === privateWorkspace) continue;
    if (entry.isDirectory() && excludedDirectories.has(entry.name)) continue;

    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await markdownFiles(entryPath)));
    else if (entry.isFile() && entry.name.endsWith('.md')) files.push(entryPath);
  }
  return files;
}
