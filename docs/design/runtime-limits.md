# Runtime limits and lifecycle

The host supplies a trusted app bundle and a validated `RenderLimits` policy.
The renderer applies that policy to each request, including bundle evaluation.
The host still owns authorization, DTO construction, and any work before rendering.

## Policy

| Setting | Development default | Accepted range |
| --- | --- | --- |
| End-to-end render deadline | 2 seconds | 1 millisecond–60 seconds, inclusive |
| Managed V8 heap budget | 64 MiB | 16 MiB–1 GiB, inclusive |
| Shutdown deadline | 2 seconds | 1 millisecond–60 seconds, inclusive |

`Renderer::start(bundle)` uses these defaults. For another policy, call
`RenderLimits::try_new(render_deadline, heap_bytes, shutdown_deadline)` and pass
the validated result to `Renderer::with_limits(bundle, limits)`. Startup error
types should preserve `LimitsError` and `WorkerError`. The defaults have not been
sized against a real application's load; applications must measure their needs.

Poll rendering and shutdown inside a Tokio runtime with its time driver enabled
(`enable_time` or `enable_all`). Missing timer capability produces
`HostRuntimeUnavailable`; rendering rejects it before queue admission. An invalid
shutdown context still invokes the owner's Drop fallback to stop the worker.
Tokio reports this capability failure by panicking in its timer constructor;
the renderer catches that dependency panic, though the host's panic hook may log it.

The render deadline starts when the future is first polled. It includes queue
time, isolate creation, bundle evaluation, the render call, promise resolution,
and output handling. The caller has a Tokio deadline; a dedicated watchdog
uses a monotonic OS clock to call V8's thread-safe termination API. Returning a
Rust timeout alone would leave synchronous JavaScript running, so both are needed.

V8 receives the managed-heap budget in its isolate creation parameters. A
near-heap-limit callback marks the request as failed and terminates execution.
The callback permits 32 MiB of temporary extra headroom per invocation so V8 can
deliver termination. This is a soft managed-heap policy: V8 rounding, external
ArrayBuffer storage, native allocations, Rust memory, and process RSS are outside
the budget. A fatal native or operating-system OOM can still kill the host.

The renderer rejects a V8 string above the output limit in UTF-16 code units
before copying it into Rust. Smaller strings then pass through the existing
2 MiB UTF-8 byte check. This prevents a compact JavaScript rope from causing an
enormous Rust allocation simply to reject oversized HTML.

## Request interruption

One renderer owns one execution thread and one watchdog thread. The watchdog
tracks only the current isolate and its request signal. Its short critical
sections never call V8. Isolate handles remain safe if termination races teardown;
each request still gets a fresh isolate.

Request signals have three states: pending, finished, or interrupted with a
reason. The first interruption reason wins, and successful completion is terminal.
This prevents a late timeout or drop from changing a completed request's outcome.

Dropping the render future marks queued work as cancelled or interrupts active
JavaScript. A closed reply channel is skipped before creating another isolate.
The library observes future cancellation, not browser sockets; the host controls
when a disconnected HTTP request's future is dropped.

Rust unwinds during a render become `RenderError::WorkerPanicked`. The isolate is
disposed, and the worker accepts another request. This boundary cannot catch a
process abort, fatal V8 error, or memory exhaustion. Panic payloads are absent
from the returned error; the host's process-wide panic hook may still log them.

## Shutdown

`worker.shutdown()` first closes admission, then drains accepted messages within
the shutdown budget. It joins both threads on success. Queue admission and the
transition to draining share one synchronization boundary, so new work cannot
enter after draining begins.

On shutdown expiry, the watchdog interrupts active JavaScript and the worker
discards queued requests with `Unavailable`. The caller receives
`WorkerError::ShutdownTimedOut` if it cannot confirm completed cleanup within
the budget. That error does not certify that native cleanup has finished.
Waiting for thread completion does not create an unabortable Tokio blocking task.

Dropping the lifecycle owner, including cancelling its shutdown future, closes
admission and requests immediate interruption. Drop performs synchronous signalling;
it does not wait or spawn asynchronous cleanup. Threads can finish after the
caller has returned. A native engine stall cannot be forcibly repaired by safely
destroying a Rust thread; hard failure containment requires a separate process.

## HTTP behavior and evidence

The Axum adapter maps overload, unavailability, deadline expiry, and cancellation
to an opaque 503. Heap-budget failures, Rust render unwinds, and other rendering
defects become an opaque 500. Both retain no-store and nosniff policy. Hosts can
use the typed rendering capability to choose their own response policy.

Native regression tests prove recovery after synchronous loops, microtask loops,
heap pressure, cancellation, and a simulated upstream Rust unwind. They also
cover bundle initialization, queue deadlines, owner drop, shutdown cancellation,
and shutdown expiry. Linux additionally checks peak resident memory after
rejecting a 500 MiB JavaScript rope. See [verification](../verification.md) for
the checks actually run and [security](../security.md) for remaining boundaries.
