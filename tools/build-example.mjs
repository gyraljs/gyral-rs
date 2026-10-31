import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { build } from 'esbuild';

const root = 'examples/axum-app/dist';
// The manifest names the installed release; tools/check.py pins the adopted one exactly.
const installed = async pkg => JSON.parse(await readFile(`node_modules/@gyral/${pkg}/package.json`, 'utf8')).version;
const release = await installed('core');
assert.equal(await installed('ssr'), release, 'Server and browser must come from one Gyral release');
assert.match(release, /^\d+\.\d+\.\d+$/, 'Build bundles from a stable Gyral release');
const common = {
  bundle: true, write: false, minify: true, target: 'es2022',
  define: { 'process.env.NODE_ENV': '"production"' },
};
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const client = await build({
  ...common, entryPoints: ['examples/axum-app/web/client.mjs'],
  outfile: 'client.js', format: 'esm', platform: 'browser',
});
assert.equal(client.outputFiles.length, 1, 'Protocol 1 uses one self-contained browser entry');
const clientBytes = client.outputFiles[0].contents;
const clientHash = digest(clientBytes);
const path = `/assets/client.${clientHash}.js`;
const integrity = `sha256-${createHash('sha256').update(clientBytes).digest('base64')}`;
const server = await build({
  ...common, entryPoints: ['examples/axum-app/web/server.mjs'],
  outfile: 'server.js', format: 'iife', platform: 'neutral',
  define: { ...common.define, __GYRAL_CLIENT_PATH__: JSON.stringify(path), __GYRAL_CLIENT_INTEGRITY__: JSON.stringify(integrity) },
});
assert.equal(server.outputFiles.length, 1);
// Register the same static component specs using the exact published server APIs.
await import('../examples/axum-app/web/components.mjs');
// Gyral 0.3.10 exports its SSR protocol; earlier releases implement protocol 1.
const { styleHashes, SSR_PROTOCOL: protocol = 1 } = await import('@gyral/ssr');
assert.equal(protocol, 1, 'web/server.mjs declares protocol 1; update it with the new protocol');
const { documentCss } = await import('../examples/axum-app/web/styles.mjs');
const styles = (await styleHashes()).map(source => {
  assert.match(source, /^'sha256-[A-Za-z0-9+/]{43}='$/);
  return Buffer.from(source.slice(8, -1), 'base64').toString('hex');
});
const manifest = {
  abi: protocol, gyral: release, server_sha256: digest(server.outputFiles[0].contents),
  client: { path, sha256: clientHash },
  style_hashes: [...new Set([...styles, digest(documentCss)])].sort(),
};
await rm(`${root}/assets`, { recursive: true, force: true });
await mkdir(`${root}/assets`, { recursive: true });
await writeFile(`${root}${path}`, clientBytes);
await writeFile(`${root}/server.js`, server.outputFiles[0].contents);
await writeFile(`${root}/manifest.json`, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Built Gyral ${release} protocol ${protocol} server/browser pair, immutable client ${clientHash}, and ${manifest.style_hashes.length} static style hashes.`);
