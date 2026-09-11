//! Validate public maintenance boundaries without importing the provider or store.
use std::collections::BTreeSet;
use uuid::Uuid;
use veoveo_computers_contract::{
    MaintenancePhase, MaintenanceState, MaintenanceView, UpdateTemplateInput,
};

fn template_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
pub(super) fn input(bytes: &[u8], computer: Uuid) -> Result<Vec<u8>, ()> {
    let input: UpdateTemplateInput = serde_json::from_slice(bytes).map_err(|_| ())?;
    if input.computer_id != computer
        || input.request_id.is_nil()
        || input
            .template_id
            .as_deref()
            .is_some_and(|id| !template_id(id))
    {
        return Err(());
    }
    serde_json::to_vec(&input).map_err(|_| ())
}
fn valid(view: &MaintenanceView, computer: Uuid, task: Option<Uuid>) -> bool {
    view.computer_id == computer
        && view.task_id.get_version_num() == 7
        && task.is_none_or(|id| view.task_id == id)
        && template_id(&view.source_template_id)
        && template_id(&view.target_template_id)
        && view.updated_at >= view.created_at
        && (view.phase == MaintenancePhase::RecoveryRequired) == view.recovery.is_some()
}
pub(super) fn receipt(bytes: &[u8], computer: Uuid, task: Option<Uuid>) -> Result<Vec<u8>, ()> {
    let view: MaintenanceView = serde_json::from_slice(bytes).map_err(|_| ())?;
    if !valid(&view, computer, task) {
        return Err(());
    }
    serde_json::to_vec(&view).map_err(|_| ())
}
pub(super) fn state(bytes: &[u8], computer: Uuid) -> Result<Vec<u8>, ()> {
    let state: MaintenanceState = serde_json::from_slice(bytes).map_err(|_| ())?;
    let mut names = BTreeSet::new();
    if state.computer_id != computer
        || state.targets.len() > 64
        || state
            .targets
            .iter()
            .any(|t| !template_id(&t.template_id) || !names.insert(&t.template_id))
        || state
            .active
            .as_ref()
            .is_some_and(|view| !valid(view, computer, None))
        || state.can_update && (state.targets.is_empty() || state.active.is_some())
    {
        return Err(());
    }
    serde_json::to_vec(&state).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn maintenance_identity_and_recovery_cannot_be_substituted() {
        let computer = Uuid::now_v7();
        let task = Uuid::now_v7();
        let time = chrono::Utc::now();
        let view = json!({"computerId":computer,"taskId":task,"sourceTemplateId":"development-retained","targetTemplateId":"development","phase":"queued","recovery":null,"createdAt":time,"updatedAt":time});
        let bytes = serde_json::to_vec(&view).unwrap();
        assert!(receipt(&bytes, computer, Some(task)).is_ok());
        assert!(receipt(&bytes, Uuid::now_v7(), Some(task)).is_err());
        assert!(receipt(&bytes, computer, Some(Uuid::now_v7())).is_err());
        let mut changed = view;
        changed["phase"] = "recovery_required".into();
        assert!(receipt(&serde_json::to_vec(&changed).unwrap(), computer, Some(task)).is_err());
        changed["recovery"] = "observation_budget_exhausted".into();
        assert!(receipt(&serde_json::to_vec(&changed).unwrap(), computer, Some(task)).is_ok());
        let state_bytes = serde_json::to_vec(
            &json!({"computerId":computer,"targets":[],"canUpdate":true,"active":changed}),
        )
        .unwrap();
        assert!(state(&state_bytes, computer).is_err());
        for wrong in [
            json!({"computerId":Uuid::now_v7(),"requestId":task}),
            json!({"computerId":computer,"requestId":task,"image":"untrusted"}),
            json!({"computerId":computer,"requestId":task,"templateId":"../../image"}),
        ] {
            assert!(input(&serde_json::to_vec(&wrong).unwrap(), computer).is_err());
        }
    }
}
