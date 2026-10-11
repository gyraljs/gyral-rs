use deno_core::v8;
use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    time::Instant,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopReason {
    Deadline,
    Heap,
    Cancelled,
    Shutdown,
}

#[derive(Debug, Default)]
enum RequestState {
    #[default]
    Pending,
    Finished,
    Interrupted(StopReason),
}

#[derive(Debug, Default)]
pub(crate) struct RequestSignal(Mutex<RequestState>);

impl RequestSignal {
    pub(crate) fn stop(&self, reason: StopReason) {
        let mut state = lock(&self.0);
        if matches!(*state, RequestState::Pending) {
            *state = RequestState::Interrupted(reason);
        }
    }

    pub(crate) fn reason(&self) -> Option<StopReason> {
        match *lock(&self.0) {
            RequestState::Pending | RequestState::Finished => None,
            RequestState::Interrupted(reason) => Some(reason),
        }
    }

    pub(crate) fn finish(&self) -> Option<StopReason> {
        let mut state = lock(&self.0);
        match *state {
            RequestState::Pending => {
                *state = RequestState::Finished;
                None
            }
            RequestState::Finished => None,
            RequestState::Interrupted(reason) => Some(reason),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    Running,
    Draining { until: Instant },
    Stopping,
    Stopped,
}

struct ActiveRender {
    handle: v8::IsolateHandle,
    signal: Arc<RequestSignal>,
    deadline: Instant,
    interrupted: bool,
}

struct State {
    phase: Phase,
    active: Option<ActiveRender>,
}

/// One leaf synchronization object owns admission and the active isolate handle.
pub(crate) struct Control {
    state: Mutex<State>,
    changed: Condvar,
}

impl Default for Control {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                phase: Phase::Running,
                active: None,
            }),
            changed: Condvar::new(),
        }
    }
}

impl Control {
    pub(crate) fn status(&self) -> (crate::RendererPhase, bool) {
        let state = lock(&self.state);
        let phase = match state.phase {
            Phase::Running => crate::RendererPhase::Running,
            Phase::Draining { .. } => crate::RendererPhase::Draining,
            Phase::Stopping => crate::RendererPhase::Stopping,
            Phase::Stopped => crate::RendererPhase::Stopped,
        };
        (phase, state.active.is_some())
    }

    pub(crate) fn admit<T>(
        &self,
        submit: impl FnOnce() -> Result<T, crate::RenderError>,
    ) -> Result<T, crate::RenderError> {
        let state = lock(&self.state);
        match state.phase {
            Phase::Running => submit(),
            Phase::Draining { .. } | Phase::Stopping | Phase::Stopped => {
                Err(crate::RenderError::Unavailable)
            }
        }
    }

    pub(crate) fn attach(
        self: &Arc<Self>,
        handle: v8::IsolateHandle,
        signal: Arc<RequestSignal>,
        deadline: Instant,
    ) -> Registration {
        let mut state = lock(&self.state);
        let stopping = matches!(state.phase, Phase::Stopping | Phase::Stopped);
        if stopping {
            signal.stop(StopReason::Shutdown);
        }
        state.active = Some(ActiveRender {
            handle: handle.clone(),
            signal: Arc::clone(&signal),
            deadline,
            interrupted: false,
        });
        self.changed.notify_all();
        drop(state);
        // The watchdog may already have exited if shutdown raced runtime creation.
        if stopping {
            handle.terminate_execution();
        }
        Registration {
            control: Arc::clone(self),
            signal,
        }
    }

    pub(crate) fn stop_request(&self, signal: &RequestSignal, reason: StopReason) {
        // Protect the observed predicate against lost condition-variable notifications.
        let _state = lock(&self.state);
        signal.stop(reason);
        self.changed.notify_all();
    }

    pub(crate) fn drain(&self, until: Instant) {
        let mut state = lock(&self.state);
        if matches!(state.phase, Phase::Running) {
            state.phase = Phase::Draining { until };
        }
        self.changed.notify_all();
    }

    pub(crate) fn abort(&self) {
        let handle = {
            let mut state = lock(&self.state);
            if !matches!(state.phase, Phase::Stopped) {
                state.phase = Phase::Stopping;
            }
            let handle = state.active.as_ref().map(|active| {
                active.signal.stop(StopReason::Shutdown);
                active.handle.clone()
            });
            self.changed.notify_all();
            handle
        };
        if let Some(handle) = handle {
            handle.terminate_execution();
        }
    }

    pub(crate) fn should_stop(&self, queue_empty: bool) -> bool {
        match lock(&self.state).phase {
            Phase::Running => false,
            Phase::Draining { until } => queue_empty || Instant::now() >= until,
            Phase::Stopping | Phase::Stopped => true,
        }
    }

    pub(crate) fn finished(&self) {
        let mut state = lock(&self.state);
        state.phase = Phase::Stopped;
        state.active = None;
        self.changed.notify_all();
    }

    #[cfg(test)]
    pub(crate) fn wait_for_active(&self) -> bool {
        let (state, _) = self
            .changed
            .wait_timeout_while(
                lock(&self.state),
                std::time::Duration::from_secs(2),
                |state| state.active.is_none(),
            )
            .unwrap_or_else(PoisonError::into_inner);
        state.active.is_some()
    }

    pub(crate) fn watch(&self) {
        loop {
            let (handle, stopping) = {
                let mut state = lock(&self.state);
                loop {
                    let now = Instant::now();
                    if matches!(state.phase, Phase::Draining { until } if now >= until) {
                        state.phase = Phase::Stopping;
                    }
                    match state.phase {
                        Phase::Stopped => return,
                        Phase::Stopping => {
                            let handle = state.active.as_ref().map(|active| {
                                active.signal.stop(StopReason::Shutdown);
                                active.handle.clone()
                            });
                            break (handle, true);
                        }
                        Phase::Running | Phase::Draining { .. } => {}
                    }
                    if let Some(active) = state.active.as_mut() {
                        if now >= active.deadline {
                            active.signal.stop(StopReason::Deadline);
                        }
                        if active.signal.reason().is_some() && !active.interrupted {
                            active.interrupted = true;
                            break (Some(active.handle.clone()), false);
                        }
                    }
                    let request_deadline = state
                        .active
                        .as_ref()
                        .filter(|active| !active.interrupted)
                        .map(|active| active.deadline);
                    let until = match state.phase {
                        Phase::Draining { until } => {
                            Some(request_deadline.map_or(until, |request| request.min(until)))
                        }
                        Phase::Running => request_deadline,
                        Phase::Stopping | Phase::Stopped => None,
                    };
                    state = match until {
                        Some(until) => {
                            let (state, _) = self
                                .changed
                                .wait_timeout(state, until.saturating_duration_since(now))
                                .unwrap_or_else(PoisonError::into_inner);
                            state
                        }
                        None => self
                            .changed
                            .wait(state)
                            .unwrap_or_else(PoisonError::into_inner),
                    };
                }
            };
            // Never call into V8 while holding the controller mutex.
            if let Some(handle) = handle {
                handle.terminate_execution();
            }
            if stopping {
                return;
            }
        }
    }
}

pub(crate) struct Registration {
    control: Arc<Control>,
    signal: Arc<RequestSignal>,
}

impl Drop for Registration {
    fn drop(&mut self) {
        let mut state = lock(&self.control.state);
        if state
            .active
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(&active.signal, &self.signal))
        {
            state.active = None;
        }
        self.control.changed.notify_all();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Critical sections only change protocol metadata. Poison cannot block cleanup.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_interruption_wins_and_completion_is_terminal() {
        let reasons = [
            StopReason::Deadline,
            StopReason::Heap,
            StopReason::Cancelled,
            StopReason::Shutdown,
        ];
        for first in reasons {
            for second in reasons {
                let signal = RequestSignal::default();
                signal.stop(first);
                signal.stop(second);
                assert_eq!(signal.finish(), Some(first));
                assert_eq!(signal.reason(), Some(first));
            }
            let completed = RequestSignal::default();
            assert_eq!(completed.finish(), None);
            completed.stop(first);
            assert_eq!(completed.reason(), None);
            assert_eq!(completed.finish(), None);
        }
    }
}
