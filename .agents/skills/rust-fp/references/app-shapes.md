# App Shapes — HTTP API, CLI, Library, WASM

The onion is identical in all four shapes: `domain` (leaf) → `app` (workflows + port traits) → `infra` (adapters) → an outer shell. **Only the shell changes.** Everything below is the shell; `references/scaffold.md` builds the rest.

## Which shape

| You are building | Shape | Shell crate | Key crates |
|---|---|---|---|
| A service other programs call over HTTP | **HTTP API** | `crates/api` + `crates/server` | axum 0.8.9, tower-http 0.7.0, utoipa 5.5.0 |
| A tool a human runs in a terminal | **CLI** | `crates/cli` | clap (derive), miette 7.6.0, indicatif |
| Code other crates depend on | **Library** | the crate itself is the surface | thiserror 2.0.19, `cargo-semver-checks` |
| A browser UI sharing your domain rules | **WASM / full-stack** | `crates/web` (cdylib) | wasm-bindgen, leptos/dioxus/yew |

Two shapes over one core is normal and cheap: a CLI and an HTTP API are two adapters over the same `app` crate. Never fork the workflow.

## HTTP API — axum 0.8

```
crates/
  domain/ app/ infra/          # unchanged across shapes
  api/     # router, extractors, ApiError → IntoResponse, OpenAPI annotations
  server/  # main.rs: config, tracing, pool, Ctx assembly. THE ONLY WIRING SITE.
```

**Router composition.** One `router()` per resource group, nested at the top. Groups are `Router<AppState>`; `with_state` is applied exactly once, at the root.

```rust
pub fn router(state: AppState) -> Router {
    Router::new()
        .nest("/orders", orders::router())
        .nest("/products", products::router())
        .layer(ServiceBuilder::new()              // ServiceBuilder applies outermost-first
            .layer(TraceLayer::new_for_http())
            .layer(TimeoutLayer::new(Duration::from_secs(10)))
            .layer(RequestBodyLimitLayer::new(64 * 1024)))
        .with_state(state)
}
mod orders {
    pub fn router() -> Router<AppState> {
        Router::new()
            .route("/", post(place_order))
            .route("/{id}", get(get_order))   // axum 0.8 path syntax: braces, NOT `:id`
    }
}
```

**Extractors are the parse-don't-validate boundary.** The handler's argument types decide what is reachable. A command type deserializes only via its raw DTO, so an invalid `PlaceOrder` cannot exist inside the handler body:

```rust
#[derive(serde::Deserialize)]
#[serde(try_from = "RawPlaceOrder")]      // serde structurally cannot skip validation
pub struct PlaceOrder { pub sku: Sku, pub qty: Quantity }
async fn place_order(
    State(st): State<AppState>,
    user: CurrentUser,                     // FromRequestParts — see references/di-context.md
    AppJson(cmd): AppJson<PlaceOrder>,     // already-valid domain command
) -> Result<(StatusCode, Json<OrderView>), ApiError> {
    let order = app::place_order(&st.ctx, user.0, cmd).await?;
    Ok((StatusCode::CREATED, Json(OrderView::from(order))))
}
```

**Custom rejection.** The stock `Json` extractor returns axum's plain-text `JsonRejection`, which breaks your error contract for exactly the requests most likely to be malformed. Wrap it once:

```rust
#[derive(axum::extract::FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct AppJson<T>(pub T);
impl From<JsonRejection> for ApiError {
    fn from(r: JsonRejection) -> Self { ApiError::Malformed { detail: r.body_text() } }
}
```

**One `IntoResponse` for the whole API**, in `crates/api`, and nowhere else. Status selection is an exhaustive `match` on `ApiError`, so a new error variant fails the build until you decide its status; infrastructure variants log in full and return an opaque 500 with a trace id. The domain-error → `ApiError` translation chain is in `rop-errors.md`; the response body shape, trace-id handling, and leak rules are in `production.md` §3. Do not write a second mapping in a handler.

**OpenAPI — utoipa 5.5.0 + `utoipa-axum`** (mainstream; `aide` 0.15.1 is the smaller alternative). `utoipa-axum` registers route and schema together, so the spec cannot drift from the router:

```rust
#[utoipa::path(post, path = "/orders", request_body = PlaceOrder,
    responses((status = 201, body = OrderView), (status = 409, body = ProblemDetails)))]
async fn place_order(/* … */) { }
let (router, spec) = OpenApiRouter::with_openapi(ApiDoc::openapi())
    .nest("/orders", OpenApiRouter::new().routes(routes!(place_order, get_order)))
    .split_for_parts();
```

Snapshot the generated spec with `insta` so a contract change shows up as a test diff (`references/testing.md`).

## CLI — clap derive

```
crates/cli/     # thin adapter: argv → domain command → app::workflow → render
```

The subcommand enum **is** the command surface as a sum type; adding a variant without handling it is a compile error.

```rust
#[derive(Parser)]
#[command(name = "shopctl", version, about)]
struct Cli {
    #[arg(long, global = true, env = "SHOP_API")] api: Url,
    #[command(subcommand)] command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Place an order for a single SKU.
    Order { #[arg(value_parser = parse_sku)] sku: Sku, #[arg(default_value = "1")] qty: Quantity },
    /// Show stock levels.
    Stock { #[arg(value_parser = parse_sku)] sku: Sku },
}
// clap accepts any error that is Display + Send + Sync + 'static — keep the typed one,
// never collapse it to String (SKILL.md: errors are data, not prose).
fn parse_sku(s: &str) -> Result<Sku, SkuError> { Sku::try_new(s) }
```

`value_parser` does the parse-don't-validate work **at the argv boundary**, so `run` receives `Sku` and `Quantity`, never `String`. (Implementing `FromStr` on the newtype lets clap infer the parser; be explicit when the error message matters.) Enum-valued flags get `#[derive(ValueEnum)]`.

`main` returns through `Termination`. Use `fn main() -> ExitCode` and match the error enum when exit statuses are part of the contract (`Err(CliError::NotFound(_)) => ExitCode::from(4)`) — that match is the same railway, exhaustively checked. Use `fn main() -> Result<(), anyhow::Error>` (1.0.104, **binaries only**) when every failure is just "print and exit 1". `Cli::parse()` already exits 2 on bad usage.

`miette` 7.6.0 renders human-facing diagnostics with source spans — the right shape when the CLI parses a file the user wrote:

```rust
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("invalid SKU in manifest")]
#[diagnostic(code(shopctl::bad_sku), help("SKUs look like ABC-1234"))]
struct BadSku { #[source_code] src: NamedSource<String>, #[label("this line")] span: SourceSpan }
```

`indicatif` for progress bars and spinners; wire it in the CLI crate only — a workflow that knows about a progress bar is no longer testable without one. **The CLI crate is a thin adapter over the same `app` crate the HTTP API uses.** If a `match` arm contains business logic, it belongs in `app`.

## Library crate

You are publishing a type surface, and every part of it is semver. Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).

```rust
#![forbid(unsafe_code)]
#![deny(missing_docs, rustdoc::broken_intra_doc_links)]
/// Errors returned when parsing a SKU.
///
/// ```
/// # use mylib::Sku;
/// assert!("ABC-1234".parse::<Sku>().is_ok());     // rustdoc compiles this as a doctest
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]                                   // adding a variant stays a minor bump …
pub enum ParseError {
    #[error("input was empty")] Empty,
    #[error("invalid character at byte {index}")] BadChar { index: usize },
}
mod sealed { pub trait Sealed {} }
/// Implemented only by the currencies this crate ships.
pub trait Currency: sealed::Sealed { const CODE: &'static str; }
#[must_use = "OrderBuilder does nothing until `.build()` is called"]
pub struct OrderBuilder { /* … */ }
```

| Rule | Why |
|---|---|
| No `anyhow`/`eyre`/`Box<dyn Error>` in the public API | callers cannot match on an opaque error; named enums are the contract |
| `#[non_exhaustive]` on public error enums | new variants stay minor bumps — **but downstream loses exhaustive matching** and must write `_ =>`, which is exactly what SKILL.md forbids internally. Deliberate trade, documented (`references/pattern-matching.md`) |
| Sealed traits for "implemented only here" | lets you add methods without a breaking change |
| Features **additive only** | features are unioned across the dependency graph; a `no-std` or `disable-x` feature breaks unrelated crates |
| `rust-version` (MSRV) declared in `Cargo.toml` | raising it is a semver-relevant, announced change; CI must build on it |
| `#[must_use]` on builders; doc examples on every public item | a dropped builder is always a bug; doctests are documentation that cannot rot |

CI gates: `cargo semver-checks` (blocks accidental breakage), `cargo public-api --diff` (review every surface change in the PR), `cargo doc --no-deps -D warnings`, and property tests over the public surface (`references/testing.md`) that double as executable documentation.

## WASM / full-stack

**The headline: one `domain` crate compiled to both server and browser.** Because `domain` is a leaf with no tokio, sqlx, or axum (SKILL.md's first hard rule), it already builds for `wasm32-unknown-unknown` with no changes. The same `Sku::try_new`, the same `Quantity` bounds, and the same money arithmetic run in the form field and in the request handler — one implementation, verified by one property suite. Sharing validation rules across the wire is something other stacks spend real effort duplicating or code-generating.

Layout adds one crate — `crates/web`, a cdylib holding the wasm-bindgen glue and UI, alongside the existing `domain/ app/ infra/ api/ server/`; `domain` is simply compiled twice, once natively and once for `wasm32-unknown-unknown`. `crates/web/Cargo.toml` declares `[lib] crate-type = ["cdylib", "rlib"]`, and the glue is trivial because the rule already lives in `domain`:

```rust
use wasm_bindgen::prelude::*;
/// Returns the validation message, or `None` when the input is a valid SKU.
#[wasm_bindgen]
pub fn check_sku(input: &str) -> Option<String> {
    domain::Sku::try_new(input).err().map(|e| e.to_string())
}
```

Caveats, honestly: `wasm32-unknown-unknown` has no threads and no tokio runtime, so nothing from `infra` crosses over; `getrandom` needs its wasm backend enabled explicitly if any dependency pulls it in; check that `chrono`/`uuid` features you enable are wasm-compatible. Keeping `domain` dependency-free is what makes all of this a non-event.

Frontend options — pick on team familiarity, not benchmarks. **leptos** (fine-grained signals, mature SSR + hydration story), **dioxus** (React-like components, targets web/desktop/mobile), **yew** (older, stable, more verbose component model). All three are real and shipping; all three have far smaller ecosystems than JS frameworks and move faster than 1.0-stable crates. A JS frontend consuming your OpenAPI spec is an entirely legitimate alternative — compile `domain` to wasm purely for client-side validation and keep the rendering in whatever your team knows.

## Checklist

- [ ] The shell crate contains no business logic — every handler/subcommand is parse → workflow → render
- [ ] `main.rs` is the only place anything is wired; nothing else constructs a pool, client, or clock
- [ ] **HTTP**: routers nested per resource group, `with_state` applied once at the root
- [ ] **HTTP**: axum 0.8 path params use `{id}`, not `:id`; body extractors go through a wrapper whose rejection maps to `ApiError`
- [ ] **HTTP**: `IntoResponse for ApiError` matches exhaustively and leaks no internal detail on 5xx
- [ ] **HTTP**: `TraceLayer`, timeout, and body-size limits are present; OpenAPI is generated (utoipa) and snapshot-tested
- [ ] **CLI**: subcommands are an enum; `value_parser` yields domain types so `run` never sees a `String`
- [ ] **CLI**: exit codes are deliberate; diagnostics go through `miette`; progress UI stays out of `app`
- [ ] **Library**: `#![forbid(unsafe_code)]`, `deny(missing_docs)`, no `anyhow` in the public API
- [ ] **Library**: `#[non_exhaustive]` on public error enums, with its cost to downstream matching documented
- [ ] **Library**: features additive only, MSRV declared, `#[must_use]` on builders, sealed traits where appropriate
- [ ] **Library**: `cargo semver-checks` and `cargo public-api --diff` run in CI; doctests compile
- [ ] **WASM**: `domain` builds clean for `wasm32-unknown-unknown` — verified in CI, not assumed
- [ ] **WASM**: validation rules exist once, in `domain`, and are not reimplemented in the frontend
