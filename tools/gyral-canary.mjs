// Gyral canary (Gyral ADR 0034): build the axum example with each named Gyral release
// and render it through the renderer test, the artifact smoke, and the browser suites.
// Run it for each supported protocol's minimum and newest release. Every release is
// installed into a throwaway copy, so testing a release never adopts it here.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { cp, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const flags = process.argv.slice(2).filter(argument => argument.startsWith('--'));
const releases = process.argv.slice(2).filter(argument => !argument.startsWith('--'));
const withoutAgeGate = flags.includes('--without-age-gate');
assert.ok(flags.every(flag => flag === '--without-age-gate'), `Unknown flag in ${flags.join(' ')}`);
assert.ok(releases.length > 0, 'Usage: node tools/gyral-canary.mjs [--without-age-gate] <Gyral release>...');
for (const release of releases) assert.match(release, /^\d+\.\d+\.\d+$/, `${release}: test stable releases only`);

const root = resolve('.');
const run = async (command, args, { cwd = root, env = {} } = {}) => {
  const child = spawn(command, args, { cwd, env: { ...process.env, ...env }, stdio: 'inherit' });
  const [code, signal] = await once(child, 'exit');
  assert.equal(code, 0, `${command} ${args.join(' ')} exited with ${code ?? signal}`);
};
// The copy holds only the JavaScript inputs; Rust runs from this checkout's build.
const inputs = ['pnpm-workspace.yaml', 'pnpm-lock.yaml', 'tools/build-example.mjs', 'tools/artifact-smoke.mjs', 'tools/browser-smoke.mjs', 'examples/axum-app/web'];

for (const release of releases) {
  const copy = await mkdtemp(join(tmpdir(), `gyral-canary-${release}-`));
  try {
    for (const path of inputs) await cp(path, join(copy, path), { recursive: true });
    const packageJson = JSON.parse(await readFile('package.json', 'utf8'));
    for (const name of ['@gyral/core', '@gyral/ssr']) packageJson.dependencies[name] = release;
    await writeFile(join(copy, 'package.json'), `${JSON.stringify(packageJson, null, 2)}\n`);
    if (withoutAgeGate) {
      const workspace = await readFile('pnpm-workspace.yaml', 'utf8');
      assert.match(workspace, /^minimumReleaseAge: \d+$/m);
      await writeFile(join(copy, 'pnpm-workspace.yaml'), workspace.replace(/^minimumReleaseAge: \d+$/m, 'minimumReleaseAge: 0'));
    }
    await symlink(join(root, 'target'), join(copy, 'target'));

    console.log(`Gyral canary: installing ${release}${withoutAgeGate ? ' without the release-age gate' : ''}.`);
    await run('pnpm', ['install', '--no-frozen-lockfile'], { cwd: copy });
    for (const name of ['core', 'ssr']) {
      const installed = JSON.parse(await readFile(join(copy, `node_modules/@gyral/${name}/package.json`), 'utf8'));
      assert.equal(installed.version, release, `@gyral/${name} must resolve to ${release}`);
    }
    await run(process.execPath, ['tools/build-example.mjs'], { cwd: copy });
    const dist = join(copy, 'examples/axum-app/dist');
    const manifest = JSON.parse(await readFile(join(dist, 'manifest.json'), 'utf8'));
    assert.equal(manifest.gyral, release);
    assert.equal(manifest.abi, 1, `Gyral ${release} must declare protocol 1`);

    await run('cargo', ['nextest', 'run', '--locked', '-p', 'gyral-renderer', '--test', 'gyral'], { env: { GYRAL_EXAMPLE_DIST: dist } });
    await run(process.execPath, ['tools/artifact-smoke.mjs'], { cwd: copy });
    await run(process.execPath, ['tools/browser-smoke.mjs'], { cwd: copy, env: { GYRAL_EXAMPLE_DIST: dist } });
    console.log(`Gyral canary: ${release} built protocol ${manifest.abi} bundles that render, hydrate, and verify.`);
  } finally {
    await rm(copy, { recursive: true, force: true });
  }
}
console.log(`Gyral canary passed for ${releases.join(' and ')}.`);
