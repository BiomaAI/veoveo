//! SSE event names select a closed row contract before cache mutation.
use super::installation::{
    AgentSummary, ArtifactSummary, PrincipalSummary, RecordingSummary, ServerSummary, TaskSummary,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::{ArtifactAccessRequestId, ArtifactId};
use veoveo_recording_contract::RecordingId;
use veoveo_types::{ServerSlug, TaskId};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrincipalEvent {
    Upsert { row: PrincipalSummary },
    Delete { id: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskEvent {
    Upsert { row: TaskSummary },
    Delete { id: TaskId },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArtifactEvent {
    Upsert { row: Box<ArtifactSummary> },
    Delete { id: ArtifactId },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentEvent {
    Upsert { row: AgentSummary },
    Delete { id: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecordingEvent {
    Upsert { row: RecordingSummary },
    Delete { id: RecordingId },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerEvent {
    Upsert { row: Box<ServerSummary> },
    Delete { id: ServerSlug },
}

/// Producer selection couples the external SSE name to its row type.
#[derive(Clone, Debug)]
pub enum ConsoleRowEvent {
    Principal(PrincipalEvent),
    Task(TaskEvent),
    Artifact(ArtifactEvent),
    Agent(AgentEvent),
    Recording(RecordingEvent),
    Server(ServerEvent),
}

impl ConsoleRowEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::Principal(_) => "principal",
            Self::Task(_) => "task",
            Self::Artifact(_) => "artifact",
            Self::Agent(_) => "agent",
            Self::Recording(_) => "recording",
            Self::Server(_) => "server",
        }
    }

    pub fn encode(&self) -> Result<String, serde_json::Error> {
        match self {
            Self::Principal(event) => serde_json::to_string(event),
            Self::Task(event) => serde_json::to_string(event),
            Self::Artifact(event) => serde_json::to_string(event),
            Self::Agent(event) => serde_json::to_string(event),
            Self::Recording(event) => serde_json::to_string(event),
            Self::Server(event) => serde_json::to_string(event),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum ResetReason {
    #[vocabulary(rename = "cursor-out-of-range")]
    CursorOutOfRange,
    #[vocabulary(rename = "seed-failed")]
    SeedFailed,
    #[vocabulary(rename = "replay-failed")]
    ReplayFailed,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResetEvent {
    pub reason: ResetReason,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum AccessRequestEvent {
    Changed { id: ArtifactAccessRequestId },
}

/// Named schema roots used by generated clients and runtime validators.
#[derive(JsonSchema)]
#[schemars(rename = "ConsoleApi", rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsoleSchemaBundle {
    pub bootstrap: veoveo_gateway_contract::ConsoleBootstrap,
    pub snapshot: super::installation::ConsoleSnapshot,
    pub artifact: ArtifactSummary,
    pub principal_event: PrincipalEvent,
    pub task_event: TaskEvent,
    pub artifact_event: ArtifactEvent,
    pub agent_event: AgentEvent,
    pub recording_event: RecordingEvent,
    pub server_event: ServerEvent,
    pub reset_event: ResetEvent,
    pub access_request_event: AccessRequestEvent,
}

pub fn schema_bundle() -> schemars::Schema {
    schemars::schema_for!(ConsoleSchemaBundle)
}

impl Serialize for ConsoleRowEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Principal(event) => event.serialize(serializer),
            Self::Task(event) => event.serialize(serializer),
            Self::Artifact(event) => event.serialize(serializer),
            Self::Agent(event) => event.serialize(serializer),
            Self::Recording(event) => event.serialize(serializer),
            Self::Server(event) => event.serialize(serializer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn producer_event_name_matches_encoded_row_profile() {
        let event = ConsoleRowEvent::Principal(PrincipalEvent::Upsert {
            row: PrincipalSummary {
                id: "issuer#subject".into(),
                display_name: "Operator".into(),
            },
        });
        assert_eq!(event.event_name(), "principal");
        let encoded = event.encode().unwrap();
        assert!(serde_json::from_str::<PrincipalEvent>(&encoded).is_ok());
        assert!(serde_json::from_str::<TaskEvent>(&encoded).is_err());
        assert!(serde_json::from_str::<ServerEvent>(&encoded).is_err());
    }

    #[test]
    fn event_admission_rejects_unknown_operations_and_extra_fields() {
        for value in [
            serde_json::json!({"op":"replace","id":"issuer#subject"}),
            serde_json::json!({"op":"delete"}),
            serde_json::json!({"op":"delete","id":"issuer#subject","row":{}}),
            serde_json::json!({"op":"upsert","row":{"id":"issuer#subject","displayName":"Operator","extra":true}}),
        ] {
            assert!(serde_json::from_value::<PrincipalEvent>(value).is_err());
        }
        assert!(
            serde_json::from_value::<TaskEvent>(
                serde_json::json!({"op":"delete", "id":"not-a-task-id"})
            )
            .is_err()
        );
    }

    #[test]
    fn reset_reasons_are_closed() {
        for reason in ["cursor-out-of-range", "seed-failed", "replay-failed"] {
            let event =
                serde_json::from_value::<ResetEvent>(serde_json::json!({"reason":reason})).unwrap();
            assert_eq!(
                serde_json::to_value(event).unwrap(),
                serde_json::json!({"reason":reason})
            );
        }
        assert!(
            serde_json::from_value::<ResetEvent>(serde_json::json!({"reason":"unknown"})).is_err()
        );
        assert!(
            serde_json::from_value::<ResetEvent>(
                serde_json::json!({"reason":"seed-failed","op":"changed"})
            )
            .is_err()
        );
    }
}
