# Boundaries — Parse, Don't Validate

Alexis King, [*Parse, Don't Validate*](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/) (2019): a **validator** inspects data and throws the knowledge away; a **parser** consumes untrusted input and returns a value whose *type* carries the proof. Validation has to be repeated defensively everywhere; parsing happens **once, at the edge**, and the core never checks again because the check is no longer expressible.

Rust makes this mechanical: `TryFrom` is the parser, the newtypes and enums from `domain-types.md` are the proof, and serde attributes make bypassing the parser *structurally impossible*.

> **The rule: zero `as` casts, zero `unwrap`/`expect` on external data, exactly one decode per boundary.**

## The mechanism: `#[serde(try_from = "...")]`

A domain type **must not** `#[derive(Deserialize)]`. Deriving `Deserialize` on `Order` means serde can materialize an `Order` from any JSON with the right field names — the constructor invariants are simply gone. Instead: a `Raw*` DTO of plain primitives owns `Deserialize`, and the domain type is reachable only through `TryFrom`. (Single exception: a `Deserialize` listed inside a `#[nutype(... derive(...))]` block, which serde-decodes the inner primitive and then calls `try_new` — see `domain-types.md`.)

```rust
// ---------- 1. the wire shape: primitives only, no invariants ----------
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RawOrder {
    pub customer_id: String,
    pub lines: Vec<RawLine>,
    pub placed_at: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RawLine { pub sku: String, pub quantity: u32, pub unit_price_cents: i64 }

// ---------- 2. the parser: one place, total, typed error ----------
#[derive(Debug, thiserror::Error)]
pub enum OrderParseError {
    #[error("customer id: {0}")] CustomerId(#[from] CustomerIdError),
    #[error("line {index}: {source}")] Line { index: usize, #[source] source: LineParseError },
    #[error("order must have at least one line")] NoLines,
    #[error("placedAt: {0}")] PlacedAt(#[from] chrono::ParseError),
}

impl TryFrom<RawOrder> for Order {
    type Error = OrderParseError;
    fn try_from(raw: RawOrder) -> Result<Self, Self::Error> {
        let lines = raw.lines.into_iter().enumerate()
            .map(|(index, l)| OrderLine::try_from(l).map_err(|source| OrderParseError::Line { index, source }))
            .collect::<Result<Vec<_>, _>>()?;          // traverse: short-circuits on the first bad line
        Ok(Self {
            customer_id: CustomerId::parse(&raw.customer_id)?,
            lines: NonEmpty::try_from(lines).map_err(|_| OrderParseError::NoLines)?,
            placed_at: raw.placed_at.parse::<DateTime<Utc>>()?,
        })
    }
}

// ---------- 3. wire the parser into serde ----------
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(try_from = "RawOrder", into = "RawOrder")]
pub struct Order { customer_id: CustomerId, lines: NonEmpty<OrderLine>, placed_at: DateTime<Utc> }

impl From<Order> for RawOrder { fn from(o: Order) -> Self { /* … */ } } // required by `into`
```

- `try_from = "RawOrder"` makes `Order`'s `Deserialize` impl *literally* "deserialize a `RawOrder`, then run `TryFrom`". There is no other code path. Any format serde supports (JSON, msgpack, form, query string) inherits the parse for free. `Self::Error` must be `Display`.
- `into = "RawOrder"` is the encode direction and requires `Order: Clone + Into<RawOrder>`. Use it so the wire shape stays a deliberate, separately-evolvable type rather than a leak of your private fields.
- `deny_unknown_fields` on every inbound DTO: an unexpected field means the client and you disagree, and silence is the wrong answer.
- Keep DTOs `pub(crate)` or in a `wire` module. They are not domain types and must never appear in a workflow signature.

## The four boundaries — same pattern at each

| Boundary | DTO | Parser | Failure is |
|---|---|---|---|
| HTTP body / query | `RawOrder` + `Deserialize` | `#[serde(try_from)]` | a 400 with a field path |
| Database row | `OrderRow` + `sqlx::FromRow` | `TryFrom<OrderRow>` | a defect (your own data is corrupt) |
| CLI argv | clap `Args` struct | `value_parser` | clap usage error, exit 2 |
| Env / config file | `RawConfig` + `Deserialize` | `TryFrom<RawConfig>` | a **startup** failure, never runtime |

### HTTP — axum 0.8.9

```rust
// The handler's argument is the DOMAIN type. Serde ran TryFrom before the body reached you.
async fn place_order(State(app): State<AppState>, Json(order): Json<Order>) -> Result<Json<OrderView>, ApiError> {
    let placed = workflows::place_order(&app.repo, app.clock.as_ref(), order).await?;
    Ok(Json(OrderView::from(placed)))
}
```

`Json<T>` requires only `T: DeserializeOwned`, so a `try_from` type slots straight in. Map the rejection so parse failures become your typed error rather than axum's default text:

```rust
#[derive(axum::extract::FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct Body<T>(pub T);

impl From<axum::extract::rejection::JsonRejection> for ApiError {
    fn from(r: JsonRejection) -> Self { ApiError::BadRequest { detail: r.body_text() } }
}
```

Then take `Body<Order>` instead of `Json<Order>`. Response types are their own DTOs (`OrderView`), built with `From<Order>` — never serialize domain types directly.

### Database rows — the anti-corruption layer

The row shape and the domain shape do not match, and should not. A row is flat, nullable, and stringly-typed; the domain is nested, non-empty, and typestated. **Do not try to make sqlx build your domain types directly** — there is no `FromRow` impl that can produce a `PricedOrder` from three joined tables without lying, and `#[derive(FromRow)]` on a domain struct reintroduces exactly the bypass that `try_from` removed.

```rust
#[derive(Debug, sqlx::FromRow)]           // sqlx 0.9.0 — flat, primitives, nullable
struct OrderRow {
    id: Uuid, customer_id: Uuid, status: String,
    total_cents: i64, placed_at: DateTime<Utc>, tracking_code: Option<String>,
}

impl TryFrom<OrderRow> for Order {
    type Error = RowDecodeError;              // corrupt-database errors, distinct from user-input errors
    fn try_from(r: OrderRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: OrderId::from(r.id),
            customer_id: CustomerId::from(r.customer_id),
            total: Cents::new(r.total_cents),
            status: parse_status(&r.status, r.tracking_code)?,   // string column -> enum, exhaustively
            placed_at: r.placed_at,
        })
    }
}
```

The repository owns rows and `TryFrom`; the workflow only ever sees `Order`. A `RowDecodeError` means an invariant your migrations were supposed to guarantee has been violated — log it loudly and translate it to a domain error at the repository boundary (`sqlx::Error` must never escape into a workflow signature).

For a single-field newtype, let sqlx handle the column type directly:

```rust
#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(transparent)]                       // valid ONLY on a struct with exactly one field
pub struct OrderId(Uuid);
```

Newtypes over `String` with invariants still need a `TryFrom<String>` step — `transparent` skips validation by design.

### CLI / argv

clap's derive **is** parse-don't-validate at the argv boundary: `value_parser` runs your parser, and the struct field's type is the proof.

```rust
#[derive(clap::Parser, Debug)]
#[command(version, about)]
struct Cli {
    #[arg(long, value_parser = Sku::parse)]                 // fn(&str) -> Result<Sku, SkuError>
    sku: Sku,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=999))]
    quantity: u32,
    #[arg(long, value_enum, default_value_t = Format::Json)] // enum, not a free string
    format: Format,
}
```

Anything the flags cannot express (cross-field rules) goes in one `TryFrom<Cli> for Command` immediately after `Cli::parse()` in `main`. No `String` argument ever reaches a workflow.

### Env / config

```rust
#[derive(serde::Deserialize)]                 // figment or envy, once, at startup
struct RawConfig { database_url: String, port: u16, stripe_key: String, request_timeout_ms: u64 }

pub struct Config {
    pub database_url: DatabaseUrl,
    pub port: Port,
    pub stripe_key: secrecy::SecretString,    // never String: no Debug leak, zeroized on drop
    pub request_timeout: Duration,
}

impl TryFrom<RawConfig> for Config { type Error = ConfigError; /* … */ }

// main.rs — the only place config is read, and the only place `?` may end the process
let cfg: Config = Figment::new()
    .merge(Toml::file("app.toml"))
    .merge(Env::prefixed("APP_"))
    .extract::<RawConfig>()?
    .try_into()?;
```

**Config errors are startup failures, not runtime errors.** Anything that fails to parse must abort before the server binds a port — no `Option<Config>`, no lazy statics that panic on first use, no re-reading env deeper in the stack. Pass the typed `Config` (or slices of it) explicitly to the components that need it.

## Sum types over the wire

| serde representation | JSON | Use when |
|---|---|---|
| Externally tagged (default) | `{"Card": {"last4": "4242"}}` | internal Rust-to-Rust; not idiomatic JSON |
| Internally tagged `#[serde(tag = "type")]` | `{"type":"card","last4":"4242"}` | **default choice** for public APIs; variants must be structs/maps |
| Adjacently tagged `#[serde(tag = "type", content = "data")]` | `{"type":"card","data":{…}}` | variants carry newtypes/tuples/primitives |
| Untagged `#[serde(untagged)]` | `{"last4":"4242"}` | last resort — matching a schema you do not control |

```rust
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RawPaymentMethod { Card { last4: String, network: String }, PayPal { account: String }, GiftCard { code: String } }
```

**`#[serde(untagged)]` produces terrible errors.** Serde tries each variant, discards every failure, and reports only `data did not match any variant of untagged enum X` — the actual reason the intended variant failed is thrown away. Always prefer a tag. If untagged is forced on you, keep the enum tiny and write the discriminating parse by hand in `TryFrom`.

Storing rich enums in Postgres: `jsonb` column + `sqlx::types::Json<RawPaymentMethod>` on the row struct, then `TryFrom` into the domain enum. Prefer this to a `text` discriminator plus a fan of nullable columns — the nullable fan is optional-field soup with extra steps. Index with `jsonb_path_ops` if you query into it. For a closed, rarely-changing set, a Postgres `ENUM` + `#[derive(sqlx::Type)] #[sqlx(type_name = "...", rename_all = "snake_case")]` is tighter; adding a variant then requires a migration, which is a feature.

## Round-trip is the contract

Every boundary type owes a property test: **`decode(encode(x)) == x`** for all `x` in the domain type, and `encode(decode(raw))` stable for all valid `raw`. This is the single highest-value property test in the codebase because it catches every field you renamed, dropped, or truncated on one side only.

```rust
proptest! {
    #[test]
    fn order_json_round_trips(order in arb_order()) {
        let json = serde_json::to_string(&order)?;
        prop_assert_eq!(serde_json::from_str::<Order>(&json)?, order);
    }
}
```

Pair it with a decode-rejects-garbage test per boundary (`deny_unknown_fields`, out-of-range numbers, empty line arrays) and snapshot the *error messages* — a 400 body is API surface. See `testing.md`.

## Checklist

- [ ] No domain type derives `Deserialize` directly — every one uses `#[serde(try_from = "RawX")]`
- [ ] Every inbound DTO has `deny_unknown_fields`; DTOs are `pub(crate)` and never appear in workflow signatures
- [ ] Encode direction is an explicit `into = "RawX"` / dedicated view type, not serialized domain internals
- [ ] Exactly one decode per boundary; the core never re-validates
- [ ] DB rows are flat `*Row` structs with `TryFrom` into the domain — no `FromRow` on a domain type
- [ ] `#[sqlx(transparent)]` only on single-field newtypes whose inner type has no invariants
- [ ] clap `value_parser` does the parsing; cross-field rules in one `TryFrom<Cli> for Command`
- [ ] Config parsed once at startup into a typed struct; secrets are `secrecy::SecretString`; failures abort before serving
- [ ] Wire enums are tagged (internally or adjacently); `untagged` justified in a comment if present
- [ ] Zero `as` casts on external data — `i64::try_from(..)?`, `u32::try_from(..)?`
- [ ] Zero `unwrap`/`expect` in any decode path
- [ ] Every boundary type has a `decode(encode(x)) == x` property test and a rejects-garbage test
