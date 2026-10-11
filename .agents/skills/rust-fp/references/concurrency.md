# Async Rust with Functional Discipline

Rust has no runtime-enforced structured concurrency: a future does nothing until polled, a spawned
task outlives its parent, and cancellation is a `Drop`. Everything here puts those three facts
under explicit, typed control. tokio 1.53.1 / futures 0.3.33 / tokio-util 0.7.19, edition 2024.

## 1. Bounded concurrency is a hard rule

`join_all` / `try_join_all` over a caller-controlled collection is **unbounded fan-out**: one
request with 50 000 ids opens 50 000 sockets and 50 000 pool waiters — a resource-exhaustion bug,
not a performance choice. Fan-out over a collection always carries an explicit `N`.

```rust
use futures::stream::{self, StreamExt, TryStreamExt};

pub async fn load_products(cat: &dyn Catalog, ids: Vec<ProductId>) -> Result<Vec<Product>, CatalogError> {
    stream::iter(ids)                                  // free fn: collection -> Stream
        .map(|id| async move { cat.fetch(id).await })  // Stream<Item = impl Future<Output = Result<..>>>
        .buffer_unordered(16)                          // StreamExt — at most 16 in flight
        .try_collect::<Vec<_>>()                       // TryStreamExt — stops at the first Err
        .await
}
```

| Adapter | Where it lives |
|---|---|
| `stream::iter` | free fn in `futures::stream` |
| `buffer_unordered(n)` (results as they finish), `buffered(n)` (input order, head-of-line blocking) | **`StreamExt`** |
| `try_collect`, `and_then`, `try_filter_map`, `try_buffer_unordered(n)` (fallible *stream*: `Item = Result<Fut, E>`) | **`TryStreamExt`** |
| `try_join_all`, `join_all` | **free functions in `futures::future`** — NOT `TryFutureExt` methods |

`buffer_unordered` does not short-circuit; `try_collect` does — and when it does the in-flight
futures are **dropped** (§3). To collect all failures rather than the first, keep the `Result`s:
`.collect::<Vec<_>>()` + `itertools::partition_map`. For a fixed, code-controlled set of
*heterogeneous* calls use `try_join!`; it drops the remaining branches on the first `Err`, and all
branches must share one error type (`map_err` into your enum at each call).

```rust
let (profile, orders, prefs) = tokio::try_join!(
    load_profile(pool, uid), load_orders(pool, uid), load_prefs(pool, uid),
)?;   // tokio::join! instead when partial results are still useful: it yields every Result
```

## 2. `JoinSet` for dynamic sets of tasks

```rust
use tokio::task::JoinSet;

let mut set = JoinSet::new();
for shard in shards {
    let repo = Arc::clone(&repo);                    // spawn requires 'static + Send
    set.spawn(async move { repo.reindex(shard).await });
}
while let Some(joined) = set.join_next().await {     // completion order, not spawn order
    match joined {
        Ok(Ok(report)) => tracing::info!(shard = %report.shard, "reindexed"),
        Ok(Err(e)) => failed.push(e),                                        // expected error track
        Err(e) if e.is_panic() => return Err(ReindexError::WorkerPanicked),  // defect
        Err(_) => break,                                                     // aborted: shutting down
    }
}
```

`JoinSet` **aborts every remaining task when dropped** — the structured-concurrency property bare
`tokio::spawn` lacks; `set.shutdown().await` does it explicitly. The set is itself unbounded: gate
spawning behind a `Semaphore` permit when the input is caller-controlled.

## 3. Cancellation safety — the section everyone skips

**An async task is cancelled by DROPPING its future**, which can happen at any `.await`: a
`select!` branch losing, `tokio::time::timeout` firing, an axum client disconnecting mid-request,
a `JoinSet` dropping, a `try_collect` short-circuiting.

### (a) Not every future may go in `select!`

A future dropped mid-poll keeps nothing. If it had already consumed bytes from a socket or an item
from a queue, that data is **lost**.

```rust
// ❌ read_exact is NOT cancel-safe: if the shutdown branch wins after 3 of 8 bytes landed in
//    `buf`, those bytes are gone and the stream is desynchronised.
tokio::select! {
    r = socket.read_exact(&mut buf) => r?,
    () = token.cancelled() => return Ok(()),
}
// ✅ mpsc::Receiver::recv IS cancel-safe: documented to lose no message when dropped.
tokio::select! {
    maybe = rx.recv() => { let Some(msg) = maybe else { return Ok(()) }; handle(msg).await }
    () = token.cancelled() => return Ok(()),
}
```

| Cancel-safe (documented) | NOT cancel-safe |
|---|---|
| `mpsc::Receiver::recv`, `broadcast::Receiver::recv` | `AsyncReadExt::read_exact`, `read_to_end` |
| `watch::Receiver::changed`, `Notify::notified` | `AsyncWriteExt::write_all` |
| `oneshot::Receiver`, `Mutex::lock`, `Semaphore::acquire` | `AsyncBufReadExt::read_line`, `next_line` |
| `tokio::time::sleep`, `JoinHandle` | any `async fn` of yours, until proven otherwise |

**Read the "Cancel safety" section of the docs before putting anything in `select!`.** No such
section means "assume unsafe". Two fixes: pin it outside the loop so each iteration resumes the
same future (`let mut f = std::pin::pin!(read_frame(sock));` then `&mut f` in the branch), or move
it into its own task:

```rust
let mut handle = tokio::spawn(async move { read_frame(socket).await });  // JoinHandle IS cancel-safe
tokio::select! {
    joined = &mut handle => joined.map_err(|_| IoError::WorkerPanicked)?,
    () = token.cancelled() => { handle.abort(); return Ok(()) }
}
```

### (b) Cancellation abandons multi-step workflows mid-flight

A workflow that charged a card and was cancelled before recording the order never runs its
compensation. This is precisely why rollback state held in local variables or in a `Drop` guard is
**unsound** — the code that would read it never executes. Compensation must be durable and
resumable: see `references/sagas.md`.

### (c) `Drop` runs, but cannot await

```rust
impl Drop for LeaseGuard {
    fn drop(&mut self) {
        // ❌ self.client.release(self.id).await       — does not compile; Drop is sync
        // ❌ tokio::spawn(async move { release(id) }) — unawaited, and dies with the runtime at
        //    shutdown. Fire-and-forget cleanup is not cleanup.
        self.permit.release();                         // sync-only cleanup is the only safe kind
    }
}
```

RAII covers sync resources (permits, files, in-memory guards). Anything needing I/O to release
gets an explicit `release().await` on both the happy and the error path.

## 4. Cooperative cancellation: `CancellationToken`

```rust
use tokio_util::sync::CancellationToken;

let root = CancellationToken::new();
let _guard = root.clone().drop_guard();   // cancels the whole tree if this scope unwinds
let worker = root.child_token();          // parent cancels children; children never cancel the parent
set.spawn(async move {
    loop {
        tokio::select! {
            biased;                       // check cancellation before draining more work
            () = worker.cancelled() => break,
            maybe = rx.recv() => match maybe { Some(job) => process(job).await, None => break },
        }
    }
});
```

`cancelled()` is cancel-safe and level-triggered — once cancelled it stays cancelled, so a late
waiter still observes it. One `child_token()` per subsystem lets you stop ingest without stopping
the HTTP server.

## 5. Graceful shutdown and timeouts

```rust
async fn shutdown_signal(token: CancellationToken) {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("invariant: ctrl-c handler installs once at startup");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("invariant: SIGTERM handler installs once at startup")
            .recv().await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { () = ctrl_c => {}, () = terminate => {} }
    tracing::info!("shutdown signal received; draining");
    token.cancel();                 // readiness flips to NOT ready; background loops break
}

// main.rs — the single wiring site
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal(token.clone()))
    .await?;                        // returns only once in-flight requests have finished
background.shutdown().await;        // JoinSet: abort + join the workers
pool.close().await;                 // pools last — handlers may still have held connections
```

Order: stop accepting → drain requests → stop workers → close pools → flush telemetry. Set the
container termination grace period longer than your longest request timeout.

```rust
const GATEWAY_TIMEOUT: Duration = Duration::from_secs(5);
let receipt = match tokio::time::timeout(GATEWAY_TIMEOUT, gateway.charge(&req)).await {
    Ok(res) => res?,                                          // the gateway's own error track
    Err(_elapsed) => return Err(CheckoutError::GatewayTimeout { after: GATEWAY_TIMEOUT }),
};
```

Every external call — DB, HTTP, queue, lock — gets a deliberate timeout; no default infinities.
Timing out **drops** the future (§3), so the peer may well have processed the request: every
timed-out write must be idempotent (idempotency key) before you retry it.

## 6. Channels and actors: the FP answer to shared mutable state

Instead of threading `Arc<Mutex<AppState>>` through the call graph, **own the state in one task**
and talk to it by message. The loop is a `fold` over incoming messages and the transition is a
pure `(State, Command) -> (State, Reply)`, so the behaviour is testable with no runtime and no
locks, and property-testable with `proptest-state-machine`.

```rust
// ---- pure core: the entire actor's behaviour. Zero async, zero I/O.
pub fn step(state: &CartState, cmd: CartCmd) -> (CartState, Result<CartView, CartError>) { /* .. */ }

struct Msg { cmd: CartCmd, reply: oneshot::Sender<Result<CartView, CartError>> }

// ---- shell: a fold over the message stream
async fn run(mut rx: mpsc::Receiver<Msg>, token: CancellationToken) {
    let mut state = CartState::empty();          // contained mutation behind a pure signature
    loop {
        tokio::select! {
            () = token.cancelled() => break,
            maybe = rx.recv() => {                                   // cancel-safe
                let Some(Msg { cmd, reply }) = maybe else { break }; // all senders dropped
                let (next, out) = step(&state, cmd);
                state = next;
                // The one sanctioned `let _ =`: the caller was cancelled and stopped listening.
                let _ = reply.send(out);
            }
        }
    }
}

#[derive(Clone)]
pub struct CartHandle { tx: mpsc::Sender<Msg> }  // the only thing the rest of the app sees

impl CartHandle {
    pub async fn add(&self, item: LineItem) -> Result<CartView, CartError> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(Msg { cmd: CartCmd::Add(item), reply }).await
            .map_err(|_| CartError::Stopped)?;   // actor gone: a typed error, never a panic
        rx.await.map_err(|_| CartError::Stopped)?
    }
}
```

| State shape | Pick |
|---|---|
| Small leaf cache, one field, low contention, no cross-field invariant | `Arc<Mutex<HashMap<..>>>` — fine, don't over-engineer |
| Read-mostly config / feature flags | `watch` channel, or `RwLock` |
| **Invariants spanning multiple fields, or a state machine** | **actor + `mpsc`** — a lock cannot express the invariant, a reducer can |
| Many observers of the same events | `broadcast` |

Never hold a `std::sync::MutexGuard` across an `.await` (`clippy::await_holding_lock`). Needing to
await under a lock is the signal to switch to an actor, not to reach for `tokio::sync::Mutex`.

## 7. The rest of the toolbox

| Primitive | Use | Watch out |
|---|---|---|
| `tokio::sync::RwLock` | many readers, rare writer | writer starvation |
| `tokio::sync::Semaphore` | bound concurrency to a resource; crude rate limit | `acquire_owned` for spawned tasks |
| `broadcast` | pub-sub; every subscriber sees every event | slow subscribers get `RecvError::Lagged` — data loss, handle it |
| `watch` | latest value only: config reload, shutdown flag | intermediate values coalesce by design |
| `OnceCell` (tokio) / `LazyLock` (std) | async / sync lazy init | not for per-request state |
| `spawn_blocking` | CPU-bound or blocking syscalls | separate pool; `JoinError` on panic |

**Never block the async executor.** Password hashing, crypto, compression, `std::fs`, and any sync
client library stall a whole worker thread and every task scheduled on it. More than ~100 µs of
uninterrupted CPU goes in `spawn_blocking`, or on a `rayon` pool with a `oneshot` bridge back.

```rust
let hashed = tokio::task::spawn_blocking(move || argon2_hash(&password))
    .await
    .map_err(|_| AuthError::HasherPanicked)?;   // JoinError → defect
let hashed = hashed?;                           // the hasher's own error track
```

**Backpressure: bounded channels only.** `mpsc::unbounded_channel` turns a fast producer into an
OOM — ban it in `clippy.toml` next to `join_all`. With `mpsc::channel(N)`, `send().await` blocks
the producer: that is backpressure working. Use `try_send` where the right answer to overload is
to shed (`Err(TrySendError::Full)` → 503) rather than to queue.

**`Send` bounds.** AFIT futures are not `Send` by default, so a generic caller cannot
`tokio::spawn` them; `trait-variant` 0.1.3 generates the `Send`-bounded variant. AFIT is still not
dyn-compatible — `Arc<dyn Gateway>` needs `#[async_trait]`. See `di-context.md`.

```rust
#[trait_variant::make(PaymentGateway: Send)]
pub trait LocalPaymentGateway {
    async fn charge(&self, req: &ChargeRequest) -> Result<Receipt, GatewayError>;
}
```

## Checklist

- [ ] Zero `join_all` / `try_join_all` over caller-controlled collections; every fan-out has an explicit `N`
- [ ] `buffer_unordered(N)` + `try_collect` for collection fan-out, `N` chosen against the pool / rate limit
- [ ] Every spawned task is in a `JoinSet` or has an awaited `JoinHandle`; no orphan `tokio::spawn`; panics handled as defects
- [ ] Every `select!` branch is documented cancel-safe, or spawned/pinned so cancellation cannot lose data
- [ ] No `.await` and no `tokio::spawn` in `Drop`; async cleanup is explicit on both paths
- [ ] Multi-step workflows use durable compensation, not in-memory rollback state (`sagas.md`)
- [ ] `CancellationToken` tree wired from `main`; `child_token()` per subsystem; `DropGuard` where a scope must cancel its tree
- [ ] Graceful shutdown: SIGTERM + ctrl-c → `with_graceful_shutdown` → workers joined → pool closed → telemetry flushed
- [ ] Every external call has a deliberate timeout; every timed-out write is idempotent
- [ ] No `Arc<Mutex<AppState>>` threaded through the app; shared state is a leaf `Mutex` or an actor
- [ ] Actor transitions are pure `(State, Cmd) -> (State, Reply)` and tested without a runtime
- [ ] No `std::sync::MutexGuard` held across an `.await`
- [ ] All channels bounded; `unbounded_channel` banned in `clippy.toml`; overload sheds rather than queues
- [ ] Blocking / CPU-heavy work in `spawn_blocking` or `rayon`, never on the runtime
- [ ] `trait-variant` where trait futures must be `Send`; `#[async_trait]` only for `dyn`
