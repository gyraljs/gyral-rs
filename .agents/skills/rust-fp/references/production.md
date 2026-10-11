# Production Readiness — the Definition of Done

"Works on the happy path" is the starting line. Everything below is **required** for any
deployable service; each item is a handful of lines in Rust, so there is no excuse to skip one.
The checklist at the bottom is the acceptance criteria — a task is not done until it passes.

## 1. Observability

`tracing` 0.1.44 + `tracing-subscriber`; `tracing-opentelemetry` 0.33.0 for OTLP export.

### Instrument the red track for free

```rust
#[tracing::instrument(
    skip(pool, secret),                                    // never log a pool handle or a secret
    fields(order.id = %order_id, order.total_cents = tracing::field::Empty),
    err,                                                   // Err(_) ⇒ an ERROR event, automatically
)]
pub async fn place_order(
    pool: &PgPool,
    secret: &SecretString,
    order_id: OrderId,
    cmd: PlaceOrder,
) -> Result<Receipt, OrderError> {
    let total = order_total(&cmd.lines);
    tracing::Span::current().record("order.total_cents", total.get());     // value known only now
    let receipt = charge(pool, secret, total).await?;      // `?` shunts to the red track...
    Ok(receipt)                                            // ...and `err` reports it for you
}
```

| Attribute | What it does |
|---|---|
| `err` | emits an ERROR event with `error = %e` whenever the function returns `Err` — the error track becomes observable with one word |
| `err(Debug)` | records `?e` instead; use when `E`'s `Debug` carries more than its `Display` |
| `err(level = "warn")` | expected errors (404, validation) should not page anyone |
| `ret` / `ret(Debug)` | records the `Ok` value — small, non-sensitive returns only |
| `skip(..)` / `skip_all` | mandatory for pools, connections, secrets, request bodies |

**Structured fields, never string interpolation.** `tracing::info!(order.id = %id, lines = n,
"order placed")` — not `info!("order {id} placed")`. You can query a field; you cannot query
prose. Sigils: `%` = `Display`, `?` = `Debug`, bare = the type's native `Value` impl (integers,
bools, `&str`). `field::Empty` + `Span::record` for values computed later in the function.

### Subscriber wiring (exactly once, in `main.rs`)

```rust
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_telemetry(cfg: &TelemetryConfig) -> Result<(), TelemetryError> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info,hyper=warn"));
    let registry = tracing_subscriber::registry().with(filter);
    match cfg.format {
        LogFormat::Json => registry.with(fmt::layer().json().flatten_event(true)).init(),
        LogFormat::Pretty => registry.with(fmt::layer().pretty()).init(),   // local dev only
    }
    Ok(())
}
```

JSON in every deployed environment; pretty only on a developer's terminal. Add
`tracing-opentelemetry` 0.33 to export the same spans over OTLP — span parenthood, timings, and
the `err` events go out without touching a single handler:

```rust
let otel = tracing_opentelemetry::layer().with_tracer(tracer);
tracing_subscriber::registry().with(filter).with(otel).with(fmt::layer().json()).init();
```

Propagate W3C `traceparent` on outbound calls so spans stitch across services, and record the
inbound request id as a span field so a log line and a trace can be joined.

### Never log secrets

`secrecy::SecretString`: `Debug` and `Display` print `[REDACTED]`, and `expose_secret()` is the
single grep-able place where plaintext exists. Config parses straight into `SecretString`
(`boundaries.md`), and every `#[instrument]` on a function taking one carries it in `skip(..)`. A
field that is never recorded cannot leak.

## 2. Resilience is composed, not nested

`tower::Service<Req>` is `Request -> Future<Output = Result<Response, Error>>` — **an async
railway function**. A `Layer` is `Service -> Service`, i.e. a higher-order function, and
`ServiceBuilder` is function composition. Cross-cutting concerns therefore *compose* at the edge
instead of being re-implemented inside every handler.

```rust
use tower::{ServiceBuilder, limit::ConcurrencyLimitLayer};
use tower_http::{catch_panic::CatchPanicLayer, compression::CompressionLayer, cors::CorsLayer,
                 limit::RequestBodyLimitLayer, timeout::TimeoutLayer, trace::TraceLayer};

let app = routes().layer(
    ServiceBuilder::new()                              // reads OUTERMOST → innermost
        .layer(TraceLayer::new_for_http())             // 1. span covers timeouts, panics, shed load
        .layer(CatchPanicLayer::new())                 // 2. a defect becomes 500; process survives
        .layer(TimeoutLayer::new(Duration::from_secs(10)))
        .layer(ConcurrencyLimitLayer::new(512))        // 4. shed before the DB pool saturates
        .layer(RequestBodyLimitLayer::new(1 << 20))    // 5. 1 MiB, before anything reads the body
        .layer(CorsLayer::permissive())                // 6. tighten per environment; never in prod
        .layer(CompressionLayer::new()),               // 7. innermost: compress the handler's body
).with_state(state);
```

**Order matters.** Trace outermost or you will not see the requests that were shed or timed out.
`CatchPanicLayer` above the rest so it catches defects from inner layers too — and note it is
**inert under `panic = "abort"`** (§6). Body limit before any extractor reads the body. Health
endpoints go outside the auth and concurrency-limit layers.

### Retry

```rust
use backon::{ExponentialBuilder, Retryable};

let policy = ExponentialBuilder::default()
    .with_min_delay(Duration::from_millis(200))
    .with_max_times(3)                       // ALWAYS bounded
    .with_jitter();                          // ALWAYS jittered — synchronised retries are a herd

let receipt = (|| async { gateway.charge(&req).await })
    .retry(policy)
    .when(GatewayError::retriable)           // a FIELD on the error enum, never a Display substring
    .await?;
```

- Retry only transient failures. Retrying a 4xx, a validation failure, or `PaymentDeclined`
  double-charges customers — that is why the error enum carries `retriable: bool` per variant.
- Retry only idempotent operations; send an idempotency key on every retried write.
- The whole retry loop lives **inside** an outer timeout budget, or 3 × 5 s blows the request
  deadline. `tower::retry::RetryLayer` when you want it as a layer instead of a call site.
- Next step up for a flapping dependency: a circuit breaker plus `tower::load_shed`, so a dead
  dependency fails fast instead of consuming every task.

## 3. Mapping the error track to HTTP

This is the **one place errors become strings** — everywhere else they stay structured data.

```rust
#[derive(serde::Serialize)]
struct ErrorBody<'a> { code: &'a str, message: &'a str, trace_id: &'a str }

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self.kind {
            ErrorKind::Validation { field } =>
                (StatusCode::UNPROCESSABLE_ENTITY, "validation_failed", format!("invalid field: {field}")),
            ErrorKind::NotFound   => (StatusCode::NOT_FOUND, "not_found", "resource not found".to_owned()),
            ErrorKind::Conflict   => (StatusCode::CONFLICT, "conflict", "resource changed".to_owned()),
            ErrorKind::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "not authenticated".to_owned()),
            ErrorKind::RateLimited  => (StatusCode::TOO_MANY_REQUESTS, "rate_limited", "slow down".to_owned()),
            // Infrastructure: log EVERYTHING internally, tell the client NOTHING.
            ErrorKind::Internal { source } => {
                tracing::error!(error = ?source, trace_id = %self.trace_id, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", "internal error".to_owned())
            }
        };
        let body = ErrorBody { code, message: &message, trace_id: self.trace_id.as_str() };
        (status, Json(body)).into_response()
    }
}
```

- `ApiError` carries the request/trace id captured by the middleware; the client's only internal
  detail is that opaque handle, which support can grep for.
- **Never render `sqlx::Error`, `reqwest::Error`, or `std::io::Error` text to a client** — it
  leaks schema names, hostnames, and SQL. Infra errors are `#[from]`-lifted into `Internal`, never
  into a client-visible variant (`rop-errors.md`).
- One body shape for every error, documented in the OpenAPI schema (`utoipa` 5.5.0).
- Choose 404-vs-403 deliberately so authorization failures do not enumerate resources.

## 4. Health and readiness are different endpoints

```rust
// Liveness: is the process wedged? No dependency checks — a DB outage must not make the
// orchestrator restart-loop otherwise-healthy pods.
async fn livez() -> StatusCode { StatusCode::OK }

// Readiness: should THIS instance receive traffic right now?
async fn readyz(State(s): State<AppState>) -> (StatusCode, Json<Ready>) {
    let db = matches!(timeout(Duration::from_millis(250), s.pool.acquire()).await, Ok(Ok(_)));
    let draining = s.shutdown.is_cancelled();   // SIGTERM received ⇒ leave the LB before draining
    let code = if db && !draining { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (code, Json(Ready { db, draining }))
}
```

Readiness flipping to `false` on SIGTERM is what makes graceful shutdown actually graceful
(`concurrency.md` §5). Both endpoints bypass auth, rate limits, and the concurrency limit.

## 5. Containerization

```dockerfile
# syntax=docker/dockerfile:1
ARG RUST_VERSION=1.97

FROM rust:${RUST_VERSION}-slim-bookworm AS chef
RUN cargo install cargo-chef --version 0.1.77 --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Dependencies compile ONCE and stay cached until Cargo.toml / Cargo.lock actually change.
RUN cargo chef cook --release --locked --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked --bin server

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 --no-create-home app
COPY --from=builder /app/target/release/server /usr/local/bin/server
USER 10001:10001
EXPOSE 8080
# No shell and no curl in hardened bases: the binary checks its own health.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/usr/local/bin/server", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/server"]
```

| Runtime base | Why | Cost |
|---|---|---|
| `debian:bookworm-slim` | safe default; shell + apt make incidents debuggable | largest surface; must install `ca-certificates` for TLS |
| distroless `cc` | no shell, no package manager — hardest to breach | equally hard to debug: needs an ephemeral debug sidecar |
| `scratch` | nothing at all; smallest possible image | needs a static `x86_64-unknown-linux-musl` build, and musl's allocator is markedly slower under multi-threaded allocation — link jemalloc/mimalloc or stay on glibc |

Non-root `USER` always. Secrets arrive as **runtime** env or mounted files — never `ENV` or `ARG`
at build time, since both persist in the image layers. Pin the base by digest for releases.

## 6. Build profile

```toml
[profile.release]
lto             = "thin"      # "fat" if you can afford the link time
codegen-units   = 1
strip           = "symbols"
overflow-checks = true        # non-negotiable with integer-minor-unit money
# panic        = "abort"      # OFF for servers — see below. Uncomment for CLIs/jobs only.

[profile.release-debug]       # for flamegraphs and prod-shaped profiling
inherits = "release"
debug    = 1                  # line tables: usable stack traces and profiles
strip    = "none"
```

`panic = "abort"` is a deliberate trade, never a default: it removes unwinding tables and shrinks
the binary, but **`std::panic::catch_unwind` and `CatchPanicLayer` stop working**, so one
panicking request kills every in-flight request with it. Choose `abort` for CLIs, jobs, and
workers with fast restarts where a panic is a genuine defect; keep the default unwind for any
HTTP server that must convert a defect into a 500 (§2). Cargo ignores the setting for the `test`
profile, so `#[should_panic]` still works either way. `scaffold.md` therefore ships the profile
**without** `panic`, carrying a comment explaining the trade — set it only when the service has no
`CatchPanicLayer` and you have written down why.

## 7. Supply chain

```bash
cargo deny check advisories bans licenses sources   # cargo-deny 0.20.2 — deny.toml in scaffold.md
cargo audit --deny warnings                         # cargo-audit 0.22.2 — RustSec vs Cargo.lock
```

- `cargo-deny` gates four things: known advisories, license allow-list, banned/unmaintained crates
  (`unmaintained = "all"` — an archived crate is a vulnerability with a long fuse), and duplicate
  versions of the same crate in the graph.
- `cargo-vet` when the org needs recorded human audits of third-party code rather than just
  advisory scanning.
- Generate an SBOM (CycloneDX or SPDX) in CI and attach it to the release artifact.
- Declare MSRV as `rust-version = "1.88"` in `[workspace.package]` and verify it in CI.
- **Commit `Cargo.lock`** for every binary crate; reproducible builds depend on it and every
  container build uses `--locked`.

## 8. Metrics

RED per endpoint (rate, errors, duration) from the trace/metrics middleware; pool utilization
(`pool.size()` / `pool.num_idle()`); queue and channel depth; external-call latency and failures
split by error variant; plus the business counters that tell you the product still works. Use the
`metrics` crate facade with a Prometheus/OTLP exporter, or OpenTelemetry meters if you are already
exporting OTLP. Histograms, not averages. Keep label cardinality bounded — a user id or an order
id as a label will take down the metrics backend before it takes down you.

## Checklist — a service is not done until every box is ticked

- [ ] Every workflow and every I/O method carries `#[tracing::instrument]` with `skip` for pools/secrets and `err` (or `err(level = ..)`) so the error track emits events
- [ ] Logs are JSON with `EnvFilter` in every deployed environment; zero `println!`/`dbg!`; all context in structured fields, none in interpolated strings
- [ ] OTLP export wired via `tracing-opentelemetry`; `traceparent` propagated outbound; request id joins logs to traces
- [ ] No secret is loggable: `SecretString` everywhere, `expose_secret()` call sites countable on one hand, every sensitive arg in `skip(..)`
- [ ] Tower stack applied with a deliberate order: trace, catch-panic, timeout, concurrency limit, body limit, CORS, compression
- [ ] Every external call has a timeout; retries are bounded, jittered, predicate-driven on an error field, and only on idempotent operations
- [ ] `IntoResponse` maps every error variant to a status code and one JSON body shape; infrastructure errors log in full and return an opaque 500
- [ ] No infra error text (`sqlx`, `reqwest`, `io`) can reach a client, verified by reading the `Internal` arm
- [ ] Liveness and readiness endpoints exist, differ, bypass auth, and readiness goes red on SIGTERM
- [ ] Graceful shutdown verified end to end: SIGTERM drains in-flight requests, joins workers, closes pools, flushes telemetry
- [ ] Multi-stage Dockerfile with `cargo-chef`; dependency layer cached; `--locked` on every build
- [ ] Image runs as a non-root `USER`, has `EXPOSE` + `HEALTHCHECK`, and contains no build-time secret in any layer
- [ ] Release profile chosen deliberately, including the `panic` strategy vs `CatchPanicLayer`; `overflow-checks = true`
- [ ] `cargo deny check` and `cargo audit` green in CI; SBOM produced; MSRV declared; `Cargo.lock` committed
- [ ] RED metrics per endpoint, pool utilization, and queue depth exported; label cardinality bounded
- [ ] Config parsed and validated once at startup into a typed struct; the process refuses to boot on invalid config
- [ ] Load-tested on the hot path: latency percentiles known, memory flat under sustained load, no unbounded queue growth
- [ ] `references/code-review.md` self-review pass run, and every reference checklist walked against the diff
