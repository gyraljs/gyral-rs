//! Executable contract verification.
#![allow(
    clippy::expect_used,
    reason = "test fixtures prove constructor invariants"
)]
use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};
use gyral_axum::{RenderRejection, RenderedPage, render_page};
use gyral_renderer::{PageId, RenderError, RenderRequest, RenderService, RenderedHtml};
use serde_json::json;

struct FakeRenderer;
impl RenderService for FakeRenderer {
    async fn render(&self, request: RenderRequest) -> Result<RenderedHtml, RenderError> {
        if request.page().as_str() == "busy" {
            return Err(RenderError::Busy);
        }
        RenderedHtml::try_from("<h1>Rendered</h1>".to_owned()).map_err(RenderError::Contract)
    }
}

#[tokio::test]
async fn serves_html_with_application_status_and_private_caching() {
    let html = RenderedHtml::try_from("<h1>Missing</h1>".to_owned())
        .expect("invariant: fixture is valid HTML");
    let response = RenderedPage::new(html)
        .with_status(StatusCode::NOT_FOUND)
        .into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .expect("invariant: adapter sets cache policy"),
        "no-store"
    );
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .expect("invariant: Html sets content type"),
        "text/html; charset=utf-8"
    );
    assert_eq!(
        to_bytes(response.into_body(), 1024)
            .await
            .expect("invariant: fixture body is bounded"),
        "<h1>Missing</h1>"
    );
}

#[tokio::test]
async fn hides_diagnostics_and_maps_availability_failures_to_503() {
    for (error, status) in [
        (
            RenderError::JavaScript {
                detail: "private diagnostic".to_owned(),
            },
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (RenderError::Busy, StatusCode::SERVICE_UNAVAILABLE),
        (
            RenderError::UnsupportedBundleAbi { received: Some(2) },
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (RenderError::Unavailable, StatusCode::SERVICE_UNAVAILABLE),
        (
            RenderError::HostRuntimeUnavailable,
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            RenderError::DeadlineExceeded {
                after: std::time::Duration::from_secs(2),
            },
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (RenderError::Cancelled, StatusCode::SERVICE_UNAVAILABLE),
        (
            RenderError::HeapBudgetExceeded {
                bytes: 64 * 1024 * 1024,
            },
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            RenderError::WorkerPanicked,
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ] {
        let response = RenderRejection::from(error).into_response();
        assert_eq!(response.status(), status);
        let body = to_bytes(response.into_body(), 1024)
            .await
            .expect("invariant: error body is bounded");
        assert_eq!(body, "The page could not be rendered.");
    }
}

#[tokio::test]
async fn accepts_a_rendering_capability_without_constructing_v8() {
    let request = RenderRequest::try_new(
        PageId::try_from("home".to_owned()).expect("invariant: page is valid"),
        json!({}),
    )
    .expect("invariant: props are valid");
    assert_eq!(
        render_page(&FakeRenderer, request)
            .await
            .expect("invariant: fake renders valid HTML")
            .into_response()
            .status(),
        StatusCode::OK
    );
}
