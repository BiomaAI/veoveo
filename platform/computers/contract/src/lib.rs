//! Canonical public JSON contract. Generate every client model from schema_bundle().
mod ids;
pub use ids::*;
mod template_id;
pub use template_id::{TemplateId, TemplateIdError};
pub use veoveo_artifact_contract::ArtifactId;
mod resources;
pub use resources::*;
mod scopes;
pub use scopes::*;
mod task_kind;
pub use task_kind::ComputerTaskKind;
mod access;
pub use access::*;
mod automation;
pub mod value_admission;
pub use automation::*;
mod execution;
pub use execution::*;
mod files;
pub use files::*;
mod maintenance;
pub use maintenance::*;
mod pairing;
use chrono::{DateTime, Utc};
pub use pairing::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod actions;
pub use actions::{ComputerAction, register_catalog};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("Computer result identities or limits do not agree")]
pub struct ComputerResultError;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComputerPhase {
    Reserved,
    Provisioning,
    Starting,
    Ready,
    Stopping,
    Stopped,
    Error,
    RecoveryRequired,
}

impl ComputerPhase {
    pub fn is_transitional(self) -> bool {
        matches!(self, Self::Provisioning | Self::Starting | Self::Stopping)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Create,
    Start,
    Stop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Queued,
    Running,
    Waiting,
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
    RecoveryRequired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateView {
    pub template_id: crate::TemplateId,
    pub cpus: u32,
    pub memory_mib: u32,
    pub home_capacity_mib: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerLimits {
    pub per_owner: u32,
    pub per_tenant: u32,
    pub provider: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "ComputerSnapshot")]
pub struct ComputerSnapshotValue {
    pub availability: CapacityAvailability,
    pub template: Option<TemplateView>,
    pub limits: Option<ComputerLimits>,
    pub can_create: bool,
    pub computers: Vec<ComputerView>,
    pub next_cursor: Option<crate::ComputerId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapacityAvailability {
    SetupRequired,
    Available,
    Exhausted,
    ComputeUnavailable,
    StorageUnavailable,
    Maintenance,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "ComputerView")]
pub struct ComputerViewValue {
    pub computer_id: crate::ComputerId,
    pub access_mode: ComputerAccessMode,
    /// Current named grants available to this grantee; owners manage their grant
    /// inventory through the separate owner-only resource.
    #[schemars(length(max = 64))]
    pub granted_access: Vec<ComputerGrantedAccess>,
    pub template_id: crate::TemplateId,
    pub phase: ComputerPhase,
    pub busy: bool,
    /// An authorized reserved Computer can be provisioned once unfenced.
    pub can_create: bool,
    /// An authorized stopped Computer can be started once unfenced.
    pub can_start: bool,
    /// A ready Computer can be stopped once unfenced.
    pub can_stop: bool,
    pub can_delete: bool,
    pub can_connect: bool,
    pub can_transfer_files: bool,
    /// The shared command/file slot can remain held while an owner Stop completes.
    pub active_execution: Option<ComputerExecution>,
    pub active_task_id: Option<veoveo_types::TaskId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComputerAccessMode {
    Owner,
    Granted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ComputerExecution {
    Command { task_id: veoveo_types::TaskId },
    File { task_id: veoveo_types::TaskId },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationReceipt {
    pub task_id: veoveo_types::TaskId,
    pub computer_id: crate::ComputerId,
    pub action: Action,
    pub status: OperationStatus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifecycleResult {
    #[serde(rename = "result_uri", skip_serializing_if = "Option::is_none")]
    result_uri: Option<ComputerResultUri>,
    computer_id: crate::ComputerId,
    operation_id: veoveo_types::TaskId,
    action: Action,
}
impl LifecycleResult {
    pub fn new(
        computer_id: ComputerId,
        operation_id: veoveo_types::TaskId,
        action: Action,
    ) -> Self {
        Self {
            result_uri: (action == Action::Create).then(|| ComputerResultUri::new(computer_id)),
            computer_id,
            operation_id,
            action,
        }
    }
    pub fn result_uri(&self) -> Option<ComputerResultUri> {
        self.result_uri
    }
    pub fn computer_id(&self) -> crate::ComputerId {
        self.computer_id
    }
    pub fn operation_id(&self) -> veoveo_types::TaskId {
        self.operation_id
    }
    pub fn action(&self) -> Action {
        self.action
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LifecycleResultWire {
    #[serde(rename = "result_uri", skip_serializing_if = "Option::is_none")]
    result_uri: Option<ComputerResultUri>,
    computer_id: crate::ComputerId,
    operation_id: veoveo_types::TaskId,
    action: Action,
}

impl<'de> Deserialize<'de> for LifecycleResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = LifecycleResultWire::deserialize(deserializer)?;

        let result = Self::new(wire.computer_id, wire.operation_id, wire.action);
        if wire.result_uri != result.result_uri {
            return Err(serde::de::Error::custom(ComputerResultError));
        }
        Ok(result)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaintenanceResult {
    #[serde(rename = "result_uri")]
    result_uri: ComputerResultUri,
    computer_id: crate::ComputerId,
    maintenance_id: veoveo_types::TaskId,
    template_id: crate::TemplateId,
}
impl MaintenanceResult {
    pub fn new(
        computer_id: ComputerId,
        maintenance_id: veoveo_types::TaskId,
        template_id: TemplateId,
    ) -> Self {
        Self {
            result_uri: ComputerResultUri::new(computer_id),
            computer_id,
            maintenance_id,
            template_id,
        }
    }
    pub fn result_uri(&self) -> ComputerResultUri {
        self.result_uri
    }
    pub fn computer_id(&self) -> crate::ComputerId {
        self.computer_id
    }
    pub fn maintenance_id(&self) -> veoveo_types::TaskId {
        self.maintenance_id
    }
    pub fn template_id(&self) -> &crate::TemplateId {
        &self.template_id
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MaintenanceResultWire {
    #[serde(rename = "result_uri")]
    result_uri: ComputerResultUri,
    computer_id: crate::ComputerId,
    maintenance_id: veoveo_types::TaskId,
    template_id: crate::TemplateId,
}

impl<'de> Deserialize<'de> for MaintenanceResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = MaintenanceResultWire::deserialize(deserializer)?;

        let result = Self::new(wire.computer_id, wire.maintenance_id, wire.template_id);
        if wire.result_uri != result.result_uri {
            return Err(serde::de::Error::custom(ComputerResultError));
        }
        Ok(result)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationView {
    pub task_id: veoveo_types::TaskId,
    pub computer_id: crate::ComputerId,
    pub action: Action,
    pub status: OperationStatus,
    pub computer: Option<ComputerView>,
    pub error: Option<ApiError>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateInput {
    pub request_id: crate::RequestId,
    /// Continue provisioning an owned reservation, or omit for a new Computer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computer_id: Option<crate::ComputerId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartInput {
    pub request_id: crate::RequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<crate::AutomationGrantId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StopInput {
    pub request_id: crate::RequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<crate::AutomationGrantId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifecycleInput {
    pub computer_id: crate::ComputerId,
    pub request_id: crate::RequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<crate::AutomationGrantId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TerminalTicketInput {}

/// Explicit serialization is the only public exposure path. No Debug or Display.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct TerminalToken(String);

impl TerminalToken {
    pub fn new(token: String) -> Self {
        Self(token)
    }
    /// Only for a bounded terminal first frame; never put this value in a URL or log.
    ///
    /// ```compile_fail
    /// use veoveo_computers_contract::TerminalToken;
    /// fn cannot_log(token: TerminalToken) { let _ = format!("{token:?}"); }
    /// ```
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalTicket {
    pub computer_id: crate::ComputerId,
    pub token: TerminalToken,
    pub expires_at: DateTime<Utc>,
    /// Same-origin authenticated WebSocket path; the BFF rewrites its own edge.
    pub endpoint: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Forbidden,
    NotFound,
    Busy,
    InvalidInput,
    InvalidState,
    CapacityFull,
    AccessLimit,
    StorageUnavailable,
    StorageHeadroom,
    TemplateMismatch,
    TicketRejected,
    Unavailable,
    OperationFailed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComputerEventKind {
    SnapshotChanged,
}

/// An authorized invalidation; clients reread current state after each event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerEvent {
    pub kind: ComputerEventKind,
    pub computer_id: Option<crate::ComputerId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TerminalAttachKind {
    Attach,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TerminalResizeKind {
    Resize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TerminalReadyKind {
    Ready,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TerminalReplayCompleteKind {
    ReplayComplete,
}

/// This fence is not permission to send input until historical rendering drains.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalReplayComplete {
    #[serde(rename = "type")]
    pub kind: TerminalReplayCompleteKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TerminalServerControl {
    Ready(TerminalReady),
    ReplayComplete(TerminalReplayComplete),
    Lease(TerminalLease),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TerminalLeaseKind {
    Lease,
}

/// A current service-issued deadline. Relays preserve it and cannot extend it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalLease {
    #[serde(rename = "type")]
    pub kind: TerminalLeaseKind,
    #[schemars(range(min = 1))]
    pub sequence: u64,
    pub expires_at: DateTime<Utc>,
}

pub const TERMINAL_VERSION: u8 = 2;

/// The relay enforces version, dimensions and a 1024-byte first-frame bound.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalAttach {
    #[schemars(range(min = 2, max = 2))]
    pub version: u8,
    #[serde(rename = "type")]
    pub kind: TerminalAttachKind,
    pub computer_id: crate::ComputerId,
    pub token: TerminalToken,
    #[schemars(range(min = 2, max = 500))]
    pub cols: u32,
    #[schemars(range(min = 1, max = 200))]
    pub rows: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalResize {
    #[serde(rename = "type")]
    pub kind: TerminalResizeKind,
    #[schemars(range(min = 2, max = 500))]
    pub cols: u32,
    #[schemars(range(min = 1, max = 200))]
    pub rows: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalReady {
    #[schemars(range(min = 2, max = 2))]
    pub version: u8,
    #[serde(rename = "type")]
    pub kind: TerminalReadyKind,
    pub expires_at: DateTime<Utc>,
}

/// Schema root is a bundle of DTO definitions, not a route response.
#[derive(JsonSchema)]
#[allow(dead_code)]
#[schemars(rename = "ComputersApi")]
struct SchemaBundle {
    revoke_automation_grant_body: RevokeAutomationGrantBody,
    automation_grant_result: AutomationGrantResult,
    execute_input: ExecuteInput,
    execution_result: ExecutionResult,
    transfer_file_input: TransferFileInput,
    file_transfer_result: FileTransferResult,
    file_transfer_stage: FileTransferStage,
    file_transfer_view: FileTransferView,
    cancel_file_transfer_body: CancelFileTransferBody,
    issue_automation_grant: IssueAutomationGrantInput,
    automation_grant: AutomationGrantView,
    automation_grants: AutomationGrantCollection,
    revoke_automation_grant: RevokeAutomationGrantInput,
    cli_pairing_input: CliPairingInput,
    cli_pairing_challenge: CliPairingChallenge,
    cli_pairing_confirm_body: CliPairingConfirmBody,
    cli_pairing_result: CliPairingResult,
    access_grants: AccessGrantCollection,
    revoke_access_input: RevokeAccessInput,
    revoke_access_body: RevokeAccessBody,
    access_revocation: AccessRevocation,
    snapshot: ComputerSnapshot,
    computer: ComputerView,
    template: TemplateView,
    limits: ComputerLimits,
    receipt: OperationReceipt,
    lifecycle_result: LifecycleResult,
    maintenance_result: MaintenanceResult,
    maintenance_state: MaintenanceState,
    maintenance_view: MaintenanceView,
    update_template_input: UpdateTemplateInput,
    resume_update_input: ResumeUpdateInput,
    operation: OperationView,
    create_input: CreateInput,
    start_input: StartInput,
    stop_input: StopInput,
    lifecycle_input: LifecycleInput,
    terminal_ticket_input: TerminalTicketInput,
    terminal_ticket: TerminalTicket,
    error: ApiError,
    event: ComputerEvent,
    terminal_attach: TerminalAttach,
    terminal_resize: TerminalResize,
    terminal_ready: TerminalReady,
    terminal_server_control: TerminalServerControl,
}

pub fn schema_bundle() -> schemars::Schema {
    schemars::schema_for!(SchemaBundle)
}

mod audit;
pub use audit::{ComputerAuditTarget, register_audit_target};

/// Admitted ComputerView; callers assemble its value and build before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ComputerViewValue", into = "ComputerViewValue")]
pub struct ComputerView(veoveo_types::Checked<ComputerViewValue>);
impl std::ops::Deref for ComputerView {
    type Target = ComputerViewValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for ComputerView {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ComputerView".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ComputerViewValue::json_schema(generator)
    }
}
impl TryFrom<ComputerViewValue> for ComputerView {
    type Error = crate::ComputerResultError;
    fn try_from(value: ComputerViewValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<ComputerView> for ComputerViewValue {
    fn from(value: ComputerView) -> Self {
        value.0.into_inner()
    }
}
impl ComputerViewValue {
    pub fn build(self) -> Result<ComputerView, crate::ComputerResultError> {
        self.try_into()
    }
}
impl veoveo_types::Check for ComputerViewValue {
    type Error = crate::ComputerResultError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.updated_at < self.created_at
            || self.granted_access.len() > 64
            || self
                .granted_access
                .iter()
                .map(|grant| grant.grant_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.granted_access.len()
        {
            return Err(crate::ComputerResultError);
        }
        Ok(())
    }
}

/// Admitted ComputerSnapshot; callers assemble its value and build before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ComputerSnapshotValue", into = "ComputerSnapshotValue")]
pub struct ComputerSnapshot(veoveo_types::Checked<ComputerSnapshotValue>);
impl std::ops::Deref for ComputerSnapshot {
    type Target = ComputerSnapshotValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for ComputerSnapshot {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ComputerSnapshot".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ComputerSnapshotValue::json_schema(generator)
    }
}
impl TryFrom<ComputerSnapshotValue> for ComputerSnapshot {
    type Error = crate::ComputerResultError;
    fn try_from(value: ComputerSnapshotValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<ComputerSnapshot> for ComputerSnapshotValue {
    fn from(value: ComputerSnapshot) -> Self {
        value.0.into_inner()
    }
}
impl ComputerSnapshotValue {
    pub fn build(self) -> Result<ComputerSnapshot, crate::ComputerResultError> {
        self.try_into()
    }
}
impl veoveo_types::Check for ComputerSnapshotValue {
    type Error = crate::ComputerResultError;
    fn check(&self) -> Result<(), Self::Error> {
        if self
            .computers
            .iter()
            .map(|computer| computer.computer_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != self.computers.len()
        {
            return Err(crate::ComputerResultError);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_controls_have_one_closed_generated_replay_shape() {
        let control = TerminalServerControl::ReplayComplete(TerminalReplayComplete {
            kind: TerminalReplayCompleteKind::ReplayComplete,
        });
        assert_eq!(
            serde_json::to_value(&control).unwrap(),
            serde_json::json!({"type":"replay_complete"})
        );
        assert_eq!(
            serde_json::from_str::<TerminalServerControl>("{\"type\":\"replay_complete\"}")
                .unwrap(),
            control
        );
        for text in [
            "{}",
            "null",
            "[]",
            "{\"type\":\"replay_complete\",\"sequence\":1}",
            "{\"type\":\"unknown\"}",
            "{\"type\":\"replay_complete\",\"type\":\"replay_complete\"}",
        ] {
            assert!(serde_json::from_str::<TerminalServerControl>(text).is_err());
        }
        let schema = serde_json::to_value(schema_bundle()).unwrap();
        assert_eq!(
            schema["$defs"]["TerminalAttach"]["properties"]["version"]["minimum"],
            TERMINAL_VERSION
        );
        assert_eq!(
            schema["$defs"]["TerminalReady"]["properties"]["version"]["maximum"],
            TERMINAL_VERSION
        );
        assert!(schema["$defs"].get("TerminalServerControl").is_some());
    }
    #[test]
    fn public_inputs_reject_authority_and_provider_fields() {
        for name in [
            "owner",
            "tenant",
            "profile",
            "image",
            "templateId",
            "computerId",
            "command",
        ] {
            let value = serde_json::json!({name: "forged"});
            assert!(serde_json::from_value::<CreateInput>(value.clone()).is_err());
            assert!(serde_json::from_value::<StartInput>(value.clone()).is_err());
            assert!(serde_json::from_value::<StopInput>(value.clone()).is_err());
            assert!(serde_json::from_value::<TerminalTicketInput>(value).is_err());
        }
    }

    #[test]
    fn schema_contains_all_public_contracts_and_dates() {
        let schema = serde_json::to_value(schema_bundle()).unwrap();
        let defs = schema["$defs"].as_object().unwrap();
        for name in [
            "ComputerSnapshot",
            "ExecutionResult",
            "TransferFileInput",
            "FileTransferResult",
            "FileTransferStage",
            "MaintenanceResult",
            "MaintenanceState",
            "MaintenanceView",
            "UpdateTemplateInput",
            "ResumeUpdateInput",
            "ExecutionOutput",
            "ComputerView",
            "TemplateView",
            "ComputerLimits",
            "OperationReceipt",
            "OperationView",
            "CreateInput",
            "StartInput",
            "StopInput",
            "TerminalTicketInput",
            "TerminalTicket",
            "ApiError",
            "ComputerEvent",
            "ComputerPhase",
            "OperationStatus",
            "Action",
            "ErrorCode",
        ] {
            assert!(defs.contains_key(name), "missing {name}");
        }
        assert_eq!(
            defs["TerminalTicket"]["properties"]["expiresAt"]["format"],
            "date-time"
        );
        for name in ["canCreate", "canStart", "canStop", "canConnect"] {
            assert_eq!(defs["ComputerView"]["properties"][name]["type"], "boolean");
            assert!(
                defs["ComputerView"]["required"]
                    .as_array()
                    .unwrap()
                    .contains(&serde_json::json!(name))
            );
        }
        for name in [
            "CreateInput",
            "StartInput",
            "StopInput",
            "TerminalTicketInput",
        ] {
            assert_eq!(defs[name]["additionalProperties"], false);
        }
    }
}
