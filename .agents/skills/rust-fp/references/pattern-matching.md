# Exhaustive Pattern Matching

`match` exhaustiveness is the compiler-checked backbone of ADT modelling. The entire value of modelling a lifecycle as an enum is this: **adding a variant must break the build in every place that inspects the type.** That build break is the feature. Never suppress it.

## Ban the catch-all over domain enums

```rust
// ❌ adding OrderStatus::Refunded compiles silently and is treated as "not shipped"
match status {
    OrderStatus::Shipped { tracking, .. } => Some(tracking),
    _ => None,
}

// ✅ adding a variant fails the build here, which is where you want to be reminded
match status {
    OrderStatus::Shipped { tracking, .. } => Some(tracking),
    OrderStatus::Placed { .. } | OrderStatus::Paid { .. } | OrderStatus::Cancelled { .. } => None,
}
```

Enforced by `clippy::wildcard_enum_match_arm` (denied in `[workspace.lints]`; see `scaffold.md`), with `clippy::match_wildcard_for_single_variants` as backup.

| Want | Write | Not |
|---|---|---|
| Ignore a variant's payload | `Variant { .. }` / `Variant(_)` | `_ =>` |
| One handler for several variants | `A { .. } \| B { .. } =>` | `_ =>` |
| Catch-all over a **non-domain** type (`u8`, `&str`, external enum) | `_ =>` is correct | — |

`_ =>` on primitives and third-party enums is fine and unavoidable. The rule is about *your* domain enums, where the build break is the design.

## `#[non_exhaustive]` — a deliberate trade-off

`#[non_exhaustive]` lets you add variants (or fields) without a semver-major bump, because downstream crates are **forced** to write a `_` arm. That is precisely the exhaustiveness you spent the whole design budget acquiring — you are trading it away for additive evolution.

| Context | Use it? |
|---|---|
| Public error enum in a **published** library, where new failure modes are expected | ✅ yes, and document why |
| Public config/options struct in a published library | ✅ yes (forces `..Default::default()` construction) |
| Anything inside your own workspace | ❌ **never** — it has no semver benefit and destroys the build break you want |
| An enum whose variant set is closed by the domain (`Currency`, `OrderStatus`) | ❌ no |

It has no effect within the defining crate, so the cost is invisible in your own tests and lands entirely on your users. Do not sprinkle it.

## Matching ergonomics

```rust
// matches! — a boolean question, not a branch
let is_terminal = matches!(status, OrderStatus::Shipped { .. } | OrderStatus::Cancelled { .. });

// binding @ — keep the whole value while constraining it
match qty.into_inner() {
    n @ 1..=9 => Tier::Small(n),
    n @ 10..=99 => Tier::Bulk(n),
    n => Tier::Wholesale(n),
}

// guards — refine, but note guards are NOT exhaustiveness-checked; keep a real arm behind them
match (&line.sku, line.quantity) {
    (sku, q) if catalog.is_restricted(sku) && q.into_inner() > 1 => Err(LineError::RestrictedQuantity),
    (_, _) => Ok(()),
}

// nested destructuring + or-patterns in nested position
// (Cents' field is private, so constrain it with a guard rather than a tuple-struct pattern)
match event {
    OrderEvent::PaymentTaken { amount, .. } if amount == Cents::ZERO => Err(PaymentError::ZeroAmount),
    OrderEvent::OrderPlaced { total, .. } | OrderEvent::PaymentTaken { amount: total, .. } => Ok(total),
    OrderEvent::StockLow { .. } => Ok(Cents::ZERO),
}

// slice patterns — total handling of a Vec/&[T] without any indexing
match lines {
    [] => Err(CartError::Empty),
    [only] => Ok(Summary::Single(only.sku.clone())),
    [first, .., last] => Ok(Summary::Range(first.sku.clone(), last.sku.clone())),
}
```

Slice patterns are how you obey the no-indexing rule: `lines[0]` panics, `[first, ..]` cannot.

**Binding modes.** Matching on a reference makes bindings references automatically (`match &order.status { OrderStatus::Shipped { tracking, .. } => …}` gives `tracking: &TrackingCode`). Edition 2024 tightened the rules on mixing an explicit `ref`/`ref mut` with an inherited binding mode; if the compiler objects, drop the `ref` or destructure through an explicit `&` pattern rather than fighting it.

## `let ... else` (stable 1.65) — early exit on the error track

The railway idiom for "extract or bail", without a rightward-drifting `match`:

```rust
pub fn apply_coupon(cart: &Cart, code: &CouponCode, coupons: &CouponBook)
    -> Result<Cents, CouponError>
{
    let Some(coupon) = coupons.find(code) else {
        return Err(CouponError::Unknown { code: code.clone() });
    };
    let CouponKind::Percentage(pct) = coupon.kind else {
        return Err(CouponError::NotApplicable { code: code.clone() });
    };
    let Ok(subtotal) = cart.subtotal() else {
        return Err(CouponError::Overflow);
    };
    Ok(subtotal.scaled(pct))
}
```

The `else` block must diverge (`return`, `continue`, `break`, `?`-free `panic!`-free divergence). It binds in the *enclosing* scope, which is what makes it flatter than `match`. Use it whenever the failure branch is one line; use `?` when a `From` conversion already exists.

## let-chains — stable 1.88, **edition 2024 only**

```rust
// Cargo.toml: edition = "2024"   ← required; this is a compile ERROR on edition 2021
if let OrderStatus::Paid { receipt, .. } = &order.status
    && let Some(refund) = refunds.for_receipt(receipt)
    && refund.amount == order.total
{
    return Ok(Settlement::FullyRefunded);
}
```

If a chain errors with "let expressions are unstable", the crate is on edition 2021 — fix the edition, do not restructure the code. Keep chains to `if let`; anywhere else, nest or use `let ... else`.

## Combinators vs `match` vs `?`

`rop-errors.md` is the authority and its position holds here: **compose with `?`, not with combinator chains** — `?` avoids closures that capture a `&mut` you still need and produces no `and_then(|x| async move { .. })` soup. The table below is for what is left over: short, sync, local transformations.

| Situation | Reach for |
|---|---|
| Transform the Ok value | `.map(f)` |
| Chain another fallible step | `.and_then(f)` |
| `Option` → `Result` on the error track | `.ok_or(E::Missing)` / `.ok_or_else(\|\| …)` |
| Adapt a foreign error into yours | `.map_err(E::from)` — or a `#[from]` variant and plain `?` |
| Supply a total fallback | `.unwrap_or_else(f)` / `.unwrap_or_default()` |
| Propagate to the caller | **`?`** — beats every combinator; use it |
| Both branches do real work / need destructuring | `match` |
| Boolean question about a variant | `matches!` |

```rust
// ✅ combinator chain: one linear expression, no branch names to read
let email = raw.email.as_deref().map(str::trim).filter(|s| !s.is_empty())
    .ok_or(SignupError::EmailMissing)
    .and_then(|s| Email::try_new(s).map_err(SignupError::Email))?;

// ❌ the same thing as a match pyramid — three levels of nesting, zero extra information
```

Rule of thumb: if an arm is `Ok(v) => Ok(v)` or `Err(e) => Err(e.into())`, you wanted `?`. If every arm is a one-liner mapping over one field, you wanted a combinator. Reserve `match` for genuine multi-way branching on domain meaning.

## `ControlFlow` — two-track early exit inside a fold

`std::ops::ControlFlow<B, C>` is the "keep going / stop with this answer" type, and it implements `Try`, so `try_fold` and `try_for_each` accept it directly.

```rust
use std::ops::ControlFlow;

/// Charge lines until the gift card runs out; returns what's left and where it stopped.
pub fn spend_gift_card(balance: Cents, lines: &[OrderLine]) -> (Cents, Option<usize>) {
    match lines.iter().enumerate().try_fold(balance, |left, (i, line)| {
        match left.checked_sub(line.line_total()) {
            Some(rest) => ControlFlow::Continue(rest),
            None => ControlFlow::Break((left, i)),
        }
    }) {
        ControlFlow::Continue(left) => (left, None),
        ControlFlow::Break((left, i)) => (left, Some(i)),
    }
}
```

Use it for visitors, tree walks, and loops that need to report *why* they stopped without inventing a sentinel or abusing `Result` for a non-error outcome. `Result` means failure; `ControlFlow` means "done early, successfully".

## State machines as total functions

An exhaustive `match` over the full `(State, Event)` product **is** the transition table. Write it as one total reducer and the compiler audits the table for you every time either enum grows — this is the event-sourcing fold.

```rust
// states: Placed | Paid | Shipped | Cancelled     events: OrderPlaced | PaymentTaken | Shipped | Cancelled | StockLow
pub fn apply(state: OrderStatus, event: OrderEvent) -> OrderStatus {
    use {OrderEvent as E, OrderStatus as S};
    match (state, event) {
        (S::Placed { placed_at }, E::PaymentTaken { receipt, .. }) => S::Paid { placed_at, receipt },
        (S::Paid { receipt, .. }, E::Shipped { tracking, at }) => S::Shipped { receipt, tracking, shipped_at: at },
        (S::Placed { .. } | S::Paid { .. }, E::Cancelled { reason, at }) => S::Cancelled { reason, cancelled_at: at },

        // Every no-op pair is spelled out — no `_`. A new state OR a new event fails to compile here.
        (s @ (S::Placed { .. } | S::Paid { .. }), E::OrderPlaced { .. } | E::StockLow { .. }) => s,
        (s @ S::Placed { .. }, E::Shipped { .. }) => s,
        (s @ S::Paid { .. }, E::PaymentTaken { .. }) => s,
        (s @ (S::Shipped { .. } | S::Cancelled { .. }), E::OrderPlaced { .. } | E::PaymentTaken { .. }
            | E::Shipped { .. } | E::Cancelled { .. } | E::StockLow { .. }) => s,
    }
}

let current = history.iter().cloned().fold(OrderStatus::initial(), apply);
```

`apply` is pure, total, and infallible — ideal for `proptest` (replaying any event sequence never panics; replay is idempotent for terminal states). If an impossible pair should be an error rather than a no-op, return `Result<OrderStatus, InvalidTransition>` and keep the same exhaustive shape. Guard-heavy alternatives are a smell: guards are not exhaustiveness-checked, so a table built from `if` guards silently rots.

## No custom `?`

`try_trait_v2` (implementing `Try` for your own type so `?` works on it) is **nightly only** with open design concerns and no stabilization path. Per SKILL.md, `Result` and `Option` are the only *railway* types with syntax support — `ControlFlow` rides the same machinery in `try_fold`/`try_for_each`, but it signals "finished early, successfully", not failure. Design around those three — do not invent a `Validated<T, E>` and expect `?`; give it `into_result()` instead and convert at the point of use.

## Checklist

- [ ] No `_ =>` arm over any domain enum — `clippy::wildcard_enum_match_arm` denied and clean
- [ ] Ignoring a payload uses `Variant { .. }`, not `_`
- [ ] `#[non_exhaustive]` appears only on published-library error/option types, with a documented reason — never inside the workspace
- [ ] Guards are never the only thing standing between you and a missing case
- [ ] Slice patterns instead of indexing; `[]` handled explicitly
- [ ] `let ... else` used for single-line bail-outs; `?` used wherever a `From` impl exists
- [ ] Crate is edition 2024 if let-chains appear anywhere
- [ ] `match` reserved for real multi-way branching; `map`/`and_then`/`ok_or`/`map_err` for one-line transforms
- [ ] `ControlFlow` — not `Result`, not a sentinel — for successful early exit in folds and visitors
- [ ] State transitions are one exhaustive `(State, Event)` match; a new variant breaks that build
- [ ] The reducer is property-tested over arbitrary event sequences
