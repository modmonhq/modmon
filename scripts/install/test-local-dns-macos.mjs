#!/usr/bin/env node
// Test macOS DNS file generation and privileged command sequencing without
// modifying the host. Real resolver behavior is checked on a Mac.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '../..');
mkdirSync(path.join(root, '.modmon/install'), { recursive: true });
const temp = mkdtempSync(path.join(root, '.modmon/install/macos-dns-fixture.'));
const mocks = path.join(temp, 'bin');
const prefix = path.join(temp, 'brew prefix');
mkdirSync(mocks);
mkdirSync(path.join(prefix, 'opt/dnsmasq/sbin'), { recursive: true });
const mock = `#!${process.execPath}
const fs = require('node:fs'), path = require('node:path');
const args = process.argv.slice(2), name = path.basename(process.argv[1]), root = process.env.FIX_ROOT;
const has = (file) => fs.existsSync(path.join(root, file));
const touch = (file) => fs.writeFileSync(path.join(root, file), file);
fs.appendFileSync(path.join(root, 'trace'), JSON.stringify([name, ...args])+'\\n');
if (name === 'dscacheutil') { if (has('ready')) console.log('name: '+args[3]+'\\nip_address: 127.0.0.1'); process.exit(0); }
if (name === 'brew') { console.log(process.env.FIX_PREFIX); process.exit(0); }
if (name === 'otool') { console.log(args[1]+':\\n\\t'+(process.env.FIX_FOREIGN_LIB || '/usr/lib/libSystem.B.dylib (compatibility version 1.0.0)')); process.exit(0); }
if (name === 'ifconfig') { console.log('lo0: flags=8049<UP,LOOPBACK>\\n\\tinet 127.0.0.1 netmask 0xff000000 '); if (has('alias')) console.log('\\tinet 127.0.77.53 netmask 0xffffffff '); process.exit(0); }
if (name === 'dnsmasq') { if (args[0] === '--test') process.exit(0); process.exit(1); }
if (name === 'sudo') {
  const [command, ...rest] = args;
  if (command === 'install' && !rest.includes('-d')) {
    const destination = path.join(root, 'system', rest.at(-1));
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.copyFileSync(rest.at(-2), destination);
  }
  if (command === 'launchctl' && rest[0] === 'bootout') process.exit(3);
  if (command === 'launchctl' && rest[0] === 'bootstrap') {
    if (rest[2].endsWith('local.mm.loopback-alias.plist')) touch('alias');
    if (rest[2].endsWith('local.mm.dns.plist') && has('alias')) touch('ready');
  }
}
process.exit(0);
`;
for (const name of ['dscacheutil', 'brew', 'otool', 'ifconfig', 'plutil', 'sudo']) {
  writeFileSync(path.join(mocks, name), mock);
  chmodSync(path.join(mocks, name), 0o755);
}
writeFileSync(path.join(prefix, 'opt/dnsmasq/sbin/dnsmasq'), mock);
chmodSync(path.join(prefix, 'opt/dnsmasq/sbin/dnsmasq'), 0o755);
const env = { ...process.env, FIX_ROOT: temp, FIX_PREFIX: prefix, PATH: `${mocks}:${process.env.PATH}` };
const run = (args = [], extra = {}) => spawnSync('bash', [path.join(root, 'scripts/install/local-dns-macos.sh'), ...args], { env: { ...env, ...extra }, encoding: 'utf8' });
const trace = () => readFileSync(path.join(temp, 'trace'), 'utf8').trim().split('\n').map((line) => JSON.parse(line));
const system = (file) => readFileSync(path.join(temp, 'system', file), 'utf8');
try {
  const foreign = run([], { FIX_FOREIGN_LIB: '/opt/homebrew/lib/libfoo.dylib (compatibility version 1.0.0)' });
  assert.notEqual(foreign.status, 0);
  assert.match(foreign.stderr, /non-system libraries/);
  assert.ok(!existsSync(path.join(temp, 'system')), 'no files are installed when the helper would not be self-contained');

  const installed = run();
  assert.equal(installed.status, 0, installed.stdout + installed.stderr);
  assert.match(installed.stdout, /Wildcard .test DNS is ready/);
  assert.equal(system('etc/resolver/test'), '# Managed by mm local setup\nnameserver 127.0.77.53\n');
  const config = system('Library/Application Support/mm/dns/dnsmasq.conf');
  for (const line of ['listen-address=127.0.77.53', 'bind-interfaces', 'no-resolv', 'local=/test/', 'address=/test/127.0.0.1', 'user=nobody']) {
    assert.ok(config.split('\n').includes(line), `dnsmasq.conf has ${line}`);
  }
  const daemon = system('Library/LaunchDaemons/local.mm.dns.plist');
  assert.match(daemon, /<string>\/Library\/PrivilegedHelperTools\/local\.mm\.dnsmasq<\/string>/);
  assert.match(daemon, /<string>--conf-file=\/Library\/Application Support\/mm\/dns\/dnsmasq\.conf<\/string>/);
  assert.match(daemon, /Managed by mm local setup/);
  assert.match(system('Library/LaunchDaemons/local.mm.loopback-alias.plist'), /<string>\/sbin\/ifconfig<\/string>\s*<string>lo0<\/string>\s*<string>alias<\/string>\s*<string>127\.0\.77\.53<\/string>/);
  assert.equal(system('Library/PrivilegedHelperTools/local.mm.dnsmasq'), readFileSync(path.join(prefix, 'opt/dnsmasq/sbin/dnsmasq'), 'utf8'));
  const calls = trace().filter(([name]) => name === 'sudo').map((call) => call.slice(1, 4).join(' '));
  const aliasBoot = calls.indexOf('launchctl bootstrap system');
  assert.ok(aliasBoot > calls.findIndex((call) => call.startsWith('install -o root')), 'files are installed before jobs load');
  assert.ok(trace().some((call) => call[0] === 'dscacheutil' && call.includes('mail.mm.test')), 'verification uses system resolution for mail.mm.test');

  const before = trace().length;
  const reused = run();
  assert.equal(reused.status, 0, reused.stderr);
  assert.match(reused.stdout, /reusing it/);
  assert.ok(!trace().slice(before).some(([name]) => name === 'sudo'), 'a working resolver is reused without sudo');

  const removed = run(['--remove']);
  assert.equal(removed.status, 0, removed.stderr);
  const removal = trace().slice(before).filter(([name]) => name === 'sudo').map((call) => call.slice(1).join(' '));
  assert.ok(removal.some((call) => call === 'launchctl bootout system/local.mm.dns'));
  assert.ok(removal.some((call) => call.startsWith('rm -f /etc/resolver/test')));
  console.log('macOS DNS fixtures passed: self-contained helper, scoped resolver, ordered loading, verification, reuse, removal.');
} finally {
  rmSync(temp, { recursive: true, force: true });
}
