//! Host-owned readiness, telemetry, and shutdown policy for the SSR example.
use axum::{
    body::Body,
    extract::{MatchedPath, State},
    http::{Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use gyral_renderer::{PageId, RenderRequest, Renderer};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use opentelemetry::{propagation::Extractor, trace::TracerProvider};
use opentelemetry_otlp::{WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::{propagation::TraceContextPropagator, trace::SdkTracerProvider};
use std::{
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, thiserror::Error)]
pub(crate) enum OperationsError {
    #[error("invalid telemetry configuration")]
    Configuration,
    #[error("could not initialize telemetry")]
    Telemetry,
    #[error("could not initialize request identifiers")]
    Random,
    #[error("renderer startup probe failed: {category}")]
    Probe { category: &'static str },
    #[error("could not install shutdown signals")]
    Signal,
    #[error("HTTP drain exceeded its five-second budget")]
    Drain,
    #[error("telemetry shutdown did not complete")]
    Flush,
    #[error("readiness task failed")]
    Task,
}

pub(crate) struct Telemetry {
    pub(crate) metrics: PrometheusHandle,
    provider: Option<SdkTracerProvider>,
}

impl Telemetry {
    // Called once outside the host executor; the blocking OTLP client and batch
    // processor own their own threads. Endpoint/header values never enter logs.
    pub(crate) fn start() -> Result<Self, OperationsError> {
        let filter = match std::env::var("RUST_LOG") {
            Ok(value) => EnvFilter::try_new(value).map_err(|_| OperationsError::Configuration)?,
            Err(std::env::VarError::NotPresent) => EnvFilter::new("info,hyper=warn,reqwest=warn"),
            Err(std::env::VarError::NotUnicode(_)) => return Err(OperationsError::Configuration),
        };
        let endpoint = match std::env::var("GYRAL_OTLP_ENDPOINT") {
            Ok(value) => {
                let uri = value
                    .parse::<axum::http::Uri>()
                    .map_err(|_| OperationsError::Configuration)?;
                if !matches!(uri.scheme_str(), Some("http" | "https"))
                    || uri.host().is_none()
                    || uri
                        .authority()
                        .is_some_and(|authority| authority.as_str().contains('@'))
                    || uri.query().is_some()
                {
                    return Err(OperationsError::Configuration);
                }
                Some(value)
            }
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => return Err(OperationsError::Configuration),
        };
        let provider = endpoint
            .map(|endpoint| {
                let exporter = opentelemetry_otlp::SpanExporter::builder()
                    .with_http()
                    .with_endpoint(endpoint)
                    .with_protocol(opentelemetry_otlp::Protocol::HttpBinary)
                    .with_timeout(Duration::from_secs(1))
                    .with_retry_policy(
                        opentelemetry_otlp::RetryPolicy::default().with_max_retries(0),
                    )
                    .build()
                    .map_err(|_| OperationsError::Telemetry)?;
                Ok(SdkTracerProvider::builder()
                    .with_batch_exporter(exporter)
                    .with_resource(
                        opentelemetry_sdk::Resource::builder()
                            .with_service_name("gyral-axum-example")
                            .build(),
                    )
                    .build())
            })
            .transpose()?;
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
        let otel = provider.as_ref().map(|provider| {
            tracing_opentelemetry::layer().with_tracer(provider.tracer("gyral-rs"))
        });
        tracing_subscriber::registry()
            .with(filter)
            .with(otel)
            .with(tracing_subscriber::fmt::layer().json())
            .try_init()
            .map_err(|_| OperationsError::Telemetry)?;
        let metrics = PrometheusBuilder::new()
            .set_buckets(&[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 3.0])
            .map_err(|_| OperationsError::Telemetry)?
            .install_recorder()
            .map_err(|_| OperationsError::Telemetry)?;
        Ok(Self { metrics, provider })
    }

    pub(crate) fn exporting(&self) -> bool {
        self.provider.is_some()
    }

    pub(crate) fn shutdown(self) -> Result<(), OperationsError> {
        if let Some(provider) = self.provider {
            provider
                .shutdown_with_timeout(Duration::from_secs(2))
                .map_err(|_| OperationsError::Flush)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
enum Probe {
    Never,
    Healthy(Instant),
    Failed,
}

#[derive(Debug)]
pub(crate) struct Operations {
    renderer: Renderer,
    metrics: PrometheusHandle,
    probe: Mutex<Probe>,
    draining: AtomicBool,
    stop: tokio::sync::Notify,
    prefix: String,
    sequence: AtomicU64,
    traces_enabled: bool,
}

impl Operations {
    pub(crate) fn new(
        renderer: Renderer,
        metrics: PrometheusHandle,
        traces_enabled: bool,
    ) -> Result<Arc<Self>, OperationsError> {
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).map_err(|_| OperationsError::Random)?;
        let prefix = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(Arc::new(Self {
            renderer,
            metrics,
            probe: Mutex::new(Probe::Never),
            draining: AtomicBool::new(false),
            stop: tokio::sync::Notify::new(),
            prefix,
            sequence: AtomicU64::new(0),
            traces_enabled,
        }))
    }

    pub(crate) async fn probe(&self) -> Result<(), OperationsError> {
        let request = PageId::try_from("home".to_owned())
            .and_then(|page| RenderRequest::try_new(page, serde_json::json!({"name": "Readiness"})))
            .map_err(|_| OperationsError::Probe {
                category: "contract",
            })?;
        match self.renderer.render(request).await {
            Ok(_) => {
                *self.probe.lock().unwrap_or_else(PoisonError::into_inner) =
                    Probe::Healthy(Instant::now());
                Ok(())
            }
            Err(error) => {
                // Temporary queue saturation does not erase a recent successful probe.
                if !matches!(error, gyral_renderer::RenderError::Busy) {
                    *self.probe.lock().unwrap_or_else(PoisonError::into_inner) = Probe::Failed;
                }
                Err(OperationsError::Probe {
                    category: error.category().as_str(),
                })
            }
        }
    }

    fn ready(&self) -> bool {
        !self.draining.load(Ordering::Acquire)
            && self.renderer.status().accepting()
            && matches!(*self.probe.lock().unwrap_or_else(PoisonError::into_inner), Probe::Healthy(at) if at.elapsed() < Duration::from_secs(15))
    }

    pub(crate) fn begin_drain(&self) {
        self.draining.store(true, Ordering::Release);
        self.stop.notify_one();
        tracing::info!("HTTP admission draining");
    }

    pub(crate) async fn maintain(self: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // The startup probe has already run. Do not probe twice at startup.
        interval.tick().await;
        loop {
            if self.draining.load(Ordering::Acquire) {
                break;
            }
            tokio::select! {
                () = self.stop.notified() => break,
                _ = interval.tick() => {
                    if let Err(error) = self.probe().await { tracing::warn!(%error, "readiness probe failed"); }
                    self.metrics.run_upkeep();
                }
            }
        }
    }
}

pub(crate) async fn live() -> Response {
    super::opaque(StatusCode::OK, "alive")
}

pub(crate) async fn ready(State(operations): State<Arc<Operations>>) -> Response {
    if operations.ready() {
        super::opaque(StatusCode::OK, "ready")
    } else {
        super::opaque(StatusCode::SERVICE_UNAVAILABLE, "not ready")
    }
}

pub(crate) async fn metrics(State(operations): State<Arc<Operations>>) -> Response {
    let status = operations.renderer.status();
    // Gauges are snapshots at scrape time, avoiding stale last-writer values.
    metrics::gauge!("gyral_renderer_queue_depth")
        .set(u32::try_from(status.queued).map_or(f64::MAX, f64::from));
    metrics::gauge!("gyral_renderer_queue_capacity")
        .set(u32::try_from(status.capacity).map_or(f64::MAX, f64::from));
    metrics::gauge!("gyral_renderer_active").set(f64::from(u8::from(status.active)));
    metrics::gauge!("gyral_renderer_ready").set(f64::from(u8::from(operations.ready())));
    operations.metrics.run_upkeep();
    (
        [
            (
                header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        operations.metrics.render(),
    )
        .into_response()
}

struct Headers<'a>(&'a axum::http::HeaderMap);
impl Extractor for Headers<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }
    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|key| key.as_str()).collect()
    }
}

pub(crate) async fn observe(
    State(operations): State<Arc<Operations>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("unmatched", MatchedPath::as_str)
        .to_owned();
    let method = match request.method().as_str() {
        "GET" => "GET",
        "HEAD" => "HEAD",
        "POST" => "POST",
        "PUT" => "PUT",
        "DELETE" => "DELETE",
        "OPTIONS" => "OPTIONS",
        "PATCH" => "PATCH",
        "CONNECT" => "CONNECT",
        "TRACE" => "TRACE",
        _ => "OTHER",
    };
    let request_id = format!(
        "{}-{:016x}",
        operations.prefix,
        operations.sequence.fetch_add(1, Ordering::Relaxed)
    );
    let span = tracing::info_span!("http_request", request.id = %request_id, http.method = method, http.route = %route, http.status = tracing::field::Empty);
    let context = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&Headers(request.headers()))
    });
    if operations.traces_enabled
        && !span.is_disabled()
        && let Err(error) = span.set_parent(context)
    {
        tracing::warn!(%error, "trace parent not attached");
    }
    async {
        let started = Instant::now();
        let mut response = if operations.draining.load(Ordering::Acquire) && !matches!(route.as_str(), "/health" | "/livez" | "/readyz" | "/metrics") {
            super::opaque(StatusCode::SERVICE_UNAVAILABLE, "The server is draining.")
        } else { next.run(request).await };
        if !response.headers().contains_key(header::CACHE_CONTROL) {
            response.headers_mut().insert(header::CACHE_CONTROL, axum::http::HeaderValue::from_static("no-store"));
        }
        if !response.headers().contains_key(header::X_CONTENT_TYPE_OPTIONS) {
            response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, axum::http::HeaderValue::from_static("nosniff"));
        }
        let status = response.status().as_u16().to_string();
        tracing::Span::current().record("http.status", response.status().as_u16());
        metrics::counter!("gyral_http_requests_total", "route" => route.clone(), "method" => method, "status" => status.clone()).increment(1);
        metrics::histogram!("gyral_http_duration_seconds", "route" => route, "method" => method, "status" => status).record(started.elapsed().as_secs_f64());
        if let Ok(id) = axum::http::HeaderValue::from_str(&request_id) { response.headers_mut().insert("x-request-id", id); }
        tracing::info!("HTTP request completed");
        response
    }.instrument(span).await
}

pub(crate) struct Signals {
    terminate: tokio::signal::unix::Signal,
    interrupt: tokio::signal::unix::Signal,
}

impl Signals {
    pub(crate) fn install() -> Result<Self, OperationsError> {
        Ok(Self {
            terminate: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(|_| OperationsError::Signal)?,
            interrupt: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .map_err(|_| OperationsError::Signal)?,
        })
    }

    pub(crate) async fn wait(mut self) {
        tokio::select! {
            _ = self.interrupt.recv() => {},
            _ = self.terminate.recv() => {},
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        reason = "bounded fixtures prove startup invariants"
    )]
    use super::*;

    #[tokio::test]
    async fn readiness_requires_recent_probe_live_worker_and_open_host_admission() {
        let bundle = gyral_renderer::AppBundle::try_from(
            "globalThis.gyralBundleVersion=1;globalThis.gyralRender=()=>'<p>ready</p>';".to_owned(),
        )
        .expect("invariant: bundle bounded");
        let (renderer, worker) = Renderer::start(bundle).expect("invariant: native worker starts");
        let metrics = PrometheusBuilder::new().build_recorder().handle();
        let operations =
            Operations::new(renderer, metrics, false).expect("invariant: OS supplies randomness");
        assert!(!operations.ready(), "thread startup alone is insufficient");
        operations
            .probe()
            .await
            .expect("invariant: fixture renders");
        assert!(operations.ready());
        *operations.probe.lock().expect("invariant: lock unpoisoned") =
            Probe::Healthy(Instant::now() - Duration::from_secs(16));
        assert!(!operations.ready(), "stale evidence must not route traffic");
        operations
            .probe()
            .await
            .expect("invariant: fixture recovers");
        assert!(operations.ready());
        *operations.probe.lock().expect("invariant: lock unpoisoned") = Probe::Failed;
        assert!(!operations.ready());
        operations
            .probe()
            .await
            .expect("invariant: fixture recovers");
        worker
            .shutdown()
            .await
            .expect("invariant: idle worker joins");
        assert!(
            !operations.ready(),
            "healthy historical probe cannot hide a stopped worker"
        );
    }

    #[tokio::test]
    async fn drain_fails_readiness_before_renderer_admission_closes() {
        let bundle = gyral_renderer::AppBundle::try_from(
            "globalThis.gyralBundleVersion=1;globalThis.gyralRender=()=>'<p>ready</p>';".to_owned(),
        )
        .expect("invariant: bundle bounded");
        let (renderer, worker) = Renderer::start(bundle).expect("invariant: native worker starts");
        let operations = Operations::new(
            renderer.clone(),
            PrometheusBuilder::new().build_recorder().handle(),
            false,
        )
        .expect("invariant: OS supplies randomness");
        operations
            .probe()
            .await
            .expect("invariant: fixture renders");
        operations.begin_drain();
        assert!(!operations.ready());
        assert!(
            renderer.status().accepting(),
            "accepted HTTP handlers can still finish"
        );
        worker
            .shutdown()
            .await
            .expect("invariant: idle worker joins");
    }
}
