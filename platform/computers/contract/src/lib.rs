//! Canonical public JSON contract. Generate every client model from schema_bundle().
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const COMPUTERS_URI: &str = "computer://computers";
pub fn computer_uri(id: Uuid) -> String {
    format!("{COMPUTERS_URI}/{id}")
}

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
    pub template_id: String,
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
pub struct ComputerSnapshot {
    pub availability: CapacityAvailability,
    pub template: Option<TemplateView>,
    pub limits: Option<ComputerLimits>,
    pub can_create: bool,
    pub computers: Vec<ComputerView>,
    pub next_cursor: Option<Uuid>,
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
pub struct ComputerView {
    pub computer_id: Uuid,
    pub template_id: String,
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
    pub active_task_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationReceipt {
    pub task_id: Uuid,
    pub computer_id: Uuid,
    pub action: Action,
    pub status: OperationStatus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifecycleResult {
    #[serde(rename = "result_uri", skip_serializing_if = "Option::is_none")]
    pub result_uri: Option<String>,
    pub computer_id: Uuid,
    pub operation_id: Uuid,
    pub action: Action,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationView {
    pub task_id: Uuid,
    pub computer_id: Uuid,
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
    pub request_id: Uuid,
    /// Continue provisioning an owned reservation, or omit for a new Computer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computer_id: Option<Uuid>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartInput {
    pub request_id: Uuid,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StopInput {
    pub request_id: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifecycleInput {
    pub computer_id: Uuid,
    pub request_id: Uuid,
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
    pub computer_id: Uuid,
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
    pub computer_id: Option<Uuid>,
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
    pub computer_id: Uuid,
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
    snapshot: ComputerSnapshot,
    computer: ComputerView,
    template: TemplateView,
    limits: ComputerLimits,
    receipt: OperationReceipt,
    lifecycle_result: LifecycleResult,
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
