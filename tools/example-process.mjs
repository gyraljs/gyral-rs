// Isolated SSR process helpers shared by operating and load evidence.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';

export async function startExample({ binary = 'target/debug/gyral-axum-example', env = {} } = {}) {
  const child = spawn(binary, [], { env: { ...process.env, GYRAL_EXAMPLE_BIND: '127.0.0.1:0', ...env }, stdio: ['ignore', 'pipe', 'pipe'] });
  let output = '';
  child.stdout.on('data', bytes => { output += bytes; });
  child.stderr.on('data', bytes => { output += bytes; });
  const exited = once(child, 'exit');
  let origin;
  for (let attempt = 0; attempt < 200; attempt += 1) {
    for (const line of output.split('\n')) {
      try { const log = JSON.parse(line); if (log.fields?.message === 'Gyral example listening') origin = `http://${log.fields.address}`; } catch { /* An incomplete line may still arrive. */ }
    }
    if (origin) break;
    assert.equal(child.exitCode, null, `Example failed before binding: ${output}`);
    await delay(25);
  }
  assert.ok(origin, `Example did not bind: ${output}`);
  return {
    child, origin, output: () => output,
    async stop(signal = 'SIGTERM') {
      child.kill(signal);
      const result = await Promise.race([exited, delay(12_000).then(() => { throw new Error('Example failed to drain within 12 seconds'); })]);
      assert.deepEqual(result, [0, null], output);
      return output;
    },
    async cleanup() { if (child.exitCode === null && child.signalCode === null) { child.kill('SIGKILL'); await exited; } },
  };
}

export const request = (origin, path, options = {}) => fetch(`${origin}${path}`, { signal: AbortSignal.timeout(6_000), ...options });
