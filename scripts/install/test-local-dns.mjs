#!/usr/bin/env node
// Test generated DNS configuration and privileged command sequencing without
// modifying the host. Real resolved/dnsmasq behavior is checked separately.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, writeFileSync, readFileSync, chmodSync, rmSync, existsSync } from 'node:fs';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '../..');
mkdirSync(path.join(root, '.modmon/install'), { recursive: true });
const temp = mkdtempSync(path.join(root, '.modmon/install/dns-fixture.'));
const mocks = path.join(temp, 'bin');
mkdirSync(mocks);
const mock = `#!${process.execPath}
const fs = require('node:fs'), path = require('node:path');
const args = process.argv.slice(2), name = path.basename(process.argv[1]), root = process.env.FIX_ROOT;
fs.appendFileSync(path.join(root, 'trace'), JSON.stringify([name, ...args])+'\\n');
if (name === 'getent') {
 if (fs.existsSync(path.join(root, 'ready'))) { console.log('127.0.0.1 STREAM '+args[1]); process.exit(0); }
 process.exit(2);
}
if (name === 'ip') process.exit(1);
if (name === 'sudo') {
 if (args[0] === 'install' && !args.includes('-d')) {
  const destination = path.join(root, 'system', args.at(-1));
  fs.mkdirSync(path.dirname(destination), {recursive:true});
  fs.copyFileSync(args.at(-2), destination);
 }
 if (args[0] === 'systemctl' && args[1] === 'restart') fs.writeFileSync(path.join(root, 'ready'), 'ready');
}
`;
for (const name of ['getent', 'dnsmasq', 'resolvectl', 'ip', 'sudo']) {
  const file = path.join(mocks, name);
  writeFileSync(file, mock);
  chmodSync(file, 0o755);
}
const env = { ...process.env, FIX_ROOT: temp, PATH: `${mocks}:${process.env.PATH}` };
const run = () => {
  const result = spawnSync('bash', [path.join(root, 'scripts/install/local-dns.sh')], { env, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  return result.stdout;
};
try {
  assert.match(run(), /Wildcard .test DNS is ready/);
  const unit = readFileSync(path.join(temp, 'system/etc/systemd/system/mm-test-dns.service'), 'utf8');
  assert.match(unit, /link add mm-test-dns type dummy/);
  assert.match(unit, /dns mm-test-dns 169\.254\.77\.53/);
  assert.match(unit, /domain mm-test-dns ~test/);
  assert.match(unit, /default-route mm-test-dns no/);
  assert.doesNotMatch(unit, /resolvectl (dns|domain|default-route|revert) lo/);
  assert.match(unit, /StartLimitBurst=3/);
  const config = readFileSync(path.join(temp, 'system/etc/mm/dnsmasq.conf'), 'utf8');
  assert.match(config, /address=\/test\/127\.0\.0\.1/);
  const before = readFileSync(path.join(temp, 'trace'), 'utf8');
  assert.match(run(), /reusing it/);
  const added = readFileSync(path.join(temp, 'trace'), 'utf8').slice(before.length);
  assert.doesNotMatch(added, /sudo/);
  assert.ok(existsSync(path.join(temp, 'ready')));
  console.log('DNS setup fixtures passed: dedicated interface, split DNS, bounded restarts, verification, and reuse.');
} finally {
  rmSync(temp, { recursive: true, force: true });
}
