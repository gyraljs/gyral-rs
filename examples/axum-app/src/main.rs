//! Runnable Axum integration example.
#![forbid(unsafe_code)]
mod operations;

use axum::{
    Router,
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use gyral_axum::DocumentPolicy;
use gyral_renderer::{AppArtifacts, AppBundle, BundleManifest, PageId, RenderRequest, Renderer};
use serde_json::json;
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
struct AppState {
    renderer: Renderer,
    artifacts: Arc<AppArtifacts>,
    policy: DocumentPolicy,
}

#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error("GYRAL_EXAMPLE_BIND must be valid Unicode")]
    BindEnvironment(#[source] std::env::VarError),
    #[error("GYRAL_EXAMPLE_BIND must be an IP address and port")]
    BindAddress(#[from] std::net::AddrParseError),
    #[error("could not read build artifacts; run pnpm build:example")]
    ArtifactFile(#[source] std::io::Error),
    #[error("build artifact {path:?} exceeds its size limit")]
    ArtifactSize { path: PathBuf },
    #[error("a build metadata/source file is not UTF-8")]
    ArtifactUtf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    Bundle(#[from] gyral_renderer::BundleError),
    #[error(transparent)]
    Manifest(#[from] gyral_renderer::ManifestError),
    #[error(transparent)]
    Artifacts(#[from] gyral_renderer::ArtifactError),
    #[error(transparent)]
    Policy(#[from] gyral_axum::PolicyError),
    #[error(transparent)]
    Worker(#[from] gyral_renderer::WorkerError),
    #[error(transparent)]
    Operations(#[from] operations::OperationsError),
    #[error("could not start the HTTP server")]
    Io(#[from] std::io::Error),
}

async fn home(
    State(state): State<AppState>,
    Query(query): Query<std::collections::BTreeMap<String, String>>,
) -> Response {
    let name = query.get("name").map_or("friend", String::as_str).trim();
    if !(2..=64).contains(&name.chars().count()) {
        return state.policy.apply(opaque(
            StatusCode::BAD_REQUEST,
            "Use 2–64 characters for your name.",
        ));
    }
    let request = PageId::try_from("home".to_owned())
        .and_then(|page| RenderRequest::try_new(page, json!({"name": name})));
    let response = match request {
        Ok(request) => gyral_axum::render_page(&state.renderer, request)
            .await
            .into_response(),
        Err(_) => opaque(StatusCode::BAD_REQUEST, "The supplied name is too large."),
    };
    state.policy.apply(response)
}

async fn failure(State(state): State<AppState>) -> Response {
    let request = PageId::try_from("broken".to_owned())
        .and_then(|page| RenderRequest::try_new(page, json!({})));
    let response = match request {
        Ok(request) => gyral_axum::render_page(&state.renderer, request)
            .await
            .into_response(),
        Err(_) => opaque(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The page could not be rendered.",
        ),
    };
    state.policy.apply(response)
}

async fn client(State(state): State<AppState>) -> Response {
    gyral_axum::client_asset(&state.artifacts)
}

fn opaque(status: StatusCode, text: &'static str) -> Response {
    (
        status,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        text,
    )
        .into_response()
}

fn read_artifact(path: &Path, max: usize) -> Result<Vec<u8>, StartupError> {
    let limit = u64::try_from(max).map_err(|_| StartupError::ArtifactSize {
        path: path.to_owned(),
    })?;
    let file = std::fs::File::open(path).map_err(StartupError::ArtifactFile)?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(StartupError::ArtifactFile)?;
    if bytes.len() > max {
        return Err(StartupError::ArtifactSize {
            path: path.to_owned(),
        });
    }
    Ok(bytes)
}

fn main() -> Result<(), StartupError> {
    let telemetry = operations::Telemetry::start()?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dist");
    let assets = std::env::var_os("GYRAL_EXAMPLE_DIST").map_or(root, PathBuf::from);
    let address = match std::env::var("GYRAL_EXAMPLE_BIND") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => "127.0.0.1:3037".to_owned(),
        Err(error @ std::env::VarError::NotUnicode(_)) => {
            return Err(StartupError::BindEnvironment(error));
        }
    }
    .parse::<std::net::SocketAddr>()?;
    let metadata = String::from_utf8(read_artifact(
        &assets.join("manifest.json"),
        BundleManifest::MAX_BYTES,
    )?)?;
    let manifest = BundleManifest::try_from(metadata.as_str())?;
    let source = String::from_utf8(read_artifact(
        &assets.join("server.js"),
        AppBundle::MAX_BYTES,
    )?)?;
    let browser = read_artifact(
        &assets
            .join("assets")
            .join(format!("client.{}.js", manifest.client_hash().hex())),
        AppArtifacts::MAX_CLIENT_BYTES,
    )?;
    let artifacts = Arc::new(AppArtifacts::try_new(manifest, source, browser)?);
    let policy = DocumentPolicy::try_new(artifacts.manifest())?;
    // V8 platform initialization happens before the Tokio executor and renderer worker start.
    let (renderer, worker) = Renderer::start(artifacts.bundle().clone())?;
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = executor.block_on(async {
        let operations = operations::Operations::new(
            renderer.clone(),
            telemetry.metrics.clone(),
            telemetry.exporting(),
        )?;
        operations.probe().await?;
        let signals = operations::Signals::install()?;
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn(Arc::clone(&operations).maintain());
        let result = async {
            let app = Router::new()
                .route("/", get(home))
                .route("/failure", get(failure))
                .merge(
                    Router::new()
                        .route("/health", get(operations::live))
                        .route("/livez", get(operations::live))
                        .route("/readyz", get(operations::ready))
                        .route("/metrics", get(operations::metrics))
                        .with_state(Arc::clone(&operations)),
                )
                .route(artifacts.manifest().client_path(), get(client))
                .fallback(|| async { opaque(StatusCode::NOT_FOUND, "Not found.") })
                .method_not_allowed_fallback(|| async {
                    opaque(StatusCode::METHOD_NOT_ALLOWED, "Method not allowed.")
                })
                .layer(tower_http::limit::RequestBodyLimitLayer::new(64 * 1024))
                .layer(tower_http::timeout::TimeoutLayer::with_status_code(
                    StatusCode::SERVICE_UNAVAILABLE,
                    std::time::Duration::from_secs(3),
                ))
                .layer(tower_http::catch_panic::CatchPanicLayer::new())
                .layer(axum::middleware::from_fn_with_state(
                    Arc::clone(&operations),
                    operations::observe,
                ))
                .with_state(AppState {
                    renderer,
                    artifacts,
                    policy,
                });
            let listener = tokio::net::TcpListener::bind(address).await?;
            let address = listener.local_addr()?;
            tracing::info!(%address, "Gyral example listening");
            let (noticed, notice) = tokio::sync::oneshot::channel();
            let draining = Arc::clone(&operations);
            let serving = axum::serve(listener, app).with_graceful_shutdown(async move {
                signals.wait().await;
                draining.begin_drain();
                // A dropped notice means the server already completed.
                let _ = noticed.send(());
                // Let the load balancer observe failed readiness before closing HTTP.
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            });
            let serving = std::future::IntoFuture::into_future(serving);
            tokio::pin!(serving);
            tokio::select! {
                result = &mut serving => result?,
                _ = notice => tokio::time::timeout(std::time::Duration::from_secs(5), &mut serving)
                    .await.map_err(|_| operations::OperationsError::Drain)??,
            }
            Ok::<(), StartupError>(())
        }
        .await;
        operations.begin_drain();
        let mut task_failed = false;
        while let Some(task) = tasks.join_next().await {
            task_failed |= task.is_err();
        }
        let shutdown = worker.shutdown().await;
        if shutdown.is_ok() {
            tracing::info!("renderer threads joined");
        }
        result?;
        shutdown?;
        if task_failed {
            return Err(operations::OperationsError::Task.into());
        }
        Ok::<(), StartupError>(())
    });
    let flush = telemetry.shutdown();
    result?;
    flush?;
    Ok(())
}
