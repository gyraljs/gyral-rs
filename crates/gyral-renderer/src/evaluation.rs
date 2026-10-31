//! Test-only snapshot prototype. No public API accepts native snapshot bytes.
#![allow(
    clippy::expect_used,
    reason = "evaluation fixtures prove runtime and build invariants"
)]
use super::*;
use crate::{AppArtifacts, BundleManifest, PageId};
use deno_core::JsRuntimeForSnapshot;
use serde_json::{Value, json};
use std::{
    future::{Future, poll_fn},
    path::Path,
    sync::Mutex,
    task::Poll,
    time::Duration,
};

#[derive(Default)]
pub(super) struct StageTotals {
    count: u64,
    create_ns: u128,
    initialize_ns: u128,
    render_ns: u128,
    dispose_ns: u128,
}

pub(super) fn record(
    profile: &Mutex<StageTotals>,
    begin: Instant,
    created: Instant,
    initialized: Instant,
    converted: Instant,
    disposed: Instant,
) {
    let mut totals = profile
        .lock()
        .expect("invariant: measurement recorder does not panic");
    totals.count += 1;
    totals.create_ns += created.duration_since(begin).as_nanos();
    totals.initialize_ns += initialized.duration_since(created).as_nanos();
    totals.render_ns += converted.duration_since(initialized).as_nanos();
    totals.dispose_ns += disposed.duration_since(converted).as_nanos();
}

fn app() -> AppArtifacts {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/axum-app/dist");
    let manifest_text = std::fs::read_to_string(root.join("manifest.json"))
        .expect("invariant: pnpm builds fixture first");
    let manifest = BundleManifest::try_from(manifest_text.as_str())
        .expect("invariant: fixture uses a supported protocol and release");
    let source =
        std::fs::read_to_string(root.join("server.js")).expect("invariant: fixture source exists");
    let browser = std::fs::read(
        root.join("assets")
            .join(format!("client.{}.js", manifest.client_hash().hex())),
    )
    .expect("invariant: fixture client exists");
    AppArtifacts::try_new(manifest, source, browser).expect("invariant: fixture digests match")
}

fn snapshot(bundle: &AppBundle) -> &'static [u8] {
    JsRuntime::init_platform(None);
    let mut runtime = JsRuntimeForSnapshot::try_new(RuntimeOptions {
        create_params: Some(v8::Isolate::create_params().heap_limits(0, 64 * 1024 * 1024)),
        ..RuntimeOptions::default()
    })
    .expect("invariant: pinned runtime supports snapshot creation");
    runtime
        .execute_script("gyral-app.js", bundle.0.to_string())
        .expect("invariant: trusted fixture initializes without a request");
    // deno_core requires a static slice. Each nextest/benchmark process owns one
    // bounded blob until process exit. Never use this leak strategy in a service.
    Box::leak(runtime.snapshot())
}

fn input(bytes: &'static [u8]) -> RuntimeInput {
    RuntimeInput {
        source: RuntimeSource::Snapshot(bytes),
        profile: None,
    }
}

fn request(page: &str, name: &str, visits: u32) -> RenderRequest {
    RenderRequest::try_new(
        PageId::try_from(page.to_owned()).expect("invariant: fixture page is valid"),
        json!({"name":name,"visits":visits}),
    )
    .expect("invariant: fixture DTO is bounded")
}

fn host() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("invariant: host has a timer driver")
}

fn poisoned_bundle() -> AppBundle {
    let bundle = app().bundle().0.to_string();
    AppBundle::try_from(format!(r#"{bundle}
        const originalRender = globalThis.gyralRender;
        let previousDto;
        globalThis.gyralRender = async request => {{
          if (previousDto || globalThis.requestPoison) throw Error('request state leaked');
          previousDto = request.props;
          globalThis.requestPoison = request.props.name;
          if (request.page === 'reject') {{
            queueMicrotask(() => {{ globalThis.requestPoison = 'microtask poison'; }});
            throw Error('rejected request');
          }}
          if (request.page === 'loop') {{ while (true) {{}} }}
          if (request.page === 'heap') {{ const a=[]; while(true) a.push(new Array(1000000).fill(request.props.name)); }}
          return originalRender(request);
        }};
    "#)).expect("invariant: reviewed wrapper is bounded")
}

async fn assert_healthy(renderer: &Renderer, name: &str, visits: u32) -> String {
    let output = renderer
        .render(request("home", name, visits))
        .await
        .expect("invariant: fresh isolate renders its own request");
    let html = output.as_str();
    assert!(html.contains(name));
    assert!(html.contains(&format!("&quot;count&quot;:{visits}")));
    assert!(html.contains("data-gyral-stores"));
    assert!(!html.contains("data-gyral-error"));
    assert!(!html.contains("microtask poison"));
    html.to_owned()
}

#[test]
fn snapshot_preserves_real_gyral_output_without_global_dto_or_store_leaks() {
    let bundle = poisoned_bundle();
    let bytes = snapshot(&bundle);
    let (fresh, fresh_worker) = Renderer::start(bundle).expect("invariant: worker starts");
    let (restored, restored_worker) = Renderer::start_input(input(bytes), RenderLimits::default())
        .expect("invariant: snapshot worker starts");
    host().block_on(async {
        for (name, visits) in [("First private DTO", 23), ("Second private DTO", 7)] {
            let expected = assert_healthy(&fresh, name, visits).await;
            let actual = assert_healthy(&restored, name, visits).await;
            assert_eq!(
                actual, expected,
                "snapshot and fresh Gyral output must match byte-for-byte"
            );
            if name.starts_with("Second") {
                assert!(!actual.contains("First private DTO"));
            }
        }
        for renderer in [&fresh, &restored] {
            for page in ["reject", "broken"] {
                assert!(matches!(
                    renderer
                        .render(request(page, "Rejected private DTO", 99))
                        .await,
                    Err(RenderError::JavaScript { .. })
                ));
                let html = assert_healthy(renderer, "After rejection", 12).await;
                assert!(!html.contains("Rejected private DTO"));
            }
        }
        fresh_worker
            .shutdown()
            .await
            .expect("invariant: fresh worker joins");
        restored_worker
            .shutdown()
            .await
            .expect("invariant: snapshot worker joins");
    });
}

async fn submit<F: Future>(future: &mut std::pin::Pin<Box<F>>) {
    poll_fn(|context| {
        assert!(matches!(future.as_mut().poll(context), Poll::Pending));
        Poll::Ready(())
    })
    .await;
}

#[test]
fn snapshot_cancellation_deadline_and_heap_failures_leave_clean_next_requests() {
    let bytes = snapshot(&poisoned_bundle());
    let limits = RenderLimits::try_new(
        Duration::from_secs(1),
        64 * 1024 * 1024,
        Duration::from_secs(2),
    )
    .expect("invariant: fixture resource policy is valid");
    let (renderer, worker) =
        Renderer::start_input(input(bytes), limits).expect("invariant: worker starts");
    host().block_on(async {
        let mut pending = Box::pin(renderer.render(request("loop", "Cancelled private DTO", 91)));
        submit(&mut pending).await;
        assert!(renderer.control.wait_for_active());
        drop(pending);
        let html = assert_healthy(&renderer, "After cancellation", 2).await;
        assert!(!html.contains("Cancelled private DTO"));
        assert!(matches!(
            renderer.render(request("loop", "Expired DTO", 92)).await,
            Err(RenderError::DeadlineExceeded { .. })
        ));
        assert_healthy(&renderer, "After deadline", 3).await;
        let heap_result = renderer.render(request("heap", "Heap DTO", 93)).await;
        assert!(
            matches!(heap_result, Err(RenderError::HeapBudgetExceeded { .. })),
            "{heap_result:?}"
        );
        assert_healthy(&renderer, "After heap termination", 4).await;
        worker
            .shutdown()
            .await
            .expect("invariant: recovered snapshot worker joins");
    });
}

#[test]
fn snapshot_shutdown_interrupts_active_work_and_discards_queued_requests() {
    let bytes = snapshot(&poisoned_bundle());
    let limits = RenderLimits::try_new(
        Duration::from_secs(2),
        64 * 1024 * 1024,
        Duration::from_millis(100),
    )
    .expect("invariant: fixture policy is valid");
    let (renderer, worker) =
        Renderer::start_input(input(bytes), limits).expect("invariant: worker starts");
    host().block_on(async {
        let mut active = Box::pin(renderer.render(request("loop", "Active DTO", 91)));
        submit(&mut active).await;
        assert!(renderer.control.wait_for_active());
        let mut queued = Box::pin(renderer.render(request("home", "Queued DTO", 92)));
        submit(&mut queued).await;
        assert!(matches!(
            worker.shutdown().await,
            Err(WorkerError::ShutdownTimedOut { .. })
        ));
        for future in [active, queued] {
            assert!(matches!(future.await, Err(RenderError::Unavailable)));
        }
        assert!(matches!(
            renderer.render(request("home", "Late DTO", 93)).await,
            Err(RenderError::Unavailable)
        ));
    });
}

fn percentile(samples: &mut [u128], percent: usize) -> u128 {
    samples.sort_unstable();
    *samples
        .get((samples.len() * percent).div_ceil(100).saturating_sub(1))
        .expect("invariant: measurement has samples")
}

#[test]
#[ignore = "release-only benchmark; tools/evaluate-runtime.py bounds each fresh process"]
#[allow(
    clippy::assertions_on_constants,
    reason = "reject running this benchmark in debug while still compiling normal debug tests"
)]
fn measure_runtime_configuration() {
    assert!(!cfg!(debug_assertions), "measure the release test binary");
    let mode = std::env::var("GYRAL_EVAL_MODE").expect("invariant: driver selects a mode");
    let worker_count = std::env::var("GYRAL_EVAL_WORKERS")
        .expect("invariant: driver selects bounded worker count")
        .parse::<usize>()
        .expect("invariant: worker count is numeric");
    assert!([1, 2, 4].contains(&worker_count));
    let app = app();
    let bundle = app.bundle().clone();
    let starting = Instant::now();
    let source = match mode.as_str() {
        "fresh" => RuntimeSource::Bundle(bundle),
        "snapshot" => RuntimeSource::Snapshot(snapshot(&bundle)),
        _ => {
            assert_eq!(mode, "fresh", "invariant: driver uses a known mode");
            return;
        }
    };
    let preparation_us = starting.elapsed().as_micros();
    let snapshot_bytes = match source {
        RuntimeSource::Bundle(_) => 0,
        RuntimeSource::Snapshot(bytes) => bytes.len(),
    };
    let profile = Arc::new(Mutex::new(StageTotals::default()));
    let mut renderers = Vec::new();
    let mut workers = Vec::new();
    let starting = Instant::now();
    for _ in 0..worker_count {
        let source = match &source {
            RuntimeSource::Bundle(bundle) => RuntimeSource::Bundle(bundle.clone()),
            RuntimeSource::Snapshot(bytes) => RuntimeSource::Snapshot(bytes),
        };
        let (renderer, worker) = Renderer::start_input(
            RuntimeInput {
                source,
                profile: Some(Arc::clone(&profile)),
            },
            RenderLimits::default(),
        )
        .expect("invariant: benchmark workers start");
        renderers.push(renderer);
        workers.push(worker);
    }
    let worker_start_us = starting.elapsed().as_micros();
    host().block_on(async {
        let renderer = renderers.first().expect("invariant: at least one worker");
        let first = Instant::now();
        assert_healthy(renderer, "First render", 1).await;
        let first_render_us = first.elapsed().as_micros();
        for _ in 0..10 { assert_healthy(renderer, "Warmup", 2).await; }
        let mut sequential = Vec::new();
        for index in 0..50 {
            let now = Instant::now();
            assert_healthy(renderer, &format!("Sequential visitor {index}"), index).await;
            sequential.push(now.elapsed().as_micros());
        }
        let sequential_p50_us = percentile(&mut sequential, 50);
        let sequential_p95_us = percentile(&mut sequential, 95);
        *profile.lock().expect("invariant: recorder is healthy") = StageTotals::default();
        let started = Instant::now();
        let until = started + Duration::from_secs(5);
        let mut tasks = tokio::task::JoinSet::new();
        // One active client per worker measures service cost without introducing
        // a queue-latency experiment on this shared host. Admission remains bounded.
        for client in 0..worker_count {
            let client_number = u32::try_from(client).expect("invariant: at most four client IDs fit u32");
            let renderer = renderers.get(client % worker_count).expect("invariant: modulo selects an existing worker").clone();
            tasks.spawn(async move {
                let mut latencies = Vec::new();
                let mut index: u32 = 0;
                while Instant::now() < until {
                    let now = Instant::now();
                    assert_healthy(&renderer, &format!("Client {client} request {index}"), client_number * 100 + index % 100).await;
                    latencies.push(now.elapsed().as_micros());
                    index += 1;
                }
                latencies
            });
        }
        let mut samples = Vec::new();
        while let Some(result) = tasks.join_next().await { samples.extend(result.expect("invariant: client assertions pass")); }
        let elapsed_us = started.elapsed().as_micros();
        let count = samples.len();
        let p50_us = percentile(&mut samples, 50);
        let p95_us = percentile(&mut samples, 95);
        let p99_us = percentile(&mut samples, 99);
        for worker in workers { worker.shutdown().await.expect("invariant: benchmark workers join"); }
        let stages = profile.lock().expect("invariant: recorder is healthy");
        assert_eq!(u128::from(stages.count), u128::try_from(count).expect("invariant: count fits"));
        let stage_count = u128::from(stages.count);
        let report: Value = json!({
            "mode":mode,"workers":worker_count,"clients":worker_count,"gyral":app.manifest().gyral_version().to_string(),"ssr_protocol":app.manifest().protocol().get(),
            "preparation_us":preparation_us,"snapshot_bytes":snapshot_bytes,"worker_start_us":worker_start_us,"first_render_us":first_render_us,
            "sequential_p50_us":sequential_p50_us,"sequential_p95_us":sequential_p95_us,
            "samples":count,"elapsed_us":elapsed_us,"p50_us":p50_us,"p95_us":p95_us,"p99_us":p99_us,
            "mean_create_us":stages.create_ns / stage_count / 1000,
            "mean_initialize_us":stages.initialize_ns / stage_count / 1000,
            "mean_render_us":stages.render_ns / stage_count / 1000,
            "mean_dispose_us":stages.dispose_ns / stage_count / 1000,
            "queue_capacity_total":16 * worker_count,
        });
        println!("GYRAL_EVALUATION={report}");
    });
}
