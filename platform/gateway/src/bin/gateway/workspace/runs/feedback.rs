//! Observe actual tool bodies, never model proposals or private tool data.
use std::sync::{Arc, Mutex};
use veoveo_platform_store::workspace::{WorkspaceRunFeedback, WorkspaceRunPhase};

#[derive(Default)]
struct State {
    active: usize,
    value: WorkspaceRunFeedback,
}

#[derive(Clone, Default)]
pub(super) struct Feedback(Arc<Mutex<State>>);

impl Feedback {
    pub fn snapshot(&self) -> WorkspaceRunFeedback {
        self.0.lock().expect("run feedback").value
    }

    pub fn responding(&self) {
        let mut state = self.0.lock().expect("run feedback");
        if state.active == 0 {
            state.value.phase = WorkspaceRunPhase::Responding;
        }
    }

    pub fn tool(&self) -> ToolActivity {
        let mut state = self.0.lock().expect("run feedback");
        state.active += 1;
        state.value.phase = WorkspaceRunPhase::CallingTools;
        ToolActivity(self.clone())
    }
}

pub(super) struct ToolActivity(Feedback);
impl ToolActivity {
    pub fn admitted(&self) {
        self.0.0.lock().expect("run feedback").value.operations += 1;
    }
}
impl Drop for ToolActivity {
    fn drop(&mut self) {
        let mut state = self.0.0.lock().expect("run feedback");
        state.active -= 1;
        if state.active == 0 {
            state.value.phase = WorkspaceRunPhase::Responding;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_tools_remain_active_until_the_last_body_settles() {
        let feedback = Feedback::default();
        let first = feedback.tool();
        let second = feedback.tool();
        first.admitted();
        drop(first);
        feedback.responding();
        assert_eq!(feedback.snapshot().phase, WorkspaceRunPhase::CallingTools);
        assert_eq!(feedback.snapshot().operations, 1);
        // A failed or cancelled body contributes no admitted operation.
        drop(second);
        assert_eq!(feedback.snapshot().phase, WorkspaceRunPhase::Responding);
        assert_eq!(feedback.snapshot().operations, 1);
    }
}
