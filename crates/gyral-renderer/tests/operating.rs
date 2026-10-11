//! Public operational contracts use finite categories and observable lifecycle.
#![allow(
    clippy::expect_used,
    reason = "bounded fixtures establish constructor invariants"
)]
use gyral_renderer::{AppBundle, PageId, RenderError, RenderRequest, Renderer, RendererPhase};

#[tokio::test]
async fn status_distinguishes_accepting_and_completed_shutdown() {
    let bundle = AppBundle::try_from(
        "globalThis.gyralBundleVersion=1;globalThis.gyralRender=()=>'<p>ready</p>';".to_owned(),
    )
    .expect("invariant: bundle is bounded");
    let (renderer, worker) = Renderer::start(bundle).expect("invariant: native worker starts");
    let running = renderer.status();
    assert_eq!(running.phase, RendererPhase::Running);
    assert!(running.accepting());
    assert_eq!(running.capacity, 16);
    assert_eq!(running.queued, 0);
    let request = RenderRequest::try_new(
        PageId::try_from("health".to_owned()).expect("invariant: page is valid"),
        serde_json::json!({}),
    )
    .expect("invariant: props bounded");
    assert_eq!(
        renderer
            .render(request)
            .await
            .expect("invariant: fixture renders")
            .as_str(),
        "<p>ready</p>"
    );
    worker
        .shutdown()
        .await
        .expect("invariant: completed request drains");
    let stopped = renderer.status();
    assert_eq!(stopped.phase, RendererPhase::Stopped);
    assert!(!stopped.accepting());
    assert!(!stopped.active);
    assert_eq!(stopped.queued, 0);
}

#[test]
fn categories_omit_sensitive_diagnostics_and_keep_errors_distinct() {
    let errors = [
        (
            RenderError::UnsupportedBundleAbi { received: Some(99) },
            "bundle_abi",
        ),
        (RenderError::HostRuntimeUnavailable, "host_runtime"),
        (RenderError::Busy, "busy"),
        (RenderError::Unavailable, "unavailable"),
        (
            RenderError::DeadlineExceeded {
                after: std::time::Duration::from_secs(1),
            },
            "deadline",
        ),
        (RenderError::HeapBudgetExceeded { bytes: 1234 }, "heap"),
        (RenderError::Cancelled, "cancelled"),
        (RenderError::WorkerPanicked, "worker_panic"),
        (
            RenderError::Runtime {
                detail: "PRIVATE_RUNTIME".to_owned(),
            },
            "runtime",
        ),
        (
            RenderError::JavaScript {
                detail: "PRIVATE_PROPS".to_owned(),
            },
            "javascript",
        ),
        (RenderError::InvalidResult, "invalid_result"),
        (
            RenderError::Contract(gyral_renderer::ContractError::HtmlTooLarge),
            "contract",
        ),
        (
            RenderError::Encode(
                serde_json::from_str::<serde_json::Value>("{")
                    .expect_err("invariant: fixture is invalid JSON"),
            ),
            "encode",
        ),
    ];
    let mut labels = std::collections::BTreeSet::new();
    for (error, expected) in errors {
        assert_eq!(error.category().as_str(), expected);
        assert!(!format!("{:?}", error.category()).contains("PRIVATE"));
        assert!(
            labels.insert(expected),
            "each error has a distinct category"
        );
    }
}

#[tokio::test]
async fn caller_recorder_counts_success_errors_and_dropped_futures_without_diagnostics() {
    use std::{future::Future, task::Poll};
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new()
        .set_buckets(&[0.01, 1.0, 3.0])
        .expect("invariant: buckets valid")
        .build_recorder();
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let bundle = AppBundle::try_from("globalThis.gyralBundleVersion=1;globalThis.gyralRender=request=>{if(request.page==='loop'){while(true){}}if(request.page==='fail')throw new Error(request.props.secret);return '<p>ok</p>';};".to_owned()).expect("invariant: source bounded");
    let (renderer, worker) = Renderer::start(bundle).expect("invariant: native worker starts");
    let request = |page: &str| {
        RenderRequest::try_new(
            PageId::try_from(page.to_owned()).expect("invariant: fixture page valid"),
            serde_json::json!({"secret":"PRIVATE_OBSERVATION"}),
        )
        .expect("invariant: props bounded")
    };
    let mut pending = Box::pin(renderer.render(request("loop")));
    std::future::poll_fn(|context| {
        assert!(pending.as_mut().poll(context).is_pending());
        Poll::Ready(())
    })
    .await;
    drop(pending);
    assert!(matches!(
        renderer.render(request("fail")).await,
        Err(RenderError::JavaScript { .. })
    ));
    assert_eq!(
        renderer
            .render(request("ok"))
            .await
            .expect("invariant: fixture renders")
            .as_str(),
        "<p>ok</p>"
    );
    worker
        .shutdown()
        .await
        .expect("invariant: finite queue drains");
    handle.run_upkeep();
    let metrics = handle.render();
    for outcome in ["ok", "cancelled", "javascript"] {
        assert!(metrics.contains(&format!(
            "gyral_render_requests_total{{outcome=\"{outcome}\"}} 1"
        )));
        assert!(metrics.contains(&format!(
            "gyral_render_duration_seconds_count{{outcome=\"{outcome}\"}} 1"
        )));
    }
    assert!(!metrics.contains("PRIVATE_OBSERVATION"));
    assert!(!metrics.contains("page="));
}
