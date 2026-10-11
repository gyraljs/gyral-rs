import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, webkit } from 'playwright';

await mkdir('.local', { recursive: true });
const manifest = JSON.parse(await readFile('examples/askama-client-app/dist/manifest.json', 'utf8'));
const scriptPath = `/assets/client.${manifest.script_sha256}.js`;
const cssPath = `/assets/page.${manifest.css_sha256}.css`;
const reservation = createServer();
reservation.listen(0, '127.0.0.1');
await once(reservation, 'listening');
const port = reservation.address().port;
await new Promise(resolve => reservation.close(resolve));
const url = `http://127.0.0.1:${port}`;
const databaseRoot = await mkdtemp(join(tmpdir(), 'gyral-client-browser-'));
const child = spawn('target/debug/gyral-askama-client-example', [], {
  env: { ...process.env, GYRAL_CLIENT_BIND: `127.0.0.1:${port}`, GYRAL_NOTES_DB: join(databaseRoot, 'notes.sqlite') }, stdio: ['ignore', 'pipe', 'pipe'],
});
let output = '';
child.stdout.on('data', data => { output += data; });
child.stderr.on('data', data => { output += data; });
const exited = once(child, 'exit');
try {
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (child.exitCode !== null) throw new Error(`Example exited: ${output}`);
    try { ready = (await fetch(`${url}/health`, { signal: AbortSignal.timeout(1_000) })).ok; } catch { /* Not yet bound. */ }
    if (ready) break;
    await delay(100);
  }
  assert.equal(ready, true, `Example startup: ${output}`);
  const initial = await fetch(`${url}/?name=Basketball&start=9`);
  assert.equal(initial.status, 200);
  assert.equal(initial.headers.get('cache-control'), 'no-store');
  const source = await initial.text();
  assert.ok(source.includes('<gyral-client-counter start="9">'));
  assert.ok(!/shadowrootmode|data-gyral-seed|<output/.test(source));
  assert.ok(source.includes('integrity="sha256-'));
  for (const [path, hash, type] of [[scriptPath, manifest.script_sha256, 'text/javascript'], [cssPath, manifest.css_sha256, 'text/css']]) {
    const response = await fetch(`${url}${path}`);
    assert.equal(response.status, 200);
    assert.equal(response.headers.get('cache-control'), 'public, max-age=31536000, immutable');
    assert.ok(response.headers.get('content-type').startsWith(type));
    assert.equal(createHash('sha256').update(Buffer.from(await response.arrayBuffer())).digest('hex'), hash);
    const head = await fetch(`${url}${path}`, { method: 'HEAD' });
    assert.equal(head.status, 200);
    assert.equal(await head.text(), '');
    const post = await fetch(`${url}${path}`, { method: 'POST' });
    assert.equal(post.status, 405);
    assert.equal(post.headers.get('cache-control'), 'no-store');
  }
  for (const path of ['/server.js', '/manifest.json', '/assets/client.js', '/assets/missing.css']) {
    const response = await fetch(`${url}${path}`);
    assert.equal(response.status, 404);
    assert.equal(response.headers.get('cache-control'), 'no-store');
  }
  for (const query of ['name=x', 'start=100', 'unexpected=1', `name=${'a'.repeat(4097)}`]) {
    const response = await fetch(`${url}/?${query}`);
    assert.equal(response.status, 400);
    assert.equal(response.headers.get('cache-control'), 'no-store');
    assert.ok(response.headers.get('content-security-policy'));
  }

  for (const [name, engine] of [['Chromium', chromium], ['WebKit', webkit]]) {
    const browser = await engine.launch({ headless: true });
    try {
      const plainContext = await browser.newContext({ javaScriptEnabled: false });
      const plain = await plainContext.newPage();
      await plain.goto(`${url}/?name=Basketball&start=9`);
      assert.equal(await plain.locator('h1').textContent(), 'Hello, Basketball');
      assert.ok(await plain.locator('.counter-fallback').isVisible());
      assert.equal(await plain.getByRole('button', { name: 'Increment' }).count(), 0);
      await plain.getByLabel('Your name', { exact: true }).fill('NoJS visitor');
      await plain.getByLabel('Starting count').fill('12');
      await Promise.all([plain.waitForURL('**/?name=NoJS+visitor&start=12'), plain.getByRole('button', { name: 'Load page' }).click()]);
      assert.equal(await plain.locator('h1').textContent(), 'Hello, NoJS visitor');
      assert.match(await plain.locator('.counter-fallback').textContent(), /12/);
      await plain.getByRole('link', { name: 'How it works' }).click();
      assert.equal(await plain.locator('h1').textContent(), 'A Rust page with browser widgets');
      await plainContext.close();

      const context = await browser.newContext();
      await context.addInitScript(() => {
        globalThis.__violations = [];
        document.addEventListener('securitypolicyviolation', event => globalThis.__violations.push(event.effectiveDirective));
      });
      const page = await context.newPage();
      const failures = [];
      page.on('pageerror', error => failures.push(error.message));
      page.on('console', event => { if (['warning', 'error'].includes(event.type())) failures.push(event.text()); });
      let release;
      const gate = new Promise(resolve => { release = resolve; });
      await page.route(`${url}${scriptPath}`, async route => { await gate; await route.continue(); });
      const response = await page.goto(`${url}/?name=Basketball&start=9`, { waitUntil: 'commit' });
      assert.ok(!/unsafe-inline|unsafe-eval/.test(response.headers()['content-security-policy']));
      await page.locator('.counter-fallback').waitFor();
      assert.equal(await page.locator('gyral-client-counter').evaluate(host => host.shadowRoot === null && !customElements.get(host.localName)), true);
      await page.getByLabel('Your name', { exact: true }).fill('Typed before client render');
      release();
      await page.getByRole('button', { name: 'Increment', exact: true }).waitFor();
      assert.equal(await page.locator('gyral-client-counter output').textContent(), '9');
      assert.equal(await page.getByLabel('Your name', { exact: true }).inputValue(), 'Typed before client render');
      assert.equal(await page.locator('.counter-fallback').isVisible(), false);
      assert.equal(await page.locator('gyral-client-greeting').evaluate(host => host.name), 'Basketball');
      assert.match(await page.locator('gyral-client-greeting').evaluate(host => host.shadowRoot.querySelector('p').textContent), /Hello, Basketball/);
      await page.getByRole('button', { name: 'Increment', exact: true }).focus();
      assert.equal(await page.getByRole('button', { name: 'Increment', exact: true }).evaluate(el => el.matches(':focus-visible') && getComputedStyle(el).outlineWidth === '3px'), true);
      await page.keyboard.press('Enter');
      await page.waitForFunction(() => document.querySelector('gyral-client-counter').shadowRoot.querySelector('output').textContent === '10');
      await page.getByRole('button', { name: 'Reset', exact: true }).click();
      await page.waitForFunction(() => document.querySelector('gyral-client-counter').shadowRoot.querySelector('output').textContent === '9');
      await page.getByLabel('Starting count').fill('14');
      await Promise.all([page.waitForURL('**/?name=Typed+before+client+render&start=14'), page.getByRole('button', { name: 'Load page' }).click()]);
      await page.getByRole('button', { name: 'Increment', exact: true }).waitFor();
      assert.equal(await page.locator('h1').textContent(), 'Hello, Typed before client render');
      assert.equal(await page.locator('gyral-client-counter output').textContent(), '14');
      assert.deepEqual(failures, []);
      assert.deepEqual(await page.evaluate(() => globalThis.__violations), []);
      await page.screenshot({ path: `.local/askama-client-${name.toLowerCase()}.png`, fullPage: true });
      await page.emulateMedia({ colorScheme: 'dark', reducedMotion: 'reduce' });
      const dark = await page.locator('body').evaluate(el => getComputedStyle(el).backgroundColor);
      await page.emulateMedia({ colorScheme: 'light' });
      assert.notEqual(await page.locator('body').evaluate(el => getComputedStyle(el).backgroundColor), dark);
      await page.setViewportSize({ width: 320, height: 700 });
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), true);
      await page.screenshot({ path: `.local/askama-client-${name.toLowerCase()}-mobile.png`, fullPage: true });
      await page.evaluate(() => { document.documentElement.style.fontSize = '200%'; });
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), true);
      await page.evaluate(() => {
        const script = document.createElement('script'); script.textContent = 'globalThis.__inlineAttack = true'; document.head.append(script);
        document.body.setAttribute('style', 'color: rgb(1, 2, 3)');
      });
      await page.waitForFunction(() => globalThis.__violations.length >= 2);
      assert.equal(await page.evaluate(() => globalThis.__inlineAttack), undefined);
      assert.notEqual(await page.locator('body').evaluate(el => getComputedStyle(el).color), 'rgb(1, 2, 3)');
      await context.close();

      const payload = '</script><img src=x onerror="globalThis.__attack=1"> 🏀';
      const escaping = await browser.newPage();
      await escaping.goto(`${url}/?name=${encodeURIComponent(payload)}&start=2`);
      await escaping.getByRole('button', { name: 'Increment', exact: true }).waitFor();
      assert.equal(await escaping.locator('h1').textContent(), `Hello, ${payload}`);
      assert.equal(await escaping.getByLabel('Your name', { exact: true }).inputValue(), payload);
      assert.equal(await escaping.locator('gyral-client-greeting').evaluate(host => host.name), payload);
      assert.equal(await escaping.locator('img').count(), 0);
      assert.equal(await escaping.evaluate(() => globalThis.__attack), undefined);
      await escaping.close();

      for (const mode of ['blocked', 'tampered']) {
        const unavailable = await browser.newPage();
        await unavailable.route(`${url}${scriptPath}`, route => mode === 'blocked' ? route.abort() : route.fulfill({ contentType: 'text/javascript', body: 'globalThis.__tampered = true;' }));
        await unavailable.goto(`${url}/?name=Fallback&start=3`);
        assert.ok(await unavailable.locator('.counter-fallback').isVisible());
        assert.equal(await unavailable.locator('h1').textContent(), 'Hello, Fallback');
        assert.equal(await unavailable.evaluate(() => customElements.get('gyral-client-counter')), undefined);
        assert.equal(await unavailable.evaluate(() => globalThis.__tampered), undefined);
        await unavailable.getByRole('link', { name: 'How it works' }).click();
        assert.equal(await unavailable.locator('h1').textContent(), 'A Rust page with browser widgets');
        await unavailable.close();
      }
      console.log(`${name}: Askama-only response, no-JS form/navigation, delayed first render, preserved form input, string/number attributes, counter/reset, escaping, CSP/SRI blocking, keyboard focus, both themes, 320px layout, and 200% text scaling passed.`);
    } finally { await browser.close(); }
  }
  assert.ok(!output.includes('Basketball'), 'Logs must omit query values');
} finally {
  child.kill('SIGTERM');
  const force = setTimeout(() => child.kill('SIGKILL'), 5_000); force.unref();
  const [code] = await exited;
  clearTimeout(force);
  assert.equal(code, 0, `Graceful SIGTERM: ${output}`);
  await rm(databaseRoot, { recursive: true, force: true });
}
