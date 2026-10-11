# Sagas — Compensating Actions for Multi-Service Workflows

## Inside one database, rollback is free

`sqlx::Transaction` implements `Drop` by queueing a `ROLLBACK` on the connection when `commit()` was never called. Combine that with `?` returning early, and **ownership does the rollback for you** — there is no `try`/`catch`/`finally`, no leaked transaction, no forgotten cleanup path. This is a genuine Rust superpower and you should lean on it hard: keep every atomic change inside one transaction and you never write a saga at all.

```rust
pub async fn transfer(db: &Db, cmd: Transfer) -> Result<Receipt, TransferError> {
    let mut tx = db.begin().await?;
    debit(&mut *tx, cmd.from, cmd.amount).await?;   // any `?` here drops tx -> ROLLBACK queued
    credit(&mut *tx, cmd.to, cmd.amount).await?;
    tx.commit().await?;                             // only an explicit commit persists anything
    Ok(Receipt::new(&cmd))
}
```

That is the whole story for single-database atomicity; `database.md` §4 owns the details (workflow-owned boundary, `&mut *tx` reborrowing, never returning a `Transaction`). Read it first — most "sagas" turn out to be one transaction.

Once a step leaves the database — charging a card, sending an email, calling a partner API — **there is nothing to roll back**. The effect happened. You need an explicit compensating action: refund the charge, release the reservation, cancel the shipment.

## The hard blocker: `Drop` cannot be async

A `SagaGuard` whose `Drop` impl fires compensating network calls **does not work in Rust**. State this plainly before designing anything:

- `AsyncDrop` is **nightly only** (`#![feature(async_drop)]`, tracking issue #126482, still open). Do not build on it.
- `tokio::spawn` inside `drop` detaches the work: you cannot await it, cannot observe its failure, and the task is lost if the runtime shuts down first. A compensation you cannot observe is not a compensation.
- `tokio::task::block_in_place` / `Handle::block_on` inside `drop` **panics on a current-thread runtime** and risks deadlock on a multi-thread one (you block a worker while holding whatever the value owned).

This is a real architectural limitation, not a footnote. Compensation must be an **explicit, awaited step on the error path**.

**Also plan for cancellation.** A future dropped at an `await` point — `tokio::select!` losing a race, a `timeout` firing, an axum handler cancelled by a client disconnect — abandons the workflow mid-flight, and no `Drop` code can await the fix-up. Therefore: **compensation state must be durable, not just in-memory.** Write "reserved inventory X for order Y" to a table or log *before* you need it, so a reconciler can finish the job after the process is gone. An in-memory compensation stack handles the error path; it does not handle `kill -9`.

## First: design the need away

The best saga is the one you never write. In priority order:

1. **Order the steps so the irreversible one is LAST.** Reserve inventory (reversible), validate the address (free), *then* capture payment, *then* commit. Every step before the point of no return can be undone cheaply or ignored.
2. **Idempotency keys on every external call.** Pass a stable key derived from the order id (`Idempotency-Key: order-{id}-capture-v1`) so a retry after a timeout cannot double-charge. Without this, retry and compensation are both unsafe.
3. **Two-phase reserve/confirm where the provider offers it.** Payment authorization then capture; inventory reservation with TTL then commit. An expired authorization or reservation compensates itself — the provider's timeout is your rollback.
4. **Collapse steps into one transactional boundary** — outbox table plus a relay, rather than "write DB, then publish event".

Only what survives all four needs a compensation stack.

## The pattern that works: an explicit compensation stack

```rust
use std::future::Future;
use futures::{future::BoxFuture, FutureExt};

#[derive(Debug, thiserror::Error)]
#[error("compensation {step} failed")]
pub struct CompensationError {
    pub step: &'static str,
    #[source] pub source: Box<dyn std::error::Error + Send + Sync>,
}
impl CompensationError {
    pub fn new(step: &'static str, e: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self { step, source: Box::new(e) }
    }
}

// The boxed `source` is deliberate type erasure: compensations come from different ports with
// different error enums. It is a `#[source]` FIELD, never the `E` of a public signature — the
// SKILL.md ban is on `-> Result<T, Box<dyn Error>>`. Justify this hit in the code-review grep.

// Heterogeneous closures cannot share one generic `F` in a `Vec<F>` — boxing is mandatory.
type Compensation = Box<dyn FnOnce() -> BoxFuture<'static, Result<(), CompensationError>> + Send>;

#[derive(Default)]
pub struct Saga {
    steps: Vec<(&'static str, Compensation)>,
}

impl Saga {
    pub fn new() -> Self { Self { steps: Vec::new() } }

    /// Register AFTER the forward step has succeeded — never before.
    pub fn push_compensation<F, Fut>(&mut self, step: &'static str, undo: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), CompensationError>> + Send + 'static,
    {
        self.steps.push((step, Box::new(move || undo().boxed())));
    }

    /// Disarm: the forward path committed, nothing to undo.
    pub fn commit(mut self) { self.steps.clear(); }

    /// LIFO. Every compensation runs even if an earlier one fails.
    pub async fn compensate(mut self) -> Vec<CompensationError> {
        let mut failures = Vec::new();
        while let Some((step, undo)) = self.steps.pop() {
            match undo().await {
                Ok(()) => tracing::info!(step, "compensated"),
                Err(e) => { tracing::error!(step, %e, "COMPENSATION FAILED"); failures.push(e) }
            }
        }
        failures
    }
}

impl Drop for Saga {
    fn drop(&mut self) {   // Drop cannot await; screaming is the only honest option left
        if !self.steps.is_empty() {
            tracing::error!(pending = self.steps.len(), "saga dropped armed — NOT compensated");
        }
    }
}
```

The caller keeps `?`-style flow in an inner function and owns the compensation decision in the outer one:

```rust
pub async fn checkout(deps: &Deps, cmd: CheckoutCommand) -> Result<Order, CheckoutError> {
    let mut saga = Saga::new();
    match forward(deps, &mut saga, &cmd).await {
        Ok(order) => { saga.commit(); Ok(order) }
        Err(e) => {
            let failures = saga.compensate().await;   // awaited BEFORE returning the error
            if !failures.is_empty() { deps.dead_letter.record(&cmd, &failures).await; }
            Err(e)
        }
    }
}

// Every `?` here exits to the match above, which compensates. Deps are Arc<dyn Port> clones,
// so each closure captures an owned 'static handle.
async fn forward(d: &Deps, saga: &mut Saga, cmd: &CheckoutCommand) -> Result<Order, CheckoutError> {
    let reservation = d.inventory.reserve(cmd.sku.clone(), cmd.qty).await?;
    let (inv, rid) = (d.inventory.clone(), reservation.id.clone());
    saga.push_compensation("release-inventory", move || async move {
        inv.release(rid).await.map_err(|e| CompensationError::new("release-inventory", e))
    });
    let charge = d.payments.capture(cmd.intent.clone(), cmd.total).await?;
    let (pay, cid) = (d.payments.clone(), charge.id.clone());
    saga.push_compensation("refund-charge", move || async move {
        pay.refund(cid).await.map_err(|e| CompensationError::new("refund-charge", e))
    });
    Ok(d.orders.insert(Order::place(cmd, charge.id)).await?)
}
```

## Compensations fail too

They run *after* something already went wrong, on the same flaky network. Treat each one as a first-class operation:

- **Idempotent.** `release(reservation_id)` on an already-released reservation must return `Ok`, not 404-as-error. Same for refunds — key them by charge id.
- **Retriable.** Wrap each compensation in bounded exponential backoff with jitter (`backon`, or `tower::retry`), inside a timeout — see `rop-errors.md` for the `retriable` predicate.
- **Never silent.** A failed compensation is money or stock stuck in limbo. Log at ERROR with the step name and correlation id, increment a dedicated `saga_compensation_failed_total` counter, alert on it, and write the payload to a dead-letter store a human or reconciler can drain.
- **Ordered LIFO.** Undo in reverse of do — the later step may depend on the earlier one's effect.

## Sync RAII guards are still great

Where no `await` is needed, `Drop` is exactly right and needs no saga:

```rust
pub struct Defer<F: FnOnce()>(Option<F>);
impl<F: FnOnce()> Defer<F> {
    pub fn new(f: F) -> Self { Self(Some(f)) }
    pub fn disarm(mut self) { self.0.take(); }
}
impl<F: FnOnce()> Drop for Defer<F> {
    fn drop(&mut self) { if let Some(f) = self.0.take() { f(); } }
}
let _dec = Defer::new(|| in_flight.fetch_sub(1, Ordering::Relaxed));   // fires on every exit path
```

| Fine in `Drop` | Not fine in `Drop` |
|---|---|
| In-memory rollback, metric decrement, gauge reset | Any network call |
| Removing a temp file, unlocking a local mutex | Refund, release, cancel, publish |
| `tokio_util::sync::DropGuard` cancelling a `CancellationToken` | Anything you need to *observe* succeeding |

## When to stop rolling your own

An in-process compensation stack dies with the process. Pivot when compensation must survive a crash or a deploy:

- **Durable workflow engine** — the shape is an append-only event log plus deterministic replay: each step's intent and outcome is persisted before/after execution, and a recovering worker replays the log to find which compensations are still owed. Buy this rather than building it.
- **Or accept eventual consistency**: persist intent (`reservation_pending`), let a periodic **reconciliation job** find orphans (reservations with no order after N minutes, charges with no shipment) and compensate them. Simpler, and often sufficient. Either way, the durable record is what makes recovery possible — the in-memory stack is only the fast path.

## Testing sagas

Inject failure through hand-written fakes of your ports (no `mockall`), and assert on an ordering log — see `testing.md`.

```rust
#[derive(Clone, Default)]
struct FakePayments { fail_capture: bool, log: Arc<Mutex<Vec<&'static str>>> }
#[tokio::test]
async fn payment_failure_releases_inventory() {
    let deps = Deps::fake(FakePayments { fail_capture: true, ..Default::default() });
    let err = checkout(&deps, cmd()).await.unwrap_err();
    assert!(matches!(err, CheckoutError::PaymentDeclined { .. }));
    assert_eq!(*deps.log.lock().unwrap(), ["reserve", "capture", "release-inventory"]);
}
```

Cover: each forward step failing in turn; compensations running in LIFO order; a **failing compensation** surfacing to the dead-letter path rather than being swallowed; and a compensation invoked twice being a no-op (idempotence).

## Checklist

- [ ] Everything that can live in one DB transaction does — `?` + `Drop` rollback, no saga
- [ ] No compensating logic in a `Drop` impl, and no `tokio::spawn`/`block_on` inside `drop`
- [ ] Steps ordered so the irreversible one is last; reserve/confirm used where the provider offers it
- [ ] Every external call carries an idempotency key derived from the workflow id
- [ ] Compensations pushed only *after* their forward step succeeded, and run LIFO
- [ ] The compensation stack is `Vec<Box<dyn FnOnce() -> BoxFuture<..> + Send>>` — boxed, not generic
- [ ] `compensate().await` is explicitly invoked on the error path before returning
- [ ] Compensations are idempotent, retried with backoff + timeout, and never silent
- [ ] Failed compensations hit a dead-letter store, a counter, and an alert
- [ ] Compensation state is durable, so cancellation or process death is recoverable
- [ ] A reconciliation job (or a durable workflow engine) exists for what the in-process path misses
- [ ] Tests inject a failure at every forward step and assert compensation order and failure surfacing
