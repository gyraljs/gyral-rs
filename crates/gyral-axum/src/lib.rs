//! Axum responses for embedded Gyral rendering; applications own routes and authorization.
//!
//! Use [`render_page`] with the application's renderer, or turn trusted rendered
//! HTML into a response directly:
//!
//! ```
//! use axum::{http::StatusCode, response::IntoResponse};
//! use gyral_axum::RenderedPage;
//! use gyral_renderer::{ContractError, RenderedHtml};
//! # fn main() -> Result<(), ContractError> {
//! let html = RenderedHtml::try_from("<p>Hello</p>".to_owned())?;
//! let response = RenderedPage::new(html).with_status(StatusCode::OK).into_response();
//! assert_eq!(response.headers()["cache-control"], "no-store");
//! # Ok(())
//! # }
//! ```
#![forbid(unsafe_code)]

mod assets;
pub use assets::{DocumentPolicy, PolicyError, client_asset};

use axum::{
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use gyral_renderer::{RenderError, RenderRequest, RenderService, RenderedHtml};

/// Rendered HTML with a status chosen by the application.
#[derive(Debug)]
pub struct RenderedPage {
    html: RenderedHtml,
    status: StatusCode,
}

impl RenderedPage {
    /// Create a successful response. Documents default to `Cache-Control: no-store`.
    #[must_use]
    pub fn new(html: RenderedHtml) -> Self {
        Self {
            html,
            status: StatusCode::OK,
        }
    }

    /// Set the document's status, such as a real 404 chosen by the host route.
    #[must_use]
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }
}

impl IntoResponse for RenderedPage {
    fn into_response(self) -> Response {
        (
            self.status,
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            Html(self.html.into_string()),
        )
            .into_response()
    }
}

/// Rendering failure mapped to an opaque HTTP response.
#[derive(Debug, thiserror::Error)]
#[error("the page could not be rendered")]
pub struct RenderRejection(RenderError);

impl From<RenderError> for RenderRejection {
    fn from(error: RenderError) -> Self {
        Self(error)
    }
}

impl IntoResponse for RenderRejection {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            RenderError::Busy
            | RenderError::Unavailable
            | RenderError::DeadlineExceeded { .. }
            | RenderError::Cancelled => StatusCode::SERVICE_UNAVAILABLE,
            RenderError::HostRuntimeUnavailable
            | RenderError::UnsupportedBundleAbi { .. }
            | RenderError::Encode(_)
            | RenderError::Runtime { .. }
            | RenderError::JavaScript { .. }
            | RenderError::InvalidResult
            | RenderError::HeapBudgetExceeded { .. }
            | RenderError::WorkerPanicked
            | RenderError::Contract(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        // JavaScript diagnostics can contain props; retain only the error category in logs.
        tracing::error!(
            status = status.as_u16(),
            error.category = self.0.category().as_str(),
            "Gyral rendering failed"
        );
        (
            status,
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            "The page could not be rendered.",
        )
            .into_response()
    }
}

/// Render data already authorized and parsed by an application handler.
///
/// # Errors
/// Returns `RenderRejection`, which maps overload to 503 and render defects to an opaque 500.
pub async fn render_page(
    renderer: &impl RenderService,
    request: RenderRequest,
) -> Result<RenderedPage, RenderRejection> {
    Ok(RenderedPage::new(renderer.render(request).await?))
}

/// Compiles the crate README examples as documentation tests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
