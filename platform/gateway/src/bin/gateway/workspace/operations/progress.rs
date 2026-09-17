//! Bounded, request-scoped observations. Durable Task status has its own source.
use rmcp::model::{ProgressNotificationParam, ProgressToken};
use tokio::sync::watch;
use veoveo_platform_store::workspace::WorkspaceOperationProgress;

#[derive(Clone)]
pub(super) struct RequestProgress {
    state: std::sync::Arc<std::sync::Mutex<State>>,
    latest: watch::Sender<Option<WorkspaceOperationProgress>>,
}
#[derive(Default)]
struct State {
    token: Option<ProgressToken>,
    pending: Option<(ProgressToken, WorkspaceOperationProgress)>,
}
impl RequestProgress {
    pub fn new() -> (Self, watch::Receiver<Option<WorkspaceOperationProgress>>) {
        let (latest, receiver) = watch::channel(None);
        (
            Self {
                state: Default::default(),
                latest,
            },
            receiver,
        )
    }
    // RMCP allocates the wire token. Keep one early observation to cover a
    // notification arriving before send_cancellable_request returns its handle.
    pub fn bind(&self, token: ProgressToken) {
        let mut state = self.state.lock().expect("progress lock");
        if let Some((pending_token, value)) = state.pending.take()
            && pending_token == token
        {
            self.latest.send_replace(Some(value));
        }
        state.token = Some(token);
    }
    pub fn observe(&self, params: ProgressNotificationParam) {
        let value = WorkspaceOperationProgress {
            completed: params.progress,
            total: params.total,
            message: params.message,
        };
        if !value.valid() {
            return;
        }
        let mut state = self.state.lock().expect("progress lock");
        match &state.token {
            None => {
                if state.pending.as_ref().is_none_or(|(token, old)| {
                    token != &params.progress_token || value.completed > old.completed
                }) {
                    state.pending = Some((params.progress_token, value));
                }
            }
            Some(token) if token == &params.progress_token => {
                self.latest.send_if_modified(|current| {
                    if current
                        .as_ref()
                        .is_some_and(|old| old.completed >= value.completed)
                    {
                        return false;
                    }
                    *current = Some(value);
                    true
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observations_require_the_exact_token_and_finite_increasing_measurements() {
        let (progress, receiver) = RequestProgress::new();
        let token = ProgressToken(rmcp::model::NumberOrString::Number(42));
        progress.bind(token.clone());
        let value = |token, completed| {
            ProgressNotificationParam::new(token, completed)
                .with_total(10.0)
                .with_message("Measured work")
        };
        progress.observe(value(
            ProgressToken(rmcp::model::NumberOrString::Number(7)),
            1.0,
        ));
        progress.observe(value(token.clone(), f64::NAN));
        assert!(receiver.borrow().is_none());
        progress.observe(value(token.clone(), 4.0));
        progress.observe(value(token.clone(), 3.0));
        progress.observe(value(token.clone(), f64::INFINITY));
        assert_eq!(receiver.borrow().as_ref().unwrap().completed, 4.0);
        progress.observe(value(token.clone(), 5.0));
        assert_eq!(receiver.borrow().as_ref().unwrap().completed, 5.0);
    }
}
