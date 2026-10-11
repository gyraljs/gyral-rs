import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { setTimeout as delay } from 'node:timers/promises';

const execute = promisify(execFile);
const docker = async (...args) => (await execute('docker', args, { timeout: 30_000 })).stdout.trim();
const id = await docker('run', '--detach', '--publish', '127.0.0.1::3037', 'gyral-rs:example');
let stopped = false;
try {
  const binding = await docker('port', id, '3037/tcp');
  assert.match(binding, /^127\.0\.0\.1:\d+$/);
  const origin = `http://${binding}`;
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      ready = (await fetch(`${origin}/readyz`, { signal: AbortSignal.timeout(1_000) })).ok;
    } catch { /* The container may not have bound its port yet. */ }
    if (ready) break;
    await delay(100);
  }
  assert.equal(ready, true, 'Container must become reachable');
  const page = await fetch(`${origin}/?name=Docker`, { signal: AbortSignal.timeout(10_000) });
  assert.equal(page.status, 200);
  assert.equal(page.headers.get('cache-control'), 'no-store');
  assert.match(page.headers.get('x-request-id'), /^[a-f0-9]{32}-[a-f0-9]{16}$/);
  const html = await page.text();
  assert.ok(html.includes('Hello, Docker'));
  assert.ok(html.includes('shadowrootmode="open"'));
  const script = /<script\b[^>]*\btype="module"[^>]*>/.exec(html)?.[0];
  assert.ok(script, 'The server must emit a client URL and SRI token');
  const path = /\bsrc="([^"]+)"/.exec(script)?.[1];
  const integrity = /\bintegrity="([^"]+)"/.exec(script)?.[1];
  assert.ok(path && integrity);
  assert.match(path, /^\/assets\/client\.[a-f0-9]{64}\.js$/);
  assert.ok(page.headers.get('content-security-policy').includes(`'${integrity}'`));
  const asset = await fetch(`${origin}${path}`, { signal: AbortSignal.timeout(5_000) });
  assert.equal(asset.status, 200);
  assert.equal(asset.headers.get('cache-control'), 'public, max-age=31536000, immutable');
  const bytes = Buffer.from(await asset.arrayBuffer());
  assert.ok(bytes.length > 0);
  assert.equal(integrity, `sha256-${createHash('sha256').update(bytes).digest('base64')}`);
  assert.equal(path, `/assets/client.${createHash('sha256').update(bytes).digest('hex')}.js`);
  const missing = await fetch(`${origin}/assets/client.js`, { signal: AbortSignal.timeout(5_000) });
  assert.equal(missing.status, 404);
  assert.equal(missing.headers.get('cache-control'), 'no-store');
  const failure = await fetch(`${origin}/failure`, { signal: AbortSignal.timeout(5_000) });
  assert.equal(failure.status, 500);
  assert.equal(await failure.text(), 'The page could not be rendered.');
  assert.equal(await docker('exec', id, 'id', '-u'), '10001');
  await docker('exec', id, 'sh', '-c', 'if command -v node; then exit 1; fi');
  const live = await fetch(`${origin}/livez`, { signal: AbortSignal.timeout(1_000) });
  assert.equal(live.status, 200);
  const metrics = await (await fetch(`${origin}/metrics`, { signal: AbortSignal.timeout(1_000) })).text();
  assert.match(metrics, /gyral_renderer_ready 1/);
  assert.match(metrics, /gyral_renderer_queue_capacity 16/);
  await docker('stop', '--timeout', '15', id);
  stopped = true;
  assert.equal(await docker('inspect', '--format', '{{.State.ExitCode}}', id), '0');
  assert.equal(await docker('inspect', '--format', '{{.Config.StopSignal}}', id), 'SIGTERM');
  assert.ok((await docker('logs', id)).includes('renderer threads joined'));
  console.log('Container: SSR, verified assets/CSP, opaque errors, UID 10001, Node absent, readiness/metrics/request IDs, and SIGTERM + worker joins passed.');
} catch (error) {
  console.error(await docker('logs', id));
  throw error;
} finally {
  if (!stopped) await docker('stop', '--timeout', '15', id);
  await docker('rm', id);
}
