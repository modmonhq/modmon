#!/usr/bin/env node
// Exercise real installer filesystem/transaction/PATH behavior, replacing only
// external builders and platform probes. Native builds run in local-install CI.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync, spawnSync } from 'node:child_process';
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '../..');
// mm embeds a few files from outside core/ at compile time; the installer
// builds from a snapshot, so each one must be in it.
const snapshot = readFileSync(path.join(root, 'install.sh'), 'utf8')
  .match(/-cf "\$work\/source\.tar" ([^\n]+)/)[1].trim().split(/\s+/);
const rustSources = (directory) => readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
  const full = path.join(directory, entry.name);
  if (entry.isDirectory()) return rustSources(full);
  return entry.name.endsWith('.rs') ? [full] : [];
});
for (const file of rustSources(path.join(root, 'core/src'))) {
  for (const [, relative] of readFileSync(file, 'utf8').matchAll(/include_(?:str|bytes)!\("([^"]+)"\)/g)) {
    const embedded = path.relative(root, path.resolve(path.dirname(file), relative));
    if (embedded.startsWith('core/')) continue;
    assert.ok(
      snapshot.some((entry) => embedded === entry || embedded.startsWith(`${entry}/`)),
      `${path.relative(root, file)} embeds ${embedded}, which install.sh leaves out of its build snapshot`,
    );
  }
}
mkdirSync(path.join(root, '.modmon/install'), { recursive: true });
const temp = mkdtempSync(path.join(root, '.modmon/install/fixtures.'));
const repo = path.join(temp, 'checkout with spaces');
const home = path.join(temp, "home's space");
const mocks = path.join(temp, 'mocks');
const cache = path.join(home, 'cache');
const bin = path.join(home, '.local/bin');
const read = (p) => readFileSync(p, 'utf8');
const write = (p, s) => { mkdirSync(path.dirname(p), { recursive: true }); writeFileSync(p, s); };
const executable = (p, s) => { write(p, s); chmodSync(p, 0o755); };
mkdirSync(repo, { recursive: true });
mkdirSync(mocks, { recursive: true });
cpSync(path.join(root, 'install.sh'), path.join(repo, 'install.sh'));
cpSync(path.join(root, 'scripts/install/build.sh'), path.join(repo, 'build.sh'));
write(path.join(repo, 'scripts/install/build.sh'), read(path.join(root, 'scripts/install/build.sh')));
write(path.join(repo, 'scripts/install/local-dns.sh'), read(path.join(root, 'scripts/install/local-dns.sh')));
write(path.join(repo, 'scripts/install/local-dns-macos.sh'), read(path.join(root, 'scripts/install/local-dns-macos.sh')));
write(path.join(repo, 'packaging/local/Dockerfile'), read(path.join(root, 'packaging/local/Dockerfile')));
for (const file of ['packaging/mm-relay-shell', 'packaging/sshd/90-mm-relay.conf', 'packaging/systemd/mmd.service', 'packaging/systemd/mm-mesh.service', 'packaging/systemd/mm-network.service']) write(path.join(repo, file), 'embedded packaging');
write(path.join(repo, 'scripts/install/workstation.sh'), '#!/bin/bash\nprintf "%s\\n" "$1" >> "$FIX_SETUP_TRACE"\n[[ ${FIX_SETUP_FAIL:-0} != 1 ]]\n');
for (const file of ['Cargo.toml', 'Cargo.lock', 'core/Cargo.toml']) write(path.join(repo, file), 'fixture');
write(path.join(repo, 'rust-toolchain.toml'), '[toolchain]\nchannel = "1.99.1"\n');
write(path.join(repo, 'core/src/lib.rs'), 'first');
write(path.join(repo, 'core/src/main.rs'), 'fixture');
write(path.join(repo, 'core/src/domain/target/mod.rs'), 'source, not build output');
write(path.join(repo, 'docs/mcp.md'), 'embedded documentation');
write(path.join(repo, 'packages/ui/src/tokens.css'), 'shared dashboard styles');
write(path.join(repo, 'core/ui/dist/stale'), 'must not be copied');
write(path.join(repo, 'core/ui/node_modules/stale'), 'must not be copied');
const driver = `#!${process.execPath}
const fs = require('node:fs'), p = require('node:path'), cp = require('node:child_process');
const name = p.basename(process.argv[1]), a = process.argv.slice(2), e = process.env;
function binary(file, text) {
 fs.mkdirSync(p.dirname(file), {recursive:true});
 fs.writeFileSync(file, '#!/bin/sh\\nprintf \\"%s\\\\n\\" \\"'+text+'\\"\\n', {mode:0o755});
}
if(name==='uname') { console.log(a[0]==='-s' ? (e.FIX_OS || 'Linux') : (e.FIX_ARCH || 'x86_64')); }
else if(name==='xcrun') { process.exit(e.FIX_CLT_FAIL ? 1 : 0); }
else if(name==='docker') {
 if(a[0]==='info') { if(e.FIX_DOCKER_FAIL) process.exit(1); if(a.includes('--format')) console.log(e.FIX_ARCH || 'x86_64'); }
 else if(a[0]==='build') { if(e.FIX_BUILD_FAIL) process.exit(9); }
 else if(a[0]==='run') {
  const mounts=a.flatMap((x,i)=>x==='-v'?[a[i+1]]:[]);
  const source=mounts.find(x=>x.endsWith(':/workspace')).slice(0,-11);
  const output=mounts.find(x=>x.endsWith(':/cache/target')).slice(0,-14);
  if(fs.existsSync(p.join(source,'core/ui/dist')) || fs.existsSync(p.join(source,'core/ui/node_modules'))) process.exit(7);
  if(!fs.existsSync(p.join(source,'core/src/domain/target/mod.rs')) || !fs.existsSync(p.join(source,'docs/mcp.md')) || !fs.existsSync(p.join(source,'packages/ui/src/tokens.css'))) process.exit(7);
  const text=fs.readFileSync(p.join(source,'core/src/lib.rs'),'utf8');
  const target=a[a.length-2];
   const profile=a.includes('MM_BUILD_PROFILE=local')?'local':'release';
   binary(p.join(output,target,profile,'mm'), e.FIX_BAD_BINARY ? 'broken' : text);
   binary(p.join(output,target,profile,'mm-ingress-helper'), text);
   fs.writeFileSync(p.join(output,target,profile,'build-toolchains.txt'),'rustc fixture\\n');
   if(e.FIX_BAD_BINARY) fs.writeFileSync(p.join(output,target,profile,'mm'),'#!/bin/sh\\nexit 8\\n');
  if(a[a.length-1]==='1') binary(p.join(output,target,profile,'mmd'), text);
  if(e.FIX_EDIT_INSTALLER) fs.writeFileSync(e.FIX_INSTALLER, "#!/bin/bash\\nprintf 'unfinished quote\\n");
  if(e.FIX_EDIT_SETUP) fs.writeFileSync(p.join(p.dirname(e.FIX_INSTALLER),'scripts/install/workstation.sh'), '#!/bin/bash\\nexit 99\\n');
 }
} else if(name==='mv') {
 if(e.FIX_MV_FAIL && a[a.length-2].endsWith('/'+e.FIX_MV_FAIL)) process.exit(1);
 const result=cp.spawnSync('/bin/mv',a,{stdio:'inherit'}).status;
 if(e.FIX_INTERRUPT && a[a.length-2].endsWith('/mm')) process.kill(process.ppid,'SIGTERM');
 process.exit(result);
} else if(name==='curl') {
 const url=a[a.length-1], destination=a[a.indexOf('--output')+1];
 if(!e.FIX_DOWNLOADS) {console.error('unexpected network download'); process.exit(4);}
 fs.copyFileSync(p.join(e.FIX_DOWNLOADS,p.basename(url)),destination);
}
else if(name==='rustup-init') {
 fs.mkdirSync(p.join(e.CARGO_HOME,'bin'),{recursive:true});
 for(const tool of ['rustup','cargo','rustc']) fs.copyFileSync(process.argv[1],p.join(e.CARGO_HOME,'bin',tool));
}
else if(name==='rustc') { console.log('rustc fixture'); }
else if(name==='rustup') { fs.appendFileSync(e.FIX_TRACE,'rustup '+a.join(' ')+'\\n'); }
else if(name==='npm') {
 fs.appendFileSync(e.FIX_TRACE,'npm '+a.join(' ')+'\\n');
 if(a.includes('build')) {fs.mkdirSync('core/ui/dist/gate',{recursive:true}); fs.writeFileSync('core/ui/dist/index.html','new UI'); fs.writeFileSync('core/ui/dist/gate/cap_wasm_bg.wasm','wasm');}
} else if(name==='cargo') {
 if(a[0]==='--version') {console.log('cargo fixture'); process.exit(0);}
 fs.appendFileSync(e.FIX_TRACE,'cargo '+a.join(' ')+'\\n');
 const target=a[a.indexOf('--target')+1];
 if(!fs.existsSync('core/ui/dist/index.html')) process.exit(1);
  const profile=a.includes('--release')?'release':a[a.indexOf('--profile')+1];
  for(let i=0;i<a.length;i++) if(a[i]==='--bin') binary(p.join(e.CARGO_TARGET_DIR,target,profile,a[i+1]),'native');
}
`;
// `claude` stands for an installed coding tool; the driver ignores its arguments.
for (const name of ['uname', 'xcrun', 'docker', 'mv', 'curl', 'claude']) executable(path.join(mocks, name), driver);
const env = { ...process.env, HOME: home, HOSTNAME: 'fixture-host', XDG_CACHE_HOME: cache, XDG_CONFIG_HOME: path.join(home, '.config'), SHELL: '/bin/bash', PATH: `${mocks}:${process.env.PATH}`, FIX_TRACE: path.join(temp, 'trace') };
delete env.ZDOTDIR;
env.FIX_SETUP_TRACE = path.join(temp, 'setup-trace');
env.FIX_INSTALLER = path.join(repo, 'install.sh');
const run = (args = [], extra = {}, failure, setup = false, rebuild = true) => {
  const result = spawnSync('bash', [path.join(repo, 'install.sh'), ...(rebuild ? ['--rebuild'] : []), ...(setup ? [] : ['--no-setup']), ...args], { cwd: temp, env: { ...env, ...extra }, encoding: 'utf8' });
  const output = result.stdout + result.stderr;
  if (failure) { assert.notEqual(result.status, 0, output); assert.match(output, failure); }
  else assert.equal(result.status, 0, output);
  return output;
};
try {
  run(['--help']);
  assert.match(execFileSync('sh', [path.join(repo, 'install.sh'), '--help'], { encoding: 'utf8', env }), /Build this checkout/);
  run(['--jobs', '0'], {}, /positive integer/);
  run(['--install-dir'], {}, /requires a value/);
  run([], { FIX_OS: 'FreeBSD' }, /unsupported OS/);
  run([], { FIX_ARCH: 'riscv64' }, /only x86-64 and ARM64/);
  run([], { FIX_DOCKER_FAIL: '1' }, /Docker is unavailable/);
  write(path.join(home, '.bashrc'), "alias mm='./mm'\n");
  run();
  assert.equal(execFileSync(path.join(bin, 'mm'), ['--version'], { encoding: 'utf8' }).trim(), 'first');
  const withoutSetup = run([], { FIX_BUILD_FAIL: '1' }, undefined, false, false);
  assert.match(withoutSetup, /Build inputs unchanged/);
  assert.doesNotMatch(withoutSetup, /Connecting AI coding tools/);
  assert.ok(!existsSync(env.FIX_SETUP_TRACE));
  // Setup ends by connecting the coding tools it finds, after the workstation.
  const connected = /Setting up local TLS[\s\S]*Connecting AI coding tools to the mm MCP server[\s\S]*==> Installed:/;
  assert.match(run([], {}, undefined, true), connected);
  assert.equal(read(env.FIX_SETUP_TRACE).trim(), path.join(bin, 'mm'));
  assert.match(run(['--setup-only'], { FIX_BUILD_FAIL: '1' }, undefined, true), /Connecting AI coding tools/);
  assert.equal(read(env.FIX_SETUP_TRACE).trim().split('\n').length, 2);
  run([], { FIX_SETUP_FAIL: '1' }, /workstation setup is incomplete/, true);
  assert.ok(existsSync(path.join(bin, 'mm')));
  run(['--dev']);
  assert.match(read(path.join(bin, 'install-receipt.txt')), /profile=local/);
  assert.ok(existsSync(path.join(cache, 'mm-installer/workspaces/x86_64-unknown-linux-musl/core/src/lib.rs')));
  const installerSource = read(env.FIX_INSTALLER);
  try { assert.match(run([], { FIX_EDIT_INSTALLER: '1' }), /==> Installed:/); }
  finally { write(env.FIX_INSTALLER, installerSource); }
  const setupPath = path.join(repo, 'scripts/install/workstation.sh');
  const setupSource = read(setupPath);
  try { assert.match(run([], { FIX_EDIT_SETUP: '1' }, undefined, true), /==> Installed:/); }
  finally { write(setupPath, setupSource); }
  assert.ok(!existsSync(path.join(bin, 'mmd')));
  const bashProfile = read(path.join(home, '.bashrc'));
  assert.match(bashProfile, /mm local install/);
  // Verify the emitted quoting works for spaces and apostrophes.
  const shellPath = execFileSync('bash', ['-c', '. "$HOME/.bashrc"; . "$HOME/.bashrc"; printf "%s" "$PATH"'], { env, encoding: 'utf8' });
  assert.ok(shellPath.startsWith(`${bin}:`));
  assert.equal(shellPath.split(':').filter((entry) => entry === bin).length, 1);
  run();
  assert.equal(read(path.join(home, '.bashrc')), bashProfile);
  // Reloading startup files must remove the legacy alias, including one already
  // active in the calling shell, while retaining intentional custom aliases.
  const checkAliases = (shell, profile) => {
    const options = { env: { ...env, PROFILE: profile }, cwd: temp, encoding: 'utf8' };
    const resolved = execFileSync(shell, ['-c', 'alias mm=./mm; . "$PROFILE"; . "$PROFILE"; command -v mm'], options).trim();
    assert.equal(resolved, path.join(bin, 'mm'));
    assert.equal(execFileSync(shell, ['-c', '. "$PROFILE"; eval "mm --version"'], options).trim(), 'first');
    const contents = read(profile);
    write(profile, contents.replace("alias mm='./mm'", "alias mm='custom-mm --flag'"));
    assert.match(execFileSync(shell, ['-c', '. "$PROFILE"; alias mm'], options), /custom-mm --flag/);
    write(profile, contents);
  };
  checkAliases('bash', path.join(home, '.bashrc'));
  write(path.join(repo, 'core/src/lib.rs'), 'second');
  run(['--with-daemon']);
  assert.match(read(path.join(bin, 'mm')), /second/);
  assert.match(read(path.join(bin, 'mmd')), /second/);
  const old = read(path.join(bin, 'mm'));
  const receipt = read(path.join(bin, 'install-receipt.txt'));
  write(path.join(repo, 'core/src/lib.rs'), 'third');
  run(['--with-daemon'], { FIX_MV_FAIL: 'mmd' }, /Installation failed/);
  assert.equal(read(path.join(bin, 'mm')), old);
  assert.equal(read(path.join(bin, 'mmd')), old);
  assert.equal(read(path.join(bin, 'install-receipt.txt')), receipt);
  run(['--with-daemon'], { FIX_INTERRUPT: '1' }, /Installation failed/);
  assert.equal(read(path.join(bin, 'mm')), old);
  assert.equal(read(path.join(bin, 'mmd')), old);
  run([], { FIX_BUILD_FAIL: '1' }, /Installation failed/);
  run([], { FIX_BAD_BINARY: '1' }, /Installation failed/);
  assert.equal(read(path.join(bin, 'mm')), old);
  const fresh = path.join(home, 'fresh');
  run(['--install-dir', fresh, '--with-daemon'], { FIX_MV_FAIL: 'mmd' }, /Installation failed/);
  assert.ok(!existsSync(path.join(fresh, 'mm')));
  const unwritable = path.join(home, 'unwritable');
  mkdirSync(unwritable);
  chmodSync(unwritable, 0o555);
  if (process.getuid() !== 0) run(['--install-dir', unwritable], {}, /not writable/);
  chmodSync(unwritable, 0o755);
  const lock = path.join(cache, 'mm-installer/install.lock');
  mkdirSync(lock);
  run([], {}, /another installer/);
  assert.ok(existsSync(lock));
  rmSync(lock, { recursive: true });
  mkdirSync(lock);
  write(path.join(lock, 'owner'), '99999999 fixture-host\n');
  assert.match(run(), /Recovering stale installer lock/);
  assert.ok(!existsSync(lock));
  const destinationLock = path.join(bin, '.mm-install.lock');
  mkdirSync(destinationLock);
  write(path.join(destinationLock, 'owner'), '99999999 fixture-host\n');
  assert.match(run(), /Recovering stale installer lock/);
  assert.ok(!existsSync(destinationLock));
  mkdirSync(lock);
  write(path.join(lock, 'owner'), `${process.pid} fixture-host\n`);
  run([], {}, /another installer.*PID/);
  assert.ok(existsSync(lock));
  rmSync(lock, { recursive: true });
  run(['--no-modify-path'], { SHELL: '/bin/zsh' });
  assert.ok(!existsSync(path.join(home, '.zshrc')));
  write(path.join(home, 'zsh/.zshrc'), "alias mm='./mm'\n");
  run([], { SHELL: '/bin/zsh', ZDOTDIR: path.join(home, 'zsh') });
  assert.match(read(path.join(home, 'zsh/.zshrc')), /export PATH=/);
  // Restore the fixture version expected by the shared alias checks.
  write(path.join(repo, 'core/src/lib.rs'), 'first');
  const zshProfile = read(path.join(home, 'zsh/.zshrc'));
  const sessionOutput = run([], { SHELL: '/bin/zsh', ZDOTDIR: path.join(home, 'zsh') });
  assert.equal(read(path.join(home, 'zsh/.zshrc')), zshProfile);
  if (spawnSync('zsh', ['--version']).status === 0) checkAliases('zsh', path.join(home, 'zsh/.zshrc'));
  else console.log('Skipping Zsh execution checks: zsh is not installed.');
  const sessionCommand = sessionOutput.split('\n').find((line) => line.startsWith('For this Bash/Zsh session: ')).slice('For this Bash/Zsh session: '.length);
  assert.equal(execFileSync('bash', ['-c', `alias mm=./mm; ${sessionCommand}; command -v mm`], { env, encoding: 'utf8' }).trim(), path.join(bin, 'mm'));
  run([], { SHELL: '/usr/bin/fish' });
  assert.match(read(path.join(home, '.config/fish/conf.d/mm-path.fish')), /fish_add_path/);
  run(['--build-only']);
  assert.ok(existsSync(path.join(repo, 'target/local-install/x86_64-unknown-linux-musl/mm')));
  run(['--build-only'], { FIX_ARCH: 'aarch64' });
  assert.ok(existsSync(path.join(repo, 'target/local-install/aarch64-unknown-linux-musl/mm')));
  const linkBin = path.join(home, 'links');
  mkdirSync(linkBin);
  symlinkSync(path.join(bin, 'mm'), path.join(linkBin, 'mm'));
  run(['--install-dir', linkBin], {}, /non-regular file/);
  run([], { FIX_OS: 'Darwin', FIX_CLT_FAIL: '1' }, /xcode-select --install/);
  // Exercise native macOS control flow using a prepopulated private cache.
  const privateCache = path.join(cache, 'mm-installer');
  executable(path.join(privateCache, 'cargo/bin/rustup'), driver);
  executable(path.join(privateCache, 'cargo/bin/cargo'), driver);
  executable(path.join(privateCache, 'cargo/bin/rustc'), driver);
  for (const arch of ['x64', 'arm64']) {
    executable(path.join(privateCache, `node-v24.10.0-darwin-${arch}/bin/node`), '#!/bin/sh\nexit 0\n');
    executable(path.join(privateCache, `node-v24.10.0-darwin-${arch}/bin/npm`), driver);
  }
  run(['--with-daemon'], { FIX_OS: 'Darwin' });
  run(['--build-only'], { FIX_OS: 'Darwin', FIX_ARCH: 'arm64' });
  assert.match(read(env.FIX_TRACE), /cargo build --package mm --bin mm --bin mm-ingress-helper --bin mmd --features unstable --profile local --locked --target x86_64-apple-darwin/);
  assert.match(read(env.FIX_TRACE), /--target aarch64-apple-darwin/);
  assert.ok(read(env.FIX_TRACE).indexOf('npm --prefix core/ui run build') < read(env.FIX_TRACE).indexOf('cargo build'));
  // Exercise first-run private toolchain downloads, checksum failure, and retry.
  rmSync(path.join(privateCache, 'cargo/bin'), { recursive: true });
  rmSync(path.join(privateCache, 'node-v24.10.0-darwin-x64'), { recursive: true });
  const downloads = path.join(temp, 'downloads');
  executable(path.join(downloads, 'rustup-init'), driver);
  write(path.join(downloads, 'rustup-init.sha256'), `${'0'.repeat(64)}  rustup-init\n`);
  run([], { FIX_OS: 'Darwin', FIX_DOWNLOADS: downloads }, /checksum mismatch/);
  const digest = createHash('sha256').update(read(path.join(downloads, 'rustup-init'))).digest('hex');
  write(path.join(downloads, 'rustup-init.sha256'), `${digest}  rustup-init\n`);
  const archive = 'node-v24.10.0-darwin-x64';
  executable(path.join(downloads, archive, 'bin/node'), '#!/bin/sh\nprintf "v24.10.0\\n"\n');
  executable(path.join(downloads, archive, 'bin/npm'), driver);
  execFileSync('tar', ['-czf', path.join(downloads, `${archive}.tar.gz`), '-C', downloads, archive]);
  const nodeDigest = createHash('sha256').update(readFileSync(path.join(downloads, `${archive}.tar.gz`))).digest('hex');
  write(path.join(downloads, 'SHASUMS256.txt'), `${'0'.repeat(64)}  ${archive}.tar.gz\n`);
  run([], { FIX_OS: 'Darwin', FIX_DOWNLOADS: downloads }, /checksum mismatch/);
  write(path.join(downloads, 'SHASUMS256.txt'), `${nodeDigest}  ${archive}.tar.gz\n`);
  run([], { FIX_OS: 'Darwin', FIX_DOWNLOADS: downloads });
  assert.ok(existsSync(path.join(privateCache, 'cargo/bin/rustup')));
  assert.match(read(path.join(bin, 'install-receipt.txt')), /node v24.10.0/);
  console.log('Local installer fixtures passed: platform preflight, working-tree snapshots, install/update, daemon opt-in, rollback, broken builds/binaries, locking, PATH, build-only, and native macOS control flow.');
} finally {
  rmSync(temp, { recursive: true, force: true });
}
