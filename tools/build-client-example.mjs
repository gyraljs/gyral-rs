import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { build } from 'esbuild';

const root = 'examples/askama-client-app';
const release = '0.3.8';
assert.equal(JSON.parse(await readFile('node_modules/@gyral/core/package.json', 'utf8')).version, release);
const browser = await build({
  entryPoints: [`${root}/web/client.mjs`], outfile: 'client.js',
  bundle: true, write: false, minify: true, target: 'es2022',
  format: 'esm', platform: 'browser', conditions: ['gyral-client-only'],
  define: { 'process.env.NODE_ENV': '"production"' }, metafile: true,
});
assert.equal(browser.outputFiles.length, 1, 'Use one self-contained browser entry');
const inputs = Object.keys(browser.metafile.inputs);
assert.ok(inputs.some(path => path.endsWith('/hydration-off.js')), 'Resolve Gyral client-only mode');
assert.ok(!inputs.some(path => path.endsWith('/hydration-loader.js') || path.includes('@gyral/ssr')), 'Do not bundle hydration or SSR');
const css = await build({ entryPoints: [`${root}/web/page.css`], outfile: 'page.css', write: false, minify: true, target: ['chrome120', 'safari18'] });
assert.equal(css.outputFiles.length, 1);
const scriptBytes = browser.outputFiles[0].contents;
const cssBytes = css.outputFiles[0].contents;
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const manifest = { format: 1, gyral: release, script_sha256: digest(scriptBytes), css_sha256: digest(cssBytes) };
await rm(`${root}/dist`, { recursive: true, force: true });
await mkdir(`${root}/dist/assets`, { recursive: true });
await writeFile(`${root}/dist/assets/client.${manifest.script_sha256}.js`, scriptBytes);
await writeFile(`${root}/dist/assets/page.${manifest.css_sha256}.css`, cssBytes);
await writeFile(`${root}/dist/manifest.json`, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Built client-only Gyral (${scriptBytes.length} bytes) and external CSS (${cssBytes.length} bytes); no server JS bundle.`);
