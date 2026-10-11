# Dependency Injection — Traits, Context Structs, One Wiring Site

Rust has **no Reader monad and no effect-requirements channel**. Do not build one: a `Reader<'a, R, A>` drowns in lifetimes the moment it meets `async`, and there is no HKT to make it compose. The substitute is boring and better: **every effect is a trait, the domain depends on trait definitions only, and exactly one file names an implementation.**

## 1. The dependency-inversion arrow

**The trait lives with the consumer; the impl lives in `infra`.** This is what keeps `domain` a leaf and `app` free of `sqlx`/`reqwest`.

```
crates/app/src/ports.rs    trait OrderRepo, Clock, IdGen, PaymentGateway  ← definitions
crates/infra/src/pg.rs     impl OrderRepo for PgOrderRepo                 ← implementations
crates/server/src/main.rs  Arc::new(PgOrderRepo::new(pool))               ← the only naming site
```

`app` never lists `infra` in its `Cargo.toml`. The edge points `infra → app`, so reaching for a database inside a workflow is a resolution error, not a review note (`references/scaffold.md`). Port methods return **domain** errors — `sqlx::Error` is translated inside the adapter and never appears in a port signature.

```rust
// crates/app/src/ports.rs
pub trait Clock: Send + Sync { fn now(&self) -> Timestamp; }
pub trait IdGen: Send + Sync { fn order_id(&self) -> OrderId; }

#[async_trait::async_trait]
pub trait OrderRepo: Send + Sync {
    async fn find(&self, id: OrderId) -> Result<Option<Order>, RepoError>;
    async fn insert(&self, order: &Order) -> Result<(), RepoError>;
}
```

## 2. Three dispatch strategies

| Strategy | Dispatch | Use when | Honest cost |
|---|---|---|---|
| (a) generic param `fn f<R: OrderRepo>(repo: &R)` | static, inlinable | hot CPU paths, library APIs, leaf helpers | monomorphization bloat, slower compiles, **generics leak into every caller's signature up the stack** |
| (b) `&dyn Trait` / `Arc<dyn Trait>` | vtable | **default for I/O-bound services** | one indirection — noise beside a 400µs round-trip. Signatures stay clean; swappable at runtime |
| (c) context struct of `Arc<dyn ...>` | vtable | a workflow needing 3+ effects | one parameter instead of five; the practical Reader substitute |

Rule of thumb: **if the function awaits I/O, use `dyn`** — `fn price_cart<C: Clock>(cart: &Cart, clock: &C)` is fine as a leaf, but `async fn checkout(repo: &dyn OrderRepo, clock: &dyn Clock)` keeps `<R, C, I, P>` out of every enclosing signature at no measurable cost.

### (c) Context struct + capability traits

Passing one `&Ctx` everywhere works, but then every function claims *all* capabilities. Refine with **capability traits** so a signature declares only what it needs — the closest Rust gets to a typed requirements channel:

```rust
#[derive(Clone)]
pub struct Ctx {
    pub repo: Arc<dyn OrderRepo>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGen>,
    pub payments: Arc<dyn PaymentGateway>,
}

pub trait HasClock     { fn clock(&self) -> &dyn Clock; }
pub trait HasOrderRepo { fn order_repo(&self) -> &dyn OrderRepo; }

impl HasClock     for Ctx { fn clock(&self) -> &dyn Clock { self.clock.as_ref() } }
impl HasOrderRepo for Ctx { fn order_repo(&self) -> &dyn OrderRepo { self.repo.as_ref() } }

// Reads as "requires Clock and OrderRepo"; a fixture supplying only those two compiles.
// One capability trait per port, never per method.
pub async fn place_order(
    ctx: &(impl HasClock + HasOrderRepo),
    cmd: PlaceOrder,
) -> Result<Order, CheckoutError> {
    let order = Order::place(cmd.customer, cmd.lines, ctx.clock().now());
    ctx.order_repo().insert(&order).await?;
    Ok(order)
}
```

## 3. Async in traits — the decision rule

| You need | Use | Why |
|---|---|---|
| static dispatch, generic param | **AFIT** — plain `async fn` in the trait (stable **1.75**) | zero boxing, no macro |
| static dispatch **and** a `Send` future (you `tokio::spawn` it) | **`trait-variant` 0.1.3** (official `rust-lang/impl-trait-utils`) | AFIT's returned future carries no `Send` bound, and you cannot add one by hand |
| `dyn Trait` / `Arc<dyn Trait>` | **`#[async_trait]` 0.1.91** | **AFIT is not dyn-compatible.** `async_trait` boxes the future so the method fits a vtable |

```rust
// Static + Send: declare the Local flavour, get the Send flavour generated.
#[trait_variant::make(StockService: Send)]
pub trait LocalStockService {
    async fn reserve(&self, id: ProductId, qty: Quantity) -> Result<(), StockError>;
}
```

Consequence: **ports stored as `Arc<dyn ...>` must use `#[async_trait]`** — and since (b) is the default, most ports carry it. One `Box::pin` per DB call is free relative to the DB call. Every port trait needs `: Send + Sync` so `Arc<dyn Port>` can cross a `tokio::spawn`.

## 4. Clock, IdGen, Rng — the highest-value injection

Non-determinism is why suites go flaky and why bugs are "not reproducible". Injecting the three sources of it is the best return on DI in this file.

```rust
// crates/infra/src/clock.rs
pub struct SystemClock;
impl Clock for SystemClock {
    #[allow(clippy::disallowed_methods, reason = "the clock adapter is the one permitted caller")]
    fn now(&self) -> Timestamp { Timestamp::from(chrono::Utc::now()) }
}

// crates/app/src/testing.rs — fakes, not mocks
pub struct FixedClock(pub Timestamp);
impl Clock for FixedClock { fn now(&self) -> Timestamp { self.0 } }

pub struct TestClock(Mutex<Timestamp>);   // advances only when the test calls .advance()
impl Clock for TestClock {
    fn now(&self) -> Timestamp { *self.0.lock().expect("test clock poisoned") }
}
```

Enforced, not suggested: `clippy.toml` bans `chrono::Utc::now`, `std::time::SystemTime::now`, `rand::random` and `uuid::Uuid::new_v4` **by path** across the workspace. The adapters above are the only legal callers, each carrying an inline `#[allow(..., reason = ...)]`.

## 5. Configuration — parsed once, at the top

Config is a boundary, and `boundaries.md` owns the pattern: a flat `RawConfig` of primitives derives `Deserialize`, `TryFrom<RawConfig> for Config` produces the typed value, and the extraction happens once in `main.rs`. A bad env var is a **startup failure with a precise message**, never a 3am `unwrap` on first use.

```rust
// crates/server/src/config.rs — the DI-relevant half: the shape the Ctx is built from.
pub struct Config {
    pub port: Port,                           // newtype, not u16
    pub database_url: secrecy::SecretString,  // Debug renders "[REDACTED]" — cannot leak into a log line
    pub stripe_key: secrecy::SecretString,
    pub environment: Environment,             // enum Development | Staging | Production
    pub pool_max: PoolSize,
}
// impl TryFrom<RawConfig> for Config { .. }  ← the parser lives here; see boundaries.md
```

`figment` when config comes from several sources, `envy` when env is the only one. What matters for DI: **no `std::env::var` below `main.rs`**; secrets stay `SecretString` and are `expose_secret()`-ed at the exact call that needs them; config travels down as plain data (or slices of it) into the `Ctx` and is never re-read.

## 6. The single wiring site

`main.rs` is the composition root — the only file that mentions a concrete implementation, and the only *production* file allowed `anyhow` and `expect` (tests and `#[cfg(feature = "testing")]` fakes may `expect` too; every one still states its invariant).

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init()?;
    let config = Config::try_from(RawConfig::load()?)?; // fail fast, before anything binds
    let pool = PgPoolOptions::new()
        .max_connections(config.pool_max.get())
        .connect(config.database_url.expose_secret()).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let ctx = Ctx {                                     // every concrete type appears exactly here
        repo: Arc::new(PgOrderRepo::new(pool.clone())),
        clock: Arc::new(SystemClock),
        ids: Arc::new(UuidGen),
        payments: Arc::new(StripeGateway::new(&config.stripe_key)), // borrow: SecretString is not Clone
    };
    api::serve(ctx, config.port).await
}
// A `use infra::...` in any other file means the onion has been breached.
```

## 7. axum: state and per-request capabilities

`AppState` holds the `Ctx`; `FromRequestParts` derives per-request context. A handler needing authentication **names `CurrentUser` in its signature**, so forgetting auth is a compile error rather than a forgotten middleware registration:

```rust
#[derive(Clone)]
pub struct AppState { pub ctx: Ctx }        // Clone is cheap: Arc bumps only

async fn place_order_handler(
    State(state): State<AppState>,
    user: CurrentUser,                      // <- auth is in the type signature
    Json(raw): Json<RawPlaceOrder>,
) -> Result<Json<OrderView>, ApiError> {
    let cmd = PlaceOrder::try_from((user.0, raw))?;      // parse at the boundary
    Ok(Json(app::place_order(&state.ctx, cmd).await?.into()))
}

pub struct CurrentUser(pub UserId);

impl<S: Send + Sync> FromRequestParts<S> for CurrentUser where AppState: FromRef<S> {
    type Rejection = ApiError;

    // axum 0.8 uses native AFIT here — no #[async_trait].
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = AppState::from_ref(state);
        let token = parts.headers.get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(ApiError::Unauthenticated)?;
        Ok(Self(state.ctx.tokens.verify(token).await?))
    }
}
```

Same trick for `Idempotency-Key`, tenant resolution, and request-scoped trace ids.

## 8. Test wiring — fakes, never mocks

**No `mockall` in the default path** — hand-written fakes of your own ports, because a fake asserts *behavior* while a mock asserts *call sequences*. `testing.md` owns the full rationale, the fake-writing patterns, and fault injection; what belongs here is the payoff for DI: **a `Ctx` of `Arc<dyn Port>` is swapped wholesale in one function.**

```rust
// Fakes live in crates/app/src/testing.rs behind #[cfg(any(test, feature = "testing"))]
// so every test crate shares one implementation — see testing.md.
fn test_ctx() -> Ctx {
    Ctx {
        repo: Arc::new(InMemoryOrderRepo::default()),
        clock: Arc::new(FixedClock(Timestamp::from_secs(1_700_000_000))),
        ids: Arc::new(SeqIdGen::default()),
        payments: Arc::new(AlwaysDeclines),   // one fake per failure mode you exercise
    }
}
```

Assert on resulting state and on the `Result` variant, never on which methods were called (`references/testing.md`).

## Checklist

- [ ] Every effect (DB, HTTP, clock, ids, random, queue, filesystem, flags) sits behind a trait
- [ ] Traits defined in `app`/`domain`, impls only in `infra`; `app/Cargo.toml` names no infra crate
- [ ] Port methods return domain errors — no `sqlx::Error`/`reqwest::Error` in a port signature
- [ ] Every port trait is `Send + Sync`; `dyn` ports use `#[async_trait]`, static+spawned use `trait-variant`
- [ ] `dyn` is the default for I/O-bound services; generics only where they don't leak upward
- [ ] 3+ dependencies in a workflow → a `Ctx` struct, with capability traits so signatures state their needs
- [ ] `Clock`, `IdGen`, `Rng` injected; `clippy.toml` bans `Utc::now`/`new_v4`/`random` by path
- [ ] One typed `Config` parsed at startup via `TryFrom<RawConfig>` (`boundaries.md`); secrets are `SecretString`; zero `std::env` reads below `main.rs`
- [ ] `main.rs` is the only file naming a concrete implementation
- [ ] Auth and tenancy arrive via `FromRequestParts` extractors so handlers cannot forget them
- [ ] Test doubles are hand-written fakes enforcing real invariants; no `mockall`
