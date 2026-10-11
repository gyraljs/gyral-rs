use std::time::Instant;

/// Renderer admission lifecycle; a snapshot does not prove a bundle can render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererPhase {
    /// New requests may enter the queue.
    Running,
    /// Accepted requests drain, with new admission closed.
    Draining,
    /// Interruption and teardown have been requested.
    Stopping,
    /// The execution thread has completed.
    Stopped,
}

/// Instantaneous worker gauges, without request data or diagnostic text.
#[derive(Debug, Clone, Copy)]
pub struct RendererStatus {
    /// Current admission phase.
    pub phase: RendererPhase,
    /// Occupied queue positions; a shutdown wake message may occupy one position.
    pub queued: usize,
    /// Maximum queue occupancy.
    pub capacity: usize,
    /// Whether an isolate is registered with the watchdog.
    pub active: bool,
}

impl RendererStatus {
    /// Whether this snapshot permits queue admission; it does not reserve capacity.
    #[must_use]
    pub fn accepting(&self) -> bool {
        self.phase == RendererPhase::Running
    }
}

/// Redacted, finite rendering error taxonomy. No props, source, or thrown text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderErrorCategory {
    /// Unsupported bundle contract.
    BundleAbi,
    /// Missing host timer capability.
    HostRuntime,
    /// Queue capacity reached.
    Busy,
    /// Worker admission or reply unavailable.
    Unavailable,
    /// Render deadline expired.
    Deadline,
    /// Managed heap budget exceeded.
    Heap,
    /// Render future dropped.
    Cancelled,
    /// Rust render unwind recovered.
    WorkerPanic,
    /// Request encoding failed.
    Encode,
    /// JavaScript runtime initialization failed.
    Runtime,
    /// Bundle threw or rejected.
    JavaScript,
    /// Bundle returned something other than a string.
    InvalidResult,
    /// HTML contract failed.
    Contract,
}

impl RenderErrorCategory {
    /// Stable label spelling within the package's compatibility policy.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BundleAbi => "bundle_abi",
            Self::HostRuntime => "host_runtime",
            Self::Busy => "busy",
            Self::Unavailable => "unavailable",
            Self::Deadline => "deadline",
            Self::Heap => "heap",
            Self::Cancelled => "cancelled",
            Self::WorkerPanic => "worker_panic",
            Self::Encode => "encode",
            Self::Runtime => "runtime",
            Self::JavaScript => "javascript",
            Self::InvalidResult => "invalid_result",
            Self::Contract => "contract",
        }
    }
}

// The caller owns the recorder. Recording is a no-op without one, and never
// installs an exporter or global subscriber from the library.
pub(crate) struct Observation {
    started: Instant,
    outcome: &'static str,
}

impl Observation {
    pub(crate) fn start() -> Self {
        Self {
            started: Instant::now(),
            outcome: "cancelled",
        }
    }

    pub(crate) fn finish<T>(&mut self, result: &Result<T, crate::RenderError>) {
        self.outcome = match result {
            Ok(_) => "ok",
            Err(error) => error.category().as_str(),
        };
        if result.is_err() {
            tracing::warn!(error.category = self.outcome, "render failed");
        }
    }
}

impl Drop for Observation {
    fn drop(&mut self) {
        metrics::counter!("gyral_render_requests_total", "outcome" => self.outcome).increment(1);
        metrics::histogram!("gyral_render_duration_seconds", "outcome" => self.outcome)
            .record(self.started.elapsed().as_secs_f64());
    }
}
