import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { cp, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const execute = promisify(execFile);
const tree = (await execute('cargo', ['tree', '--locked', '-p', 'gyral-askama-client-example', '--edges', 'normal'], { timeout: 30_000 })).stdout;
assert.doesNotMatch(tree, /\b(?:deno_core|v8|gyral|gyral-renderer|gyral-axum) v/, 'Client example must not link an embedded renderer');
const dist = 'examples/askama-client-app/dist';
const first = await readFile(`${dist}/manifest.json`);
await execute(process.execPath, ['tools/build-client-example.mjs'], { timeout: 15_000 });
assert.deepEqual(await readFile(`${dist}/manifest.json`), first, 'Client build must reproduce its metadata');
const manifest = JSON.parse(first.toString());
const scriptPath = `assets/client.${manifest.script_sha256}.js`;
const cssPath = `assets/page.${manifest.css_sha256}.css`;
const script = await readFile(`${dist}/${scriptPath}`);
const css = await readFile(`${dist}/${cssPath}`);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
assert.equal(hash(script), manifest.script_sha256);
assert.equal(hash(css), manifest.css_sha256);
await assert.rejects(readFile(`${dist}/server.js`), { code: 'ENOENT' });
const root = await mkdtemp(join(tmpdir(), 'gyral-rs-client-artifacts-'));
const cases = [
  ['script digest', dir => writeFile(join(dir, scriptPath), Buffer.concat([script, Buffer.from(' ')]))],
  ['CSS digest', dir => writeFile(join(dir, cssPath), Buffer.concat([css, Buffer.from(' ')]))],
  ['manifest format', dir => writeFile(join(dir, 'manifest.json'), JSON.stringify({ ...manifest, format: 2 }))],
  ['Gyral version', dir => writeFile(join(dir, 'manifest.json'), JSON.stringify({ ...manifest, gyral: '0.0.0' }))],
  ['Gyral prerelease', dir => writeFile(join(dir, 'manifest.json'), JSON.stringify({ ...manifest, gyral: '0.3.10-next.0' }))],
  ['hash syntax', dir => writeFile(join(dir, 'manifest.json'), JSON.stringify({ ...manifest, script_sha256: '../server' }))],
  ['unknown field', dir => writeFile(join(dir, 'manifest.json'), JSON.stringify({ ...manifest, server: 'server.js' }))],
  ['oversized manifest', dir => writeFile(join(dir, 'manifest.json'), ' '.repeat(4097))],
  ['empty CSS', dir => writeFile(join(dir, cssPath), '')],
  ['oversized CSS', dir => writeFile(join(dir, cssPath), ' '.repeat(128 * 1024 + 1))],
  ['missing script', dir => rm(join(dir, scriptPath))],
];
try {
  for (const [name, corrupt] of cases) {
    const dir = join(root, name.replaceAll(' ', '-'));
    await cp(dist, dir, { recursive: true });
    await corrupt(dir);
    await assert.rejects(execute('target/debug/gyral-askama-client-example', [], {
      env: { ...process.env, GYRAL_CLIENT_DIST: dir, GYRAL_CLIENT_BIND: '127.0.0.1:0' }, timeout: 5_000,
    }), error => {
      assert.equal(error.code, 1, `${name}: must fail rather than hang`);
      assert.ok(!error.stdout.includes('Askama client example listening'));
      return true;
    });
  }
  const invalidDatabase = join(root, 'invalid.sqlite');
  await writeFile(invalidDatabase, 'synthetic-private-invalid-database-bytes');
  for (const database of [invalidDatabase, join(invalidDatabase, 'notes.sqlite')]) {
    await assert.rejects(execute('target/debug/gyral-askama-client-example', [], {
      env: { ...process.env, GYRAL_NOTES_DB: database, GYRAL_CLIENT_BIND: '127.0.0.1:0' }, timeout: 5000,
    }), error => {
      assert.equal(error.code, 1);
      assert.ok(!error.stdout.includes('Askama client example listening'));
      assert.ok(!error.stderr.includes('synthetic-private-invalid-database-bytes'));
      return true;
    });
  }
  console.log(`Client artifacts: renderer/V8 absent, repeatable verified JS/CSS, ${cases.length} invalid asset sets and 2 invalid database paths rejected before HTTP bind.`);
} finally { await rm(root, { recursive: true, force: true }); }
