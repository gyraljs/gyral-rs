// A bounded synthetic regression profile, not an application SLO or capacity claim.
import assert from 'node:assert/strict';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import { cpus, totalmem } from 'node:os';
import { performance } from 'node:perf_hooks';
import { startExample, request } from './example-process.mjs';

const budgets = { warmupSeconds: 10, sustainedSeconds: 60, concurrency: 4, minimumRps: 10, p95Ms: 500, p99Ms: 1_000, maximumRssMiB: 512, maximumRssGrowthMiB: 64, queueCapacity: 16 };
const process = await startExample({ binary: 'target/release/gyral-axum-example' });
const samples = [];
const statuses = {};
const memory = [];
let queueMaximum = 0;
let sequence = 0;
let sampling = true;
try {
  const sample = async () => {
    while (sampling) {
      const status = await readFile(`/proc/${process.child.pid}/status`, 'utf8');
      memory.push({ at: performance.now(), rssMiB: Number(/^VmRSS:\s+(\d+)\s+kB/m.exec(status)?.[1]) / 1024 });
      const scrape = await (await request(process.origin, '/metrics')).text();
      const queued = Number(/^gyral_renderer_queue_depth (\d+)/m.exec(scrape)?.[1]);
      const capacity = Number(/^gyral_renderer_queue_capacity (\d+)/m.exec(scrape)?.[1]);
      assert.equal(capacity, budgets.queueCapacity);
      assert.ok(queued <= capacity);
      queueMaximum = Math.max(queueMaximum, queued);
      await delay(250);
    }
  };
  const samplingTask = sample();
  const run = async (seconds, record) => {
    const until = performance.now() + seconds * 1_000;
    const worker = async () => {
      while (performance.now() < until) {
        const name = `Load fixture ${sequence++}`;
        const at = performance.now();
        const response = await request(process.origin, `/?name=${encodeURIComponent(name)}`);
        const text = await response.text();
        assert.equal(response.status, 200, 'The steady bounded profile must render every request');
        assert.ok(text.includes(`Hello, ${name}`), 'Each response must retain its own DTO');
        if (record) {
          samples.push(performance.now() - at);
          statuses[response.status] = (statuses[response.status] ?? 0) + 1;
        }
      }
    };
    await Promise.all(Array.from({ length: budgets.concurrency }, worker));
  };
  await run(budgets.warmupSeconds, false);
  const started = performance.now();
  await run(budgets.sustainedSeconds, true);
  const elapsedSeconds = (performance.now() - started) / 1_000;
  sampling = false; await samplingTask;
  const steadyMemory = memory.filter(sample => sample.at >= started);
  const firstWindow = steadyMemory.slice(0, 20).map(sample => sample.rssMiB);
  const lastWindow = steadyMemory.slice(-20).map(sample => sample.rssMiB);
  const sorted = [...samples].sort((a, b) => a - b);
  const percentile = p => sorted[Math.ceil(p * sorted.length) - 1];
  const summary = {
    scope: 'Single-process Linux x86_64 release fixture, fresh isolate per render; four closed-loop clients. Synthetic regression evidence, not a deployed application SLO.',
    budgets, host: { cpu: cpus()[0].model, logicalCpus: cpus().length, memoryMiB: totalmem() / 1024 / 1024 },
    requests: samples.length, statuses, elapsedSeconds, rps: samples.length / elapsedSeconds,
    p50Ms: percentile(0.5), p95Ms: percentile(0.95), p99Ms: percentile(0.99),
    maximumRssMiB: Math.max(...steadyMemory.map(sample => sample.rssMiB)),
    rssGrowthMiB: Math.max(...lastWindow) - Math.max(...firstWindow), queueMaximum,
  };
  await mkdir('.local', { recursive: true });
  await writeFile('.local/load-evidence.json', JSON.stringify({ ...summary, rssSamples: steadyMemory }, null, 2));
  assert.ok(summary.rps >= budgets.minimumRps, 'Throughput regression budget');
  assert.ok(summary.p95Ms <= budgets.p95Ms && summary.p99Ms <= budgets.p99Ms, 'Latency regression budgets');
  assert.ok(summary.maximumRssMiB <= budgets.maximumRssMiB, 'Process RSS regression budget');
  assert.ok(summary.rssGrowthMiB <= budgets.maximumRssGrowthMiB, 'Steady-state RSS growth budget');

  // Beyond capacity, shed immediately; the bounded queue must then recover.
  const burst = await Promise.all(Array.from({ length: 64 }, (_, i) => request(process.origin, `/?name=Burst%20${i}`).then(async response => ({ status: response.status, text: await response.text() }))));
  assert.ok(burst.some(result => result.status === 503), 'Overload must shed excess renders');
  assert.ok(burst.every(result => result.status === 200 || result.status === 503));
  const after = await request(process.origin, '/?name=After%20burst');
  assert.equal(after.status, 200); await after.text();
  const scrape = await (await request(process.origin, '/metrics')).text();
  assert.match(scrape, /gyral_render_requests_total\{outcome="busy"\} [1-9]/);
  const output = await process.stop();
  assert.ok(output.includes('renderer threads joined'));
  console.log(JSON.stringify(summary, null, 2));
  console.log('Load: sustained bounded fixture, RSS/latency/queue budgets, overload shedding, and recovery passed.');
} finally { sampling = false; await process.cleanup(); }
