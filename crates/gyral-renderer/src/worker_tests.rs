#![allow(
    clippy::expect_used,
    reason = "test fixtures prove constructor and runtime invariants"
)]
use super::*;
use std::{
    future::{Future, poll_fn},
    task::Poll,
    time::Duration,
};

const BUNDLE: &str = "globalThis.gyralBundleVersion = 1; globalThis.gyralRender = ({page}) => { if(page === 'loop') { while(true) {} } return '<p>healthy</p>'; };";

fn stalled_renderer() -> (Renderer, mpsc::Receiver<Message>) {
    let (sender, receiver) = mpsc::channel(16);
    (
        Renderer {
            sender,
            control: Arc::new(Control::default()),
            limits: RenderLimits::default(),
        },
        receiver,
    )
}

#[test]
fn missing_host_runtime_returns_a_typed_error_without_admitting_work() {
    let (renderer, receiver) = stalled_renderer();
    let mut future = Box::pin(renderer.render(request("home")));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(matches!(
        future.as_mut().poll(&mut context),
        Poll::Ready(Err(RenderError::HostRuntimeUnavailable))
    ));
    assert!(receiver.is_empty());
}

#[test]
fn a_host_without_time_returns_a_typed_error_without_admitting_work() {
    let (renderer, receiver) = stalled_renderer();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("invariant: test host supports a timer-free executor");
    runtime.block_on(async {
        assert!(matches!(
            renderer.render(request("home")).await,
            Err(RenderError::HostRuntimeUnavailable)
        ));
        assert!(receiver.is_empty());
    });
}

#[test]
fn shutdown_without_time_returns_a_typed_error_and_still_ends_the_threads() {
    let (renderer, mut worker) = start(Duration::from_secs(2));
    let (render_thread, watchdog) = worker
        .joins
        .take()
        .expect("invariant: startup creates both threads");
    let mut future = Box::pin(worker.shutdown());
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(matches!(
        future.as_mut().poll(&mut context),
        Poll::Ready(Err(WorkerError::HostRuntimeUnavailable))
    ));
    drop(future);
    host().block_on(async {
        assert!(matches!(
            renderer.render(request("home")).await,
            Err(RenderError::Unavailable)
        ));
        tokio::time::timeout(Duration::from_secs(2), async {
            while !render_thread.is_finished() || !watchdog.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("invariant: invalid host configuration still requests worker cleanup");
        render_thread
            .join()
            .expect("invariant: worker did not panic");
        watchdog.join().expect("invariant: watchdog did not panic");
    });
}

fn request(page: &str) -> RenderRequest {
    RenderRequest::try_new(
        crate::PageId::try_from(page.to_owned()).expect("invariant: fixture page is valid"),
        serde_json::json!({}),
    )
    .expect("invariant: fixture props are a small object")
}

fn host() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("invariant: test host supports Tokio")
}

fn start(shutdown: Duration) -> (Renderer, RendererWorker) {
    let limits = RenderLimits::try_new(Duration::from_secs(10), 64 * 1024 * 1024, shutdown)
        .expect("invariant: fixture limits are valid");
    Renderer::with_limits(
        AppBundle::try_from(BUNDLE.to_owned()).expect("invariant: fixture bundle is valid"),
        limits,
    )
    .expect("invariant: test host supports workers")
}

async fn submit(
    future: &mut std::pin::Pin<Box<impl Future<Output = Result<RenderedHtml, RenderError>>>>,
) {
    poll_fn(|context| {
        assert!(matches!(future.as_mut().poll(context), Poll::Pending));
        Poll::Ready(())
    })
    .await;
}

async fn healthy(renderer: &Renderer) {
    let html = tokio::time::timeout(Duration::from_secs(2), renderer.render(request("home")))
        .await
        .expect("invariant: interruption must release the worker before its ten-second deadline")
        .expect("invariant: fresh healthy isolate renders after cancellation");
    assert_eq!(html.as_str(), "<p>healthy</p>");
}

#[allow(
    clippy::panic,
    reason = "inject an upstream Rust unwind after registering a real isolate"
)]
pub(super) fn inject_panic() {
    panic!("simulated renderer dependency unwind");
}

#[test]
fn dropping_an_active_future_interrupts_javascript_and_preserves_the_worker() {
    let (renderer, worker) = start(Duration::from_secs(2));
    host().block_on(async {
        let mut pending = Box::pin(renderer.render(request("loop")));
        submit(&mut pending).await;
        assert!(renderer.control.wait_for_active());
        drop(pending);
        healthy(&renderer).await;
        worker
            .shutdown()
            .await
            .expect("invariant: cancelled worker can join");
    });
}

#[test]
fn a_render_unwind_disposes_the_isolate_and_the_worker_recovers() {
    let (renderer, worker) = start(Duration::from_secs(2));
    host().block_on(async {
        assert!(matches!(
            renderer.render(request("panic")).await,
            Err(RenderError::WorkerPanicked)
        ));
        healthy(&renderer).await;
        worker
            .shutdown()
            .await
            .expect("invariant: recovered worker can join");
    });
}

#[test]
fn owner_drop_closes_admission_interrupts_active_work_and_ends_both_threads() {
    let (renderer, mut worker) = start(Duration::from_secs(2));
    let completion = worker
        .done
        .take()
        .expect("invariant: startup creates completion channel");
    let (render_thread, watchdog) = worker
        .joins
        .take()
        .expect("invariant: startup creates both threads");
    host().block_on(async {
        let mut pending = Box::pin(renderer.render(request("loop")));
        submit(&mut pending).await;
        assert!(renderer.control.wait_for_active());
        drop(worker);
        assert!(matches!(
            renderer.render(request("home")).await,
            Err(RenderError::Unavailable)
        ));
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(2), pending)
                .await
                .expect("invariant: owner drop interrupts active work"),
            Err(RenderError::Unavailable)
        ));
        tokio::time::timeout(Duration::from_secs(2), completion)
            .await
            .expect("invariant: dropped owner's thread ends")
            .expect("invariant: completion is signalled");
        tokio::time::timeout(Duration::from_secs(2), async {
            while !render_thread.is_finished() || !watchdog.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("invariant: both threads finish after owner drop");
        render_thread
            .join()
            .expect("invariant: worker did not panic");
        watchdog.join().expect("invariant: watchdog did not panic");
    });
}

#[test]
fn shutdown_deadline_interrupts_active_work_and_discards_the_queue() {
    let (renderer, worker) = start(Duration::from_millis(100));
    host().block_on(async {
        let mut active = Box::pin(renderer.render(request("loop")));
        submit(&mut active).await;
        assert!(renderer.control.wait_for_active());
        let mut queued = Box::pin(renderer.render(request("home")));
        submit(&mut queued).await;
        let result = tokio::time::timeout(Duration::from_secs(2), worker.shutdown()).await.expect("invariant: shutdown has a bounded caller wait");
        assert!(matches!(result, Err(WorkerError::ShutdownTimedOut { after }) if after == Duration::from_millis(100)), "{result:?}");
        for future in [active, queued] {
            assert!(matches!(tokio::time::timeout(Duration::from_secs(2), future).await.expect("invariant: stopping resolves accepted requests"), Err(RenderError::Unavailable)));
        }
        assert!(matches!(renderer.render(request("home")).await, Err(RenderError::Unavailable)));
    });
}

#[test]
fn cancelling_shutdown_still_interrupts_active_work_and_closes_admission() {
    let (renderer, worker) = start(Duration::from_secs(2));
    host().block_on(async {
        let mut active = Box::pin(renderer.render(request("loop")));
        submit(&mut active).await;
        assert!(renderer.control.wait_for_active());
        let mut shutdown = Box::pin(worker.shutdown());
        poll_fn(|context| {
            assert!(matches!(shutdown.as_mut().poll(context), Poll::Pending));
            Poll::Ready(())
        })
        .await;
        assert!(matches!(
            renderer.render(request("home")).await,
            Err(RenderError::Unavailable)
        ));
        drop(shutdown);
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(2), active)
                .await
                .expect("invariant: cancelled shutdown interrupts active work"),
            Err(RenderError::Unavailable)
        ));
    });
}

#[test]
fn a_full_queue_rejects_overload_and_a_queued_request_has_its_own_deadline() {
    let (sender, mut receiver) = mpsc::channel(16);
    let limits = RenderLimits::try_new(
        Duration::from_millis(100),
        64 * 1024 * 1024,
        Duration::from_secs(2),
    )
    .expect("invariant: fixture limits are valid");
    // A deliberately stalled driver makes admission and queue-time behavior deterministic.
    let renderer = Renderer {
        sender,
        control: Arc::new(Control::default()),
        limits,
    };
    host().block_on(async {
        let mut pending = Vec::new();
        for _ in 0..16 {
            let mut future = Box::pin(renderer.render(request("home")));
            submit(&mut future).await;
            pending.push(future);
        }
        assert!(matches!(renderer.render(request("home")).await, Err(RenderError::Busy)));
        let oldest = pending.remove(0);
        assert!(matches!(oldest.await, Err(RenderError::DeadlineExceeded { after }) if after == Duration::from_millis(100)));
        drop(pending);
        while let Ok(message) = receiver.try_recv() {
            if let Message::Render { signal, reply, .. } = message {
                assert!(reply.is_closed());
                assert!(signal.reason().is_some());
            }
        }
    });
}
