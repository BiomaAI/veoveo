use crate::TemplateView;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Select an installation-admitted template, or omit it to choose the current
/// default on the first request. An exact retry retains the original selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateTemplateInput {
    pub computer_id: Uuid,
    pub request_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MaintenancePhase {
    Queued,
    Stopping,
    SavingPolicy,
    Replacing,
    Starting,
    RestoringPolicy,
    Verifying,
    Succeeded,
    Cancelled,
    RecoveryRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceRecoveryReason {
    ObservationBudgetExhausted,
    AuthorityDenied,
    CancellationRequested,
    CheckpointUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaintenanceView {
    pub computer_id: Uuid,
    pub task_id: Uuid,
    pub source_template_id: String,
    pub target_template_id: String,
    pub phase: MaintenancePhase,
    pub recovery: Option<MaintenanceRecoveryReason>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaintenanceState {
    pub computer_id: Uuid,
    pub targets: Vec<TemplateView>,
    pub can_update: bool,
    pub active: Option<MaintenanceView>,
}

pub fn maintenance_uri(computer: Uuid) -> String {
    format!("{}/maintenance", crate::computer_uri(computer))
}
