// Verify the assembled binary/source distribution rather than the build tree.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { request, startExample } from './example-process.mjs';

const execute = promisify(execFile);
// The evidence directory release-evidence.py wrote for the shared workspace version.
const version = (await execute('python3', ['tools/release.py', 'version'])).stdout.trim();
const root = `.local/release/${version}`;
const descriptor = JSON.parse(await readFile(`${root}/release.json`, 'utf8'));
assert.equal(descriptor.target, 'x86_64-unknown-linux-gnu');
assert.equal(descriptor.msrv, '1.99.0');
const checksums = (await readFile(`${root}/SHA256SUMS`, 'utf8')).trim().split('\n');
assert.equal(checksums.length, descriptor.artifacts.length + 1);
for (const line of checksums) {
  const match = /^([a-f0-9]{64})  ([a-zA-Z0-9._-]+)$/.exec(line);
  assert.ok(match);
  assert.equal(createHash('sha256').update(await readFile(`${root}/${match[2]}`)).digest('hex'), match[1]);
}
const bom = JSON.parse(await readFile(`${root}/ssr.cdx.json`, 'utf8'));
const refs = new Set([bom.metadata.component['bom-ref'], ...bom.components.map(component => component['bom-ref'])]);
assert.equal(refs.size, bom.components.length + 1, 'Component references must be unique');
for (const edge of bom.dependencies) {
  assert.ok(refs.has(edge.ref));
  assert.ok((edge.dependsOn ?? []).every(ref => refs.has(ref)), 'Every inventory edge must resolve');
}
assert.ok(!JSON.stringify(bom).includes('/home/'), 'No local home paths in release inventory');
const native = bom.components.find(component => component.name === 'rusty-v8-native-archive');
assert.equal(native.hashes[0].content, descriptor.native_archive_sha256);
assert.ok(bom.components.some(component => component.name === '@gyral/core'));
assert.ok(bom.components.some(component => component.name === '@gyral/ssr'));
const npm = JSON.parse(await readFile(`${root}/npm-production.cdx.json`, 'utf8'));
assert.equal(npm.components.length, 3);
for (const component of npm.components) {
  assert.equal(component.hashes[0].alg, 'SHA-512');
  assert.match(component.hashes[0].content, /^[a-f0-9]{128}$/);
  if (component.name.startsWith('@gyral/')) assert.equal(component.version, '0.3.4');
}
const sourceName = descriptor.artifacts.find(name => name.endsWith('-source.tar.gz'));
const binaryName = descriptor.artifacts.find(name => name.endsWith('.tar.gz') && name !== sourceName);
const files = (await execute('tar', ['tzf', `${root}/${sourceName}`])).stdout.trim().split('\n');
for (const name of files) {
  assert.ok(!name.split('/').some(part => ['.beads', '.git', '.local', 'target', 'node_modules', 'dist', '__pycache__', '..'].includes(part)));
  assert.ok(!/\.(sqlite|pem|key)$/.test(name));
  assert.ok(!name.split('/').includes('vendor'));
}
assert.ok(files.some(name => name.endsWith('/tools/v8-archive.json')));
const directory = await mkdtemp(join(tmpdir(), 'gyral-release-smoke-'));
let process;
try {
  await execute('tar', ['xzf', `${root}/${sourceName}`, '-C', directory]);
  const source = join(directory, `gyral-rs-${descriptor.version}`);
  const policy = await execute('python3', ['tools/check.py'], { cwd: source });
  assert.ok(policy.stdout.includes('Policy passed'));
  await rm(source, { recursive: true });
  await execute('tar', ['xzf', `${root}/${binaryName}`, '-C', directory]);
  const binary = join(directory, `gyral-rs-${descriptor.version}`);
  const license = await readFile(join(binary, 'licenses/webpki-root-certs-1.0.9/LICENSE'), 'utf8');
  assert.ok(license.includes('Community Data License Agreement'));
  for (const name of ['@gyral-core-0.3.4', '@gyral-ssr-0.3.4']) {
    assert.ok((await readFile(join(binary, 'licenses', name, 'LICENSE'), 'utf8')).includes('MIT'));
  }
  process = await startExample({ binary: resolve(binary, 'gyral-axum-example'), env: { GYRAL_EXAMPLE_DIST: resolve(binary, 'dist') } });
  assert.equal((await request(process.origin, '/readyz')).status, 200);
  const response = await request(process.origin, '/?name=Release%20artifact');
  assert.equal(response.status, 200);
  assert.ok((await response.text()).includes('Hello, Release artifact'));
  await process.stop();
  console.log('Release: checksums, connected SBOM refs/native pin, private-data exclusion, source policy, agreement text, and extracted binary SSR/readiness/SIGTERM passed.');
} finally {
  if (process) await process.cleanup();
  await rm(directory, { recursive: true, force: true });
}
