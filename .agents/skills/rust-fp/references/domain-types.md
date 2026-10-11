# Designing with Types

From Wlaschin's [Designing with Types](https://fsharpforfunandprofit.com/series/designing-with-types/): the type system is the first line of defence. **Model the types before writing any logic.** In Rust, newtypes, enums, ownership and `PhantomData` compile to nothing — illegal-states-unrepresentable is free at runtime.

Running example throughout the references: an ecommerce catalog / cart / order domain.

## Newtypes — no naked primitives in the domain

A newtype is a tuple struct with a **private** field, a **fallible** constructor, and read-only accessors. Passing a `ProductId` where a `CustomerId` goes must not compile.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Sku(String); // private: outside this module the only way in is `parse`

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SkuError {
    #[error("sku must be 3-32 chars, got {0}")] Length(usize),
    #[error("sku must be uppercase alphanumeric or '-'")] Charset,
}

impl Sku {
    pub fn parse(raw: &str) -> Result<Self, SkuError> {
        let s = raw.trim();
        if !(3..=32).contains(&s.len()) { return Err(SkuError::Length(s.len())); }
        if !s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-') {
            return Err(SkuError::Charset);
        }
        Ok(Self(s.to_owned()))
    }
    pub fn as_str(&self) -> &str { &self.0 }
    pub fn into_inner(self) -> String { self.0 }
}
```

`as_str` for borrowing, `into_inner` for giving ownership back at the boundary. Never expose the field, never add `pub fn new(s: String) -> Self`, never `impl From<String>`.

**Naming, one convention per codebase.** `nutype` generates `try_new` + `into_inner`, so the other references spell fallible constructors `try_new` and accessors `as_str()` / `as_uuid()` / `into_inner()` (and `get()` for `Copy` numeric newtypes like `Cents`). A hand-rolled `parse` is equally fine — just do not ship both names for the same idea.

### `nutype` 0.7 — the same thing, mechanically

```rust
use nutype::nutype;

#[nutype(
    sanitize(trim, lowercase),
    validate(len_char_min = 3, len_char_max = 254, regex = r"^[^@\s]+@[^@\s]+\.[^@\s]+$"),
    derive(Debug, Clone, PartialEq, Eq, Hash, AsRef, Display, Serialize, Deserialize, TryFrom),
)]
pub struct Email(String);

#[nutype(validate(greater_or_equal = 1, less_or_equal = 999), derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord))]
pub struct Quantity(u32);

let email = Email::try_new(" Ada@Example.COM ")?; // -> Email("ada@example.com"); into_inner() gives the String back
```

The point is not brevity. **`nutype` moves the inner field into a module you cannot name, so `try_new` is the only constructor that exists** — sanitizers always run, validators always run, and no future edit can add a bypass. Validated types get `try_new` (never `new`) and a generated `EmailError` enum. Requires the `regex` feature for `regex = ...`. Prefer hand-rolled when the error type needs rich structure or the invariant is cross-field.

**The one sanctioned `Deserialize` on a domain type.** `boundaries.md` bans `#[derive(Deserialize)]` on domain types because the derived impl skips the constructor. `nutype`'s generated `Deserialize` does not: it deserializes the inner primitive and then routes it through `try_new`, so an invalid value is a deserialization error, not a constructed type. Listing `Deserialize` in a `#[nutype(... derive(...))]` block is therefore safe — a plain `#[derive(serde::Deserialize)]` on the same struct never is.

### Mechanical impls: `derive_more` 2.1.1 and `bon` 3.9.3

`derive_more` writes the boring impls — `Display`, `AsRef`, `Deref`, `Add`, `Sum`, `Constructor`, `Into`. Never derive `From<Inner>` on a *validated* type: that is the bypass you removed.

`bon` for structs with many fields or optional/defaulted fields — a `#[builder]` gives compile-time-checked required fields instead of a `..Default::default()` hole:

```rust
#[derive(Debug, Clone, bon::Builder)]
pub struct ProductListing {
    pub sku: Sku, pub title: Title, pub price: Cents,
    #[builder(default)] pub tags: Vec<Tag>,
    pub discontinued_at: Option<DateTime<Utc>>, // Option fields become optional builder args
}
```

## Algebraic data types

| Shape | Rust | Read as |
|---|---|---|
| Product ("and") | `struct` | every field present simultaneously |
| Sum ("or") | `enum` | exactly one variant, carrying only its own data |

**Anti-pattern: optional-field soup.** Three `Option`s where exactly one should be set is 8 states, 7 illegal.

```rust
// ❌ 8 representable states, 1 legal
pub struct Payment { card: Option<String>, paypal: Option<String>, gift_card: Option<String> }

// ✅ 3 states, 3 legal — and each carries the data only it needs
pub enum PaymentMethod {
    Card { last4: Last4, network: CardNetwork },
    PayPal { account: Email },
    GiftCard { code: GiftCardCode, remaining: Cents },
}
```

**Anti-pattern: boolean flag pairs.** `is_paid`/`is_shipped` is 4 states; "shipped but not paid" is one of them.

```rust
// ✅ illegal combinations cannot be written down
pub enum OrderStatus {
    Placed { placed_at: DateTime<Utc> },
    Paid { placed_at: DateTime<Utc>, receipt: ReceiptId },
    Shipped { receipt: ReceiptId, tracking: TrackingCode, shipped_at: DateTime<Utc> },
    Cancelled { reason: CancellationReason, cancelled_at: DateTime<Utc> },
}
```

## Typestates — lifecycle stages the borrow checker enforces

### (a) Distinct structs, transitions that consume `self`

```rust
pub struct DraftOrder { customer: CustomerId, lines: Vec<CartLine> }
pub struct ValidatedOrder { customer: CustomerId, lines: NonEmpty<OrderLine> }
pub struct PricedOrder { customer: CustomerId, lines: NonEmpty<OrderLine>, total: Cents }

impl DraftOrder {
    // `self` by value: the draft is MOVED. There is no draft left to re-validate.
    pub fn validate(self, catalog: &dyn Catalog) -> Result<ValidatedOrder, ValidationError> { /* … */ }
}
impl ValidatedOrder {
    pub fn price(self, prices: &PriceList) -> Result<PricedOrder, PricingError> { /* … */ }
}
```

Because `validate` takes `self`, the compiler makes double-validation and stage-skipping *unwriteable*: there is no `DraftOrder` value left after the call, and no path from `DraftOrder` to `PricedOrder`. Fields differ per stage — `ValidatedOrder` has `NonEmpty<OrderLine>`, `DraftOrder` does not.

### (b) One struct generic over a marker (`PhantomData`)

```rust
use std::marker::PhantomData;

pub struct Draft;
pub struct Validated;
pub struct Paid;

struct OrderData { id: OrderId, customer: CustomerId, lines: NonEmpty<OrderLine>, total: Cents }

pub struct Order<S> { data: OrderData, _state: PhantomData<fn() -> S> }

impl Order<Draft> {
    pub fn validate(self) -> Result<Order<Validated>, ValidationError> {
        // …checks…
        Ok(Order { data: self.data, _state: PhantomData }) // one field moved, not N
    }
}
impl Order<Validated> {
    pub fn pay(self, receipt: ReceiptId) -> Order<Paid> { /* … */ }
}
impl<S> Order<S> {
    pub fn id(&self) -> OrderId { self.data.id } // shared behaviour written once
}
```

`PhantomData<fn() -> S>` (rather than `PhantomData<S>`) keeps `Order<S>: Send + Sync` independent of the marker.

| | Separate structs | `PhantomData` marker |
|---|---|---|
| Fields may differ per state | ✅ | ❌ all states share one shape |
| Shared methods | duplicated or via trait | one `impl<S>` block |
| Transition cost | list every field | move the inner `data` |
| Readability of the model | highest — the type *is* the stage | one type, states in the turbofish |
| Best for | few fields, genuinely different data | many fields, mostly-identical stages |

Constrain the marker with a **sealed trait** so downstream crates cannot invent states:

```rust
mod sealed { pub trait Sealed {} }
pub trait OrderState: sealed::Sealed { const LABEL: &'static str; }
impl sealed::Sealed for Draft {}
impl OrderState for Draft { const LABEL: &'static str = "draft"; }
```

Sealed traits are also how you keep a public trait extensible-by-you-only. `#[non_exhaustive]` is the analogous escape hatch for enums and structs — it buys additive evolution at the cost of downstream exhaustiveness; see `pattern-matching.md` before reaching for it.

## Non-empty collections

An order with zero lines should not typecheck.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonEmpty<T> { head: T, tail: Vec<T> }

#[derive(Debug, thiserror::Error)]
#[error("expected at least one element")]
pub struct Empty;

impl<T> TryFrom<Vec<T>> for NonEmpty<T> {
    type Error = Empty;
    fn try_from(mut v: Vec<T>) -> Result<Self, Self::Error> {
        let tail = if v.is_empty() { return Err(Empty) } else { v.split_off(1) };
        Ok(Self { head: v.pop().ok_or(Empty)?, tail }) // no indexing, no unwrap
    }
}

impl<T> NonEmpty<T> {
    pub fn first(&self) -> &T { &self.head }
    pub fn len(&self) -> NonZeroUsize { NonZeroUsize::MIN.saturating_add(self.tail.len()) }
    pub fn iter(&self) -> impl Iterator<Item = &T> { std::iter::once(&self.head).chain(&self.tail) }
}
```

`first()` returns `&T`, not `Option<&T>` — that is the whole point. The `nonempty` and `vec1` crates provide this if you prefer a dependency; either way the domain signature is `lines: NonEmpty<OrderLine>`, never `Vec<OrderLine>` plus a runtime check. Same idea for bounded numbers (`Quantity` above) and uniqueness (`BTreeSet<Sku>` in the type, not `assert` in five call sites).

**Prefer `BTreeMap`/`BTreeSet` over `HashMap`/`HashSet` inside `domain`.** Ordered iteration makes hashing-order nondeterminism impossible, so a shrunk `proptest` counterexample reproduces and snapshots do not churn; `scaffold.md` can enforce it with a `disallowed-types` entry. `HashMap` stays fine in `infra`, adapters, and test fixtures where you never iterate for output — but never assert on its iteration order (`testing.md`).

## Money

Never `f64`. Clippy's `float_arithmetic` (restriction) is denied in the workspace lints precisely so this is a build failure, not a code-review comment.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Cents(i64); // signed: refunds and adjustments are negative

impl Cents {
    pub const ZERO: Self = Self(0);
    pub const fn new(minor_units: i64) -> Self { Self(minor_units) }
    pub const fn get(self) -> i64 { self.0 }
    pub fn checked_add(self, rhs: Self) -> Option<Self> { self.0.checked_add(rhs.0).map(Self) }
    pub fn checked_sub(self, rhs: Self) -> Option<Self> { self.0.checked_sub(rhs.0).map(Self) }
    pub fn checked_mul(self, q: Quantity) -> Option<Self> { i64::from(q.into_inner()).checked_mul(self.0).map(Self) }
}

// So `.sum()` works on iterators of Cents, per the canonical `order_total` in SKILL.md.
impl std::iter::Sum for Cents {
    fn sum<I: Iterator<Item = Self>>(it: I) -> Self { Self(it.fold(0i64, |a, c| a.saturating_add(c.0))) }
}
impl<'a> std::iter::Sum<&'a Cents> for Cents {
    fn sum<I: Iterator<Item = &'a Self>>(it: I) -> Self { it.copied().sum() }
}
```

`i64` minor units carries ±9.2×10¹⁸ units of headroom, so `Sum` saturating rather than panicking is a defensible total function; use `try_fold` with `checked_add` where inputs are attacker-controlled and unbounded. Reach for `rust_decimal::Decimal` (1.42.1) instead when you need **fractional minor units** (unit prices, tax rates, per-unit costs) or exact decimal rounding semantics — never `f64`, and never mix the two representations in one type.

Currency belongs in the type, not in a comment: `struct Money { amount: Cents, currency: Currency }` with arithmetic returning `Result<Money, CurrencyMismatch>`.

## Time

- **`chrono` is the safe public-API default.** `jiff` (0.2.34) has the better model but is **still pre-1.0** — if it appears in a public signature, say so in the crate docs and accept semver churn. Internal use is fine.
- **Model instants as UTC**: `DateTime<Utc>`. Local time and time zones are presentation concerns, converted at the edge.
- **Time never comes from `now()` inside the domain.** It is a parameter, or it comes from an injected `Clock`. `chrono::Utc::now` is on the `clippy.toml` `disallowed-methods` list.

```rust
// ✅ pure, total, trivially property-testable
pub fn place(id: OrderId, order: PricedOrder, now: DateTime<Utc>) -> PlacedOrder { /* … */ }

// domain-crate trait; the impl lives in infrastructure
pub trait Clock: Send + Sync { fn now(&self) -> DateTime<Utc>; }
```

Durations are `chrono::Duration` or a newtype (`RetentionDays`), never a bare `i64` "seconds, probably".

## Commands in, events out

Model a workflow as `Command -> Result<NonEmpty<Event>, Error>`. The workflow **decides**; the shell **acts** on the events. Tests read "given this command, exactly these events", and adding a subscriber (email, analytics, outbox) is an edit at the edge, not inside the domain.

```rust
pub struct PlaceOrder { pub customer: CustomerId, pub lines: NonEmpty<OrderLine>, pub method: PaymentMethod }

pub enum OrderEvent { // facts, past tense, carrying what subscribers need
    OrderPlaced { order_id: OrderId, total: Cents, at: DateTime<Utc> },
    PaymentTaken { order_id: OrderId, receipt: ReceiptId, amount: Cents },
    StockLow { sku: Sku, remaining: Quantity },
}

pub fn place_order(cmd: PlaceOrder, prices: &PriceList, now: DateTime<Utc>)
    -> Result<NonEmpty<OrderEvent>, PlaceOrderError>;
```

Events crossing a process boundary (queue, outbox table) are boundary types — `Raw*` DTO + `TryFrom`, per `boundaries.md`. Replaying events to rebuild state is a total `(State, Event) -> State` fold — see `pattern-matching.md`.

## Checklist

- [ ] Zero naked `String`/`i64`/`u32`/`bool` in domain signatures — every primitive is a newtype
- [ ] Every newtype has a private field and a fallible constructor; no `pub` field, no `From<String>` on a validated type
- [ ] `nutype` used where the invariant is expressible as sanitize + validate; hand-rolled where the error needs structure
- [ ] No optional-field soup — "exactly one of" is an `enum`
- [ ] No boolean pairs encoding a state machine — lifecycle is an `enum`, distinct structs, or a `PhantomData` typestate
- [ ] Stage transitions take `self` by value so the previous stage is gone from memory
- [ ] Typestate markers behind a sealed trait; `#[non_exhaustive]` used only across a published crate boundary
- [ ] "At least one" is `NonEmpty<T>`; bounded numbers are constrained newtypes; uniqueness is in the type
- [ ] Money is integer minor units (or `rust_decimal::Decimal`) with currency in the type — zero `f64`
- [ ] Time is `DateTime<Utc>`, passed in as a parameter or via `Clock`; no `now()` in domain code
- [ ] Workflows are `Command -> Result<NonEmpty<Event>, Error>`; side effects happen at the edge
- [ ] Types written, reviewed, and compiling before any workflow logic exists
