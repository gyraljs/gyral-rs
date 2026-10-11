#![allow(clippy::expect_used)]
use super::*;
use axum::body::to_bytes;
use tower::ServiceExt as _;

pub(super) async fn app() -> Router {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dist");
    router(AppState {
        assets: Arc::new(
            Assets::load(&root).expect("invariant: make test builds the client artifacts"),
        ),
        notes: Arc::new(
            sqlite::SqliteNotes::open(std::path::Path::new(":memory:"))
                .await
                .expect("invariant: fresh memory database opens"),
        ),
        notebooks: Arc::new(
            notebook::NotebookPolicy::new().expect("invariant: test host provides OS randomness"),
        ),
    })
}

#[tokio::test]
async fn askama_response_contains_tags_and_fallbacks_without_gyral_ssr() {
    let response = app()
        .await
        .oneshot(
            Request::builder()
                .uri("/?name=Basketball&start=9")
                .body(Body::empty())
                .expect("invariant: static request is valid"),
        )
        .await
        .expect("invariant: router is infallible");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
    assert!(
        response.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .expect("invariant: generated CSP is ASCII")
            .contains("script-src 'sha256-")
    );
    let html = String::from_utf8(
        to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("invariant: document is bounded")
            .to_vec(),
    )
    .expect("invariant: Askama emits UTF-8");
    assert!(html.contains("<bdi>Basketball</bdi>"));
    assert!(html.contains("<gyral-client-counter start=\"9\">"));
    assert!(html.contains("Interactive controls appear when JavaScript loads."));
    for marker in ["shadowrootmode", "data-gyral-seed", "<output", "server.js"] {
        assert!(!html.contains(marker), "unexpected SSR marker: {marker}");
    }
}

#[tokio::test]
async fn invalid_and_unknown_page_parameters_are_uncached_bad_requests() {
    for uri in [
        "/?name=x",
        "/?start=100",
        "/?start=-1",
        "/?unknown=1",
        "/?name=One&name=Two",
        "/?name=a%0Ab",
    ] {
        let response = app()
            .await
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("invariant: static request is valid"),
            )
            .await
            .expect("invariant: router is infallible");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert!(
            response
                .headers()
                .contains_key(header::CONTENT_SECURITY_POLICY)
        );
    }
}

#[tokio::test]
async fn template_failures_return_an_opaque_error() {
    let error = PageError::Render(askama::Error::Custom(Box::new(std::io::Error::other(
        "synthetic-private-template-diagnostic",
    ))));
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        to_bytes(response.into_body(), 1024)
            .await
            .expect("invariant: error body is bounded"),
        "The page could not be rendered."
    );
}
