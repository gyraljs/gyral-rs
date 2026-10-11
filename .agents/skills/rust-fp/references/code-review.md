# Self-Review Pass — run before declaring any work done

After implementing, review your own output as a hostile reviewer would. Do this **every time**, before reporting completion. Fix everything found, then re-run the pass. Clippy catches most of section 1 already — run the greps anyway, because they also catch what lives behind an `#[allow]`.

## 1. Mechanical sweep

Every hit is either a violation to fix or a documented, justified exception (adapter crate, test module, `main.rs`). Zero unexplained hits.

```bash
# --- totality: no partial functions in domain/app ---
rg -n '\.unwrap\(\)|\.expect\(' crates/domain crates/app
rg -n 'panic!|todo!|unimplemented!|unreachable!' crates/domain crates/app
rg -n '\w\[[0-9a-z_.]+\]' crates/domain crates/app | rg -v '^\S+:\s*#\['   # slice indexing; use .get()

# --- error channel ---
rg -n 'Box<dyn (std::error::)?Error' crates/
rg -n 'Result<[^,>]+, *String>' crates/
rg -n '\banyhow::|anyhow!' crates/ | rg -v 'crates/server/src/main\.rs|build\.rs'
rg -n 'sqlx::Error|reqwest::Error|hyper::Error' crates/app crates/domain

# --- boundaries: no unchecked coercion of external data ---
rg -n '\bas (i8|i16|i32|i64|u8|u16|u32|u64|usize|isize|f32|f64)\b' crates/
rg -n 'derive\(.*Deserialize' crates/domain    # must be #[serde(try_from = "Raw..")]; the only
                                               # allowed hit is inside a #[nutype(..)] derive list,
                                               # whose impl routes through try_new (domain-types.md)
rg -n 'from_utf8_unchecked|transmute|unsafe' crates/

# --- money ---
rg -n '\bf64\b|\bf32\b' crates/domain crates/app

# --- injected capabilities: only the adapter may call these ---
rg -n 'Utc::now|Local::now|SystemTime::now|Instant::now' crates/ | rg -v 'crates/infra/src/clock\.rs'
rg -n 'Uuid::new_v4|rand::random|thread_rng' crates/ | rg -v 'crates/infra/src/(clock|ids)\.rs'

# --- exhaustiveness: no catch-all over a domain enum ---
rg -n '^\s*_ =>' crates/domain crates/app

# --- style / smells ---
rg -c '\.clone\(\)' crates/ | sort -t: -k2 -rn | head               # clone density: is ownership wrong?
rg -n '#\[async_trait\]' crates/                                    # justified ONLY for dyn traits
rg -n 'mockall' crates/                                             # fakes, not mocks
rg -n 'std::env::var|dotenv' crates/ | rg -v 'crates/server|config'  # config is parsed once, typed
rg -n 'println!|eprintln!' crates/ | rg -v 'crates/cli|crates/server' # use tracing
```

`#[async_trait]` hits: keep only where the trait is used as `dyn Trait`. If every call site is generic, delete the macro and use native AFIT (stable since 1.75), adding `trait-variant` when you need `Send` bounds on the returned futures.

## 2. Dependency-direction audit — the Rust superpower

Unlike a grep-based layering audit, this one is enforced by the build. A violation is not a style slip; it means **someone edited a `Cargo.toml`**, and that edit is the thing to revert.

```bash
cargo tree -p domain --edges normal --depth 1      # expect: thiserror, nutype, rust_decimal, serde. nothing else.
cargo tree -p domain --edges normal | rg -i '^(tokio|sqlx|axum|reqwest|hyper|diesel|sea-orm)\b' && echo VIOLATION
cargo tree -p app    --edges normal --depth 1      # domain + traits' deps. no sqlx, no axum.
cargo build -p domain --no-default-features        # domain still compiles with no serde, no std features
cargo tree -d                                      # duplicate versions of the same crate in the graph
```

- [ ] `domain` depends on no async runtime, no driver, no web framework, no HTTP client
- [ ] `app` names service **traits**, never concrete `infra` types; `infra` depends on `app`, not the reverse
- [ ] Nothing but `crates/server` depends on both `infra` and `api`
- [ ] No `[dev-dependencies]` smuggling tokio into `domain` and then leaking into non-test code

## 3. Error-channel audit

Read every `pub fn` signature that returns `Result` and check:

- [ ] `E` is a **named enum** derived with `thiserror` — never `Box<dyn Error>`, never `anyhow::Error`, never `String`
- [ ] One error enum per layer/module, not one god-enum; `#[from]` used at most once per source type per enum
- [ ] Infra errors are translated at the service boundary: no `sqlx::Error`, `reqwest::Error`, or `serde_json::Error` visible in a workflow, handler, or trait method signature
- [ ] Every variant carries **actionable data**, not prose: `NotFound { id: OrderId }`, `Insufficient { requested: Quantity, available: Quantity }`, `Upstream { retriable: bool }` — a caller must be able to branch on it without parsing a string
- [ ] Retry predicates read a field of the error, never a substring of its `Display`
- [ ] Every surviving `expect` carries an invariant proof: `expect("invariant: currencies checked in Order::place")` — not `expect("should work")`
- [ ] Defect vs expected error is deliberate: unreachable-by-construction states may panic and are documented under `# Panics`; anything a user or the network can cause is a `Result`
- [ ] `#[tracing::instrument(err)]` on workflow entry points so the error track is observable
- [ ] No `.ok()` / `let _ =` silently discarding a `Result`

```bash
rg -n '\.ok\(\);|let _ = ' crates/app crates/infra      # swallowed errors
rg -n 'map_err\(\|_\|' crates/                          # discarded source: is the cause really worthless?
```

## 4. Type-design audit

- [ ] No naked `String` / `i64` / `u32` / `bool` crossing a domain function boundary — newtypes with private fields and fallible constructors
- [ ] No `Option` field-pairs where exactly one must be set → one enum (`card: Option<..>, paypal: Option<..>` → `enum PaymentMethod`)
- [ ] No boolean state flags encoding a lifecycle (`is_paid`, `is_shipped`) → `enum OrderState`, or distinct types / a `PhantomData` typestate
- [ ] Every `match` over a domain enum is exhaustive with named arms; adding a variant must break the build
- [ ] `#[non_exhaustive]` appears only where downstream exhaustive matching is deliberately forbidden, with a doc comment saying why
- [ ] Every boundary decodes exactly once via `TryFrom` on a `Raw*` DTO; domain types use `#[serde(try_from = "RawX")]`, never a bare `#[derive(Deserialize)]`
- [ ] Zero `as` casts on external data — `u64::try_from(n)?`
- [ ] Money is integer minor units or `rust_decimal::Decimal`; `overflow-checks = true` is on in release
- [ ] Collections that must be non-empty are typed non-empty, not asserted
- [ ] Constructors are private + `try_new`; no public struct literal can build an invalid value

## 5. Runtime & resource audit

- [ ] Exactly one wiring site (`crates/server/src/main.rs`): one runtime, one pool, one config parse, one tracing init. Everything else takes its dependencies as arguments
- [ ] Every fan-out is **bounded**: `stream::iter(xs).map(f).buffer_unordered(N)`, never `join_all`/`try_join_all` over caller-controlled input
- [ ] Every external call has a timeout (`tower::timeout`, or `tokio::time::timeout`); no unbounded await on a socket
- [ ] Every `select!` branch is cancellation-safe, or is wrapped in a `JoinSet`/pinned future so cancellation cannot drop half-done work
- [ ] No `async` work attempted in `Drop` — `Drop` cannot await; rollback lives in an explicit compensation stack
- [ ] Transaction boundaries live in workflows, never inside repositories; a failure path drops `tx` unreserved and rolls back
- [ ] Channels are bounded; backpressure is a design decision, not an OOM
- [ ] Graceful shutdown: `CancellationToken` propagated, `JoinSet` awaited, pool closed
- [ ] No `Arc<Mutex<_>>` threaded through call graphs to dodge ownership; no `.clone()` used purely to silence the borrow checker

```bash
rg -n 'join_all|try_join_all' crates/
rg -n 'tokio::spawn' crates/ | rg -v 'JoinSet'          # unstructured tasks: who awaits them?
rg -n 'select!' crates/                                 # audit each branch for cancellation safety
rg -n 'impl Drop' crates/ -A6 | rg 'await|block_on|spawn'
```

## 6. Test audit

- [ ] Pure domain logic has **property tests** (`proptest`): round-trip `TryFrom`/`Display` on every newtype, invariants on every aggregate operation
- [ ] Error tracks are tested as API surface — one test per reachable variant, asserting on the variant and its payload, not on a formatted string
- [ ] Atomicity is tested where transactions exist: force a mid-workflow failure, assert **no partial state** persisted
- [ ] Time is injected: a fake clock returning fixed instants; zero `tokio::time::sleep` of real duration (use `tokio::time::pause()`/`advance()`)
- [ ] Test doubles are hand-written `impl` blocks of your own traits; no `mockall`
- [ ] Integration tests hit a real database (`#[sqlx::test]` provisions a **fresh database per test** and drops it on success — it is not a rollback, and a failed test deliberately leaves the DB for inspection)
- [ ] Concurrency tests use `nextest`'s process isolation rather than shared global state

Then prove the error branches are genuinely exercised, not merely executed:

```bash
cargo mutants --in-place --file 'crates/domain/**' --timeout 60
```

Surviving mutants in domain code mean a branch has no assertion behind it. Fix the tests, not the tool.

### Honesty rule

Actually run these and report the real output. Never claim green without running.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo test --workspace --doc
cargo deny check advisories bans licenses sources
```

If a command fails and you cannot fix it in scope, say so explicitly with the failing output. A claimed-green report that is red is worse than no report.

## 7. Checklist sweep

Open every reference file used during the task and walk its end-of-file checklist against the diff. Report honestly: for each unmet item, either fix it or state explicitly why it does not apply.

## Checklist

- [ ] Section-1 greps run; every hit fixed or justified in a comment
- [ ] `cargo tree -p domain --edges normal --depth 1` clean; no infra crate in the domain graph
- [ ] Every public `Result` has a named `thiserror` enum with actionable payloads
- [ ] No `Box<dyn Error>`, no `String` errors, no `anyhow` outside `main.rs`
- [ ] No infra error type in any workflow or handler signature
- [ ] Every surviving `expect` states the invariant that makes it total
- [ ] No naked primitives, no `Option`-pairs, no boolean state flags in the domain
- [ ] Every domain `match` exhaustive; every boundary decoded once via `TryFrom`
- [ ] One wiring site; every fan-out bounded; every external call has a timeout
- [ ] No async work in `Drop`; cancellation safety reviewed for each `select!`
- [ ] Proptests on pure logic; error variants tested; atomicity tested; fakes not mocks
- [ ] `cargo mutants` run over `crates/domain`; no unexplained survivors
- [ ] fmt, clippy `-D warnings`, nextest, doctests, and deny all actually executed and reported
