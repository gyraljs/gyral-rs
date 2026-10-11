# Railway-Oriented Programming in Rust

The two-track model from [fsharpforfunandprofit.com/rop](https://fsharpforfunandprofit.com/rop/): every fallible function is a switch — success continues down the green track, failure diverts to the red track and bypasses every subsequent step. In Rust this is not a pattern you implement; it **is** `Result<T, E>`, and **`?` is the switch**: `Ok(v)` unwraps to `v` and continues, `Err(e)` converts via `From` and returns immediately.

**Idiomatic Rust ROP is written with `?`, not combinator chains.** `?` plays better with the borrow checker (no closure capturing a `&mut` you still need) and with async (no `.and_then(|x| async move { .. })` type soup). Use combinators — `map`, `and_then`, `map_err`, `ok_or_else`, `unwrap_or_else` — for short, local, non-async transformations only.

```rust
pub async fn checkout(db: &Db, cmd: CheckoutCommand) -> Result<Order, CheckoutError> {
    let mut tx = db.begin().await?;                      // Err -> red track; tx never commits
    let cart = load_cart(&mut *tx, cmd.user).await?;
    let order = place(&mut *tx, price(&cart)?).await?;   // price() is pure, still on the railway
    tx.commit().await?;
    Ok(order)
}
// Combinators: fine here — short, sync, nothing borrowed escapes.
let port = raw.port.ok_or(ConfigError::MissingPort)
    .and_then(|p| Port::try_new(p).map_err(ConfigError::BadPort))?;
```

## Defining errors with `thiserror` 2.0.19

One enum **per layer/module**, never one god-enum — a god-enum forces every caller to match variants that cannot occur at its level.

```rust
#[derive(Debug, thiserror::Error)]
pub enum CheckoutError {
    #[error("cart {cart_id} is empty")] CartEmpty { cart_id: CartId },
    #[error("insufficient stock for {sku}: requested {requested}, available {available}")]
    InsufficientStock { sku: Sku, requested: Quantity, available: Quantity },
    #[error("payment declined: {code}")] PaymentDeclined { code: DeclineCode, retriable: bool },
    #[error(transparent)] Repo(#[from] RepoError),   // #[from]: `?` converts RepoError for free
    #[error("payment gateway unreachable")]          // #[source]: chains cause, no conversion
    Gateway { #[source] source: GatewayError, retriable: bool },
}
```

- **Name what happened, not who threw it** — `InsufficientStock`, not `InventoryServiceError`.
- **Carry the data a handler needs**: ids, the offending value, `retriable: bool`. Never just a message — a `String` error is data you already destroyed.
- Messages: lowercase, no trailing period, no `Error:` prefix — they compose into a chain. `#[error(transparent)]` forwards `Display`/`source`, for pure pass-through variants only.
- `#[non_exhaustive]` on public library error enums (a new variant stays non-breaking); **not** on internal ones, where a new variant should break the build.

### The `#[from]` collision gotcha

**Two `#[from]` variants for the same source type in one enum will not compile** — the generated `From` impls conflict.

```rust
// ❌ two `From<std::io::Error> for ConfigError` impls
    #[error("reading {path}")] Read  { path: PathBuf, #[from] source: std::io::Error },
    #[error("writing {path}")] Write { path: PathBuf, #[from] source: std::io::Error },

// ✅ #[source] instead, with explicit context at each call site
    #[error("reading config {path}")] Read  { path: PathBuf, #[source] source: std::io::Error },
    #[error("writing cache {path}")]  Write { path: PathBuf, #[source] source: std::io::Error },

let text = fs::read_to_string(&path)
    .map_err(|source| ConfigError::Read { path: path.clone(), source })?;
```

The other fix: newtype one of the sources (`struct CacheIo(std::io::Error);`) so the two `#[from]` impls target distinct types, and `?` keeps working for the dominant call site. `snafu` context selectors (see the matrix) exist to make the `#[source]` route ergonomic.

## Error taxonomy per layer, translated at boundaries

`sqlx::Error` must **never** appear in a workflow signature. The repository is where infra vocabulary dies.

```rust
// infra — classify, don't leak. No #[from]: classification is a decision, not a coercion.
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("order {0} not found")]                     NotFound(OrderId),
    #[error("unique constraint {constraint} violated")] Conflict { constraint: &'static str },
    #[error("database unavailable")]                    Unavailable(#[source] sqlx::Error),
}

impl RepoError {
    pub fn classify(e: sqlx::Error, ctx: RepoCtx) -> Self {
        match &e {
            sqlx::Error::RowNotFound => Self::NotFound(ctx.order_id),
            sqlx::Error::Database(db) if db.is_unique_violation() => Self::Conflict { constraint: ctx.constraint },
            // `_` is legitimate here: `sqlx::Error` is a foreign #[non_exhaustive] enum, not a
            // domain enum (`pattern-matching.md`). Bad SQL / schema drift is arguably a defect — decide.
            _ => Self::Unavailable(e),
        }
    }
    pub fn retriable(&self) -> bool { matches!(self, Self::Unavailable(_)) }
}

// web — the last translation. One exhaustive match, in one place.
impl axum::response::IntoResponse for CheckoutError {
    fn into_response(self) -> axum::response::Response {
        use axum::http::StatusCode as S;
        let status = match &self {
            Self::CartEmpty { .. } => S::UNPROCESSABLE_ENTITY,
            Self::InsufficientStock { .. } | Self::Repo(RepoError::Conflict { .. }) => S::CONFLICT,
            Self::PaymentDeclined { .. } => S::PAYMENT_REQUIRED,
            Self::Repo(RepoError::NotFound(_)) => S::NOT_FOUND,
            Self::Repo(RepoError::Unavailable(_)) | Self::Gateway { .. } => S::SERVICE_UNAVAILABLE,
        };
        tracing::error!(error = ?self, "request failed");            // full chain server-side
        (status, axum::Json(ApiError::from(&self))).into_response()  // stable body to the client
    }
}
```

The chain is `sqlx::Error` → `RepoError` → `CheckoutError` → HTTP status, and every arrow is a deliberate `match` — never a blanket `#[from]` on a foreign type. This file owns the *translation chain*; the client-facing half — one JSON body shape, a trace id, and never rendering infra error text — is `production.md` §3.

## Expected error vs defect (panic)

| | Expected error (`Result`) | Defect (panic) |
|---|---|---|
| What | Anticipated domain/infra outcome a caller might handle | Broken invariant, programmer error, impossible state |
| Examples | `OrderNotFound`, `PaymentDeclined`, `RateLimited`, parse failure | Index out of bounds, config missing *after* startup validation, unreachable enum combination |
| In the signature? | Yes, typed in `E` | No — invisible to the type system |
| Mechanism | `Result<T, E>` + `?` | `panic!` / `expect("invariant: ...")` / `unreachable!` |
| Caller action | `match` and recover | Do not catch; fix the bug |

**Every `expect` message must PROVE the invariant**, not restate the call.

```rust
let admin = users.get(&admin_id)
    .expect("invariant: admin row seeded by migration 0001 and never deleted");
match state {
    OrderState::Paid { .. } | OrderState::Shipped { .. } => settle(state),
    OrderState::Draft => unreachable!("invariant: settle() only reachable from a Paid typestate"),
}
```

- The scaffold leaves the release profile on the **default unwind** (`scaffold.md`). `panic = "abort"` kills the process immediately — smaller binaries, no unwinding, and **`catch_unwind` stops working**; take it only for CLIs and single-purpose workers. Unwind keeps the process alive: a panicking `tokio` task aborts only that task, and a panicking thread poisons any `Mutex` it held.
- Web servers: keep unwind and add `tower_http::catch_panic::CatchPanicLayer` (tower-http 0.7.0) so one bad request returns 500 instead of degrading a worker — the layer is **inert under `panic = "abort"`**. That is an **observability boundary, not error handling** — alert on every hit. Full trade-off: `production.md` §6.
- `std::panic::catch_unwind` belongs only at process boundaries (FFI, plugin host, job runner); its `UnwindSafe` bound is a warning sign, not a nuisance.

## Error-crate decision matrix

| Crate | Version | Use for | Never |
|---|---|---|---|
| `thiserror` | 2.0.19 | **The default.** Libraries, domain, every layer enum | — |
| `anyhow` | 1.0.104 | `main.rs`, build scripts, one-off tooling; `.context()` for a human trail | Any library, domain, or workflow signature |
| `eyre`/`color-eyre` | 0.6.12 / 0.6.5 | `anyhow`'s niche plus custom report hooks and coloured backtraces | Same prohibition as `anyhow` |
| `snafu` | 0.9.2 | **Context selectors** — very ROP-shaped. Per-call-site context (`ReadSnafu { path }`) without a new enum per layer; sidesteps the `#[from]` collision by design | Mixing with `thiserror` in one crate |
| `error-stack` (HASH) | 0.8.0 | Attaching a **context trail** as the error moves up: `change_context(..)` + `attach_printable(sku)`; `Report<C>` keeps every frame | When a plain enum already carries the data |
| `miette` | 7.6.0 | Human-facing CLI diagnostics: source spans, labels, `#[help]`, rustc-style rendering | Server errors nobody reads on a terminal |

Default posture: `thiserror` everywhere, `anyhow` in `main` only. Reach for `snafu`/`error-stack` only after concretely feeling the missing-context pain.

## `?` in `main`

`main` may return any `T: std::process::Termination`. `Result<(), E: Debug>` prints the `Debug` form to stderr and exits `1`.

```rust
// src/main.rs — the ONE place anyhow is allowed.
fn main() -> Result<(), anyhow::Error> {
    let cfg = Config::from_env().context("loading configuration")?;
    tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(run(cfg))
}
```

For custom exit codes, return `std::process::ExitCode` instead and map explicitly: `ExitCode::SUCCESS`, `ExitCode::from(2)` for a usage error, `ExitCode::FAILURE` otherwise — printing the error yourself so the operator sees `Display`, not `Debug`.

## Traverse — the collection railway

`Result<T, E>` and `Option<T>` implement `FromIterator`, so collecting an iterator of `Result`s into a `Result` of a collection **short-circuits on the first `Err`** and stops consuming. This is `traverse`/`sequence` from FP, spelled `collect`.

```rust
// N fallible parses -> one Result, first failure wins.
let lines = raw.iter().map(Line::try_from).collect::<Result<Vec<_>, LineError>>()?;
// Any FromIterator target works, including maps.
let by_sku = rows.into_iter()
    .map(|r| Ok((Sku::try_new(r.sku)?, Cents::new(r.cents))))
    .collect::<Result<BTreeMap<_, _>, PriceError>>()?;
// Option short-circuits identically on None.
let all_ids: Option<Vec<UserId>> = names.iter().map(|n| directory.get(n).copied()).collect();
// Fallible fold, and Sum<Result<U, E>> for Result<T, E> — same short-circuit, no loop.
let total = lines.iter().try_fold(Cents::ZERO, |a, l| a.checked_add(l.total()).ok_or(Overflow))?;
// `Sum<Result<Cents, E>> for Result<Cents, E>` comes free once `Cents: Sum` — note the mapped
// function is the FALLIBLE one; the infallible `line_total()` just `.sum()`s to `Cents`.
let total: Cents = lines.iter().map(|l| l.line_total_checked()).sum::<Result<Cents, PriceError>>()?;

// itertools 0.15.0: consume as a plain iterator of T while keeping the first Err.
use itertools::Itertools;
let max = itertools::process_results(rows.iter().map(Row::parse), |it| it.map(|r| r.n).max())?;
let count = rows.iter().map(Row::parse).fold_ok(0usize, |acc, _| acc + 1)?;
```

Async traverse: `stream::iter(..).map(..).buffer_unordered(n).try_collect()`. `futures::future::try_join_all` (a **free function** in `futures::future`, not a `TryFutureExt` method) does the same thing *unbounded*, which is why `scaffold.md` bans it by path — use it only for a small, fixed, code-controlled set, with a narrow `#[allow]`. Never over a caller-supplied collection. See `concurrency.md` §1.

## Fail-fast vs error accumulation

`?` and `collect::<Result<_, _>>()` fail fast. A user filling in a form wants **all** the errors at once.

```rust
// itertools partition_map + Either: successes and failures in one pass, no short-circuit.
use itertools::{Either, Itertools};
let (ok, errors): (Vec<Line>, Vec<LineError>) = raw.into_iter().enumerate()
    .partition_map(|(i, r)| match Line::try_from(r) {
        Ok(line) => Either::Left(line),
        Err(e)   => Either::Right(LineError::at(i, e)),
    });
if !errors.is_empty() { return Err(ImportError::Rejected { errors }); }

// garde 0.23.0: derive field validation reporting every failing path at once.
#[derive(garde::Validate)]
pub struct RawSignup {
    #[garde(email)]                      pub email: String,
    #[garde(length(chars, min = 12))]    pub password: String,
    #[garde(range(min = 18, max = 130))] pub age: u8,
}
// garde::Validate::validate(&raw)? -> Err(garde::Report) listing every violation with its path.
```

Applicative options: `validated` 1.0.0 (`Validated<T, E>` with `map2`/`map3`/`map4` and `FromIterator`, so `.collect::<Validated<Vec<_>, E>>()` accumulates) and `frunk` 0.5.0's `frunk::validated::Validated` (HList-based, combines heterogeneous types). **Honest note:** both are small, low-traffic crates, and a hand-rolled `Vec<FieldError>` accumulator is usually the better call — reach for them only when combining many independent validations of *different* types.

Rule of thumb: **accumulate at input boundaries and in batch jobs; fail fast inside sequential business workflows.**

## Retry belongs on the error track

Retry is a decision made *about* an error, so the error must carry the decision.

```rust
impl GatewayError {
    pub fn retriable(&self) -> bool { matches!(self, Self::Timeout | Self::Upstream5xx { .. }) }
}
// backon: exponential backoff + jitter, gated on the predicate, ALWAYS inside a timeout.
use backon::{ExponentialBuilder, Retryable};
let fut = (|| async { gateway.charge(&req).await })
    .retry(ExponentialBuilder::default().with_jitter().with_max_times(4))
    .when(|e: &GatewayError| e.retriable());
let charged = tokio::time::timeout(Duration::from_secs(10), fut).await??;
```

`tower::retry` (tower 0.5.3) does the same as a `Layer` when the call is already a `Service`. Retry without a timeout amplifies outages; retry without an idempotency key double-charges. See `production.md` for budgets and circuit breaking, and `sagas.md` for what to do when retry finally gives up.

## `Infallible`, and no custom railway type

```rust
use std::convert::Infallible;

impl TryFrom<Cents> for i64 {                     // provably total, still on the railway
    type Error = Infallible;
    fn try_from(c: Cents) -> Result<Self, Infallible> { Ok(c.get()) }
}
impl From<Infallible> for CheckoutError {         // lets generic `?` code compile
    fn from(x: Infallible) -> Self { match x {} } // total: no variants to match
}
```

`Result<T, Infallible>` is zero-cost — the compiler knows the `Err` arm is dead. The never type `!` is not stable in type position, so use `Infallible`. If a function genuinely cannot fail, return `T`; use `Result<T, Infallible>` only to satisfy a trait signature.

`try_trait_v2` — implementing `Try` so `?` works on *your* type — is **nightly only** (tracking #84277, flagged with design concerns, no stabilization path). Do not design around a home-grown `Railway<T, E>` or any HKT-emulation crate. `Result` and `Option` are the only types with `?` support; model everything as one of them and get syntax, `From` conversion, `FromIterator`, and the whole std ecosystem for free.

## Checklist

- [ ] Every fallible function returns `Result<T, E>` with a named `thiserror` enum — no `Box<dyn Error>`, no `String`, no `anyhow` outside `main.rs`
- [ ] One error enum per layer/module; no god-enum
- [ ] Variants carry structured data (ids, offending values, `retriable`), not prose
- [ ] Composition uses `?`; combinators only for short, sync, local transformations
- [ ] No duplicate `#[from]` source types — explicit `.map_err` with context, or a newtype
- [ ] Infra errors (`sqlx::Error`, `reqwest::Error`) translated at the repository/client boundary, absent from workflow signatures
- [ ] Expected-vs-defect decided per failure mode; every `expect` message proves an invariant
- [ ] Panic policy chosen (`abort` vs unwind) and a panic boundary installed for servers (`CatchPanicLayer`)
- [ ] Domain-error → HTTP status / exit-code mapping lives in exactly one exhaustive `match`
- [ ] Collections traversed with `collect::<Result<_, _>>()` / `try_fold` — no manual push loops
- [ ] Input boundaries accumulate all errors (`partition_map`, `garde`); workflows fail fast
- [ ] Retry predicated on a `retriable` field and always wrapped in a timeout
