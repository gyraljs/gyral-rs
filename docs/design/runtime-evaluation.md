# Snapshot and worker evaluation

The renderer still evaluates a trusted application bundle in a fresh isolate for
every request. This experiment asks whether repeated initialization costs enough
to justify a different startup path. It supplies fixture evidence for a later
application decision; no complete application's latency or throughput target has
been selected.

## Candidate and boundaries

The test binary can prepare the pinned Gyral SSR bundle with
`JsRuntimeForSnapshot`, then restore a new isolate from that prepared state for
each render. The pinned runtime's [upstream snapshot tests](https://github.com/denoland/deno/blob/0c071246a412575e07423263404a5d13e7ed6aa2/libs/core/runtime/tests/snapshot.rs)
provide the creation/restoration pattern. Preparation evaluates only the reviewed
bundle. It never calls `gyralRender` or receives request DTOs. The actual fixture
creates provider stores inside the render function rather than during bundle
initialization.

Snapshot input and phase recording compile only under `cfg(test)`. There is no
public snapshot loader, external snapshot file, new Cargo feature, or change to
the HTTP example's startup selection. Both candidates run through the same
bounded worker, watchdog, heap callback, admission, cancellation, and shutdown
paths. All V8 handles leave scope before isolate disposal is measured.

`deno_core` accepts snapshot bytes through a static slice. The prototype keeps
one process-lifetime allocation per fixture test or benchmark process, following
the upstream test pattern. That deliberate test-only allocation is not a service
ownership design. A production implementation would need immutable artifact
ownership, startup verification, bounded preparation, and compatibility tied to
the exact bundle, runtime, V8 build, and target. A new framework/runtime/native
release requires a new snapshot and verification evidence.

The worker comparison starts one, two, or four independent workers. Each render
still creates and destroys an isolate. This evaluates concurrency, without
reusing initialized isolates. Each worker owns a watchdog and sixteen queued
positions: the aggregate queue grows to 32 or 64 in the larger configurations.
A production pool would need one deliberate admission budget, scheduling, host
readiness, and coordinated drain rather than copying these experiment handles.

## Correctness evidence

Ordinary nextest runs execute three snapshot checks before performance testing:

- Real Gyral output matches the fresh-bundle path byte for byte, including
  hydration seeds, provider stores, styles, and request-specific values. A wrapper
  deliberately retains module-local DTOs and poisoned globals, so a reused state
  would fail the next render. A rejected promise queues a poisoning microtask;
  the next request must still render cleanly. A real Gyral view error is also
  followed by a healthy render.
- Dropping an active render future interrupts a loop. Deadline and managed-heap
  failures use the existing typed error categories; healthy subsequent renders
  contain only their own request data.
- Shutdown interrupts an active loop, discards queued work, rejects new work, and
  preserves the existing bounded shutdown result.

These checks cover the reviewed fixture and wrapper. They do not prove that
every application's top-level initialization is appropriate for a snapshot, or
that a reused isolate could be reset safely. No isolate-reuse candidate exists.

## Measurement method

Run `make evaluate-runtime` on the supported Linux x86_64 host after installing
the frozen JavaScript dependencies. It builds the pinned published Gyral fixture,
checks the snapshot behavior, and compiles a release library test binary. The
driver launches eighteen separate processes: three repetitions of fresh and
snapshot startup with one, two, or four workers. Configuration order reverses
in the middle repetition to expose ordering and host effects.

Each process records preparation time, worker startup, first rendering, ten
warmup requests, and fifty sequential samples. It then serves one closed-loop
client per worker for a five-second window, completing the last accepted requests before
timing ends. Every render checks its own synthetic name, store count, and absence
of Gyral errors. All worker owners are explicitly shut down and joined. The
driver kills a stalled test process after forty seconds and waits for its exit.

One active client per worker compares service cost without an intentional waiting
queue. Fresh and snapshot modes use the same client count at each worker count;
comparisons across worker counts also increase offered concurrency. They are
worker-scaling observations rather than a fixed-client load comparison. The
separate operating load tool remains the queueing/overload fixture.

Latency includes admission, queueing, encoding, rendering, output checks and
conversion, and disposal; it excludes HTTP and startup artifact I/O. Phase means
measure isolate creation/restoration, registration plus bundle evaluation,
ABI/request encoding plus Gyral rendering/output conversion, and disposal. Phase
recording uses the same test-only recorder in both modes and counts successful
renders. The recorder resets before the concurrent window. Phase means need not
add up to client latency because the latter includes queueing and host checks.

The driver samples each process's `/proc` resident memory every 20 ms throughout
its lifetime, including snapshot preparation and sequential warmup. Reported
peaks are sampled observations, including the native engine and retained
snapshot, rather than hard limits. Three-run summaries use the median of each
run's statistic, not a percentile over merged samples. Short fixture windows
cannot establish application capacity, tail behavior under open-loop load,
long-duration leaks, or a production memory budget.

Raw reports, manifest/lock hashes, compiler details, and summaries stay in ignored
`.local/runtime-evaluation/report.json`; individual processes have separate JSON
reports. The durable measurements and decision belong here and in
[verification](../verification.md).

## Observed results and decision

The completed October 11, 2026 run, on the Gyral 0.3.1 fixture, produced 6,161 successful concurrent-window
renders across eighteen processes. No sampled render failed or exceeded the
unchanged two-second deadline. The twelve-CPU shared host remained busy: the
recorded one-minute load averages ranged from 20.69 to 24.96. This is local
comparative evidence, not a dedicated-host capacity measurement.

The table reports medians of the three run-level statistics. Each worker has one
active client; peak RSS includes preparation and sequential warmup.

| Startup | Workers / clients | Renders/second | p50 / p95 / p99 (ms) | Sampled peak RSS (MiB) |
| --- | --- | --- | --- | --- |
| Bundle evaluation | 1 / 1 | 23.72 | 38.27 / 60.58 / 73.58 | 38.21 |
| Snapshot restoration | 1 / 1 | 47.49 | 19.10 / 35.06 / 52.96 | 39.76 |
| Bundle evaluation | 2 / 2 | 43.69 | 39.38 / 71.28 / 85.15 | 44.18 |
| Snapshot restoration | 2 / 2 | 82.90 | 21.56 / 38.73 / 47.86 | 44.68 |
| Bundle evaluation | 4 / 4 | 68.35 | 54.80 / 84.68 / 105.00 | 55.42 |
| Snapshot restoration | 4 / 4 | 135.66 | 28.70 / 47.35 / 60.12 | 52.29 |

For one worker, fifty warmed sequential samples per process gave median
run-level p50 values of 35.09 ms for evaluation and 18.80 ms for restoration.
The concurrent-window phase means explain the difference:

| Phase mean, median across runs | Bundle evaluation (ms) | Snapshot restoration (ms) |
| --- | --- | --- |
| Isolate creation/restoration | 13.35 | 4.86 |
| Registration and bundle initialization | 10.57 | 0.24 |
| ABI, encoding, rendering, and output conversion | 16.64 | 14.97 |
| Disposal | 0.77 | 0.97 |

Snapshot preparation took a median 74.90 ms in the one-worker configuration.
First-render medians were 118.25 ms for evaluation and 66.48 ms for restoration,
excluding preparation. The process-lifetime blob was approximately 0.99 MiB;
individual snapshots ranged from 1,037,952 to 1,037,960 bytes. Preparation is
additional startup work, and these blobs have no reproducible-byte claim.

An earlier attempt used four clients per configuration and failed when one
snapshot configuration exceeded its two-second deadline under host load above
32. That incomplete attempt is retained locally as
`.local/runtime-evaluation-contended-attempt.log` and excluded from this table.
The retry changed offered concurrency, not either candidate's resource limits.
The completed result does not establish that the original queued workload meets
an application budget.

**Decision:** pursue an opt-in snapshot application trial, while keeping the
fresh-bundle public default. Restoration removes enough repeated work to merit
that trial in this fixture. It preserves fresh-isolate isolation, so initialized
isolate reuse has no demonstrated need here. Two/four-worker concurrency raises
throughput and native resident memory; it needs a shared admission and lifecycle
design before becoming a pool API.

The next application trial must select a reviewed complete bundle, a deployment
host, explicit latency/throughput and memory targets, and a verified immutable
snapshot artifact lifecycle. It must rerun isolation, startup, readiness, drain,
browser hydration, and representative load checks against the unchanged fallback.
This spike does not promote a snapshot or pool into a shipped startup option.
