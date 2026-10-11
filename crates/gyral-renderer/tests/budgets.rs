//! Real V8 failures run in separate nextest processes with a runner timeout.
#![allow(
    clippy::expect_used,
    reason = "test fixtures prove constructor and runtime invariants"
)]
use gyral_renderer::{AppBundle, PageId, RenderError, RenderLimits, RenderRequest, Renderer};
use serde_json::json;
use std::time::Duration;

fn request(page: &str) -> RenderRequest {
    RenderRequest::try_new(
        PageId::try_from(page.to_owned()).expect("invariant: fixture page is valid"),
        json!({}),
    )
    .expect("invariant: fixture props are a small object")
}

fn host() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("invariant: test host supports Tokio")
}

#[test]
fn infinite_script_is_interrupted_and_the_worker_accepts_a_healthy_request() {
    let source = "globalThis.gyralBundleVersion = 1; globalThis.gyralRender = ({page}) => { if(page === 'loop') { while(true) {} } return '<p>healthy</p>'; };";
    let limits = RenderLimits::try_new(
        Duration::from_millis(400),
        64 * 1024 * 1024,
        Duration::from_secs(2),
    )
    .expect("invariant: fixture policy is within documented bounds");
    let (renderer, worker) = Renderer::with_limits(
        AppBundle::try_from(source.to_owned()).expect("invariant: fixture bundle is valid"),
        limits,
    )
    .expect("invariant: test host supports worker threads");
    host().block_on(async {
        let error = renderer.render(request("loop")).await;
        assert!(matches!(error, Err(RenderError::DeadlineExceeded { after }) if after == Duration::from_millis(400)), "{error:?}");
        assert_eq!(renderer.render(request("home")).await.expect("invariant: interruption permits a fresh healthy isolate").as_str(), "<p>healthy</p>");
        worker.shutdown().await.expect("invariant: completed worker joins");
    });
}

#[test]
fn infinite_microtask_is_interrupted_and_the_next_request_succeeds() {
    let source = "globalThis.gyralBundleVersion = 1; globalThis.gyralRender = ({page}) => page === 'loop' ? Promise.resolve().then(() => {while(true) {}}) : '<p>healthy</p>';";
    let limits = RenderLimits::try_new(
        Duration::from_millis(400),
        64 * 1024 * 1024,
        Duration::from_secs(2),
    )
    .expect("invariant: fixture policy is valid");
    let (renderer, worker) = Renderer::with_limits(
        AppBundle::try_from(source.to_owned()).expect("invariant: fixture bundle is valid"),
        limits,
    )
    .expect("invariant: test host supports workers");
    host().block_on(async {
        assert!(matches!(
            renderer.render(request("loop")).await,
            Err(RenderError::DeadlineExceeded { .. })
        ));
        assert_eq!(
            renderer
                .render(request("home"))
                .await
                .expect("invariant: fresh isolate survives prior microtask termination")
                .as_str(),
            "<p>healthy</p>"
        );
        worker
            .shutdown()
            .await
            .expect("invariant: completed worker joins");
    });
}

#[test]
fn the_deadline_also_interrupts_bundle_initialization() {
    let limits = RenderLimits::try_new(
        Duration::from_millis(400),
        64 * 1024 * 1024,
        Duration::from_secs(2),
    )
    .expect("invariant: fixture policy is valid");
    let (renderer, worker) = Renderer::with_limits(
        AppBundle::try_from("while(true) {}".to_owned())
            .expect("invariant: fixture bundle is valid"),
        limits,
    )
    .expect("invariant: test host supports workers");
    host().block_on(async {
        assert!(matches!(
            renderer.render(request("home")).await,
            Err(RenderError::DeadlineExceeded { .. })
        ));
        worker
            .shutdown()
            .await
            .expect("invariant: terminated initialization joins");
    });
}

#[test]
fn managed_heap_pressure_is_reported_and_a_fresh_request_succeeds() {
    let source = "globalThis.gyralBundleVersion = 1; globalThis.gyralRender = ({page}) => { if(page === 'heap') { const values=[]; while(true) { values.push(new Array(10000).fill(values.length)); } } return '<p>healthy</p>'; };";
    let bytes = 16 * 1024 * 1024;
    let limits = RenderLimits::try_new(Duration::from_secs(10), bytes, Duration::from_secs(2))
        .expect("invariant: fixture policy is valid");
    let (renderer, worker) = Renderer::with_limits(
        AppBundle::try_from(source.to_owned()).expect("invariant: fixture bundle is valid"),
        limits,
    )
    .expect("invariant: test host supports workers");
    host().block_on(async {
        let error = tokio::time::timeout(Duration::from_secs(3), renderer.render(request("heap"))).await.expect("invariant: heap callback should interrupt well before the render deadline");
        assert!(matches!(error, Err(RenderError::HeapBudgetExceeded { bytes: reported }) if reported == bytes), "{error:?}");
        assert_eq!(renderer.render(request("home")).await.expect("invariant: fresh isolate survives prior heap termination").as_str(), "<p>healthy</p>");
        worker.shutdown().await.expect("invariant: completed worker joins");
    });
}

#[test]
fn oversized_rope_is_rejected_without_a_large_rust_allocation() {
    let source = "globalThis.gyralBundleVersion = 1; globalThis.gyralRender = ({page}) => page === 'huge' ? 'x'.repeat(500*1024*1024) : '<p>healthy</p>';";
    let (renderer, worker) = Renderer::start(
        AppBundle::try_from(source.to_owned()).expect("invariant: fixture bundle is valid"),
    )
    .expect("invariant: test host supports workers");
    host().block_on(async {
        assert!(matches!(
            renderer.render(request("huge")).await,
            Err(RenderError::Contract(
                gyral_renderer::ContractError::HtmlTooLarge
            ))
        ));
        assert_eq!(
            renderer
                .render(request("home"))
                .await
                .expect("invariant: oversized output leaves the worker available")
                .as_str(),
            "<p>healthy</p>"
        );
        worker
            .shutdown()
            .await
            .expect("invariant: completed worker joins");
    });
    #[cfg(target_os = "linux")]
    {
        // Linux's high-water mark catches a regression that first copies the
        // 500 MiB rope into Rust and only then rejects its UTF-8 length.
        let status = std::fs::read_to_string("/proc/self/status")
            .expect("invariant: Linux test host exposes proc status");
        let peak_kib = status
            .lines()
            .find_map(|line| {
                line.strip_prefix("VmHWM:")?
                    .split_whitespace()
                    .next()?
                    .parse::<usize>()
                    .ok()
            })
            .expect("invariant: proc status records numeric VmHWM in KiB");
        assert!(
            peak_kib < 300 * 1024,
            "oversized output copied into native memory: peak {peak_kib} KiB"
        );
    }
}
