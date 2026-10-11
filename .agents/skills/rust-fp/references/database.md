# Persistence as a Boundary — sqlx, Repositories, Transactions

Verified against **sqlx 0.9.0** (2026-07). The repository now lives at **`transact-rs/sqlx`** — old launchbadge links are dead. The database is untrusted input like any other boundary: a row is a DTO, the domain type is what you get after parsing it.

## 1. Choosing the library

| Crate | Model | Take |
|---|---|---|
| **sqlx 0.9** | raw SQL, **compile-time checked against a live schema** | **Default.** `query_as!` verifies the SQL *and* its result types at build time — parse-don't-validate applied to the SQL boundary. No DSL to fight over CTEs, window functions, `RETURNING`, `FOR UPDATE` |
| diesel 2.3 | typed query DSL, macro-generated schema | strongest compile-time guarantees, but a heavy DSL, sync-first, and complex SQL falls back to `sql_query` anyway |
| sea-orm 2.0 | active-record ORM over sqlx | released 2026-07-19 — **very new; expect churn and tutorials written for 1.x.** Also pulls entity structs toward being your domain model, which is the mistake below |

## 2. The anti-corruption layer — the core of this file

**Never let sqlx construct your domain types.** The impedance mismatch is real:

| Domain | Table |
|---|---|
| data-carrying enums (`PaymentMethod::Card { last4 }`) | flat columns |
| typestates (`Order<Placed>` vs `Order<Paid>`) | one `status` text column |
| `NonEmpty<OrderLine>` | a child table that may legally be empty |
| newtypes with private fields and validation | `uuid`, `int8`, `text` |
| nothing nullable unless it means "absent" | everything nullable after the next migration |

So: define a **flat row struct mirroring the table exactly**, then a `TryFrom` that enforces the invariants.

```rust
// crates/infra/src/orders/row.rs — mirrors the DDL 1:1. Nullable column ⇒ Option. No logic.
#[derive(sqlx::FromRow)]
pub struct OrderRow {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub status: OrderStatusDb,                    // Postgres enum, §3
    pub total_cents: i64,
    pub placed_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,           // NULL unless status = 'paid'
    pub payment: Option<Json<PaymentMethodDto>>,  // JSONB, §3
}

impl TryFrom<(OrderRow, Vec<OrderLineRow>)> for Order {
    type Error = RowDecodeError;

    fn try_from((row, lines): (OrderRow, Vec<OrderLineRow>)) -> Result<Self, Self::Error> {
        let lines: Vec<OrderLine> =
            lines.into_iter().map(OrderLine::try_from).collect::<Result<_, _>>()?;   // traverse
        let lines = NonEmpty::try_from(lines).map_err(|_| RowDecodeError::NoLines(row.id))?;

        let state = match (row.status, row.paid_at, row.payment) {
            (OrderStatusDb::Placed, None, None) => OrderState::Placed,
            (OrderStatusDb::Paid, Some(at), Some(Json(pm))) => OrderState::Paid {
                at: Timestamp::from(at),
                method: PaymentMethod::try_from(pm)?,
            },
            // Named, not `_`: adding a variant to OrderStatusDb breaks THIS build, which is the
            // point (`pattern-matching.md`). Any other column combination is corrupt data.
            (status @ (OrderStatusDb::Placed | OrderStatusDb::Paid
                     | OrderStatusDb::Shipped | OrderStatusDb::Cancelled), _, _) =>
                return Err(RowDecodeError::InconsistentState { id: row.id, status }),
        };
        Ok(Self::rehydrate(OrderId::from(row.id), CustomerId::from(row.customer_id), lines, state))
    }
}
```

`Order::rehydrate` is `pub(crate)` in `domain`, exists only for this path, and is the single door through which persisted state re-enters the domain. Newtypes and C-like enums cross the edge with derives; anything richer gets a hand-written `sqlx::Encode` + `sqlx::Decode` **in `infra`** — never in `domain`, which must not depend on sqlx.

```rust
#[derive(sqlx::Type)]
#[sqlx(transparent)]                     // valid only on a struct with exactly one field
pub struct OrderId(Uuid);

#[derive(sqlx::Type)]
#[sqlx(type_name = "order_status", rename_all = "snake_case")]
pub enum OrderStatusDb { Placed, Paid, Shipped, Cancelled }
```

Domain newtypes keep their field private (`domain-types.md`), so bind query parameters through a
borrowing accessor — `id.as_uuid()`, `sku.as_str()` — never `id.0`, which does not even compile
from another crate.

## 3. Rich enums in a relational table

| Option | Verdict |
|---|---|
| discriminator column + one nullable column per variant | **Bad.** `status='placed'` with a non-null `paid_at` is representable — you reintroduce exactly the illegal states the domain enum removed |
| **Postgres enum type** (`CREATE TYPE order_status AS ENUM (...)`) | correct for **C-like** variants with no payload: queryable, indexable, DB-checked. New variant = `ALTER TYPE ... ADD VALUE` |
| **JSONB + serde** for **data-carrying** variants | **Recommended.** One column holds the whole variant faithfully |

```rust
// column: payment jsonb
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
pub enum PaymentMethodDto {
    Card { last4: String, brand: String },
    PayPal { account: String },
    StoreCredit { voucher: String },
}

sqlx::query!(
    "UPDATE orders SET status = 'paid', paid_at = $2, payment = $3 WHERE id = $1",
    order_id.as_uuid(), paid_at, Json(&method) as _,   // sqlx::types::Json<T> encodes as jsonb
)
```

Caveats, non-negotiable: you **lose queryability** (`WHERE payment->>'kind' = 'card'` is a seq scan without an expression/GIN index), and the JSON shape becomes a **wire format you must version**. Always `#[serde(tag = "...")]` so variants are self-describing, never `untagged`, and keep the DTO separate from the domain enum so a domain rename is not a data migration.

## 4. Transactions — the most important section

1. **The workflow owns the transaction boundary.** `begin`/`commit` live in `app`, never in a repository.
2. **Repositories are transaction-unaware.** They take a connection, not a pool.

```rust
// crates/infra — works inside or outside a transaction; cannot start one.
pub async fn insert_order(conn: &mut PgConnection, order: &Order) -> Result<(), RepoError>;
pub async fn clear_cart(conn: &mut PgConnection, customer: CustomerId) -> Result<(), RepoError>;
```
`&mut PgConnection` is the default. `impl sqlx::Executor<'_, Database = Postgres>` is more general (it accepts `&PgPool` too) but noisier, and it lets a caller pass a pool where atomicity was intended — prefer the concrete form for anything that must be atomic.

```rust
// crates/app/src/checkout.rs
pub async fn checkout(pool: &PgPool, clock: &dyn Clock, customer: CustomerId)
    -> Result<Order, CheckoutError>
{
    let mut tx = pool.begin().await?;

    let lines = load_cart_lines(&mut *tx, customer).await?;      // <- reborrow
    let lines = NonEmpty::try_from(lines).map_err(|_| CheckoutError::CartEmpty)?;

    for line in &lines {
        reserve_stock(&mut *tx, line.product, line.qty).await?;  // any failure exits here
    }

    let order = Order::place(customer, lines, clock.now());
    insert_order(&mut *tx, &order).await?;
    clear_cart(&mut *tx, customer).await?;

    tx.commit().await?;
    Ok(order)
}
```

**Why `&mut *tx` and not `&mut tx`.** `Transaction<'_, Postgres>` derefs to `PgConnection`. Writing `&mut tx` **moves** the mutable borrow into the callee, so `tx` is unusable afterwards — fatal inside a loop. `&mut *tx` *reborrows*: it dereferences to the connection and lends a fresh `&mut PgConnection` that expires when the call returns, handing the borrow back.

**The payoff.** If any `?` fires, the function returns early, `tx` drops **without `commit()`**, and sqlx's `Drop` impl issues a **ROLLBACK**. Nothing you wrote does this — the type system does.

> Railway-oriented early exit and Rust ownership are the same mechanism here. `?` is the switch to the error track, `Drop` is the compensating action, and a value that goes out of scope un-committed is a value that never happened. You get transactional correctness by writing straight-line happy-path code.

Corollary: never `let _ = tx;`, never `std::mem::forget(tx)`, and **never return a `Transaction` from a repository function** — you would be handing out the rollback guard.

### A Unit-of-Work helper — and when not to bother

```rust
pub async fn with_tx<T, E, F>(pool: &PgPool, f: F) -> Result<T, E>
where
    E: From<sqlx::Error>,
    F: for<'c> AsyncFnOnce(&'c mut PgConnection) -> Result<T, E>,
{
    let mut tx = pool.begin().await?;
    let out = f(&mut *tx).await?;      // on Err: early return, tx drops, ROLLBACK
    tx.commit().await?;
    Ok(out)
}
```

Honest note: the HRTB on the closure fights you the moment the body captures something by mutable reference or the return type mentions a lifetime. The explicit `begin`/`?`/`commit` form is shorter to read and always compiles. **Prefer explicit; reach for `with_tx` only when you are genuinely repeating the same three lines a dozen times.**

## 5. Atomic conditional updates

Read-then-write is a race. Push the condition into the `UPDATE` and let Postgres take the row lock:

```rust
/// Returns the remaining stock, or None if there wasn't enough. No race window.
pub async fn reserve_stock(conn: &mut PgConnection, product: ProductId, qty: Quantity)
    -> Result<Option<Stock>, RepoError>
{
    // No `as` casts on the way to the driver: Quantity's inner u32 is checked into the column
    // type, and the newtype exposes a borrowing accessor rather than a public field.
    let qty_col = i32::try_from(qty.into_inner()).map_err(|_| RepoError::QuantityOutOfRange)?;
    let row = sqlx::query!(
        "UPDATE products SET stock = stock - $1 WHERE id = $2 AND stock >= $1 RETURNING stock",
        qty_col,
        product.as_uuid(),
    )
    .fetch_optional(conn)
    .await?;

    row.map(|r| Stock::try_new(r.stock)).transpose().map_err(RepoError::Decode)
}
```

Return a **domain-meaningful** `Option`/`bool`, never `rows_affected()` — a bare `u64` at the call site is an invitation to forget the `== 0` check. The caller writes `.ok_or(CheckoutError::OutOfStock { product })?` and the error track carries it home.

## 6. Errors — translate at the repository boundary

Some database errors are **domain outcomes**; the rest are **defects**.

```rust
pub async fn insert_customer(conn: &mut PgConnection, c: &Customer) -> Result<(), SignupError> {
    match sqlx::query!("INSERT INTO customers (id, email) VALUES ($1, $2)", c.id.as_uuid(), c.email.as_str())
        .execute(conn)
        .await
    {
        Ok(_) => Ok(()),
        // Expected: the unique index IS the business rule.
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(SignupError::EmailTaken),
        Err(sqlx::Error::Database(e)) if e.is_foreign_key_violation() => Err(SignupError::UnknownTenant),
        // Pool exhausted, connection reset, missing column: operational defects. Log and 500.
        Err(e) => Err(SignupError::Unavailable(e)),
    }
}
```

`sqlx::Error` may sit behind a `#[source]` on a `thiserror` variant, but **never in a workflow or handler signature**. Keep an `is_retriable`/`is_defect` predicate on the enum so a `tower` retry layer can act on it (`references/production.md`).

## 7. N+1 — two queries plus a pure group-by

Never a query per row. Fetch parents, fetch children with `= ANY($1)`, group **in memory with a pure function** you can property-test.

```rust
let orders = sqlx::query_as!(OrderRow, "SELECT * FROM orders WHERE customer_id = $1", cid.0)
    .fetch_all(&mut *conn).await?;
let ids: Vec<Uuid> = orders.iter().map(|o| o.id).collect();
let lines = sqlx::query_as!(OrderLineRow, "SELECT * FROM order_lines WHERE order_id = ANY($1)", &ids)
    .fetch_all(&mut *conn).await?;

/// Pure, total, no I/O — lives beside the row types and is trivially testable.
pub fn assemble(orders: Vec<OrderRow>, lines: Vec<OrderLineRow>) -> Result<Vec<Order>, RowDecodeError> {
    let mut by_order: HashMap<Uuid, Vec<OrderLineRow>> =
        lines.into_iter().into_group_map_by(|l| l.order_id);   // itertools 0.15
    orders
        .into_iter()
        .map(|o| { let ls = by_order.remove(&o.id).unwrap_or_default(); Order::try_from((o, ls)) })
        .collect()                                             // traverse: Result<Vec<_>, _>
}
```

For deep trees prefer one query with `json_agg` over a join that multiplies rows.

## 8. Migrations and offline builds

Numbered, forward-only, checked into the repo, applied by the same code in dev, CI, and prod.

```rust
// migrations/0001_create_customers.sql, 0002_create_orders.sql, 0003_add_orders_payment_jsonb.sql
sqlx::migrate!("./migrations").run(&pool).await?;   // in main.rs, before serving
```

For `query!`/`query_as!` to compile without a live database, run `cargo sqlx prepare --workspace` and **commit the `.sqlx/` directory**. CI then builds offline, and a stale `.sqlx` after a schema change is a compile error — which is the point. Add `cargo sqlx prepare --check` to CI so nobody forgets.

## 9. Testing

```rust
#[sqlx::test(migrations = "./migrations")]
async fn reserving_more_than_stock_leaves_it_untouched(pool: PgPool) -> sqlx::Result<()> { /* ... */ }
```

Be precise about `#[sqlx::test]`: it provisions a **fresh database per test**, runs the migrations, hands you a `PgPool`, and **drops the database on success**. It is **not** a rolled-back transaction — writes really land. A **failed test deliberately leaves its database behind** for inspection, so expect stragglers after a red run.

`testcontainers` 0.27.3 boots a real Postgres when integration tests need control over the server itself (extensions, version matrix, dump-based fixtures). Do not substitute SQLite: JSONB, `ANY($1)`, enum types, and `RETURNING` semantics all differ. The atomicity test — force a failure mid-workflow, assert **nothing** persisted — is in `references/testing.md` and is mandatory for every multi-write workflow.

## 10. Pool and timeouts

```rust
PgPoolOptions::new()
    .max_connections(config.pool_max)          // start near (cores × 2) + spindles, then measure
    .acquire_timeout(Duration::from_secs(3))   // fail fast; a hung acquire is an outage in disguise
    .idle_timeout(Duration::from_secs(600))
    .max_lifetime(Duration::from_secs(1800))   // survives failovers and rolling proxy restarts
    .after_connect(|conn, _meta| Box::pin(async move {
        sqlx::query("SET statement_timeout = '5s'; SET idle_in_transaction_session_timeout = '10s'")
            .execute(conn).await?;
        Ok(())
    }))
    .connect(config.database_url.expose_secret())
    .await?
```

Pool size is a **global** budget: `replicas × max_connections` must stay under the server's `max_connections` minus headroom for migrations and psql. Always set `statement_timeout` — without it one bad query pins a connection until the OOM killer arrives.

## Checklist

- [ ] Every query is `query!` / `query_as!` (compile-time checked); `.sqlx/` committed and CI-verified
- [ ] A flat `*Row` struct per table mirrors the DDL; nullable columns are `Option`
- [ ] Domain types built only via `TryFrom<Row>`; sqlx never derives onto a domain type
- [ ] Illegal column combinations return `RowDecodeError`, not a silently-wrong value
- [ ] Data-carrying enums stored as tagged, versioned JSONB; C-like enums as a Postgres enum type
- [ ] Repositories take `&mut PgConnection` and never call `begin`/`commit`
- [ ] Workflows own transactions; steps receive `&mut *tx`; rollback is left to `Drop`
- [ ] No `Transaction` returned from, or stored outside, the workflow that opened it
- [ ] Conditional writes are atomic `UPDATE ... WHERE ... RETURNING`, returning a domain `Option`/`bool`
- [ ] `sqlx::Error` translated at the repository edge; unique violations mapped to domain errors
- [ ] No `sqlx::Error` in any workflow or handler signature
- [ ] No query inside a loop over rows — two queries plus a pure group-by
- [ ] Migrations numbered, forward-only, run by the app at startup in every environment
- [ ] `#[sqlx::test]` used knowing it is a fresh database, not a rollback
- [ ] Pool sized against the server's global limit; `acquire_timeout` and `statement_timeout` both set
