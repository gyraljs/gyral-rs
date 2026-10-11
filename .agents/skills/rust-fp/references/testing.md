# Testing — Properties First, Fakes over Mocks

From [The Property-Based Testing series](https://fsharpforfunandprofit.com/series/property-based-testing/): example tests prove your code works for the cases you thought of; property tests attack the ones you didn't. The architecture — pure core, effects behind traits — is what makes both cheap. `fn order_total(&[OrderLine]) -> Cents` needs no runtime, no fixture, no fake; a workflow over `Arc<dyn OrderRepo>` needs a 20-line struct, not a framework.

Stack: `proptest` 1.11.0, `proptest-derive` 0.8.0, `proptest-state-machine` 0.8.0, `rstest` 0.26.1, `insta` 1.48.0, `wiremock` 0.6.5, `testcontainers` 0.27.3, `cargo-nextest` 0.9.140, `cargo-mutants` 27.1.0.

## The test pyramid

| Tier | Tooling | Volume |
|---|---|---|
| 1. **Pure domain functions** | plain `#[test]` + `proptest!`. No async, no fakes, no runtime | **most tests** |
| 2. **Workflows** | `#[tokio::test]` + hand-written fake trait impls + `FixedClock` | a solid layer |
| 3. **Adapters / integration** | `#[sqlx::test]`, `testcontainers`, `wiremock`, axum on port 0 | a few, deliberately |

Tier 1 is large *because* logic was pushed into the pure core. If a calculation needs a fake to test it, the calculation is in the wrong crate — move it into `domain`.

## proptest

```rust
use proptest::prelude::*;
proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]
    #[test]                                       // the inner #[test] is required
    fn total_is_order_independent(lines in prop::collection::vec(arb_line(), 1..12)) {
        let mut rev = lines.clone();
        rev.reverse();
        prop_assert_eq!(order_total(&lines), order_total(&rev));
    }
}
// prop_compose!: compose strategies the way the domain composes, so every value is
// valid BY CONSTRUCTION and nested structs get nested strategies.
prop_compose! {
    // Go through the real constructors — a strategy that bypasses them tests a type you don't ship.
    fn arb_address()(city in "[A-Za-z ]{3,20}", post in "[0-9]{5}") -> Address {
        Address::new(
            City::try_new(city).expect("strategy: regex generates 3-20 letters"),
            PostCode::try_new(post).expect("strategy: regex generates 5 digits"),
        )
    }
}
prop_compose! {
    fn arb_order()(ship in arb_address(), lines in prop::collection::vec(arb_line(), 1..8)) -> Order {
        Order::draft(ship, NonEmpty::try_from(lines).expect("strategy guarantees 1..8"))
    }
}
// proptest-derive: the opposite job — generating garbage at a boundary.
#[derive(Debug, Clone, proptest_derive::Arbitrary)]
struct RawCheckout {
    #[proptest(regex = "[a-zA-Z0-9-]{0,64}")]  product_id: String,
    #[proptest(strategy = "any::<i64>()")]     quantity:   i64,   // deliberately unconstrained
}
```

Vocabulary: ranges (`1..=100u8`, `0.0..1.0f64`), regex strings (`"[A-Z]{3}-[0-9]{4}"`), `prop::collection::vec(inner, 1..8)`, `prop::option::of(..)`, `prop::sample::select(..)`, `any::<T>()`, tuples, `.prop_map(f)`, `.prop_filter("why", pred)`, `prop_oneof![2 => a, 1 => b]`. Use `prop_assert!`/`prop_assert_eq!` inside `proptest!` — they feed the shrinker instead of unwinding.

| Goal | Tool |
|---|---|
| "does `TryFrom` panic, or accept garbage, for *any* input?" — red-track fuzzing | `#[derive(Arbitrary)]` on the `Raw*` DTO |
| green-track tests where the input must satisfy domain constraints | `prop_compose!` |

Never derive `Arbitrary` on a domain newtype — it would bypass the validated constructor. **Shrinking:** On failure proptest repeatedly simplifies the counterexample (smaller ints, shorter vecs, earlier variants) until no further reduction still fails, then reports that minimal reproducer — a 400-element failure becomes `[1, 1]`. It also appends the failing seed to `proptest-regressions/<module>.txt`, replayed first on every later run. **Commit those files.** Deleting one silently drops a known bug from CI.

## Property patterns

| Pattern | Ecommerce example |
|---|---|
| Round-trip ("there and back again") | `Order::try_from(RawOrder::from(o)) == Ok(o)`; `row_to_order(order_to_row(o)) == Ok(o)` |
| Invariants | `order_total(l) >= Cents::ZERO`; `apply_discount(t, d) <= t`; splitting a shipment preserves total quantity |
| Idempotence | `normalize_email(normalize_email(e)) == normalize_email(e)`; replaying a webhook id changes nothing |
| Commutativity | `add(add(cart, a), b) == add(add(cart, b), a)`; tax-then-round vs round-then-tax — specify it, then pin it |
| Oracle / naive model | the indexed stock lookup equals a linear scan; the incremental total equals `lines.iter().map(..).sum()` |
| Induction | `total(&[]) == ZERO`, and `total(x :: xs) == x.line_total() + total(xs)` |
| Hard to prove, easy to verify | don't recompute the allocation — assert it sums to the request and no line exceeds available stock |

Reject **tautological properties**. `prop_assert_eq!(order_total(&l), l.iter().map(|x| x.qty * x.price).sum())` tests nothing: a wrong formula is wrong on both sides. Assert a relation the implementation does not state (ordering-independence, monotonicity, a bound), or compare against a deliberately naive model.

**The minimum bar, every app:** round-trip for every boundary type in both directions where meaningful (`references/boundaries.md`); every state machine — valid sequences never reach an illegal state and every invalid transition returns its specific typed error, never a panic and never a silent no-op; every money calculation — non-negative where the domain says so, sums preserved under split/merge, no `f64` in the path; every normalize/parse/format function — idempotent **and** round-trips.

## Model-based testing — proptest-state-machine

For stateful systems (the cart/order lifecycle is the canonical case): write a trivial reference model, let proptest generate random operation sequences, assert the real system agrees at every step. It shrinks the *sequence*, yielding the shortest operation list that breaks the invariant.

```rust
use proptest_state_machine::{ReferenceStateMachine, StateMachineTest, prop_state_machine};
#[derive(Clone, Debug)] enum Op { Add(Sku, Quantity), Checkout }
type Model = BTreeMap<Sku, u32>;              // the reference: a map, ordered so shrinking replays
struct CartModel;
impl ReferenceStateMachine for CartModel {
    type State = Model;
    type Transition = Op;
    fn init_state() -> BoxedStrategy<Model> { Just(Model::new()).boxed() }
    fn transitions(s: &Model) -> BoxedStrategy<Op> {
        if s.is_empty() { arb_add().boxed() }
        else { prop_oneof![4 => arb_add(), 1 => Just(Op::Checkout)].boxed() }
    }
    fn apply(mut s: Model, t: &Op) -> Model {
        match t {
            Op::Add(sku, q) => { *s.entry(sku.clone()).or_insert(0) += q.into_inner(); s }
            Op::Checkout    => Model::new(),
        }
    }
}
impl StateMachineTest for Cart {
    type SystemUnderTest = Cart;
    type Reference = CartModel;
    fn init_test(_r: &Model) -> Cart { Cart::empty() }
    fn apply(cart: Cart, _r: &Model, t: Op) -> Cart {
        match t {
            Op::Add(sku, q) => cart.add(sku, q),
            Op::Checkout    => cart.checkout().map_or(cart, |_| Cart::empty()),
        }
    }
    fn check_invariants(cart: &Cart, r: &Model) {
        assert_eq!(cart.quantities(), *r);                // model equivalence + real invariants
        assert!(cart.total() >= Cents::ZERO);
    }
}
prop_state_machine! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]
    #[test] fn cart_lifecycle_matches_model(sequential 1..30 => Cart);
}
```

## Testing the error track — it is API surface

```rust
#[tokio::test]
async fn checkout_below_stock_reports_availability_and_leaves_no_partial_state() {
    let ctx = ctx_with(FakeStock::new([(sku("SKU-1"), 3)]), FixedClock(TS));
    let err = checkout(&ctx, cmd_for(sku("SKU-1"), 5)).await.unwrap_err();
    // `matches!`, not `assert_eq!`: error enums wrap sqlx::Error / io::Error and
    // frequently cannot — and should not — implement PartialEq.
    assert!(matches!(err, CheckoutError::InsufficientStock { available: 3, .. }), "got {err:?}");
    // THE POINT OF THE TRANSACTION BOUNDARY: the failure left nothing behind.
    assert_eq!(ctx.stock.on_hand(&sku("SKU-1")), 3);
    assert_eq!(ctx.cart.items(USER).len(), 1);
    assert!(ctx.orders.all().is_empty());
}
```

`unwrap`/`expect`/`unwrap_err` are banned in `domain` and `app` but **permitted in tests** — a panicking test is a failing test. Prefer `unwrap_err()` to `match { Ok(_) => panic!() }`. Assert on structured *fields* (`available: 3`), never on `err.to_string()` — display text is for humans and churns. Every error variant a workflow can produce needs a test; an untested `Err` arm is a path your users find first.

## Fakes, not mocks

```rust
#[derive(Default)]
pub struct FakeOrderRepo {
    orders: Mutex<HashMap<OrderId, Order>>,
    pub fail_next: Mutex<Option<RepoError>>,          // fault injection, explicitly
}
#[async_trait::async_trait]
impl OrderRepo for FakeOrderRepo {
    async fn find(&self, id: OrderId) -> Result<Option<Order>, RepoError> {
        Ok(self.orders.lock().expect("poisoned").get(&id).cloned())
    }
    async fn insert(&self, order: &Order) -> Result<(), RepoError> {
        if let Some(e) = self.fail_next.lock().expect("poisoned").take() { return Err(e); }
        self.orders.lock().expect("poisoned").insert(order.id(), order.clone());
        Ok(())
    }
}
pub struct FixedClock(pub Timestamp);
impl Clock for FixedClock { fn now(&self) -> Timestamp { self.0 } }
```

A fake is a real, simplified implementation, so tests assert **behavior** ("after checkout the repo holds one order"). A mock framework asserts **call sequences** — a test of your implementation, which fails on refactors that change nothing observable. Fakes are also reusable: put them in `crates/app/src/testing.rs` behind `#[cfg(any(test, feature = "testing"))]`. `mockall` 0.15.0 is an escape hatch only — a wide trait you don't control where a test touches two of ten methods. If you control it, split the trait.

## Determinism

Inject `Clock` and `IdGen`; clippy bans `Utc::now`/`Uuid::new_v4` by path (`references/scaffold.md`). Seed every RNG (`StdRng::seed_from_u64(42)`). Never assert on `HashMap` iteration order. **No real sleeps in tests, ever** — timeouts and retries run in virtual time:

```rust
#[tokio::test(start_paused = true)]                          // == tokio::time::pause()
async fn payment_retries_three_times_then_gives_up() {
    let task = tokio::spawn(charge_with_retry(FlakyGateway::failing(3), amount()));
    tokio::time::advance(Duration::from_secs(30)).await;      // instant: virtual time
    assert!(matches!(task.await.expect("panicked").unwrap_err(),
                     PaymentError::Unavailable { attempts: 3 }));
}
```

## Integration

`#[sqlx::test]` **provisions a fresh database per test** from `migrations/`, hands you a `PgPool`, and drops that database when the test passes. It is **not** a rolled-back transaction — writes really commit — and a **failing test deliberately leaves its database behind** for inspection. Tests are therefore isolated and safely parallel.

```rust
#[sqlx::test(fixtures("products"))]
async fn insert_then_find_round_trips(pool: PgPool) -> sqlx::Result<()> {
    let (repo, order) = (PgOrderRepo::new(pool), example_order());
    repo.insert(&order).await.expect("insert");
    assert_eq!(repo.find(order.id()).await.expect("find"), Some(order));
    Ok(())
}
```
- `testcontainers` 0.27.3 for a real Postgres outside that model, or Redis/Kafka — start it once per suite. `wiremock` 0.6.5 for every external HTTP dependency, and mount the **failure** responses (402, 500, timeout) too: nothing else exercises those branches.
- Full-stack: `TcpListener::bind("127.0.0.1:0")`, read `local_addr()`, `tokio::spawn(axum::serve(listener, router(state)))`, drive it with `reqwest` and assert on `StatusCode`. Port 0 keeps the suite parallel-safe.

## Tooling

| Tool | Use |
|---|---|
| `cargo-nextest` 0.9.140 | default runner: parallel, one process per test, so a panic or `abort` cannot poison the suite |
| `insta` 1.48.0 | snapshots — OpenAPI spec, rendered error messages, JSON responses. `cargo insta review` |
| `rstest` 0.26.1 | table-driven cases: `#[rstest] #[case("", Err(Empty))] #[case("a@b.co", Ok(..))]`, plus fixtures |
| `cargo-llvm-cov` 0.8.7 | coverage — a number, not a goal. 100% lines says nothing about the error track |

**`cargo-mutants` 27.1.0** is the real check on the error track. It rewrites your source — replacing bodies with default returns, flipping `>` to `>=`, **and turning `Ok(..)` arms into `Err(..)` and back** — then reruns the tests. A mutant that **survives** is a line no test constrains. This is how you learn that `CheckoutError::InsufficientStock` is never asserted anywhere while coverage reports 100%. Scope it to the pure crates — `cargo mutants -p domain -p app --timeout 60`; mutating infrastructure is slow and mostly re-tests the database.

## Rules

- No mocking libraries in the default path. Hand-written fakes of your own traits, shared from `crates/app/src/testing.rs`.
- Deterministic always: injected `Clock`/`IdGen`, `tokio::time::pause()` for anything timed, seeded RNG, no wall-clock sleeps, no network in tiers 1–2.
- Test names state **behavior**, not implementation: `checkout_below_stock_leaves_cart_intact`, never `calls_repo_insert_once`.
- Every error variant gets a test, and that test asserts no partial state was left behind.
- Every production bug and every shrunk counterexample becomes a pinned regression test — `proptest-regressions/` committed, plus an explicit `#[test]` for the minimal case.

## Checklist

- [ ] Most tests are tier 1: `#[test]`/`proptest!` over pure `domain` functions, no runtime and no fakes
- [ ] `proptest-regressions/` files are committed and not gitignored
- [ ] Every boundary type has a round-trip property; every state machine has a legal-sequence property
- [ ] Every money path is property-tested for non-negativity and sum preservation, no `f64` in the path; every normalize/parse function for idempotence and round-trip
- [ ] Stateful lifecycles (cart, order, subscription) have a `proptest-state-machine` model test
- [ ] No tautological properties that re-implement the function under test
- [ ] Every error variant is asserted with `matches!` on structured fields, never on `to_string()`
- [ ] Every failure test asserts the failure left **no partial state** — stock, cart, orders unchanged
- [ ] Test doubles are hand-written fakes; `mockall` appears nowhere, or carries a written justification
- [ ] No `sleep`, no `Utc::now()`, no unseeded RNG anywhere; timeouts/retries use `#[tokio::test(start_paused = true)]` + `tokio::time::advance`
- [ ] Integration: `#[sqlx::test]` for repositories, `wiremock` (including failure responses), port 0 for the server
- [ ] `cargo nextest run` is the CI runner; `cargo mutants -p domain` runs at least nightly with no survivors in error branches
