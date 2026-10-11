//! Embedded Gyral rendering with a typed Rust interface.
//!
//! Start the worker during process initialization with [`Renderer::start`], then
//! pass its cloneable handle to application workflows. Keep [`RendererWorker`]
//! until requests drain and explicitly call its shutdown method.
//!
//! ```no_run
//! use gyral_renderer::{PageId, RenderError, RenderRequest, Renderer};
//! use serde_json::json;
//! # async fn example(renderer: &Renderer) -> Result<(), RenderError> {
//! let request = RenderRequest::try_new(
//!     PageId::try_from("home".to_owned())?,
//!     json!({"name": "friend"}),
//! )?;
//! let document = renderer.render(request).await?;
//! assert!(!document.as_str().is_empty());
//! # Ok(())
//! # }
//! ```
#![forbid(unsafe_code)]

mod artifacts;
mod supervision;
mod telemetry;
mod worker;
pub use artifacts::{AppArtifacts, ArtifactError};
pub use telemetry::{RenderErrorCategory, RendererPhase, RendererStatus};

pub use gyral_renderer_domain::{
    BUNDLE_ABI_VERSION, BundleManifest, ContentHash, ContractError, GYRAL_VERSION, LimitsError,
    ManifestError, PageId, RenderLimits, RenderRequest, RenderedHtml,
};
use std::{fmt, sync::Arc};
use thiserror::Error;
pub use worker::{Renderer, RendererWorker};

/// Capability implemented by the embedded renderer or a hand-written application fake.
pub trait RenderService: Send + Sync {
    /// Render already parsed and authorized data.
    ///
    /// # Errors
    /// Returns a typed runtime, overload, availability, or contract failure.
    fn render(
        &self,
        request: RenderRequest,
    ) -> impl std::future::Future<Output = Result<RenderedHtml, RenderError>> + Send;
}

impl RenderService for Renderer {
    async fn render(&self, request: RenderRequest) -> Result<RenderedHtml, RenderError> {
        Renderer::render(self, request).await
    }
}

/// Compiled, trusted JavaScript exposing `gyralBundleVersion = 1` and `gyralRender(request)`.
///
/// This parser bounds source size. Use `AppArtifacts` to also verify source and
/// browser bytes against a validated build manifest before starting the renderer.
#[derive(Clone)]
pub struct AppBundle(Arc<str>);

impl AppBundle {
    /// Largest compiled server source accepted at startup.
    pub const MAX_BYTES: usize = 8 * 1024 * 1024;
}

impl fmt::Debug for AppBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppBundle")
            .field("bytes", &self.0.len())
            .finish()
    }
}

impl TryFrom<String> for AppBundle {
    type Error = BundleError;

    fn try_from(source: String) -> Result<Self, Self::Error> {
        if source.trim().is_empty() {
            return Err(BundleError::Empty);
        }
        if source.len() > Self::MAX_BYTES {
            return Err(BundleError::TooLarge);
        }
        Ok(Self(Arc::from(source)))
    }
}

/// Invalid bundle supplied at startup.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BundleError {
    /// No executable source was supplied.
    #[error("the app bundle is empty")]
    Empty,
    /// Source exceeded 8 MiB.
    #[error("the app bundle exceeds the 8 MiB limit")]
    TooLarge,
}

/// Failures starting or joining a renderer worker.
#[derive(Debug, Error)]
pub enum WorkerError {
    /// Shutdown was polled without an enabled Tokio timer driver.
    #[error("renderer shutdown requires a Tokio runtime with time enabled")]
    HostRuntimeUnavailable,
    /// The operating system could not start the dedicated thread.
    #[error("could not start the renderer thread")]
    Thread(#[source] std::io::Error),
    /// The worker's Tokio executor could not start.
    #[error("could not start the renderer executor")]
    Executor(#[source] std::io::Error),
    /// Worker completion or join ownership was unavailable during shutdown.
    #[error("the renderer worker stopped")]
    Stopped,
    /// The execution or watchdog thread panicked.
    #[error("the renderer worker panicked")]
    Panicked,
    /// The configured shutdown deadline expired; remaining JavaScript was interrupted.
    #[error("renderer shutdown exceeded {after:?}")]
    ShutdownTimedOut {
        /// Configured shutdown budget.
        after: std::time::Duration,
    },
}

/// Typed failures crossing the rendering boundary.
#[derive(Debug, Error)]
pub enum RenderError {
    /// The bundle did not declare the supported interface before the render call.
    #[error("unsupported bundle ABI {received:?}; expected {BUNDLE_ABI_VERSION}")]
    UnsupportedBundleAbi {
        /// Declared unsigned integer version, or absence/malformed metadata.
        received: Option<u32>,
    },
    /// Rendering was polled without an enabled Tokio timer driver.
    #[error("rendering requires a Tokio runtime with time enabled")]
    HostRuntimeUnavailable,
    /// All sixteen queue positions are occupied.
    #[error("the renderer queue is full")]
    Busy,
    /// The worker has stopped or the reply channel closed.
    #[error("the renderer worker is unavailable")]
    Unavailable,
    /// The end-to-end render deadline expired, including queue time.
    #[error("render deadline exceeded {after:?}")]
    DeadlineExceeded {
        /// Configured render budget.
        after: std::time::Duration,
    },
    /// V8 approached its managed-heap budget and execution was terminated.
    #[error("render exceeded the V8 heap budget of {bytes} bytes")]
    HeapBudgetExceeded {
        /// Configured managed-heap budget, excluding native allocations and emergency headroom.
        bytes: usize,
    },
    /// The caller dropped its render future.
    #[error("render was cancelled")]
    Cancelled,
    /// A Rust render operation unwound; the next request uses a fresh isolate.
    #[error("the render operation panicked")]
    WorkerPanicked,
    /// Request encoding failed.
    #[error("could not encode the render request")]
    Encode(#[source] serde_json::Error),
    /// V8 initialization failed; details are for application logs only.
    #[error("could not initialize the JavaScript runtime: {detail}")]
    Runtime {
        /// Runtime diagnostic; do not send it to clients.
        detail: String,
    },
    /// The app bundle threw or rejected its rendering promise.
    #[error("JavaScript rendering failed: {detail}")]
    JavaScript {
        /// JavaScript diagnostic; do not send it to clients.
        detail: String,
    },
    /// The bundle did not return a string.
    #[error("the app bundle must return an HTML string")]
    InvalidResult,
    /// Returned HTML violated the contract.
    #[error(transparent)]
    Contract(#[from] ContractError),
}

impl RenderError {
    /// Bounded, redacted category suitable for logs, spans, and metric labels.
    #[must_use]
    pub fn category(&self) -> RenderErrorCategory {
        match self {
            Self::UnsupportedBundleAbi { .. } => RenderErrorCategory::BundleAbi,
            Self::HostRuntimeUnavailable => RenderErrorCategory::HostRuntime,
            Self::Busy => RenderErrorCategory::Busy,
            Self::Unavailable => RenderErrorCategory::Unavailable,
            Self::DeadlineExceeded { .. } => RenderErrorCategory::Deadline,
            Self::HeapBudgetExceeded { .. } => RenderErrorCategory::Heap,
            Self::Cancelled => RenderErrorCategory::Cancelled,
            Self::WorkerPanicked => RenderErrorCategory::WorkerPanic,
            Self::Encode(_) => RenderErrorCategory::Encode,
            Self::Runtime { .. } => RenderErrorCategory::Runtime,
            Self::JavaScript { .. } => RenderErrorCategory::JavaScript,
            Self::InvalidResult => RenderErrorCategory::InvalidResult,
            Self::Contract(_) => RenderErrorCategory::Contract,
        }
    }
}
