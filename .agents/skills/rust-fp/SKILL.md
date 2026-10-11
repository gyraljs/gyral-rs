---
name: rust-fp-skill
description: Build production-grade Rust applications using functional programming and railway-oriented error handling throughout. Use whenever creating or modifying a Rust app, service, CLI, library, or workspace. Enforces typed error tracks, parse-don't-validate boundaries, illegal-states-unrepresentable domain modeling, trait-based dependency injection, and property-based testing.
---

# Functional Rust — Railway-Oriented, Type-Driven

You build every Rust application as a **pure functional core wrapped in a thin effectful shell**, with every failure travelling on a typed error track. The design philosophy comes from Scott Wlaschin's F# work (railway-oriented programming, "Designing with Types", "Parse, don't validate", functional core / imperative shell) and Alexis King's *Parse, Don't Validate* — Rust's `Result`, enums, ownership, and trait system are its native realization.

**Target Rust 1.88+ on edition 2024** (APIs herein verified 2026-07). Edition 2024 is required: let-chains are edition-gated, and the 2024 capture/lifetime rules are assumed throughout.

## Philosophy

1. **Railway-oriented programming.** Every operation that can fail returns `Result<T, E>` with a named, structured `E`. Errors are values on a typed track, never panics. `?` is the switch that shunts to the error track. Compose the happy path; handle failures where you have context to act.
2. **Make illegal states unrepresentable.** Newtypes for every domain primitive, enums for every "or", typestates for every lifecycle. If it compiles, it should be valid. `Option<T>` for absence — never a sentinel, never a nullable field pair.
3. **Parse, don't validate.** Untrusted data is decoded exactly once at each boundary (HTTP, DB, env, CLI, queue, file) into rich domain types via `TryFrom`. The core only ever sees types that are already correct. A domain type that can be constructed from raw input without validation is a bug.
4. **Functional core, imperative shell.** The domain is pure functions over immutable data, in its own crate with no async runtime, no database, no web framework. Cargo enforces the dependency direction — it is not a convention, it is a compile error.
5. **Totality.** Every function handles every input in its type. No partial functions, no `unwrap` in domain code, no wildcard match arms over domain enums, no silent defaults.

## Hard rules (non-negotiable)

Every rule below has a mechanical enforcer — see `references/scaffold.md` for the `[workspace.lints]` and `clippy.toml` that make them CI-failing rather than aspirational.

- **The `domain` crate is a leaf.** Its `Cargo.toml` contains no `tokio`, `sqlx`, `axum`, `reqwest`, `chrono`-with-clock, or `rand`. Serde is allowed for boundary types only. It should compile with `--no-default-features` on a machine with no network.
- **No `unwrap`, `expect`, `panic!`, `todo!`, `unreachable!`, or slice indexing in `domain/` or `app/`.** Denied by clippy. `expect` is permitted only at startup wiring and in tests, and must carry a proof: `expect("invariant: seeded at migration")`.
- **Every fallible function returns `Result<T, E>` with a named error enum**, defined with `thiserror`. Never `Box<dyn Error>`, never `anyhow::Error`, never `String` as an error in library or domain code. `anyhow`/`eyre` are permitted **only** in `main.rs` and build scripts.
- **No naked primitives across domain function boundaries.** `String`, `i64`, `u32` become newtypes with private inner fields and fallible constructors (`nutype`, or hand-written `TryFrom` + `parse`). Money is integer minor units in a `Cents`-style newtype — never a float.
- **Decode at every boundary with `TryFrom`.** Domain types do not derive `Deserialize` directly; they derive `#[serde(try_from = "RawX")]` so serde structurally cannot produce an unvalidated value. Never `as`-cast or transmute external data into domain types.
- **Sum types, not flag soup.** Multiple `Option` fields where exactly one should be set is an enum. Boolean pairs encoding a state machine (`is_paid`, `is_shipped`) are an enum. Lifecycle stages are separate types or a generic typestate.
- **Exhaustive matching.** No `_ => ...` catch-all over a domain enum — adding a variant must break the build. Reserve `#[non_exhaustive]` for types you deliberately want downstream users *unable* to match exhaustively, and document why.
- **Time, randomness, and IDs are injected.** No `SystemTime::now()`, `Utc::now()`, `rand::random()`, or `Uuid::new_v4()` in domain or workflow code — take a `&dyn Clock` / `&dyn IdGen`, or take the value as a parameter. Banned by path in `clippy.toml`.
- **Immutability by default.** `let` over `let mut`; return new values rather than mutating arguments. Local `mut` inside a function whose signature is pure is fine and idiomatic — contained mutation is not a violation.
- **`#![forbid(unsafe_code)]`** in every crate unless there is a written justification in the crate docs.
- **Infrastructure errors are translated at the service boundary.** `sqlx::Error` must never appear in a workflow or handler signature. Repositories map it to a domain error or treat it as a defect.
- **Fakes, not mocks.** Test doubles are hand-written `impl` blocks of your own traits. No `mockall` in the default path.

## Decision table

| Situation | Reach for |
|---|---|
| Fallible operation | `Result<T, E>` with a `thiserror` enum; `?` to compose |
| Absence of a value | `Option<T>` |
| Domain error type | `#[derive(thiserror::Error)] enum XError` — one per layer/module, not one god-enum |
| Error needing an attached context trail | `error-stack` `Report<C>`, or `snafu` context selectors |
| Error surfaced to a human on a terminal | `miette` diagnostics |
| Error in `main` only | `anyhow::Result` (nowhere else) |
| Constrained primitive / ID | newtype with private field — `nutype` for validators + sanitizers |
| Money | integer minor units newtype, or `rust_decimal::Decimal`. Never `f64` |
| State machine / variants | `enum` + exhaustive `match` |
| Lifecycle stage (unvalidated → validated → paid) | distinct structs, or one struct generic over a `PhantomData` marker |
| Boundary data (JSON body, DB row, env, argv) | a `Raw*` DTO + `TryFrom` + `#[serde(try_from = "...")]` |
| Applying a fallible fn over a collection | `.collect::<Result<Vec<_>, E>>()` — the traverse; short-circuits |
| Collecting *all* failures instead of the first | `partition_map` / `process_results` (itertools), or `validated` / `frunk::Validated` |
| Accumulating field validation for a form | `garde` (or hand-rolled `Vec<FieldError>`) |
| Folding fallibly | `try_fold`, `sum::<Result<_, _>>()` |
| Dependency (DB, clock, gateway) | a trait + generic parameter (static) or `Arc<dyn Trait>` (dynamic) |
| Async method in a trait, static dispatch | native `async fn` in trait (AFIT) + `trait-variant` for `Send` bounds |
| Async method in a trait, `dyn` object | `#[async_trait]` — AFIT is still not dyn-compatible |
| Configuration & secrets | `figment`/`envy` into a typed `Config` struct at startup; `secrecy::SecretString` for secrets |
| Retry / backoff | `tower::retry` / `backon`, with a predicate on a `retriable` field of your error |
| Bounded concurrency over a collection | `futures::stream::iter(..).map(..).buffer_unordered(n)` — always bounded |
| Structured background tasks | `tokio::task::JoinSet` + `CancellationToken` |
| Resource cleanup | RAII guard with `Drop` (sync only — `Drop` cannot await) |
| Multi-service rollback | explicit compensation stack on the error path — see `references/sagas.md` |
| Cross-cutting HTTP concerns | `tower` layers — `Service` is itself a railway function |
| Shared immutable state with history | `imbl` (NOT `im` — archived). Try `Arc::make_mut` / `Cow` first |
| Unbounded / streaming data | `futures::Stream` / `tokio_stream` |
| Two-track early exit in a loop | `std::ops::ControlFlow` |
| A function that provably cannot fail | `Result<T, Infallible>` or just `T` |

## Anti-patterns — never do these

```rust
// ❌ panic!/unwrap/expect in domain code   → return a typed error
// ❌ fn parse(s: &str) -> Product          → fn parse(s: &str) -> Result<Product, ParseError>
// ❌ -> Result<T, Box<dyn Error>>          → a named thiserror enum
// ❌ -> Result<T, String>                  → errors are data, not prose
// ❌ anyhow::Result in a library/domain    → anyhow only in main.rs
// ❌ struct Order { email: String }        → struct Order { email: Email }  (newtype)
// ❌ #[derive(Deserialize)] on a domain type → #[serde(try_from = "RawOrder")]
// ❌ price: f64                            → price_cents: Cents  (integer minor units)
// ❌ is_paid: bool, is_shipped: bool       → enum OrderState { Pending, Paid{..}, Shipped{..} }
// ❌ card: Option<String>, paypal: Option<String> → enum PaymentMethod { Card(..), PayPal(..) }
// ❌ match state { .. _ => {} }            → exhaustive arms; a new variant must break the build
// ❌ Utc::now() inside a workflow          → clock.now(), injected
// ❌ let mut v = vec![]; for x in xs { v.push(f(x)?) }  → xs.iter().map(f).collect::<Result<Vec<_>,_>>()?
// ❌ for x in xs { total += x.price }      → xs.iter().map(|x| x.price).sum()
// ❌ fn sum(xs: &[i32]) -> i32 { recurse } → iterator fold; Rust has no guaranteed TCO
// ❌ sqlx::Error in a workflow signature   → translated to a domain error in the repository
// ❌ async fn in Drop / spawn in Drop      → Drop cannot await; use an explicit compensation stack
// ❌ Arc<Mutex<AppState>> threaded everywhere → own the state in a task, talk to it over a channel
// ❌ mockall in the default test path      → hand-written fake impls of your traits
// ❌ .clone() to escape the borrow checker → restructure ownership, or Arc/Cow deliberately
```

## Workflow for building an app

1. **Scaffold** (`references/scaffold.md`): workspace with crate-per-layer, edition 2024, `[workspace.lints]`, `clippy.toml` disallowed methods, `forbid(unsafe_code)`. The dependency direction is set here and cargo enforces it forever.
2. **Model the domain first** (`references/domain-types.md`): newtypes, enums, typestates. Write the types before any logic — wrong states should fail to compile.
3. **Define the error taxonomy** (`references/rop-errors.md`): one `thiserror` enum per layer; decide expected-error vs defect (panic) per failure mode.
4. **Define the boundary decoders** (`references/boundaries.md`): `Raw*` DTOs and `TryFrom` for every place untrusted data enters. Do this before wiring any I/O.
5. **Define service traits** the workflows need (`references/di-context.md`) — interfaces first, implementations later.
6. **Write workflows** as `Result`-returning functions over domain types and service traits. Pure decisions in pure functions; `async` only for I/O. Transaction boundaries live here, never in repositories.
7. **Implement infrastructure** (`references/database.md`, `references/concurrency.md`): repositories, HTTP clients, adapters. Wire everything in exactly one place in `main.rs`.
8. **Test** (`references/testing.md`): pure functions with `proptest`, workflows with fake service impls, atomicity/rollback with real transactions. Error tracks are API surface — test them.
9. **Production-harden** (`references/production.md`): `#[instrument(err)]`, OpenTelemetry, tower resilience layers, graceful shutdown, `cargo-deny`. Non-optional — see the checklist there.
10. **Self-review** (`references/code-review.md`): run the full review pass before declaring the work done. Mandatory.

## Reference files — read before working in each area

| File | Read when |
|---|---|
| `references/scaffold.md` | Starting a project: workspace layout, lint tiers, clippy.toml, toolchain, CI |
| `references/domain-types.md` | Modeling the domain: newtypes, nutype, enums, typestates, PhantomData, money & time |
| `references/rop-errors.md` | Designing error types, `?` and `From`, panic-vs-error, traversing collections, accumulation |
| `references/boundaries.md` | Any place untrusted data enters: serde `try_from`, DTOs, clap, env, DB rows |
| `references/pattern-matching.md` | Any branching over enums: exhaustiveness, let-else, let-chains, `#[non_exhaustive]` |
| `references/di-context.md` | Dependency injection: traits vs generics vs `dyn`, AFIT, async-trait, axum state |
| `references/database.md` | Persistence: sqlx, repositories, transaction threading, row→domain mapping, migrations |
| `references/concurrency.md` | Async: bounded concurrency, JoinSet, cancellation safety, channels, actors, shutdown |
| `references/sagas.md` | Multi-service workflows needing rollback; why async Drop doesn't work |
| `references/testing.md` | Any test writing: proptest, state-machine tests, fakes, cargo-mutants, integration |
| `references/production.md` | Observability, resilience, containerization, supply chain, deployment checklist |
| `references/app-shapes.md` | Starting an HTTP API, CLI, library, or WASM frontend |
| `references/code-review.md` | ALWAYS, at the end of every task — the mandatory self-review pass |

## What does NOT map from other FP languages

Be honest about this rather than emulating it badly:

- **No higher-kinded types.** You cannot write a generic `Monad`/`Functor` trait that abstracts over `Result`, `Option`, and `Future`. Do not try, and do not pull in `fp-core` or `higher` — both are abandoned. Write concrete code per type.
- **No custom `?`.** The `Try` trait (`try_trait_v2`) is nightly-only with no stabilization path, so `Result` and `Option` are the only railway types with syntax support. Design around them rather than inventing your own.
- **No effect polymorphism.** The keyword-generics initiative is dormant. Sync and async are separate colours and you will sometimes write a function twice. There is no `Effect<A, E, R>` equivalent: you get `E` free via `Result`, but requirements (`R`) are plain generics or trait objects, and there is no `Layer`.
- **No guaranteed tail calls.** `become` is nightly. Deep recursion overflows the stack — use iterators, or an explicit worklist/stack for tree walks.
- **`Drop` cannot be async.** RAII cleanup that needs to await (releasing a distributed lock, compensating a remote call) does not work. `AsyncDrop` is nightly. See `references/sagas.md`.
- **What you get in exchange:** ownership makes aliasing bugs impossible without immutability, enums + exhaustiveness are checked by the compiler rather than a linter, cargo enforces architecture boundaries, and zero-cost abstractions mean typestates and newtypes compile to nothing.

## Canonical style

```rust
use crate::domain::{Cents, Order, OrderId, Quantity};

// Pure core: total, no I/O, no clock, trivially testable and property-testable.
pub fn order_total(lines: &[OrderLine]) -> Cents {
    lines.iter().map(OrderLine::line_total).sum()
}

// Boundary: parse, don't validate. Raw shape in, domain type out, once.
impl TryFrom<RawCheckout> for CheckoutCommand {
    type Error = CheckoutParseError;

    fn try_from(raw: RawCheckout) -> Result<Self, Self::Error> {
        Ok(Self {
            // `?` is the railway switch; From converts each step's error into ours.
            product_id: ProductId::try_new(raw.product_id)?,
            quantity: Quantity::try_new(raw.quantity)?,
        })
    }
}

// Workflow: the railway. Effects injected, transaction boundary owned here,
// every failure a named variant of one error enum.
#[tracing::instrument(skip(db, clock), err)]
pub async fn checkout(
    db: &Db,
    clock: &dyn Clock,
    user_id: UserId,
) -> Result<Order, CheckoutError> {
    let mut tx = db.begin().await?;

    let lines = cart_lines(&mut *tx, user_id).await?;
    let lines = NonEmpty::try_from(lines).map_err(|_| CheckoutError::CartEmpty)?;

    // Traverse: the first reservation failure shunts the whole thing to the red track,
    // and dropping `tx` unreserved rolls back everything already done.
    for line in &lines {
        reserve_stock(&mut *tx, line).await?;
    }

    let order = Order::place(OrderId::new(), user_id, lines, clock.now());
    insert_order(&mut *tx, &order).await?;
    clear_cart(&mut *tx, user_id).await?;

    tx.commit().await?;
    Ok(order)
}
```
