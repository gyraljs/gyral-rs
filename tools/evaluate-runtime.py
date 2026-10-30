#!/usr/bin/env python3
"""Compare test-only startup strategies in fresh, bounded Linux processes."""
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
TEST = 'worker::evaluation::measure_runtime_configuration'


def main():
    if os.uname().sysname != 'Linux':
        raise ValueError('The sampled /proc RSS comparison requires Linux')
    build = subprocess.run(['cargo', 'test', '--release', '--locked', '-p', 'gyral-renderer', '--lib', '--no-run', '--message-format=json'], cwd=ROOT, text=True, stdout=subprocess.PIPE, check=True)
    artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith('{')]
    binary = next(item['executable'] for item in artifacts if item.get('reason') == 'compiler-artifact' and item.get('executable') and item['target']['name'] == 'gyral_renderer')
    output = ROOT / '.local/runtime-evaluation'
    output.mkdir(parents=True, exist_ok=True)
    reports = []
    configurations = [(mode, workers) for workers in (1, 2, 4) for mode in ('fresh', 'snapshot')]
    for repetition in range(3):
        # Reverse ordering on alternate rounds to expose order/host effects.
        order = configurations if repetition % 2 == 0 else list(reversed(configurations))
        for mode, workers in order:
            load_before = os.getloadavg()
            environment = {**os.environ, 'GYRAL_EVAL_MODE': mode, 'GYRAL_EVAL_WORKERS': str(workers)}
            with tempfile.TemporaryFile(mode='w+') as log:
                process = subprocess.Popen([binary, TEST, '--exact', '--ignored', '--nocapture'], cwd=ROOT, env=environment, stdout=log, stderr=subprocess.STDOUT)
                rss = []
                deadline = time.monotonic() + 40
                try:
                    while process.poll() is None:
                        if time.monotonic() >= deadline:
                            raise TimeoutError('Evaluation exceeded its 40-second process budget')
                        try:
                            status = Path(f'/proc/{process.pid}/status').read_text()
                            resident = next(line for line in status.splitlines() if line.startswith('VmRSS:'))
                            rss.append(int(resident.split()[1]) * 1024)
                        except (FileNotFoundError, ProcessLookupError, StopIteration):
                            pass  # Process can exit between poll and the proc read.
                        time.sleep(0.02)
                finally:
                    if process.poll() is None:
                        process.kill()
                    process.wait(timeout=5)
                log.seek(0)
                transcript = log.read()
                if process.returncode != 0:
                    raise ValueError(f'{mode}/{workers} evaluation failed:\n{transcript[-8000:]}')
            report = json.loads(next(line.removeprefix('GYRAL_EVALUATION=') for line in transcript.splitlines() if line.startswith('GYRAL_EVALUATION=')))
            if not rss:
                raise ValueError('No resident-memory samples collected')
            report.update(repetition=repetition + 1, sampled_peak_rss_mib=max(rss) / 1024 / 1024, rss_samples=len(rss), responses_per_second=report['samples'] * 1_000_000 / report['elapsed_us'], host_load_average_start=load_before, host_load_average_end=os.getloadavg())
            (output / f'round-{repetition + 1}-{mode}-{workers}.json').write_text(json.dumps(report, indent=2) + '\n')
            reports.append(report)
            print(f'Round {repetition + 1}: {mode}, {workers} worker(s): {report["responses_per_second"]:.2f} renders/s; p95 {report["p95_us"] / 1000:.2f} ms; sampled RSS {report["sampled_peak_rss_mib"]:.2f} MiB', flush=True)
    summary = []
    for mode, workers in configurations:
        group = [report for report in reports if report['mode'] == mode and report['workers'] == workers]
        fields = ['responses_per_second', 'p50_us', 'p95_us', 'p99_us', 'sequential_p50_us', 'sequential_p95_us', 'preparation_us', 'first_render_us', 'mean_create_us', 'mean_initialize_us', 'mean_render_us', 'mean_dispose_us', 'sampled_peak_rss_mib']
        summary.append({'mode': mode, 'workers': workers, 'median_of_three_runs': {field: statistics.median(report[field] for report in group) for field in fields}, 'snapshot_bytes': group[0]['snapshot_bytes']})
    evidence = {
        'scope': 'Synthetic Gyral fixture, release test binary, fresh isolate per request, one closed-loop client per worker, three separate five-second windows per configuration. No HTTP, queued load, application SLO, isolate reuse, leak proof, or capacity claim.',
        'gyral': '0.3.9', 'deno_core': '0.412.0', 'v8': '150.4.0',
        'rustc': subprocess.check_output(['rustc', '--version'], cwd=ROOT, text=True).strip(),
        'cpu_count': os.cpu_count(), 'architecture': os.uname().machine,
        'manifest_sha256': hashlib.sha256((ROOT / 'examples/axum-app/dist/manifest.json').read_bytes()).hexdigest(),
        'lock_sha256': hashlib.sha256((ROOT / 'pnpm-lock.yaml').read_bytes()).hexdigest(),
        'summary': summary, 'runs': reports,
    }
    (output / 'report.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print('Saved .local/runtime-evaluation/report.json; production defaults are unchanged.')


if __name__ == '__main__':
    main()
