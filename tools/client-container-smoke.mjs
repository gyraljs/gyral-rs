import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { setTimeout as delay } from 'node:timers/promises';

const execute = promisify(execFile);
const docker = async (...args) => (await execute('docker', args, { timeout: 30_000 })).stdout.trim();
const volume = await docker('volume', 'create');
const id = await docker('run', '--detach', '--mount', `type=volume,source=${volume},target=/app/data`, '--publish', '127.0.0.1::3038', 'gyral-rs:askama-client');
let stopped = false;
try {
  const binding = await docker('port', id, '3038/tcp');
  assert.match(binding, /^127\.0\.0\.1:\d+$/);
  const origin = `http://${binding}`;
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try { ready = (await fetch(`${origin}/health`, { signal: AbortSignal.timeout(1_000) })).ok; } catch { /* Not yet bound. */ }
    if (ready) break;
    await delay(100);
  }
  assert.equal(ready, true);
  const page = await fetch(`${origin}/?name=Docker&start=8`, { signal: AbortSignal.timeout(5_000) });
  assert.equal(page.status, 200);
  assert.equal(page.headers.get('cache-control'), 'no-store');
  const html = await page.text();
  assert.ok(html.includes('<bdi>Docker</bdi>'));
  assert.ok(html.includes('<gyral-client-counter start="8">'));
  assert.ok(!/shadowrootmode|data-gyral-seed|<output/.test(html));
  for (const [tag, location, extension] of [['script', 'src', 'js'], ['link', 'href', 'css']]) {
    const markup = new RegExp(`<${tag}\\b[^>]*\\bintegrity="sha256-[^"]+"[^>]*>`).exec(html)?.[0];
    assert.ok(markup);
    const path = new RegExp(`\\b${location}="([^"]+)"`).exec(markup)?.[1];
    const integrity = /\bintegrity="([^"]+)"/.exec(markup)?.[1];
    assert.ok(path && integrity);
    const response = await fetch(`${origin}${path}`, { signal: AbortSignal.timeout(5_000) });
    assert.equal(response.status, 200);
    assert.equal(response.headers.get('cache-control'), 'public, max-age=31536000, immutable');
    const bytes = Buffer.from(await response.arrayBuffer());
    const digest = createHash('sha256').update(bytes);
    assert.equal(integrity, `sha256-${digest.digest('base64')}`);
    assert.ok(path.endsWith(`.${createHash('sha256').update(bytes).digest('hex')}.${extension}`));
    if (tag === 'script') assert.ok(page.headers.get('content-security-policy').includes(`'${integrity}'`));
  }
  const about = await fetch(`${origin}/about`);
  assert.equal(about.status, 200);
  assert.ok((await about.text()).includes('A Rust page with browser widgets'));
  const form = await fetch(`${origin}/notes/new`);
  assert.equal(form.status, 200);
  const cookie = form.headers.get('set-cookie').split(';')[0];
  const csrf = /name="csrf" value="([^"]+)"/.exec(await form.text())[1];
  const created = await fetch(`${origin}/notes`, {
    method: 'POST', redirect: 'manual',
    headers: { cookie, 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({ csrf, title: 'Container persistence', body: 'Synthetic saved note' }),
  });
  assert.equal(created.status, 303);
  const notePath = created.headers.get('location');
  for (const path of ['/server.js', '/manifest.json', '/assets/client.js']) {
    const missing = await fetch(`${origin}${path}`);
    assert.equal(missing.status, 404);
    assert.equal(missing.headers.get('cache-control'), 'no-store');
  }
  assert.equal(await docker('exec', id, 'id', '-u'), '10001');
  await docker('exec', id, 'sh', '-c', 'if command -v node || command -v deno; then exit 1; fi; test ! -e /app/dist/server.js');
  await docker('stop', '--timeout', '5', id);
  stopped = true;
  assert.equal(await docker('inspect', '--format', '{{.State.ExitCode}}', id), '0');
  // Replace the container while retaining its named volume and the browser cookie.
  const replacement = await docker('run', '--detach', '--mount', `type=volume,source=${volume},target=/app/data`, '--publish', '127.0.0.1::3038', 'gyral-rs:askama-client');
  try {
    const reopenedOrigin = `http://${await docker('port', replacement, '3038/tcp')}`;
    let reopened;
    for (let attempt = 0; attempt < 100; attempt += 1) {
      try { reopened = await fetch(`${reopenedOrigin}${notePath}`, { headers: { cookie }, signal: AbortSignal.timeout(1000) }); } catch { /* Starting. */ }
      if (reopened?.ok) break;
      await delay(100);
    }
    assert.equal(reopened.status, 200);
    assert.ok((await reopened.text()).includes('Synthetic saved note'));
    const edit = await fetch(`${reopenedOrigin}${notePath}/edit`, { headers: { cookie } });
    const newCsrf = /name="csrf" value="([^"]+)"/.exec(await edit.text())[1];
    const updated = await fetch(`${reopenedOrigin}${notePath}`, { method: 'POST', redirect: 'manual', headers: { cookie, 'content-type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams({ csrf: newCsrf, title: 'Updated in container', body: 'Retained volume' }) });
    assert.equal(updated.status, 303);
    const deleted = await fetch(`${reopenedOrigin}${notePath}/delete`, { method: 'POST', redirect: 'manual', headers: { cookie, 'content-type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams({ csrf: newCsrf }) });
    assert.equal(deleted.status, 303);
    assert.equal((await fetch(`${reopenedOrigin}${notePath}`, { headers: { cookie } })).status, 404);
  } finally {
    await docker('stop', '--timeout', '5', replacement);
    assert.equal(await docker('inspect', '--format', '{{.State.ExitCode}}', replacement), '0');
    await docker('rm', replacement);
  }
  console.log('Client container: native CRUD/303, SQLite survives container replacement through a named volume, cookie ownership, verified assets/CSP, UID 10001, Node/Deno/V8 absent, and graceful SIGTERM passed.');
} catch (error) {
  console.error(await docker('logs', id));
  throw error;
} finally {
  if (!stopped) await docker('stop', '--timeout', '5', id);
  await docker('rm', id);
  await docker('volume', 'rm', volume);
}
