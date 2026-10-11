import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { cp, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const execute = promisify(execFile);
const dist = 'examples/axum-app/dist';
const first = await readFile(`${dist}/manifest.json`);
await execute(process.execPath, ['tools/build-example.mjs'], { timeout: 15_000 });
assert.deepEqual(await readFile(`${dist}/manifest.json`), first, 'Unchanged inputs must produce identical artifact metadata');
const manifest = JSON.parse(first.toString());
const server = await readFile(`${dist}/server.js`);
const client = await readFile(`${dist}${manifest.client.path}`);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
assert.equal(hash(server), manifest.server_sha256);
assert.equal(hash(client), manifest.client.sha256);
assert.equal(manifest.client.path, `/assets/client.${hash(client)}.js`);

const root = await mkdtemp(join(tmpdir(), 'gyral-rs-artifacts-'));
try {
  for (const [name, corrupt] of [
    ['server digest', async dir => writeFile(join(dir, 'server.js'), Buffer.concat([server, Buffer.from(' ')]))],
    ['client digest', async dir => writeFile(join(dir, manifest.client.path.slice(1)), Buffer.concat([client, Buffer.from(' ')]))],
    ['manifest ABI', async dir => writeFile(join(dir, 'manifest.json'), JSON.stringify({ ...manifest, abi: 2 }))],
    ['oversized manifest', async dir => writeFile(join(dir, 'manifest.json'), ' '.repeat(16 * 1024 + 1))],
    ['missing manifest', async dir => rm(join(dir, 'manifest.json'))],
  ]) {
    const dir = join(root, name.replaceAll(' ', '-'));
    await cp(dist, dir, { recursive: true });
    await corrupt(dir);
    await assert.rejects(
      execute('target/debug/gyral-axum-example', [], {
        env: { ...process.env, GYRAL_EXAMPLE_DIST: dir, GYRAL_EXAMPLE_BIND: '127.0.0.1:0' },
        timeout: 5_000,
      }),
      error => {
        assert.equal(error.code, 1, `${name} must fail startup, not hang`);
        assert.ok(!error.stdout.includes('Gyral example listening'));
        return true;
      },
    );
  }
  console.log('Artifacts: repeatable build, matching digests, and five corrupt/missing artifact sets rejected before HTTP startup passed.');
} finally { await rm(root, { recursive: true, force: true }); }
