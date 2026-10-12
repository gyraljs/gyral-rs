import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, webkit } from 'playwright';

await mkdir('.local', { recursive: true });
const root = await mkdtemp(join(tmpdir(), 'gyral-notes-browser-'));
const manifest = JSON.parse(await readFile('examples/askama-client-app/dist/manifest.json', 'utf8'));
const scriptPath = `/assets/client.${manifest.script_sha256}.js`;
const reservation = createServer();
reservation.listen(0, '127.0.0.1');
await once(reservation, 'listening');
const port = reservation.address().port;
await new Promise(resolve => reservation.close(resolve));
const url = `http://127.0.0.1:${port}`;
let running;
let logs = '';
async function start() {
  const child = spawn('target/debug/gyral-askama-client-example', [], {
    env: { ...process.env, GYRAL_CLIENT_BIND: `127.0.0.1:${port}`, GYRAL_NOTES_DB: join(root, 'notes.sqlite') },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  running = { child, exited: once(child, 'exit') };
  child.stdout.on('data', chunk => { logs += chunk; });
  child.stderr.on('data', chunk => { logs += chunk; });
  for (let attempt = 0; attempt < 100; attempt += 1) {
    assert.equal(child.exitCode, null, logs);
    try { if ((await fetch(`${url}/health`, { signal: AbortSignal.timeout(1000) })).ok) return; } catch { /* Not yet bound. */ }
    await delay(100);
  }
  throw new Error(`Notes startup timed out: ${logs}`);
}
async function stop() {
  if (!running) return;
  const { child, exited } = running;
  running = undefined;
  child.kill('SIGTERM');
  const force = setTimeout(() => child.kill('SIGKILL'), 5000); force.unref();
  const [code] = await exited;
  clearTimeout(force);
  assert.equal(code, 0, logs);
}
async function save(page) {
  const response = page.waitForResponse(r => r.request().method() === 'POST');
  await page.getByRole('button', { name: 'Save note', exact: true }).click();
  const posted = await response;
  assert.equal(posted.status(), 303);
  await page.waitForURL(/\/notes\/\d+$/);
  return page.url();
}

try {
  await start();
  // The restart contract includes durable data, cookie ownership and stale proofs.
  const form = await fetch(`${url}/notes/new`);
  const cookie = form.headers.get('set-cookie').split(';')[0];
  const csrf = /name="csrf" value="([^"]+)"/.exec(await form.text())[1];
  const posted = await fetch(`${url}/notes`, { method: 'POST', redirect: 'manual', headers: { cookie, 'content-type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams({ csrf, title: 'Restart evidence', body: 'Synthetic durable record' }) });
  assert.equal(posted.status, 303);
  const persistedPath = posted.headers.get('location');
  await stop();
  await start();
  const reopened = await fetch(`${url}${persistedPath}`, { headers: { cookie } });
  assert.equal(reopened.status, 200);
  assert.ok((await reopened.text()).includes('Synthetic durable record'));
  const stale = await fetch(`${url}${persistedPath}`, { method: 'POST', headers: { cookie, 'content-type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams({ csrf, title: 'Stale form', body: 'Must not replace saved content' }) });
  assert.equal(stale.status, 403);
  const staleHtml = await stale.text();
  assert.ok(staleHtml.includes('value="Stale form"'));
  assert.ok(staleHtml.includes('Must not replace saved content'));
  assert.notEqual(/name="csrf" value="([^"]+)"/.exec(staleHtml)[1], csrf);
  const unchanged = await fetch(`${url}${persistedPath}`, { headers: { cookie } });
  assert.ok((await unchanged.text()).includes('Synthetic durable record'));

  for (const [name, engine] of [['Chromium', chromium], ['WebKit', webkit]]) {
    const browser = await engine.launch({ headless: true });
    try {
      for (const mode of ['disabled', 'blocked']) {
        const context = await browser.newContext({ javaScriptEnabled: mode !== 'disabled' });
        const page = await context.newPage();
        if (mode === 'blocked') await page.route(`${url}${scriptPath}`, route => route.abort());
        await page.goto(`${url}/notes`);
        assert.ok(await page.getByText('Your notebook is empty.', { exact: false }).isVisible());
        // A link click returns once navigation commits, before the new page is
        // parsed. Wait for its load event before a non-retrying check.
        await page.getByRole('link', { name: 'New note', exact: true }).click();
        await page.waitForURL(`${url}/notes/new`);
        assert.ok(await page.locator('.preview-fallback').isVisible());
        await page.getByLabel('Title', { exact: true }).fill(`Native ${name} ${mode}`);
        await page.getByLabel('Note', { exact: true }).fill('Saved without a client renderer.\nSecond line.');
        const detail = await save(page);
        await page.reload();
        assert.equal(await page.locator('.note-body').textContent(), 'Saved without a client renderer.\nSecond line.');
        await page.getByRole('link', { name: 'Edit note', exact: true }).click();
        await page.getByLabel('Title', { exact: true }).fill('   ');
        await page.getByLabel('Note', { exact: true }).fill('Preserve this unsaved draft.');
        const invalid = page.waitForResponse(r => r.request().method() === 'POST');
        await page.getByRole('button', { name: 'Save note', exact: true }).click();
        assert.equal((await invalid).status(), 422);
        await page.locator('#title-error').waitFor();
        assert.equal(await page.getByLabel('Title', { exact: true }).inputValue(), '   ');
        assert.equal(await page.getByLabel('Note', { exact: true }).inputValue(), 'Preserve this unsaved draft.');
        assert.equal(await page.getByLabel('Title', { exact: true }).getAttribute('aria-invalid'), 'true');
        await page.getByLabel('Title', { exact: true }).fill('Corrected note');
        assert.equal(await save(page), detail);
        await page.getByRole('link', { name: 'All notes', exact: false }).click();
        await page.getByRole('link', { name: 'Corrected note', exact: true }).click();
        await page.getByRole('link', { name: 'Delete note…', exact: true }).click();
        await page.waitForURL(/\/notes\/\d+\/delete$/);
        assert.equal(await page.locator('h1').textContent(), 'Delete this note?');
        const deleted = page.waitForResponse(r => r.request().method() === 'POST');
        await page.getByRole('button', { name: 'Delete note', exact: true }).click();
        assert.equal((await deleted).status(), 303);
        await page.waitForURL(`${url}/notes`);
        assert.ok(await page.getByText('Your notebook is empty.', { exact: false }).isVisible());
        await context.close();
      }

      const context = await browser.newContext();
      await context.addInitScript(() => {
        globalThis.__violations = [];
        document.addEventListener('securitypolicyviolation', e => globalThis.__violations.push(e.effectiveDirective));
      });
      const page = await context.newPage();
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      let release;
      const gate = new Promise(resolve => { release = resolve; });
      await page.route(`${url}${scriptPath}`, async route => { await gate; await route.continue(); });
      await page.goto(`${url}/notes/new`, { waitUntil: 'commit' });
      await page.locator('#note-title').waitFor();
      assert.equal(await page.locator('gyral-note-preview').evaluate(host => host.shadowRoot), null);
      await page.getByLabel('Title', { exact: true }).fill('Typed before JavaScript');
      const draft = '<img src=x onerror="globalThis.__attack=1">\n📝 Native text';
      await page.getByLabel('Note', { exact: true }).fill(draft);
      release();
      await page.waitForFunction(() => document.querySelector('gyral-note-preview').shadowRoot?.querySelector('h3')?.textContent === 'Typed before JavaScript');
      assert.equal(await page.getByLabel('Note', { exact: true }).inputValue(), draft);
      assert.equal(await page.locator('gyral-note-preview .body').textContent(), draft);
      assert.equal(await page.locator('gyral-note-preview output').textContent(), String([...draft].length));
      assert.equal(await page.locator('gyral-note-preview img').count(), 0);
      assert.equal(await page.evaluate(() => globalThis.__attack), undefined);
      assert.deepEqual(await page.evaluate(() => {
        const form = document.querySelector('#note-editor');
        const shadow = document.querySelector('gyral-note-preview').shadowRoot;
        const probe = document.createElement('input'); probe.name = 'shadow_probe'; probe.value = 'Excluded'; probe.setAttribute('form', 'note-editor'); shadow.append(probe);
        const result = { names: [...new FormData(form).keys()], shadowForm: probe.form === null, roots: [...form.querySelectorAll('input,textarea')].every(el => el.getRootNode() === document) };
        probe.remove();
        return result;
      }), { names: ['csrf', 'title', 'body'], shadowForm: true, roots: true });
      await page.setViewportSize({ width: 320, height: 700 });
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), true);
      await page.evaluate(() => { document.documentElement.style.fontSize = '200%'; });
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), true);
      await page.getByLabel('Title', { exact: true }).fill('Edited native title');
      await page.waitForFunction(() => document.querySelector('gyral-note-preview').shadowRoot.querySelector('h3').textContent === 'Edited native title');
      await page.evaluate(() => {
        const preview = document.querySelector('gyral-note-preview');
        globalThis.__detachedPreview = preview;
        preview.remove();
      });
      await page.getByLabel('Title', { exact: true }).fill('Typed while detached');
      await page.evaluate(() => { document.querySelector('.preview-panel').append(globalThis.__detachedPreview); });
      await page.waitForFunction(() => document.querySelector('gyral-note-preview').shadowRoot.querySelector('h3').textContent === 'Typed while detached');
      assert.deepEqual(errors, []);
      assert.deepEqual(await page.evaluate(() => globalThis.__violations), []);
      // WebKit's screenshot helper inserts an inline body{} stylesheet. Capture after CSP assertions.
      await page.evaluate(() => { document.documentElement.style.fontSize = ''; });
      await page.setViewportSize({ width: 1280, height: 900 });
      await page.screenshot({ path: `.local/notes-${name.toLowerCase()}-desktop.png`, fullPage: true });
      await page.setViewportSize({ width: 320, height: 700 });
      await page.screenshot({ path: `.local/notes-${name.toLowerCase()}-mobile.png`, fullPage: true });
      const detail = await save(page);
      assert.equal(await page.locator('.note-body').textContent(), draft);
      assert.equal(await page.locator('img').count(), 0);
      await page.getByRole('link', { name: 'Edit note', exact: true }).click();
      await page.getByLabel('Note', { exact: true }).fill('x'.repeat(4001));
      const invalid = page.waitForResponse(r => r.request().method() === 'POST');
      await page.getByRole('button', { name: 'Save note', exact: true }).click();
      assert.equal((await invalid).status(), 422);
      await page.locator('#body-error').waitFor();
      assert.equal((await page.getByLabel('Note', { exact: true }).inputValue()).length, 4001);
      await page.goto(detail);
      assert.equal(await page.locator('.note-body').textContent(), draft);
      await context.close();
      console.log(`${name}: native create/read/edit/delete with JS disabled and blocked; 303/422 flows; escaped, delayed and reattached Gyral preview; native FormData/shadow boundary; CSP, 320px and 200% text passed.`);
    } finally { await browser.close(); }
  }
  assert.ok(!logs.includes('Synthetic durable record') && !logs.includes(csrf) && !logs.includes(cookie));
  console.log('Notes restart: same SQLite file and notebook cookie retain ownership/data; old CSRF proofs are rejected; logs omit note content and capabilities.');
} finally {
  await stop();
  await rm(root, { recursive: true, force: true });
}
