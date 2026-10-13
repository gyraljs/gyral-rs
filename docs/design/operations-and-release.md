# Operating and releasing the packages

The host owns service operations. The renderer reports bounded error categories
and worker state; it never installs a global subscriber, exporter, or HTTP endpoint.
The SSR example shows how an application can connect those signals to readiness,
request diagnostics, metrics, and shutdown. It still renders synthetic data and
provides no application authentication.

## Readiness and liveness

`/livez` and its compatibility alias `/health` report HTTP reachability without
calling JavaScript. `/readyz` returns 200 only when the host is accepting traffic,
the renderer is running, and a successful bundle probe is less than 15 seconds old.
All three responses use no-store and nosniff headers.

Before binding HTTP, the example renders `home` with a synthetic `Readiness` DTO.
A thrown error or deadline failure prevents startup even when artifact hashes
match. A supervised task repeats that probe every five seconds, skipping missed
ticks. Failure withdraws readiness; queue saturation retains a recent success
until it expires. The endpoint reads cached evidence and worker state, so health
polling cannot fill the render queue. Hosts should choose a probe page that is
safe and representative of their application's render path.

`Renderer::status()` reports admission phase, queue occupancy/capacity, and whether
an isolate is registered with the watchdog. These gauges are observations, not
reservations or proof that a bundle can render. The capacity remains sixteen;
a shutdown wake message may occupy a queue position.

## Logs, request IDs, and tracing

The example creates a random process prefix and an increasing request number.
Each response gets an `X-Request-ID` such as a 32-digit hexadecimal prefix followed
by a 16-digit number. Incoming IDs are replaced. IDs correlate the response with
JSON HTTP spans, including failures; they are neither credentials nor metric labels.
The span records the matched route, a finite HTTP method category, and status.
Query strings, cookies, props, rendered HTML, source, and JavaScript diagnostics
are omitted. A render message carries its tracing span across the worker thread.

`RenderError::category()` returns a finite `RenderErrorCategory`. Its `as_str()`
labels distinguish queue saturation, unavailability, deadlines, heap failures,
contract failures, and JavaScript errors without formatting the underlying error.
The renderer emits category-only warning events; the Axum adapter records the
same category and returns its existing fixed error body. Explicitly formatting
`RenderError` still exposes diagnostics to a caller. Hosts must apply their own
redaction policy when inspecting those values or installing a panic hook.

JSON logging defaults to `info,hyper=warn,reqwest=warn`. `RUST_LOG` accepts an
operator-supplied filter and rejects malformed configuration. Enabling dependency
trace logs requires a separate review of what those dependencies record.

Set `GYRAL_OTLP_ENDPOINT` to a complete HTTP/protobuf trace endpoint, for example
`http://127.0.0.1:4318/v1/traces`, to enable OTLP. The host validates its URL before
startup, forbids credentials in its authority and query parameters, and installs
one subscriber. With export enabled, valid W3C `traceparent` headers attach HTTP
spans to their parent trace; the generated request ID is a span attribute.
The exporter uses a bounded SDK batch queue, a one-second request timeout, and
no retries. Telemetry outages do not retry application renders or change readiness.
Shutdown flushes the provider within two seconds and reports failure. Collector
credentials may use the SDK's standard header configuration; they must stay out
of logs. Export is disabled when the example endpoint is absent.

## Metrics and request limits

The renderer records through the `metrics` facade. Without a host recorder,
recording is a no-op. The host supplies a Prometheus recorder and `/metrics`:

| Metric | Labels or interpretation |
| --- | --- |
| `gyral_render_requests_total` | Outcome `ok` or a finite error category; includes probes and dropped render futures. |
| `gyral_render_duration_seconds` | Histogram by outcome, including queue time and early failures. |
| `gyral_http_requests_total` | Matched route, finite method category, status. |
| `gyral_http_duration_seconds` | Histogram with the same labels; measures response construction, not slow body delivery. |
| `gyral_renderer_queue_depth`, `gyral_renderer_queue_capacity` | Worker queue snapshots at scrape time. |
| `gyral_renderer_active`, `gyral_renderer_ready` | Zero/one snapshots at scrape time. |

There are no page IDs, URLs, request IDs, or DTO values in metric labels. Histograms
use fixed buckets from 5 ms to 3 seconds. Scrapes and the supervised probe task run
recorder upkeep. Hosts with several renderer handles must name their own bounded
instance gauges; facade counters aggregate the process's render activity.

HTTP observation surrounds panic handling, a three-second response timeout, and
a 64 KiB body limit. Responses missing a cache policy receive no-store; existing
immutable asset caching is preserved. Renderer admission sheds excess work rather
than adding another HTTP waiting queue: one active render and sixteen queued
positions bound accepted rendering. Health and metrics do not enter that queue.
The example makes no cross-origin API calls, so it installs no permissive CORS
policy. Compression and application rate limits remain host choices. A panic hook,
native V8 failure, or process OOM is outside the category-only error boundary.

## Signal shutdown

SIGTERM and SIGINT are installed before HTTP binds. Either signal closes host
admission and fails readiness immediately. New page requests return 503 while
accepted handlers continue. The example gives readiness observers 250 ms before
stopping HTTP acceptance, then allows at most five seconds for HTTP to drain.
It joins the readiness task, calls the renderer's bounded shutdown, and confirms
both native threads joined before flushing OTLP outside the async executor.
The existing two-second render and worker shutdown policies remain in effect.

Allow at least fifteen seconds when stopping a container; HTTP drain, a current
probe, renderer cleanup, and telemetry flush have separate budgets. Docker uses
SIGTERM and probes `/readyz`. A drain or cleanup timeout is an error, not proof of
finished native teardown. Embedded native stalls still require process containment.

## Sustained fixture budget

`make load` runs a release binary with ten seconds of warmup and sixty seconds
of sampling. Four closed-loop clients render distinct synthetic names. The runner
checks DTO isolation, HTTP success, latency percentiles, sampled Linux RSS, and
queue bounds; a 64-request burst must produce 503 shedding and recover afterward.

| Regression budget | Required result |
| --- | --- |
| Throughput | At least 10 successful responses/second. |
| Latency | p95 at most 500 ms; p99 at most 1 second. |
| Process RSS | At most 512 MiB during the sampled minute. |
| RSS growth | Last five-second window maximum at most 64 MiB above the first window maximum. |
| Queue occupancy | At most sixteen positions. |

These deliberately broad fixture budgets catch regressions on the supported
build host. A finite RSS observation is not a leak proof or process-memory cap.
This is a closed-loop test, not an arrival-rate capacity model. Application SLOs,
request mixes, deployment quotas, open-loop overload, and longer soak tests need
application-specific evidence. Reports stay in `.local/load-evidence.json`.

## Supported builds and distribution

The initial supported target is `x86_64-unknown-linux-gnu`. The tested minimum
Rust version is **1.99.0**, edition 2024; CI uses that exact toolchain. Earlier
compilers, musl, ARM, Windows, and macOS have no support claim. The reference
container builds against Debian bookworm and runs with glibc 2.36 and libstdc++.
Local binaries can inherit a newer host ABI; release evidence extracts its binary
from the tested container rather than distributing the local compiler's output.

The four library crates share version 0.1.0 and are prepared for crates.io.
Applications add `gyral`, the front door, which re-exports `gyral-renderer` and,
with its `axum` feature, `gyral-axum`. Those building blocks and their pure
`gyral-renderer-domain` dependency are published alongside it. Each library sets
`publish = true`; examples set `publish = false` and remain integration material.
The owner publishes with the [release runbook](../releasing.md); until 0.1.0 is
on crates.io, consumers use a reviewed checkout and path dependencies. Published
requirements are caret ranges, except the exact `deno_core` pin that the runbook
justifies; `Cargo.lock` keeps this repository's builds exact. Gyral build inputs
come from the published npm 0.3.4 packages, pinned with SHA-512 registry
integrity in the frozen lockfile. An empty build cache requires access to the
registry.

Before 1.0, incompatible public API or documented telemetry-label changes require
a minor version increase. Compatible fixes use a patch increase. Package versions
move together. The JavaScript bundle ABI and artifact format have separate explicit
versions: a Rust release must not silently reinterpret old metadata. Gyral release
changes require a new compatibility fixture and matching server/client artifacts.
There is no published baseline yet. The release workflow runs cargo-semver-checks
for each crate whose latest published version is 0.1.0 or later and skips the
others: three crates are unpublished, and `gyral` has only the owner's 0.0.1
name-holding placeholder, which has no API to compare. The check starts
automatically once 0.1.0 exists. Document intentional breaks in the
changelog. Do not claim semver validation against an unpublished baseline.

## Native archive and release evidence

`tools/v8-archive.json` pins the V8 150.4.0 SIMDUTF release archive for the supported
target, including its size and SHA-256. The digest was checked against the
[upstream release](https://github.com/denoland/rusty_v8/releases/tag/v150.4.0).
`make prepare-native` downloads to an ignored cache and verifies every use in the
root build workflow. A corrupt existing cache fails rather than being accepted.
Root Cargo configuration forces `RUSTY_V8_ARCHIVE` to that local file. Docker
verifies the same pin before compiling. The filename is also the upstream default
for this feature set; `Cargo.lock` alone would not verify those native bytes.

The pin establishes integrity against the reviewed release input. It is not a
source audit, signature verification, or reproducible build of V8 itself.
Other applications do not inherit this repository's `.cargo/config.toml`: they
must adopt the preparation step and pin before compiling the renderer. The
[renderer guide](../../crates/gyral-renderer/README.md#pin-the-native-v8-archive)
gives them the lockfile, download, digest, and configuration steps. docs.rs needs
no archive: it sets `DOCS_RS`, and the V8 build script then skips the native
library; `make package` checks that path offline. Upgrading
V8, the target, or its native feature set requires a new reviewed archive entry,
configuration, and verification evidence.

Install cargo-cyclonedx 0.5.9 and run `make release-evidence`. It builds and checks
the reference container, then assembles local artifacts under `.local/release/0.1.0`:

- A source archive with the workspace, documentation, pinned skills, and both
  lockfiles; local tracker records, credentials, databases,
  build caches, and generated bundles are excluded.
- A Linux binary archive with verified browser/server assets and run instructions.
- CycloneDX 1.5 JSON inventories for Rust dependency/build inputs, the native V8
  archive, and npm production inputs, with local workspace paths removed.
- A release descriptor, both lockfiles, native pin, and `SHA256SUMS`.

The image records a compiler-input fingerprint. Assembly rejects a stale image
or a mismatched asset manifest. Inventory generation checks that Cargo did not
change the lockfile. Fixed archive metadata makes unchanged archive contents
repeatable; compiler output and SBOM timestamps need not be reproducible.
The inventories cover repository build inputs, not every operating-system package
or JavaScript build/test tool. Container and toolchain inventories are separate
requirements for an application distribution. Nothing is uploaded or published
by this command. CI prepares the same evidence as workflow artifacts after checks;
the release workflow attaches it to the GitHub release for each tagged version.
The workflow has passed on GitHub's hosted `ubuntu-24.04` runner; the
[verification record](../verification.md) names the run.

Gyral core and SSR now use the published npm release `0.3.4`. The frozen pnpm
lockfile supplies SHA-512 integrity for registry archives. `npm-production.cdx.json`
records both Gyral packages and their resolved production dependency graph,
including the type-only Standard Schema package; inclusion does not assert that
every dependency contributes executable bytes to the bundles. Binary archives
carry the available npm license texts alongside Rust notices. Source archives
contain no vendored JavaScript archives and need registry access on an empty
package cache. Historical prerelease measurements remain in the verification
record and do not describe the upgraded bundle.

## Rust production requirements in this workspace

The pinned [production reference](../../.agents/skills/rust-fp/references/production.md)
applies to deployable hosts. The following review distinguishes implemented
package/example behavior from application responsibilities. It does not certify
an authenticated production service.

| Requirement | Implementation or boundary |
| --- | --- |
| Structured red-track instrumentation | HTTP/render spans and safe category events; raw diagnostic formatting is deliberately excluded. Notes workflows already log typed, redacted errors. |
| JSON logs and bounded OTLP | Example subscriber and optional exporter; W3C inbound context and request-ID correlation tested. No outbound application calls to propagate. |
| Secret handling | DTOs, cookies, thrown text, and export headers omitted. The renderer has no secret configuration. Applications own secrets, safe Debug types, and panic hooks. |
| Deliberate Tower stack | Observation, panic handling, response timeout, body limit; renderer queue provides shedding. CORS, compression, auth, and rate limits depend on host routes. |
| External timeouts and retries | Renderer deadlines and one-second telemetry export; no automatic render retries. Application gateways do not exist here. |
| HTTP error policy | Existing opaque page errors and status mapping retained, with request IDs in headers. No JSON API or OpenAPI schema to describe. |
| Health/readiness and shutdown | Real startup/periodic probes, freshness/worker/drain checks, active-request SIGTERM/SIGINT drain, explicit joins and provider flush. |
| Containers and profiles | Pinned multi-stage images, BuildKit dependency/target/native caches, non-root UID, readiness healthcheck, unwind retained, overflow checks, profiling profile. Cargo-chef adds no necessary cache boundary here. |
| Supply chain and MSRV | Locked builds on Rust 1.99.0, deny and audit gates with the existing scoped paste exception, native digest pin and local SBOM evidence. Publishing a lockfile is a later Git/release action. |
| Metrics and resource bounds | Finite RED/outcome labels, histograms, queue/active/readiness gauges, sustained fixture budget. No renderer database pool; the separate notes host owns its SQL pool. |
| Typed startup configuration | Bind address, artifacts, log filter, endpoint, and renderer policy parsed before serving. Defaults documented; malformed values fail. |
| Load and review | Sustained profile plus overload/recovery; final Rust self-review and full local gate. Application workloads, longer soak tests, identity, deployment resource policy, and process containment remain host responsibilities. |
