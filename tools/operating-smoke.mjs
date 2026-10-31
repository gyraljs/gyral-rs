import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { cp, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { startExample, request } from './example-process.mjs';

const execute = promisify(execFile);
const directory = await mkdtemp(join(tmpdir(), 'gyral-operating-'));
const payloads = [];
const collector = createServer((req, res) => {
  const chunks = [];
  req.on('data', chunk => chunks.push(chunk));
  req.on('end', () => {
    assert.equal(req.url, '/v1/traces');
    assert.equal(req.headers['content-type'], 'application/x-protobuf');
    payloads.push(Buffer.concat(chunks));
    res.writeHead(200, { 'content-type': 'application/x-protobuf' }); res.end();
  });
});
collector.listen(0, '127.0.0.1');
await once(collector, 'listening');
const endpoint = `http://127.0.0.1:${collector.address().port}/v1/traces`;
let process;
try {
  const fixture = join(directory, 'dist');
  await cp('examples/axum-app/dist', fixture, { recursive: true });
  const original = await readFile(join(fixture, 'server.js'), 'utf8');
  const extension = `\nconst originalRender = globalThis.gyralRender;
    globalThis.gyralRender = request => {
      if (request.page === 'broken') throw new Error('PRIVATE_DIAGNOSTIC_83');
      if (request.props.name === 'Operating slow') { const end = Date.now() + 600; while (Date.now() < end) {} }
      return originalRender(request);
    };`;
  const source = original + extension;
  const manifest = JSON.parse(await readFile(join(fixture, 'manifest.json'), 'utf8'));
  await writeFile(join(fixture, 'server.js'), source);
  await writeFile(join(fixture, 'manifest.json'), JSON.stringify({ ...manifest, server_sha256: createHash('sha256').update(source).digest('hex') }));
  for (const signal of ['SIGTERM', 'SIGINT']) {
    process = await startExample({ env: { GYRAL_EXAMPLE_DIST: fixture, GYRAL_OTLP_ENDPOINT: endpoint } });
    const { origin, child } = process;
    for (const path of ['/health', '/livez', '/readyz']) {
      const response = await request(origin, path);
      assert.equal(response.status, 200);
      assert.equal(response.headers.get('cache-control'), 'no-store');
      await response.text();
    }
    const traceID = '18273645182736451827364518273645';
    const secret = 'PRIVATE_QUERY_49';
    const page = await request(origin, `/?name=${secret}`, { headers: { 'x-request-id': 'FORGED_IDENTIFIER', traceparent: `00-${traceID}-1234567812345678-01`, cookie: 'PRIVATE_COOKIE_17' } });
    assert.equal(page.status, 200);
    const requestID = page.headers.get('x-request-id');
    assert.match(requestID, /^[a-f0-9]{32}-[a-f0-9]{16}$/);
    assert.notEqual(requestID, 'FORGED_IDENTIFIER');
    await page.text();
    const failure = await request(origin, '/failure');
    assert.equal(failure.status, 500);
    assert.equal(await failure.text(), 'The page could not be rendered.');
    const unknown = await request(origin, '/missing?token=PRIVATE_QUERY_49');
    assert.equal(unknown.status, 404); await unknown.text();
    const scrape = await request(origin, '/metrics');
    assert.equal(scrape.status, 200);
    const metrics = await scrape.text();
    assert.match(metrics, /gyral_render_requests_total\{outcome="javascript"\} 1/);
    assert.match(metrics, /gyral_http_duration_seconds_bucket\{/);
    assert.match(metrics, /gyral_renderer_queue_capacity 16/);
    assert.match(metrics, /gyral_renderer_ready 1/);
    assert.ok(!metrics.includes(secret) && !metrics.includes(requestID));

    // Wait for an isolate to start, then signal while the accepted page is rendering.
    const pending = request(origin, '/?name=Operating%20slow');
    let active = false;
    for (let attempt = 0; attempt < 100; attempt += 1) {
      const metrics = await (await request(origin, '/metrics')).text();
      if (/gyral_renderer_active 1\b/.test(metrics)) { active = true; break; }
      await delay(5);
    }
    assert.equal(active, true, 'The request must be active before signalling');
    child.kill(signal);
    let draining = false;
    for (let attempt = 0; attempt < 50; attempt += 1) {
      const response = await request(origin, '/readyz');
      await response.text();
      if (response.status === 503) { draining = true; break; }
      await delay(5);
    }
    assert.equal(draining, true, 'Readiness must fail before HTTP stops');
    const shed = await request(origin, '/?name=After%20signal');
    assert.equal(shed.status, 503); await shed.text();
    const drained = await pending;
    assert.equal(drained.status, 200);
    assert.ok((await drained.text()).includes('Operating slow'));
    const output = await process.stop(signal);
    assert.ok(output.includes('renderer threads joined'));
    assert.ok(output.includes(requestID));
    assert.ok(output.includes('"error.category":"javascript"'));
    for (const privateText of [secret, 'PRIVATE_COOKIE_17', 'PRIVATE_DIAGNOSTIC_83', 'FORGED_IDENTIFIER']) assert.ok(!output.includes(privateText), `Logs must omit ${privateText}`);
    const exported = Buffer.concat(payloads);
    assert.ok(exported.includes(Buffer.from(traceID, 'hex')), 'OTLP must retain the W3C trace parent');
    assert.ok(exported.includes(Buffer.from(requestID)), 'OTLP must correlate the opaque request ID');
    for (const privateText of [secret, 'PRIVATE_COOKIE_17', 'PRIVATE_DIAGNOSTIC_83']) assert.ok(!exported.includes(Buffer.from(privateText)), 'OTLP must omit private data');
    process = undefined;
  }
  // A disabled exporter is supported normal operation, without warning spam.
  process = await startExample();
  const localPage = await request(process.origin, '/?name=Local%20logging');
  assert.equal(localPage.status, 200); await localPage.text();
  const localOutput = await process.stop();
  assert.ok(!localOutput.includes('trace parent not attached'));
  const logs = localOutput.split('\n').flatMap(line => { try { return [JSON.parse(line)]; } catch { return []; } });
  const verified = logs.find(log => log.fields?.message === 'Gyral bundle verified');
  assert.equal(verified?.fields.gyral, manifest.gyral, 'Startup must log the bundle\'s Gyral release');
  assert.equal(verified?.fields.ssr_protocol, manifest.abi, 'Startup must log the bundle\'s SSR protocol');
  process = undefined;
  // Verified bytes are necessary but insufficient: an executable probe must pass.
  for (const [label, body] of [['throw', "throw new Error('PRIVATE_STARTUP_71')"], ['deadline', 'while(true){}']]) {
    const source = `globalThis.gyralBundleVersion=1;globalThis.gyralRender=()=>{${body}};`;
    await writeFile(join(fixture, 'server.js'), source);
    await writeFile(join(fixture, 'manifest.json'), JSON.stringify({ ...manifest, server_sha256: createHash('sha256').update(source).digest('hex') }));
    await assert.rejects(execute('target/debug/gyral-axum-example', [], { env: { ...globalThis.process.env, GYRAL_EXAMPLE_BIND: '127.0.0.1:0', GYRAL_EXAMPLE_DIST: fixture }, timeout: 6_000 }), error => {
      assert.equal(error.code, 1, `${label} probe must fail before binding`);
      assert.ok(!error.stdout.includes('Gyral example listening'));
      assert.ok(!error.stdout.includes('PRIVATE_STARTUP_71') && !error.stderr.includes('PRIVATE_STARTUP_71'));
      return true;
    });
  }
  console.log('Operating: logged Gyral release and protocol, real readiness probes, finite metrics, generated IDs, redacted JSON/OTLP with W3C correlation, and active-request SIGTERM/SIGINT drain + worker joins passed.');
} finally {
  if (process) await process.cleanup();
  collector.closeAllConnections();
  await new Promise(resolve => collector.close(resolve));
  await rm(directory, { recursive: true, force: true });
}
