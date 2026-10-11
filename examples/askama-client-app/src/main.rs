//! Askama renders the document; Gyral executes only in the browser.
#![forbid(unsafe_code)]
mod assets;
mod model;
mod notebook;
mod notes;
#[cfg(test)]
mod notes_tests;
mod sqlite;
#[cfg(test)]
mod tests;

use askama::Template;
use assets::Assets;
use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, FromRef, Query, State, rejection::QueryRejection},
    http::{Request, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use model::{InputError, PageParams, RawPageParams};
use std::{path::PathBuf, sync::Arc};
use tracing_subscriber::util::SubscriberInitExt as _;

#[derive(Clone)]
struct AppState {
    assets: Arc<Assets>,
    notes: Arc<dyn gyral_example_notes_app::NoteStore>,
    notebooks: Arc<notebook::NotebookPolicy>,
}
impl FromRef<AppState> for Arc<Assets> {
    fn from_ref(state: &AppState) -> Self {
        Arc::clone(&state.assets)
    }
}

#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error("GYRAL_CLIENT_BIND must be valid Unicode")]
    Environment(#[from] std::env::VarError),
    #[error("GYRAL_CLIENT_BIND must be an IP address and port")]
    Address(#[from] std::net::AddrParseError),
    #[error(transparent)]
    Assets(#[from] assets::AssetError),
    #[error("could not start the HTTP server or signal handler")]
    Io(#[from] std::io::Error),
    #[error("could not initialize logging")]
    Logging(#[from] tracing_subscriber::util::TryInitError),
    #[error(transparent)]
    Database(#[from] sqlite::OpenError),
    #[error(transparent)]
    Session(#[from] notebook::SessionError),
}

#[derive(Debug, thiserror::Error)]
enum PageError {
    #[error("invalid page query")]
    Query(#[from] QueryRejection),
    #[error(transparent)]
    Input(#[from] InputError),
    #[error("template rendering failed")]
    Render(#[from] askama::Error),
}

impl IntoResponse for PageError {
    fn into_response(self) -> Response {
        let (status, text) = match self {
            Self::Query(_) => (
                StatusCode::BAD_REQUEST,
                "Use only name and start page parameters.",
            ),
            Self::Input(InputError::Name) => (
                StatusCode::BAD_REQUEST,
                "Use 2–64 non-control characters for your name.",
            ),
            Self::Input(InputError::Count) => {
                (StatusCode::BAD_REQUEST, "Use a starting count from 0–99.")
            }
            Self::Render(_) => {
                tracing::error!(category = "template", "page rendering failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "The page could not be rendered.",
                )
            }
        };
        (status, text).into_response()
    }
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate<'a> {
    assets: &'a Assets,
    title: &'static str,
    section: &'static str,
    name: &'a str,
    start: u16,
}

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate<'a> {
    assets: &'a Assets,
    title: &'static str,
    section: &'static str,
}

#[tracing::instrument(skip_all)]
async fn home(
    State(assets): State<Arc<Assets>>,
    query: Result<Query<RawPageParams>, QueryRejection>,
) -> Result<Html<String>, PageError> {
    let params = PageParams::try_from(query?.0)?;
    Ok(Html(
        HomeTemplate {
            assets: &assets,
            title: "Askama + Gyral",
            section: "demo",
            name: params.name.as_str(),
            start: params.start.value(),
        }
        .render()?,
    ))
}

async fn about(State(assets): State<Arc<Assets>>) -> Result<Html<String>, PageError> {
    Ok(Html(
        AboutTemplate {
            assets: &assets,
            title: "How this example works",
            section: "about",
        }
        .render()?,
    ))
}

async fn script(State(assets): State<Arc<Assets>>) -> Response {
    asset_response(
        assets.script.bytes.clone(),
        "text/javascript; charset=utf-8",
    )
}

async fn css(State(assets): State<Arc<Assets>>) -> Response {
    asset_response(assets.css.bytes.clone(), "text/css; charset=utf-8")
}

fn asset_response(bytes: axum::body::Bytes, content_type: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    )
        .into_response()
}

async fn response_policy(
    State(assets): State<Arc<Assets>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let mut response = if request
        .uri()
        .query()
        .is_some_and(|query| query.len() > 4096)
    {
        (StatusCode::BAD_REQUEST, "Page parameters are too large.").into_response()
    } else {
        next.run(request).await
    };
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_SECURITY_POLICY, assets.policy.clone());
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("same-origin"),
    );
    headers
        .entry(header::CACHE_CONTROL)
        .or_insert(axum::http::HeaderValue::from_static("no-store"));
    response
}

fn router(state: AppState) -> Router {
    let assets = &state.assets;
    let notes = Router::new()
        .route("/notes", get(notes::list).post(notes::create))
        .route("/notes/new", get(notes::new_form))
        .route("/notes/{id}", get(notes::detail).post(notes::update))
        .route("/notes/{id}/edit", get(notes::edit_form))
        .route(
            "/notes/{id}/delete",
            get(notes::delete_form).post(notes::delete),
        )
        .route_layer(middleware::from_fn_with_state(
            Arc::clone(&state.notebooks),
            notebook::session_middleware,
        ));
    Router::new()
        .merge(notes)
        .route("/", get(home))
        .route("/about", get(about))
        .route("/health", get(|| async { "reachable" }))
        .route(&assets.script.path, get(script))
        .route(&assets.css.path, get(css))
        .fallback(|| async { (StatusCode::NOT_FOUND, "Not found.") })
        .method_not_allowed_fallback(|| async {
            (StatusCode::METHOD_NOT_ALLOWED, "Method not allowed.")
        })
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn_with_state(
            Arc::clone(assets),
            response_policy,
        ))
        .layer(
            tower_http::trace::TraceLayer::new_for_http().make_span_with(
                |request: &Request<Body>| {
                    let route = request
                        .extensions()
                        .get::<axum::extract::MatchedPath>()
                        .map_or("unmatched", axum::extract::MatchedPath::as_str);
                    tracing::info_span!("http_request", method = %request.method(), route)
                },
            ),
        )
        .with_state(state)
}

fn main() -> Result<(), StartupError> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter("info")
        .finish()
        .try_init()?;
    let address = match std::env::var("GYRAL_CLIENT_BIND") {
        Ok(address) => address,
        Err(std::env::VarError::NotPresent) => "127.0.0.1:3038".to_owned(),
        Err(error @ std::env::VarError::NotUnicode(_)) => return Err(error.into()),
    }
    .parse::<std::net::SocketAddr>()?;
    let root = std::env::var_os("GYRAL_CLIENT_DIST").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dist"),
        PathBuf::from,
    );
    let assets = Arc::new(Assets::load(&root)?);
    let database = std::env::var_os("GYRAL_NOTES_DB").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.local/notes.sqlite"),
        PathBuf::from,
    );
    if let Some(parent) = database.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let notebooks = Arc::new(notebook::NotebookPolicy::new()?);
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    executor.block_on(async {
        let notes = Arc::new(sqlite::SqliteNotes::open(&database).await?);
        let state = AppState {
            assets,
            notes: notes.clone(),
            notebooks,
        };
        let listener = tokio::net::TcpListener::bind(address).await?;
        #[cfg(unix)]
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tracing::info!(%address, "Askama client example listening");
        axum::serve(listener, router(state))
            .with_graceful_shutdown(async move {
                let interrupt = async {
                    if tokio::signal::ctrl_c().await.is_err() {
                        tracing::error!(category = "signal", "could not wait for interrupt");
                    }
                };
                #[cfg(unix)]
                tokio::select! { () = interrupt => {}, _ = terminate.recv() => {} }
                #[cfg(not(unix))]
                interrupt.await;
            })
            .await?;
        notes.close().await;
        Ok(())
    })
}
