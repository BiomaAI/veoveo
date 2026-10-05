//! Agent-owned JSON envelopes; backend reconstruction data stays opaque JSON.
use super::{AgentInputRequestId, WakeKind};
use serde::{Deserialize, Serialize};
use surrealdb::types::{Error, Kind, SurrealValue, Value};
use uuid::Uuid;
use veoveo_types::{CanonicalTaskId, PrincipalId, ResourceUri, WorkContextId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum InputWakePhase {
    #[vocabulary(rename = "pending")]
    Pending,
    #[vocabulary(rename = "answered")]
    Answered,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum TimerKind {
    #[vocabulary(rename = "heartbeat")]
    Heartbeat,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum WakePayload {
    TaskResult {
        task_id: CanonicalTaskId,
    },
    ResourceChanged {
        uri: ResourceUri,
    },
    Timer {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timer_kind: Option<TimerKind>,
    },
    OperatorMessage {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<Uuid>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor_id: Option<PrincipalId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work_context: Option<WorkContextId>,
    },
    InputRequest {
        input_request_id: AgentInputRequestId,
        phase: InputWakePhase,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<Uuid>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor_id: Option<PrincipalId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work_context: Option<WorkContextId>,
    },
}
impl WakePayload {
    pub fn kind(&self) -> WakeKind {
        match self {
            Self::TaskResult { .. } => WakeKind::TaskResult,
            Self::ResourceChanged { .. } => WakeKind::ResourceChanged,
            Self::Timer { .. } => WakeKind::Timer,
            Self::OperatorMessage { .. } => WakeKind::OperatorMessage,
            Self::InputRequest { .. } => WakeKind::InputRequest,
        }
    }
    pub fn operator(text: impl Into<String>) -> Self {
        Self::OperatorMessage {
            text: text.into(),
            request_id: None,
            actor_id: None,
            work_context: None,
        }
    }
    pub fn timer(name: impl Into<String>) -> Self {
        Self::Timer {
            name: name.into(),
            timer_kind: None,
        }
    }
    pub fn input(id: AgentInputRequestId, phase: InputWakePhase) -> Self {
        Self::InputRequest {
            input_request_id: id,
            phase,
            request_id: None,
            actor_id: None,
            work_context: None,
        }
    }
}

/// Rig v1 spelling is preserved; only `payload` belongs to the backend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum DeferredTaskDescriptor {
    Complete {
        version: u16,
        #[serde(rename = "backendType")]
        backend_type: String,
        #[serde(rename = "executionId")]
        execution_id: String,
        payload: serde_json::Value,
    },
    Incomplete {},
}
impl DeferredTaskDescriptor {
    pub fn validate(&self, complete: bool, task_id: &str) -> bool {
        match self {
            Self::Complete {
                version,
                backend_type,
                execution_id,
                ..
            } => {
                complete
                    && *version == 1
                    && !backend_type.trim().is_empty()
                    && execution_id == task_id
            }
            Self::Incomplete {} => !complete,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum TaskDelivery {
    #[vocabulary(rename = "in_run")]
    InRun,
    #[vocabulary(rename = "watcher")]
    Watcher,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum AgentTaskOutcome {
    Output {
        output: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delivered: Option<TaskDelivery>,
    },
    Error {
        error: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delivered: Option<TaskDelivery>,
    },
    /// The retained error wrapper used by detached watchers.
    WrappedError { value: TaskErrorBody },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskErrorBody {
    pub error: String,
}
impl AgentTaskOutcome {
    pub fn output(value: impl Into<serde_json::Value>, delivered: TaskDelivery) -> Self {
        Self::Output {
            output: value.into(),
            delivered: Some(delivered),
        }
    }
    pub fn watcher_error(error: impl Into<String>) -> Self {
        Self::WrappedError {
            value: TaskErrorBody {
                error: error.into(),
            },
        }
    }
}

fn decode<T: serde::de::DeserializeOwned>(
    mut value: Value,
    optional_fields: &[&str],
) -> Result<T, Error> {
    // SCHEMAFULL may materialize declared optional members as NONE. Only
    // these known members represent absence; nested backend NONE is invalid JSON.
    if let Value::Object(fields) = &mut value {
        for field in optional_fields {
            if fields.get(*field) == Some(&Value::None) {
                fields.remove(*field);
            }
        }
    }
    serde_json::from_value(veoveo_platform_store::native_json_from_value_strict(value)?)
        .map_err(|error| Error::internal(format!("invalid Agent envelope: {error}")))
}
fn encode(value: impl Serialize) -> Value {
    veoveo_platform_store::native_json_into_value(
        serde_json::to_value(value).expect("Agent JSON envelope"),
    )
}
impl SurrealValue for WakePayload {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        encode(self)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        let optional = match &value {
            Value::Object(fields) if fields.contains_key("name") => &["timer_kind"][..],
            Value::Object(fields)
                if fields.contains_key("text") || fields.contains_key("input_request_id") =>
            {
                &["request_id", "actor_id", "work_context"][..]
            }
            _ => &[],
        };
        decode(value, optional)
    }
}
impl SurrealValue for DeferredTaskDescriptor {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        encode(self)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        decode(value, &[])
    }
}
impl SurrealValue for AgentTaskOutcome {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        encode(self)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        let optional = match &value {
            Value::Object(fields)
                if fields.contains_key("output") || fields.contains_key("error") =>
            {
                &["delivered"][..]
            }
            _ => &[],
        };
        decode(value, optional)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn controlled_envelopes_reject_unknown_fields_and_native_values() {
        for value in [
            json!({"name":"tick","extra":true}),
            json!({"text":"hello","phase":"answered"}),
            json!({"task_id":"task","uri":"resource"}),
        ] {
            assert!(serde_json::from_value::<WakePayload>(value.clone()).is_err());
            assert!(
                WakePayload::from_value(veoveo_platform_store::native_json_into_value(value))
                    .is_err()
            );
        }
        assert!(
            WakePayload::from_value(Value::RecordId(surrealdb::types::RecordId::new(
                "wake", "native"
            )))
            .is_err()
        );
    }
    #[test]
    fn five_wake_variants_preserve_wire_spelling_and_typed_identity() {
        let request_id = Uuid::now_v7();
        let input_request_id = AgentInputRequestId::new();
        let cases = [
            (
                WakePayload::TaskResult {
                    task_id: CanonicalTaskId::parse("gtr_test_wake").unwrap(),
                },
                json!({"task_id":"gtr_test_wake"}),
            ),
            (
                WakePayload::ResourceChanged {
                    uri: ResourceUri::new("memo://insights").unwrap(),
                },
                json!({"uri":"memo://insights"}),
            ),
            (
                WakePayload::Timer {
                    name: "heartbeat".into(),
                    timer_kind: Some(TimerKind::Heartbeat),
                },
                json!({"name":"heartbeat","timer_kind":"heartbeat"}),
            ),
            (
                WakePayload::OperatorMessage {
                    text: "hello".into(),
                    request_id: Some(request_id),
                    actor_id: Some(PrincipalId::parse("https://issuer.test#actor").unwrap()),
                    work_context: Some(WorkContextId::parse("operations").unwrap()),
                },
                json!({"text":"hello","request_id":request_id,"actor_id":"https://issuer.test#actor","work_context":"operations"}),
            ),
            (
                WakePayload::input(input_request_id, InputWakePhase::Answered),
                json!({"input_request_id":input_request_id,"phase":"answered"}),
            ),
        ];
        for (payload, wire) in cases {
            assert_eq!(serde_json::to_value(&payload).unwrap(), wire);
            assert_eq!(
                serde_json::from_value::<WakePayload>(wire).unwrap(),
                payload
            );
            assert_eq!(
                WakePayload::from_value(payload.clone().into_value()).unwrap(),
                payload
            );
        }
        assert!(
            serde_json::from_value::<WakePayload>(json!({"uri":"not a resource URI"})).is_err()
        );
        assert!(
            serde_json::from_value::<WakePayload>(json!({"text":"hello","actor_id":""})).is_err()
        );
        let Value::Object(mut fields) = WakePayload::timer("tick").into_value() else {
            unreachable!()
        };
        fields.insert("timer_kind", Value::None);
        assert!(WakePayload::from_value(Value::Object(fields.clone())).is_ok());
        fields.insert("actor_id", Value::None);
        assert!(WakePayload::from_value(Value::Object(fields)).is_err());
    }

    #[test]
    fn opaque_backend_and_tool_bodies_reject_nested_native_values() {
        for native in [
            Value::None,
            Value::RecordId(surrealdb::types::RecordId::new("tool", "native")),
            chrono::Utc::now().into_value(),
        ] {
            let mut nested = surrealdb::types::Object::new();
            nested.insert("native", native);
            let body = Value::Array(vec![Value::Object(nested)].into());
            let descriptor = DeferredTaskDescriptor::Complete {
                version: 1,
                backend_type: "mcp".into(),
                execution_id: "task".into(),
                payload: json!({}),
            };
            let Value::Object(mut fields) = descriptor.into_value() else {
                unreachable!()
            };
            fields.insert("payload", body.clone());
            assert!(DeferredTaskDescriptor::from_value(Value::Object(fields)).is_err());
            let outcome = AgentTaskOutcome::output(json!({}), TaskDelivery::Watcher);
            let Value::Object(mut fields) = outcome.into_value() else {
                unreachable!()
            };
            fields.insert("output", body);
            assert!(AgentTaskOutcome::from_value(Value::Object(fields)).is_err());
        }
    }

    #[test]
    fn descriptor_recovery_and_result_delivery_preserve_open_bodies() {
        let incomplete = DeferredTaskDescriptor::Incomplete {};
        assert!(incomplete.validate(false, "task"));
        assert!(!incomplete.validate(true, "task"));
        let descriptor: DeferredTaskDescriptor = serde_json::from_value(json!({"version":1,"backendType":"mcp","executionId":"task","payload":{"extension":{"max":u64::MAX}}})).unwrap();
        assert!(descriptor.validate(true, "task"));
        assert!(!descriptor.validate(true, "other"));
        let Value::Object(mut fields) = descriptor.clone().into_value() else {
            unreachable!()
        };
        fields.insert(
            "payload",
            Value::RecordId(surrealdb::types::RecordId::new("tool", "native")),
        );
        assert!(DeferredTaskDescriptor::from_value(Value::Object(fields)).is_err());
        assert_eq!(
            DeferredTaskDescriptor::from_value(descriptor.clone().into_value()).unwrap(),
            descriptor
        );
        for outcome in [
            AgentTaskOutcome::output(json!({"opaque":[null,u64::MAX]}), TaskDelivery::Watcher),
            AgentTaskOutcome::Error {
                error: "cancelled".into(),
                delivered: Some(TaskDelivery::InRun),
            },
            AgentTaskOutcome::watcher_error("failed"),
        ] {
            assert_eq!(
                AgentTaskOutcome::from_value(outcome.clone().into_value()).unwrap(),
                outcome
            );
        }
        assert!(
            serde_json::from_value::<AgentTaskOutcome>(
                json!({"output":"ok","delivered":"elsewhere"})
            )
            .is_err()
        );
    }
}
