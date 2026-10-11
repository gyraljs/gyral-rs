use crate::{
    AppBundle, BUNDLE_ABI_VERSION, RenderError, RenderLimits, RenderRequest, RenderedHtml,
    WorkerError,
    supervision::{Control, Registration, RequestSignal, StopReason},
};
use deno_core::{JsRuntime, PollEventLoopOptions, RuntimeOptions, v8};
use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    thread,
    time::Instant,
};
use tokio::sync::{mpsc, oneshot};
use tracing::Instrument;

// Snapshot input exists only in the evaluation test binary. Published/default
// workers continue to evaluate a trusted bundle in a fresh isolate per request.
enum RuntimeSource {
    Bundle(AppBundle),
    #[cfg(test)]
    Snapshot(&'static [u8]),
}

struct RuntimeInput {
    source: RuntimeSource,
    #[cfg(test)]
    profile: Option<Arc<std::sync::Mutex<evaluation::StageTotals>>>,
}

impl From<AppBundle> for RuntimeInput {
    fn from(bundle: AppBundle) -> Self {
        Self {
            source: RuntimeSource::Bundle(bundle),
            #[cfg(test)]
            profile: None,
        }
    }
}

impl RuntimeInput {
    fn snapshot(&self) -> Option<&'static [u8]> {
        match self.source {
            RuntimeSource::Bundle(_) => None,
            #[cfg(test)]
            RuntimeSource::Snapshot(bytes) => Some(bytes),
        }
    }
}

enum Message {
    Render {
        request: RenderRequest,
        deadline: Instant,
        signal: Arc<RequestSignal>,
        reply: oneshot::Sender<Result<RenderedHtml, RenderError>>,
        span: tracing::Span,
    },
    Wake,
}

/// Cloneable rendering handle. JavaScript runs on a dedicated Rust thread.
#[derive(Clone)]
pub struct Renderer {
    sender: mpsc::Sender<Message>,
    control: Arc<Control>,
    limits: RenderLimits,
}

impl fmt::Debug for Renderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Renderer")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

/// Owns the worker and watchdog lifecycle; explicit shutdown drains and joins them.
///
/// Dropping the owner closes admission and interrupts active JavaScript synchronously.
/// It cannot await thread cleanup. Use `shutdown` to observe completed cleanup.
pub struct RendererWorker {
    sender: mpsc::Sender<Message>,
    control: Arc<Control>,
    limits: RenderLimits,
    done: Option<oneshot::Receiver<()>>,
    joins: Option<(thread::JoinHandle<()>, thread::JoinHandle<()>)>,
}

impl fmt::Debug for RendererWorker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RendererWorker")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// Read lifecycle and queue gauges. Hosts must also probe their bundle for readiness.
    #[must_use]
    pub fn status(&self) -> crate::RendererStatus {
        let (phase, active) = self.control.status();
        crate::RendererStatus {
            phase,
            active,
            queued: self.sender.max_capacity() - self.sender.capacity(),
            capacity: self.sender.max_capacity(),
        }
    }

    /// Start a bounded worker with development defaults and a fresh isolate per request.
    ///
    /// Call at process startup before other threads initialize V8. The renderer accepts
    /// trusted app code. V8 heap budgeting is not a hard process-memory sandbox.
    ///
    /// # Errors
    /// Returns `WorkerError` if a thread or executor cannot start.
    pub fn start(bundle: AppBundle) -> Result<(Self, RendererWorker), WorkerError> {
        Self::with_limits(bundle, RenderLimits::default())
    }

    /// Start a renderer with a resource policy parsed by the host application.
    ///
    /// # Errors
    /// Returns `WorkerError` if a thread or executor cannot start.
    pub fn with_limits(
        bundle: AppBundle,
        limits: RenderLimits,
    ) -> Result<(Self, RendererWorker), WorkerError> {
        Self::start_input(bundle.into(), limits)
    }

    fn start_input(
        input: RuntimeInput,
        limits: RenderLimits,
    ) -> Result<(Self, RendererWorker), WorkerError> {
        JsRuntime::init_platform(None);
        let executor = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(WorkerError::Executor)?;
        let (sender, receiver) = mpsc::channel(16);
        let control = Arc::new(Control::default());
        let watching = Arc::clone(&control);
        let watchdog = thread::Builder::new()
            .name("gyral-watchdog".to_owned())
            .spawn(move || watching.watch())
            .map_err(WorkerError::Thread)?;
        let (done, completion) = oneshot::channel();
        let working = Arc::clone(&control);
        let join = thread::Builder::new()
            .name("gyral-renderer".to_owned())
            .spawn(move || {
                let _completion = WorkerCompletion {
                    control: Arc::clone(&working),
                    done: Some(done),
                };
                // Local ownership ensures executor teardown precedes completion,
                // including unwinds. No host Tokio blocking task waits on this thread.
                let executor = executor;
                run_worker(receiver, &input, limits, &working, &executor);
            });
        let join = match join {
            Ok(join) => join,
            Err(error) => {
                control.finished();
                watchdog.join().map_err(|_| WorkerError::Panicked)?;
                return Err(WorkerError::Thread(error));
            }
        };
        Ok((
            Self {
                sender: sender.clone(),
                control: Arc::clone(&control),
                limits,
            },
            RendererWorker {
                sender,
                control,
                limits,
                done: Some(completion),
                joins: Some((join, watchdog)),
            },
        ))
    }

    /// Render within an end-to-end deadline, including queue time.
    ///
    /// Dropping this future cancels queued work and interrupts active JavaScript.
    /// Poll inside a Tokio runtime with its time driver enabled.
    ///
    /// # Errors
    /// Returns a typed overload, availability, deadline, heap, panic, or rendering failure.
    /// Returns `HostRuntimeUnavailable` before admission if the timer driver is missing.
    #[tracing::instrument(skip_all, fields(page = request.page().as_str()))]
    pub async fn render(&self, request: RenderRequest) -> Result<RenderedHtml, RenderError> {
        let mut observation = crate::telemetry::Observation::start();
        let result = self.render_inner(request).await;
        observation.finish(&result);
        result
    }

    async fn render_inner(&self, request: RenderRequest) -> Result<RenderedHtml, RenderError> {
        let deadline = Instant::now() + self.limits.render_deadline();
        let mut timer =
            deadline_timer(deadline).map_err(|_| RenderError::HostRuntimeUnavailable)?;
        let signal = Arc::new(RequestSignal::default());
        let _cancellation = CancelOnDrop {
            control: Arc::clone(&self.control),
            signal: Arc::clone(&signal),
        };
        let (reply, response) = oneshot::channel();
        self.control.admit(|| {
            self.sender
                .try_send(Message::Render {
                    request,
                    deadline,
                    signal: Arc::clone(&signal),
                    reply,
                    span: tracing::Span::current(),
                })
                .map_err(|error| match error {
                    mpsc::error::TrySendError::Full(_) => RenderError::Busy,
                    mpsc::error::TrySendError::Closed(_) => RenderError::Unavailable,
                })
        })?;
        // Oneshot receive and Sleep are cancellation-safe. The timer branch
        // signals interruption before dropping the response receiver.
        tokio::select! {
            biased;
            result = response => result.map_err(|_| RenderError::Unavailable)?,
            () = &mut timer => {
                self.control.stop_request(&signal, StopReason::Deadline);
                Err(stop_error(
                    signal.reason().unwrap_or(StopReason::Deadline),
                    self.limits,
                ))
            }
        }
    }
}

impl RendererWorker {
    /// Close admission, drain accepted messages, and join both threads within the shutdown budget.
    ///
    /// On deadline expiry, active JavaScript is interrupted and queued work is discarded.
    /// Native engine stalls and process OOM cannot be forcibly repaired by an embedded renderer.
    ///
    /// # Errors
    /// Returns `ShutdownTimedOut`, `Stopped`, or `Panicked` when cleanup cannot be confirmed,
    /// or `HostRuntimeUnavailable` when the host has no enabled Tokio timer driver.
    pub async fn shutdown(mut self) -> Result<(), WorkerError> {
        let deadline = Instant::now() + self.limits.shutdown_deadline();
        let mut timer =
            deadline_timer(deadline).map_err(|_| WorkerError::HostRuntimeUnavailable)?;
        self.control.drain(deadline);
        self.wake();
        let completion = self.done.take().ok_or(WorkerError::Stopped)?;
        let waiting = async {
            completion.await.map_err(|_| WorkerError::Stopped)?;
            let (worker, watchdog) = self.joins.take().ok_or(WorkerError::Stopped)?;
            while !worker.is_finished() || !watchdog.is_finished() {
                tokio::task::yield_now().await;
            }
            // Both entrypoints have finished; join cannot wait on active JavaScript.
            let worker = worker.join();
            let watchdog = watchdog.join();
            worker.map_err(|_| WorkerError::Panicked)?;
            watchdog.map_err(|_| WorkerError::Panicked)
        };
        // Dropping `waiting` is safe: owner Drop closes admission and interrupts
        // execution; no Tokio joining task survives the losing branch.
        tokio::select! {
            biased;
            result = waiting => match result {
                Ok(()) if Instant::now() >= deadline => Err(WorkerError::ShutdownTimedOut {
                    after: self.limits.shutdown_deadline(),
                }),
                result => result,
            },
            () = &mut timer => Err(WorkerError::ShutdownTimedOut {
                after: self.limits.shutdown_deadline(),
            }),
        }
    }

    fn wake(&self) {
        // A full queue already wakes the worker; a closed queue needs no message.
        match self.sender.try_send(Message::Wake) {
            Ok(())
            | Err(mpsc::error::TrySendError::Full(_))
            | Err(mpsc::error::TrySendError::Closed(_)) => {}
        }
    }
}

impl Drop for RendererWorker {
    fn drop(&mut self) {
        self.control.abort();
        self.wake();
    }
}

struct WorkerCompletion {
    control: Arc<Control>,
    done: Option<oneshot::Sender<()>>,
}

impl Drop for WorkerCompletion {
    fn drop(&mut self) {
        self.control.finished();
        if let Some(done) = self.done.take() {
            // The owner can be dropped before completion; this is expected cancellation.
            let _ = done.send(());
        }
    }
}

struct CancelOnDrop {
    control: Arc<Control>,
    signal: Arc<RequestSignal>,
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.control
            .stop_request(&self.signal, StopReason::Cancelled);
    }
}

fn run_worker(
    mut receiver: mpsc::Receiver<Message>,
    input: &RuntimeInput,
    limits: RenderLimits,
    control: &Arc<Control>,
    executor: &tokio::runtime::Runtime,
) {
    while !control.should_stop(receiver.is_empty()) {
        let Some(message) = receiver.blocking_recv() else {
            break;
        };
        match message {
            Message::Render {
                request,
                deadline,
                signal,
                reply,
                span,
            } => {
                if reply.is_closed() {
                    continue;
                }
                if Instant::now() >= deadline {
                    control.stop_request(&signal, StopReason::Deadline);
                }
                let rendered = match signal.reason() {
                    Some(reason) => Err(stop_error(reason, limits)),
                    None => recover_panic(|| {
                        executor.block_on(
                            render(input, &request, limits, control, &signal, deadline)
                                .instrument(span),
                        )
                    }),
                };
                let rendered = match signal.finish() {
                    Some(reason) => Err(stop_error(reason, limits)),
                    None => rendered,
                };
                // Caller cancellation can close the reply while the isolate unwinds.
                let _ = reply.send(rendered);
            }
            Message::Wake => {}
        }
    }
    receiver.close();
    while let Ok(message) = receiver.try_recv() {
        if let Message::Render { reply, .. } = message {
            // A discarded request may already have been cancelled by its caller.
            let _ = reply.send(Err(RenderError::Unavailable));
        }
    }
}

fn recover_panic<T>(operation: impl FnOnce() -> Result<T, RenderError>) -> Result<T, RenderError> {
    catch_unwind(AssertUnwindSafe(operation)).map_err(|_| RenderError::WorkerPanicked)?
}

#[derive(Debug, thiserror::Error)]
enum TimerError {
    #[error("the host Tokio timer driver is unavailable")]
    Unavailable,
}

fn deadline_timer(deadline: Instant) -> Result<std::pin::Pin<Box<tokio::time::Sleep>>, TimerError> {
    // Tokio reports missing timer capability through a constructor panic,
    // without a fallible constructor. Preserve our library's typed error track.
    catch_unwind(|| tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)))
        .map(Box::pin)
        .map_err(|_| TimerError::Unavailable)
}

fn stop_error(reason: StopReason, limits: RenderLimits) -> RenderError {
    match reason {
        StopReason::Deadline => RenderError::DeadlineExceeded {
            after: limits.render_deadline(),
        },
        StopReason::Heap => RenderError::HeapBudgetExceeded {
            bytes: limits.heap_bytes(),
        },
        StopReason::Cancelled => RenderError::Cancelled,
        StopReason::Shutdown => RenderError::Unavailable,
    }
}

// Field order matters: dispose the isolate before unregistering its watchdog handle.
struct Session {
    runtime: JsRuntime,
    _registration: Registration,
}

async fn render(
    input: &RuntimeInput,
    request: &RenderRequest,
    limits: RenderLimits,
    control: &Arc<Control>,
    signal: &Arc<RequestSignal>,
    deadline: Instant,
) -> Result<RenderedHtml, RenderError> {
    #[cfg(test)]
    let beginning = Instant::now();
    let mut runtime = JsRuntime::try_new(RuntimeOptions {
        startup_snapshot: input.snapshot(),
        create_params: Some(v8::Isolate::create_params().heap_limits(0, limits.heap_bytes())),
        ..RuntimeOptions::default()
    })
    .map_err(|error| RenderError::Runtime {
        detail: error.to_string(),
    })?;
    #[cfg(test)]
    let created = Instant::now();
    let handle = runtime.v8_isolate().thread_safe_handle();
    let registration = control.attach(handle.clone(), Arc::clone(signal), deadline);
    let heap_signal = Arc::clone(signal);
    let heap_control = Arc::clone(control);
    runtime.add_near_heap_limit_callback(move |current, _initial| {
        heap_control.stop_request(&heap_signal, StopReason::Heap);
        handle.terminate_execution();
        // Allow V8 temporary allocation headroom to deliver termination.
        // This does not enforce an OS process-memory cap.
        current.saturating_add(32 * 1024 * 1024)
    });
    let mut session = Session {
        runtime,
        _registration: registration,
    };
    #[cfg(test)]
    if request.page().as_str() == "panic" {
        tests::inject_panic();
    }
    #[cfg(test)]
    let initialized;
    // All V8 handles/scopes end before the measured isolate disposal below.
    let output = {
        let runtime = &mut session.runtime;
        match &input.source {
            RuntimeSource::Bundle(bundle) => {
                runtime
                    .execute_script("gyral-app.js", bundle.0.to_string())
                    .map_err(|error| RenderError::JavaScript {
                        detail: error.to_string(),
                    })?;
            }
            #[cfg(test)]
            RuntimeSource::Snapshot(_) => {}
        }
        #[cfg(test)]
        {
            initialized = Instant::now();
        }
        let version = runtime
            .execute_script("gyral-contract.js", "globalThis.gyralBundleVersion")
            .map_err(|error| RenderError::JavaScript {
                detail: error.to_string(),
            })?;
        {
            deno_core::scope!(scope, runtime);
            let value = v8::Local::new(scope, version);
            let received = if value.is_uint32() {
                value.uint32_value(scope)
            } else {
                None
            };
            if received != Some(BUNDLE_ABI_VERSION) {
                return Err(RenderError::UnsupportedBundleAbi { received });
            }
        }
        let encoded = serde_json::to_string(request).map_err(RenderError::Encode)?;
        let literal = serde_json::to_string(&encoded).map_err(RenderError::Encode)?;
        let call = format!("globalThis.gyralRender(JSON.parse({literal}))");
        let value = runtime
            .execute_script("gyral-request.js", call)
            .map_err(|error| RenderError::JavaScript {
                detail: error.to_string(),
            })?;
        let resolving = runtime.resolve(value);
        let resolved = runtime
            .with_event_loop_promise(resolving, PollEventLoopOptions::default())
            .await
            .map_err(|error| RenderError::JavaScript {
                detail: error.to_string(),
            })?;
        {
            deno_core::scope!(scope, runtime);
            let value = v8::Local::new(scope, resolved);
            let html =
                v8::Local::<v8::String>::try_from(value).map_err(|_| RenderError::InvalidResult)?;
            // A rope can represent enormous output using little managed heap. Reject it
            // before flattening/copying into Rust. At most this many UTF-16 code units
            // reach conversion; the domain constructor then enforces exact UTF-8 bytes.
            if html.length() > RenderedHtml::MAX_BYTES {
                return Err(RenderError::Contract(crate::ContractError::HtmlTooLarge));
            }
            RenderedHtml::try_from(html.to_rust_string_lossy(scope)).map_err(RenderError::Contract)
        }
    };
    #[cfg(test)]
    let converted = Instant::now();
    drop(session);
    #[cfg(test)]
    if let Some(profile) = &input.profile {
        evaluation::record(
            profile,
            beginning,
            created,
            initialized,
            converted,
            Instant::now(),
        );
    }
    output
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "evaluation.rs"]
mod evaluation;
