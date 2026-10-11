import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { chromium, webkit } from 'playwright';

const manifest = JSON.parse(await readFile('examples/axum-app/dist/manifest.json', 'utf8'));
const reservation = createServer();
reservation.listen(0, '127.0.0.1');
await once(reservation, 'listening');
const port = reservation.address().port;
await new Promise(resolve => reservation.close(resolve));
const url = `http://127.0.0.1:${port}`;
const child = spawn('target/debug/gyral-axum-example', [], {
  env: { ...process.env, GYRAL_EXAMPLE_BIND: `127.0.0.1:${port}` },
  stdio: ['ignore', 'pipe', 'pipe'],
});
let output = '';
child.stdout.on('data', data => { output += data; });
child.stderr.on('data', data => { output += data; });
const exited = once(child, 'exit');
try {
  await new Promise((resolve, reject) => {
    const deadline = setTimeout(() => { clearInterval(poll); reject(new Error(`Example startup timed out: ${output}`)); }, 15_000);
    const poll = setInterval(async () => {
      try {
        if ((await fetch(`${url}/health`, { signal: AbortSignal.timeout(1_000) })).ok) {
          clearTimeout(deadline); clearInterval(poll); resolve();
        }
      } catch { /* The executable has not bound its port yet. */ }
    }, 100);
    child.once('exit', code => { clearTimeout(deadline); clearInterval(poll); reject(new Error(`Example exited ${code}: ${output}`)); });
  });

  const asset = await fetch(`${url}${manifest.client.path}`);
  assert.equal(asset.status, 200);
  assert.equal(asset.headers.get('cache-control'), 'public, max-age=31536000, immutable');
  assert.match(asset.headers.get('content-type'), /^text\/javascript/);
  const head = await fetch(`${url}${manifest.client.path}`, { method: 'HEAD' });
  assert.equal(head.status, 200);
  assert.equal(await head.text(), '');
  for (const path of ['/assets/client.js', '/assets/missing.js', '/server.js', '/manifest.json']) {
    const miss = await fetch(`${url}${path}`);
    assert.equal(miss.status, 404);
    assert.equal(miss.headers.get('cache-control'), 'no-store');
  }
  const method = await fetch(`${url}${manifest.client.path}`, { method: 'POST' });
  assert.equal(method.status, 405);
  assert.equal(method.headers.get('cache-control'), 'no-store');
  const failed = await fetch(`${url}/failure`);
  assert.equal(failed.status, 500);
  assert.equal(failed.headers.get('cache-control'), 'no-store');
  assert.ok(failed.headers.get('content-security-policy'));
  assert.equal(await failed.text(), 'The page could not be rendered.');
  assert.ok(!output.includes('synthetic-private-render-diagnostic'));

  for (const [name, engine] of [['Chromium', chromium], ['WebKit', webkit]]) {
    const browser = await engine.launch({ headless: true });
    try {
      const noJs = await browser.newContext({ javaScriptEnabled: false });
      const plain = await noJs.newPage();
      await plain.goto(`${url}/?name=Basketball`);
      assert.equal(await plain.locator('h1').textContent(), 'Hello, Basketball');
      assert.equal(await plain.locator('gyral-counter output').textContent(), '0');
      assert.deepEqual(await plain.locator('gyral-store-card output').allTextContents(), ['7', '7']);
      assert.match(await plain.locator('time').textContent(), /1:30.*EST/);
      assert.equal(await plain.getByLabel('Your name', { exact: true }).inputValue(), 'Basketball');
      assert.equal(await plain.getByLabel('Synthetic private note').isDisabled(), true);
      assert.equal(await plain.locator('gyral-counter button').evaluate(el => getComputedStyle(el).minBlockSize), '44px');
      await plain.getByLabel('Your name', { exact: true }).fill('NoJS visitor');
      await Promise.all([plain.waitForURL('**/?name=NoJS+visitor'), plain.getByRole('button', { name: 'Save name' }).click()]);
      assert.equal(await plain.locator('h1').textContent(), 'Hello, NoJS visitor');
      assert.equal(new URL(plain.url()).searchParams.has('private_note'), false);
      await noJs.close();

      const context = await browser.newContext();
      await context.addInitScript(() => {
        globalThis.__cspViolations = [];
        document.addEventListener('securitypolicyviolation', event => globalThis.__cspViolations.push(event.effectiveDirective));
      });
      const page = await context.newPage();
      const failures = [];
      page.on('pageerror', error => failures.push(error.message));
      page.on('console', event => { if (['warning', 'error'].includes(event.type())) failures.push(event.text()); });
      let release;
      const gate = new Promise(resolve => { release = resolve; });
      await page.route(`${url}${manifest.client.path}`, async route => { await gate; await route.continue(); });
      const response = await page.goto(`${url}/?name=Basketball`, { waitUntil: 'commit' });
      assert.equal(response.status(), 200);
      assert.equal(response.headers()['cache-control'], 'no-store');
      const policy = response.headers()['content-security-policy'];
      assert.ok(policy.includes('script-src \'sha256-'));
      assert.ok(!policy.includes('unsafe-inline'));
      assert.ok(!policy.includes('unsafe-eval'));
      await page.locator('gyral-counter output').waitFor();
      await page.evaluate(() => {
        globalThis.__originalNodes = [
          document.querySelector('gyral-counter').shadowRoot.querySelector('output'),
          ...Array.from(document.querySelectorAll('gyral-store-card'), host => host.shadowRoot.querySelector('output')),
          document.querySelector('gyral-name-form').shadowRoot.querySelector('input[name="name"]'),
          document.querySelector('gyral-time-card').shadowRoot.querySelector('time'),
        ];
      });
      await page.getByLabel('Your name', { exact: true }).fill('Typed before hydration');
      release();
      await page.waitForFunction(() => document.querySelector('gyral-name-form').shadowRoot.querySelector('#private-note').disabled === false);
      assert.equal(await page.getByLabel('Your name', { exact: true }).inputValue(), 'Typed before hydration');
      await page.getByRole('button', { name: 'Increment', exact: true }).click();
      await page.waitForFunction(() => document.querySelector('gyral-counter').shadowRoot.querySelector('output').textContent === '1');
      await page.getByRole('button', { name: 'Add visit' }).first().click();
      await page.waitForFunction(() => Array.from(document.querySelectorAll('gyral-store-card')).every(host => host.shadowRoot.querySelector('output').textContent === '8'));

      await page.getByLabel('Your name', { exact: true }).fill('x');
      await page.getByLabel('Synthetic private note').fill('synthetic-never-echo');
      await page.getByRole('button', { name: 'Save name' }).click();
      await page.getByRole('alert').waitFor();
      assert.match(await page.getByRole('alert').textContent(), /2–64 characters/);
      assert.equal(await page.getByLabel('Your name', { exact: true }).inputValue(), 'x');
      assert.equal(await page.getByLabel('Your name', { exact: true }).getAttribute('aria-invalid'), 'true');
      const state = await page.evaluate(() => JSON.stringify(document.querySelector('gyral-name-form').state));
      assert.ok(!state.includes('synthetic-never-echo'));
      assert.ok(!state.includes('private_note'));
      assert.equal(page.url(), `${url}/?name=Basketball`);

      await page.getByLabel('Your name', { exact: true }).fill('Hydrated visitor');
      await page.getByRole('button', { name: 'Save name' }).click();
      await page.locator('gyral-name-form').getByRole('status').waitFor();
      assert.equal(await page.locator('gyral-name-form').getByRole('status').textContent(), 'Saved for Hydrated visitor.');
      assert.deepEqual(await page.locator('gyral-store-card strong').allTextContents(), ['Hydrated visitor', 'Hydrated visitor']);
      assert.equal(await page.evaluate(() => {
        const current = [
          document.querySelector('gyral-counter').shadowRoot.querySelector('output'),
          ...Array.from(document.querySelectorAll('gyral-store-card'), host => host.shadowRoot.querySelector('output')),
          document.querySelector('gyral-name-form').shadowRoot.querySelector('input[name="name"]'),
          document.querySelector('gyral-time-card').shadowRoot.querySelector('time'),
        ];
        return current.every((node, index) => node === globalThis.__originalNodes[index]);
      }), true);
      assert.deepEqual(await page.evaluate(() => globalThis.__cspViolations), []);
      assert.deepEqual(failures, []);
      await page.getByRole('button', { name: 'Increment', exact: true }).focus();
      await page.keyboard.press('Tab');
      assert.equal(await page.getByRole('button', { name: 'Add visit' }).first().evaluate(el =>
        el.getRootNode().activeElement === el && el.matches(':focus-visible') && getComputedStyle(el).outlineWidth === '3px'), true);
      await page.keyboard.press('Enter');
      await page.waitForFunction(() => Array.from(document.querySelectorAll('gyral-store-card')).every(host => host.shadowRoot.querySelector('output').textContent === '9'));
      await page.screenshot({ path: `.local/compat-${name.toLowerCase()}.png`, fullPage: true });
      await page.emulateMedia({ colorScheme: 'dark', reducedMotion: 'reduce' });
      const dark = await page.locator('body').evaluate(el => getComputedStyle(el).backgroundColor);
      await page.emulateMedia({ colorScheme: 'light' });
      assert.notEqual(await page.locator('body').evaluate(el => getComputedStyle(el).backgroundColor), dark);
      await page.setViewportSize({ width: 320, height: 700 });
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), true);
      await page.screenshot({ path: `.local/compat-${name.toLowerCase()}-mobile.png`, fullPage: true });
      await page.evaluate(() => { document.documentElement.style.fontSize = '200%'; });
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), true);
      await page.evaluate(() => { document.documentElement.style.fontSize = ''; });

      // Enforcing CSP must block an actual attempted inline script and style attribute.
      await page.evaluate(() => {
        const script = document.createElement('script');
        script.textContent = 'globalThis.__inlineAttack = true'; document.head.append(script);
        document.body.setAttribute('style', 'color: rgb(1, 2, 3)');
      });
      await page.waitForFunction(() => globalThis.__cspViolations.length >= 2);
      assert.equal(await page.evaluate(() => globalThis.__inlineAttack), undefined);
      assert.notEqual(await page.locator('body').evaluate(el => getComputedStyle(el).color), 'rgb(1, 2, 3)');
      assert.ok((await page.evaluate(() => globalThis.__cspViolations)).some(value => value.startsWith('script-src')));
      assert.ok((await page.evaluate(() => globalThis.__cspViolations)).some(value => value.startsWith('style-src')));
      await context.close();

      const escaping = await browser.newPage();
      const payload = '</script><img src=x onerror="globalThis.__attack=1"> 🏀';
      await escaping.goto(`${url}/?name=${encodeURIComponent(payload)}`);
      assert.equal(await escaping.locator('h1').textContent(), `Hello, ${payload}`);
      assert.equal(await escaping.locator('img').count(), 0);
      assert.equal(await escaping.getByLabel('Your name', { exact: true }).inputValue(), payload);
      assert.equal(await escaping.evaluate(() => globalThis.__attack), undefined);
      await escaping.close();

      const tampered = await browser.newContext();
      const damaged = await tampered.newPage();
      await damaged.route(`${url}${manifest.client.path}`, route => route.fulfill({ contentType: 'text/javascript', body: 'globalThis.__tampered = true;' }));
      await damaged.goto(`${url}/?name=Integrity`);
      assert.equal(await damaged.locator('h1').textContent(), 'Hello, Integrity');
      assert.equal(await damaged.evaluate(() => globalThis.__tampered), undefined);
      assert.equal(await damaged.evaluate(() => customElements.get('gyral-counter')), undefined);
      assert.equal(await damaged.locator('gyral-counter output').textContent(), '0');
      await tampered.close();
      console.log(`${name}: no-JS form, seeded stores, DOM adoption, typed-before-hydration input, form rejection/redaction, shared updates, timezone, enforcing CSP, SRI tamper rejection, keyboard controls, themes, narrow layout, and 200% text scaling passed.`);
    } finally { await browser.close(); }
  }
} finally {
  child.kill('SIGINT');
  const force = setTimeout(() => child.kill('SIGKILL'), 10_000);
  force.unref();
  await exited;
  clearTimeout(force);
}
