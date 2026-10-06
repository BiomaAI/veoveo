use crate::TemplateView;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Select an installation-admitted template, or omit it to choose the current
/// default on the first request. An exact retry retains the original selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateTemplateInput {
    pub computer_id: crate::ComputerId,
    pub request_id: crate::RequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<crate::TemplateId>,
}

/// A new, explicit recovery intent for the exact paused operation epoch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeUpdateInput {
    pub computer_id: crate::ComputerId,
    pub task_id: veoveo_types::TaskId,
    pub request_id: crate::RequestId,
    pub expected_updated_at: DateTime<Utc>,
    pub acknowledged_cancellation_at: Option<DateTime<Utc>>,
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
#[schemars(rename = "MaintenanceView")]
pub struct MaintenanceViewValue {
    pub computer_id: crate::ComputerId,
    pub task_id: veoveo_types::TaskId,
    pub source_template_id: crate::TemplateId,
    pub target_template_id: crate::TemplateId,
    pub phase: MaintenancePhase,
    pub recovery: Option<MaintenanceRecoveryReason>,
    pub can_resume: bool,
    pub pending_cancellation_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "MaintenanceState")]
pub struct MaintenanceStateValue {
    pub computer_id: crate::ComputerId,
    pub targets: Vec<TemplateView>,
    pub can_update: bool,
    pub active: Option<MaintenanceView>,
}

/// Admitted MaintenanceView; callers assemble its value and build before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "MaintenanceViewValue", into = "MaintenanceViewValue")]
pub struct MaintenanceView(veoveo_types::Checked<MaintenanceViewValue>);
impl std::ops::Deref for MaintenanceView {
    type Target = MaintenanceViewValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for MaintenanceView {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "MaintenanceView".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        MaintenanceViewValue::json_schema(generator)
    }
}
impl TryFrom<MaintenanceViewValue> for MaintenanceView {
    type Error = crate::ComputerResultError;
    fn try_from(value: MaintenanceViewValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<MaintenanceView> for MaintenanceViewValue {
    fn from(value: MaintenanceView) -> Self {
        value.0.into_inner()
    }
}
impl MaintenanceViewValue {
    pub fn build(self) -> Result<MaintenanceView, crate::ComputerResultError> {
        self.try_into()
    }
}
impl veoveo_types::Check for MaintenanceViewValue {
    type Error = crate::ComputerResultError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.updated_at < self.created_at
            || (self.phase == MaintenancePhase::RecoveryRequired) != self.recovery.is_some()
            || self.can_resume && self.phase != MaintenancePhase::RecoveryRequired
        {
            return Err(crate::ComputerResultError);
        }
        Ok(())
    }
}

/// Admitted MaintenanceState; callers assemble its value and build before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "MaintenanceStateValue", into = "MaintenanceStateValue")]
pub struct MaintenanceState(veoveo_types::Checked<MaintenanceStateValue>);
impl std::ops::Deref for MaintenanceState {
    type Target = MaintenanceStateValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for MaintenanceState {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "MaintenanceState".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        MaintenanceStateValue::json_schema(generator)
    }
}
impl TryFrom<MaintenanceStateValue> for MaintenanceState {
    type Error = crate::ComputerResultError;
    fn try_from(value: MaintenanceStateValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<MaintenanceState> for MaintenanceStateValue {
    fn from(value: MaintenanceState) -> Self {
        value.0.into_inner()
    }
}
impl MaintenanceStateValue {
    pub fn build(self) -> Result<MaintenanceState, crate::ComputerResultError> {
        self.try_into()
    }
}
impl veoveo_types::Check for MaintenanceStateValue {
    type Error = crate::ComputerResultError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.targets.len() > 64
            || self
                .targets
                .iter()
                .map(|target| &target.template_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.targets.len()
            || self
                .active
                .as_ref()
                .is_some_and(|view| view.computer_id != self.computer_id)
            || self.can_update && (self.targets.is_empty() || self.active.is_some())
        {
            return Err(crate::ComputerResultError);
        }
        Ok(())
    }
}
