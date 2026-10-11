use std::time::Duration;

/// Invalid renderer resource policy supplied by the host at startup.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LimitsError {
    /// A render deadline must be between one millisecond and sixty seconds.
    #[error("render deadline must be 1 ms–60 s, received {duration:?}")]
    RenderDeadline {
        /// Rejected duration.
        duration: Duration,
    },
    /// A shutdown deadline must be between one millisecond and sixty seconds.
    #[error("shutdown deadline must be 1 ms–60 s, received {duration:?}")]
    ShutdownDeadline {
        /// Rejected duration.
        duration: Duration,
    },
    /// A V8 managed-heap budget must be between 16 MiB and 1 GiB.
    #[error("V8 heap budget must be 16 MiB–1 GiB, received {bytes} bytes")]
    HeapBudget {
        /// Rejected size in bytes.
        bytes: usize,
    },
}

/// Validated, immutable policy for one renderer worker.
///
/// Defaults are a two-second end-to-end render deadline, 64 MiB managed V8 heap
/// budget, and two-second shutdown deadline. These are development defaults,
/// not measured production sizing. The heap budget is not a process RSS cap.
///
/// ```
/// use gyral_renderer_domain::{LimitsError, RenderLimits};
/// use std::time::Duration;
/// # fn main() -> Result<(), LimitsError> {
/// let limits = RenderLimits::try_new(
///     Duration::from_secs(3), 128 * 1024 * 1024, Duration::from_secs(2),
/// )?;
/// assert_eq!(limits.heap_bytes(), 128 * 1024 * 1024);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderLimits {
    render_deadline: Duration,
    heap_bytes: usize,
    shutdown_deadline: Duration,
}

impl RenderLimits {
    /// Parse a host-supplied resource policy once at startup.
    ///
    /// # Errors
    /// Returns the named invalid field when deadlines are outside 1 ms–60 s or
    /// the heap budget is outside 16 MiB–1 GiB. Bounds are inclusive.
    pub fn try_new(
        render_deadline: Duration,
        heap_bytes: usize,
        shutdown_deadline: Duration,
    ) -> Result<Self, LimitsError> {
        let valid_duration = Duration::from_millis(1)..=Duration::from_secs(60);
        if !valid_duration.contains(&render_deadline) {
            return Err(LimitsError::RenderDeadline {
                duration: render_deadline,
            });
        }
        if !valid_duration.contains(&shutdown_deadline) {
            return Err(LimitsError::ShutdownDeadline {
                duration: shutdown_deadline,
            });
        }
        if !(16 * 1024 * 1024..=1024 * 1024 * 1024).contains(&heap_bytes) {
            return Err(LimitsError::HeapBudget { bytes: heap_bytes });
        }
        Ok(Self {
            render_deadline,
            heap_bytes,
            shutdown_deadline,
        })
    }

    /// End-to-end deadline, including time spent in the bounded queue.
    #[must_use]
    pub const fn render_deadline(self) -> Duration {
        self.render_deadline
    }

    /// Managed-heap budget requested from V8, excluding emergency termination headroom.
    #[must_use]
    pub const fn heap_bytes(self) -> usize {
        self.heap_bytes
    }

    /// Maximum time the host waits for draining and joining the worker.
    #[must_use]
    pub const fn shutdown_deadline(self) -> Duration {
        self.shutdown_deadline
    }
}

impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            render_deadline: Duration::from_secs(2),
            heap_bytes: 64 * 1024 * 1024,
            shutdown_deadline: Duration::from_secs(2),
        }
    }
}
