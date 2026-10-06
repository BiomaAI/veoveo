//! Installation snapshots: owner values reach the browser without runtime dependencies.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use veoveo_agent_runtime::contract::AgentState;
use veoveo_artifact_contract::{ArtifactId, ArtifactReleaseState, ArtifactShareLinkId};
use veoveo_gateway_contract::{
    ConsoleInstallation, ConsoleSession, GatewayServerHealthState, OwnedRoutePurpose,
    UpstreamTransport, UpstreamUrl,
};
use veoveo_recording_contract::{RecordingId, RecordingLayerId, RecordingState};
use veoveo_task_contract::{RecoveryClass, TaskStatus};
use veoveo_types::{
    AccessLevel, AccessSubject, Check, Checked, DataLabelId, DelegationId, GatewayProfileId,
    GroupId, InvocationMode, LocalToolName, PolicyVersion, PrincipalId, PromptName, ResourceScheme,
    ScopeName, ServerSlug, TaskId, WorkContextId,
};

/// A byte length that a browser can represent without losing integer precision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArtifactByteLength(Checked<ArtifactByteLengthValue>);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
#[schemars(rename = "ArtifactByteLength")]
struct ArtifactByteLengthValue(#[schemars(range(max = ArtifactByteLength::MAX))] u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("byte length exceeds the browser integer range")]
pub struct ArtifactByteLengthError;

impl Check for ArtifactByteLengthValue {
    type Error = ArtifactByteLengthError;

    fn check(&self) -> Result<(), Self::Error> {
        if self.0 > ArtifactByteLength::MAX {
            return Err(ArtifactByteLengthError);
        }
        Ok(())
    }
}

impl ArtifactByteLength {
    pub const MAX: u64 = 9_007_199_254_740_991;

    pub fn new(value: u64) -> Result<Self, ArtifactByteLengthError> {
        Checked::new(ArtifactByteLengthValue(value)).map(Self)
    }

    pub fn get(self) -> u64 {
        self.0.get().0
    }
}

impl JsonSchema for ArtifactByteLength {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ArtifactByteLength".into()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::ArtifactByteLength").into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ArtifactByteLengthValue::json_schema(generator)
    }
}

impl TryFrom<u64> for ArtifactByteLength {
    type Error = ArtifactByteLengthError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

fn required_nullable<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ArtifactByteLength>, D::Error> {
    Option::deserialize(deserializer)
}

fn nullable_byte_length_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    // A direct nullable scalar avoids converter-created references between repeated
    // inline Option schemas in snapshot and event roots. The checked owner supplies
    // its bounds; this adapter changes only field nullability.
    let mut schema = ArtifactByteLength::json_schema(generator);
    schema
        .ensure_object()
        .insert("type".into(), serde_json::json!(["integer", "null"]));
    schema
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum ServiceKind {
    #[vocabulary(rename = "database")]
    Database,
    #[vocabulary(rename = "gateway")]
    Gateway,
    #[vocabulary(rename = "mcp")]
    Mcp,
    #[vocabulary(rename = "object_store")]
    ObjectStore,
    #[vocabulary(rename = "observability")]
    Observability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum PolicyState {
    #[vocabulary(rename = "draft")]
    Draft,
    #[vocabulary(rename = "active")]
    Active,
    #[vocabulary(rename = "retired")]
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum ArtifactAccessDenialReason {
    #[vocabulary(rename = "tenant_boundary")]
    TenantBoundary,
    #[vocabulary(rename = "clearance")]
    Clearance,
    #[vocabulary(rename = "need_to_know")]
    NeedToKnow,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsoleSnapshot {
    pub installation: ConsoleInstallation,
    pub session: ConsoleSession,
    pub principals: Vec<PrincipalSummary>,
    pub stream: StreamInfo,
    pub services: Vec<ServiceSummary>,
    pub tasks: Vec<TaskSummary>,
    pub artifacts: Vec<ArtifactSummary>,
    pub agents: Vec<AgentSummary>,
    pub recordings: Vec<RecordingSummary>,
    pub servers: Vec<ServerSummary>,
    pub policies: Vec<PolicySummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StreamInfo {
    pub cursor: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceSummary {
    pub id: String,
    pub name: String,
    pub kind: ServiceKind,
    pub state: GatewayServerHealthState,
    pub detail: String,
    pub checked_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrincipalSummary {
    pub id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskSummary {
    pub id: TaskId,
    pub r#type: veoveo_types::TaskTypeName,
    pub server: ServerSlug,
    pub owner: String,
    pub state: TaskStatus,
    pub recovery_class: RecoveryClass,
    pub progress: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_artifact_id: Option<ArtifactId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactSummary {
    pub id: ArtifactId,
    pub filename: String,
    pub media_type: String,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "nullable_byte_length_schema")]
    pub byte_length: Option<ArtifactByteLength>,
    pub owner: String,
    pub output_owner: ArtifactOutputOwnerSummary,
    pub provenance: ArtifactGovernanceSummary,
    pub effective_access: ArtifactEffectiveAccessSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    pub classification: String,
    pub labels: Vec<DataLabelId>,
    pub release_state: ArtifactReleaseState,
    pub authorized_grants: usize,
    pub active_links: usize,
    pub grants: Vec<ArtifactGrantSummary>,
    pub share_links: Vec<ArtifactShareLinkSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recording: Option<ArtifactRecordingSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ArtifactOutputOwnerSummary(pub AccessSubject);

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactGovernanceSummary {
    pub work_context: WorkContextId,
    pub producer: PrincipalId,
    pub invocation_mode: InvocationMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initiator: Option<PrincipalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegation_id: Option<DelegationId>,
    pub policy_revision: PolicyVersion,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactEffectiveAccessSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<AccessLevel>,
    pub read: bool,
    pub write: bool,
    pub admin: bool,
    pub clearance_satisfied: bool,
    pub requestable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub denial_reason: Option<ArtifactAccessDenialReason>,
    pub sources: Vec<ArtifactAccessSourceSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ArtifactAccessSourceSummary(pub ArtifactAccessSource);

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactRecordingSummary {
    pub recording_id: RecordingId,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer_id: Option<RecordingLayerId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordinal: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ArtifactGrantSummary(pub ArtifactGrantSummaryValue);

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactShareLinkSummary {
    pub id: ArtifactShareLinkId,
    pub permission: AccessLevel,
    pub expires_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_downloads: Option<i64>,
    pub download_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub profile: GatewayProfileId,
    pub state: AgentState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_lease_expires_at: Option<DateTime<Utc>>,
    pub pending_wakes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_episode_at: Option<DateTime<Utc>>,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordingSummary {
    pub id: RecordingId,
    pub application: String,
    pub recording_key: String,
    pub state: RecordingState,
    pub layer_count: usize,
    pub committed_layer_count: usize,
    pub committed_byte_length: i64,
    pub started_at: DateTime<Utc>,
    pub last_data_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sealed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerSummary {
    pub id: ServerSlug,
    pub name: String,
    pub uri_scheme: ResourceScheme,
    pub transport: UpstreamTransport,
    pub endpoint: UpstreamUrl,
    pub state: GatewayServerHealthState,
    pub checked_at: DateTime<Utc>,
    pub capabilities: ServerCapabilitiesSummary,
    pub tools: Vec<LocalToolName>,
    pub compatibility_helpers: Vec<String>,
    pub resources: Vec<String>,
    pub prompts: Vec<PromptName>,
    pub required_scopes: Vec<ScopeName>,
    pub owned_routes: Vec<ServerRouteSummary>,
    pub profiles: Vec<GatewayProfileId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerCapabilitiesSummary {
    pub tools: bool,
    pub resources: bool,
    pub resource_templates: bool,
    pub resource_subscriptions: bool,
    pub prompts: bool,
    pub completions: bool,
    pub tasks: bool,
    pub tools_list_changed: bool,
    pub prompts_list_changed: bool,
    pub resources_list_changed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerRouteSummary {
    pub path: String,
    pub purpose: OwnedRoutePurpose,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolicySummary {
    pub id: PolicyVersion,
    pub name: String,
    pub revision: usize,
    pub state: PolicyState,
    pub rules: usize,
    pub updated_at: DateTime<Utc>,
}

/// Access origin determines the domain of the subject identifier.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArtifactAccessSource {
    PrincipalGrant {
        subject: PrincipalId,
        level: AccessLevel,
    },
    GroupGrant {
        subject: GroupId,
        level: AccessLevel,
    },
    WorkContext {
        subject: WorkContextId,
        level: AccessLevel,
    },
}

/// Grant kind and subject cannot disagree inside a usable wire value.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "subjectKind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ArtifactGrantSummaryValue {
    Principal {
        subject: PrincipalId,
        permission: AccessLevel,
        labels: Vec<DataLabelId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
    },
    Group {
        subject: GroupId,
        permission: AccessLevel,
        labels: Vec<DataLabelId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
    },
}

impl ArtifactGrantSummary {
    pub fn new(
        subject: AccessSubject,
        permission: AccessLevel,
        labels: Vec<DataLabelId>,
        expires_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self(match subject {
            AccessSubject::Principal(subject) => ArtifactGrantSummaryValue::Principal {
                subject,
                permission,
                labels,
                expires_at,
                created_at,
            },
            AccessSubject::Group(subject) => ArtifactGrantSummaryValue::Group {
                subject,
                permission,
                labels,
                expires_at,
                created_at,
            },
        })
    }
}

impl ArtifactGrantSummary {
    pub fn subject(&self) -> AccessSubject {
        match &self.0 {
            ArtifactGrantSummaryValue::Principal { subject, .. } => {
                AccessSubject::Principal(subject.clone())
            }
            ArtifactGrantSummaryValue::Group { subject, .. } => {
                AccessSubject::Group(subject.clone())
            }
        }
    }
    pub fn subject_kind(&self) -> veoveo_artifact_contract::ArtifactGrantSubjectKind {
        match self.0 {
            ArtifactGrantSummaryValue::Principal { .. } => {
                veoveo_artifact_contract::ArtifactGrantSubjectKind::Principal
            }
            ArtifactGrantSummaryValue::Group { .. } => {
                veoveo_artifact_contract::ArtifactGrantSubjectKind::Group
            }
        }
    }
    pub fn permission(&self) -> AccessLevel {
        match self.0 {
            ArtifactGrantSummaryValue::Principal { permission, .. }
            | ArtifactGrantSummaryValue::Group { permission, .. } => permission,
        }
    }
    pub fn labels(&self) -> &[DataLabelId] {
        match &self.0 {
            ArtifactGrantSummaryValue::Principal { labels, .. }
            | ArtifactGrantSummaryValue::Group { labels, .. } => labels,
        }
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        match self.0 {
            ArtifactGrantSummaryValue::Principal { created_at, .. }
            | ArtifactGrantSummaryValue::Group { created_at, .. } => created_at,
        }
    }
    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        match self.0 {
            ArtifactGrantSummaryValue::Principal { expires_at, .. }
            | ArtifactGrantSummaryValue::Group { expires_at, .. } => expires_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nullable_byte_length_requires_the_field_and_preserves_null() {
        #[derive(Serialize, Deserialize)]
        struct ByteLength {
            #[serde(deserialize_with = "required_nullable")]
            byte_length: Option<ArtifactByteLength>,
        }
        assert!(serde_json::from_str::<ByteLength>("{}").is_err());
        assert_eq!(
            serde_json::from_str::<ByteLength>(r#"{"byte_length":null}"#)
                .unwrap()
                .byte_length,
            None
        );
        assert_eq!(
            serde_json::from_str::<ByteLength>(r#"{"byte_length":9007199254740991}"#)
                .unwrap()
                .byte_length,
            Some(ArtifactByteLength::new(ArtifactByteLength::MAX).unwrap())
        );
        assert!(serde_json::from_str::<ByteLength>(r#"{"byte_length":9007199254740992}"#).is_err());
        assert_eq!(
            serde_json::to_value(ByteLength { byte_length: None }).unwrap(),
            serde_json::json!({"byte_length":null})
        );
        let schema = serde_json::to_value(schemars::schema_for!(ArtifactSummary)).unwrap();
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|name| name == "byteLength")
        );
    }

    #[test]
    fn constructed_byte_lengths_and_decoding_admit_the_same_values() {
        for value in [0, ArtifactByteLength::MAX] {
            let admitted = ArtifactByteLength::new(value).unwrap();
            assert_eq!(admitted.get(), value);
            let wire = serde_json::to_value(admitted).unwrap();
            assert_eq!(wire, serde_json::json!(value));
            assert_eq!(
                serde_json::from_value::<ArtifactByteLength>(wire).unwrap(),
                admitted
            );
        }
        for value in [ArtifactByteLength::MAX + 1, u64::MAX] {
            assert_eq!(ArtifactByteLength::new(value), Err(ArtifactByteLengthError));
            assert!(
                serde_json::from_value::<ArtifactByteLength>(serde_json::json!(value)).is_err()
            );
        }
        let schema = serde_json::to_value(schemars::schema_for!(ArtifactByteLength)).unwrap();
        assert_eq!(schema["type"], "integer");
        assert_eq!(schema["maximum"], ArtifactByteLength::MAX);
    }

    #[test]
    fn emitted_artifact_schema_admits_required_nullable_byte_length_with_checked_bounds() {
        let artifact = ArtifactSummary {
            id: ArtifactId::new(),
            filename: "fixture.bin".into(),
            media_type: "application/octet-stream".into(),
            byte_length: None,
            owner: "Operator".into(),
            output_owner: ArtifactOutputOwnerSummary(AccessSubject::Principal(
                PrincipalId::parse("actor").unwrap(),
            )),
            provenance: ArtifactGovernanceSummary {
                work_context: WorkContextId::parse("operations").unwrap(),
                producer: PrincipalId::parse("actor").unwrap(),
                invocation_mode: InvocationMode::Automated,
                initiator: None,
                delegation_id: None,
                policy_revision: PolicyVersion::parse("r1").unwrap(),
            },
            effective_access: ArtifactEffectiveAccessSummary {
                level: None,
                read: false,
                write: false,
                admin: false,
                clearance_satisfied: true,
                requestable: true,
                denial_reason: Some(ArtifactAccessDenialReason::NeedToKnow),
                sources: Vec::new(),
            },
            task_id: None,
            classification: "internal".into(),
            labels: Vec::new(),
            release_state: ArtifactReleaseState::Private,
            authorized_grants: 0,
            active_links: 0,
            grants: Vec::new(),
            share_links: Vec::new(),
            retention_expires_at: None,
            created_at: DateTime::parse_from_rfc3339("2026-10-05T12:34:56.123456789Z")
                .unwrap()
                .with_timezone(&Utc),
            recording: None,
        };
        let schema = serde_json::to_value(schemars::schema_for!(ArtifactSummary)).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let wire = serde_json::to_value(&artifact).unwrap();
        let event = super::super::events::ArtifactEvent::Upsert {
            row: Box::new(artifact.clone()),
        };
        let event_wire = serde_json::to_value(&event).unwrap();
        assert_eq!(event_wire, serde_json::json!({"op": "upsert", "row": wire}));
        let event_schema =
            serde_json::to_value(schemars::schema_for!(super::super::events::ArtifactEvent))
                .unwrap();
        assert!(
            jsonschema::validator_for(&event_schema)
                .unwrap()
                .is_valid(&event_wire)
        );
        let decoded: super::super::events::ArtifactEvent =
            serde_json::from_value(event_wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), event_wire);

        assert!(validator.is_valid(&wire));
        for value in [
            serde_json::Value::Null,
            serde_json::json!(0),
            serde_json::json!(ArtifactByteLength::MAX),
        ] {
            let mut candidate = wire.clone();
            candidate["byteLength"] = value;
            assert!(validator.is_valid(&candidate));
            assert!(serde_json::from_value::<ArtifactSummary>(candidate).is_ok());
        }
        for value in [
            serde_json::json!(-1),
            serde_json::json!(0.5),
            serde_json::json!(ArtifactByteLength::MAX + 1),
            serde_json::json!(u64::MAX),
        ] {
            let mut candidate = wire.clone();
            candidate["byteLength"] = value;
            assert!(!validator.is_valid(&candidate));
            assert!(serde_json::from_value::<ArtifactSummary>(candidate).is_err());
        }
        let mut missing = wire;
        missing.as_object_mut().unwrap().remove("byteLength");
        assert!(!validator.is_valid(&missing));
        assert!(serde_json::from_value::<ArtifactSummary>(missing).is_err());
    }

    #[test]
    fn grant_and_access_source_subjects_preserve_their_wire_profiles() {
        let created_at = DateTime::parse_from_rfc3339("2026-10-05T12:34:56.123456789Z")
            .unwrap()
            .with_timezone(&Utc);
        let grant = ArtifactGrantSummary::new(
            AccessSubject::Group(GroupId::parse("operators").unwrap()),
            AccessLevel::Read,
            vec![DataLabelId::parse("cui").unwrap()],
            None,
            created_at,
        );
        let wire = serde_json::to_value(&grant).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({"subjectKind":"group", "subject":"operators", "permission":"read", "labels":["cui"], "createdAt":"2026-10-05T12:34:56.123456789Z"})
        );
        assert!(serde_json::from_value::<ArtifactGrantSummary>(wire.clone()).is_ok());
        assert!(!wire.as_object().unwrap().contains_key("expiresAt"));
        let mut extra = wire;
        extra["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ArtifactGrantSummary>(extra).is_err());
        let source = ArtifactAccessSourceSummary(ArtifactAccessSource::WorkContext {
            subject: WorkContextId::parse("operations").unwrap(),
            level: AccessLevel::Admin,
        });
        let wire = serde_json::to_value(&source).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({"kind":"work_context", "subject":"operations", "level":"admin"})
        );
        assert!(serde_json::from_value::<ArtifactAccessSourceSummary>(wire).is_ok());
    }

    #[test]
    fn controlled_nested_vocabularies_reject_unknown_values() {
        assert!(serde_json::from_str::<ServiceKind>("\"worker\"").is_err());
        assert!(serde_json::from_str::<PolicyState>("\"deleted\"").is_err());
        assert!(serde_json::from_str::<ArtifactAccessDenialReason>("\"unknown\"").is_err());
        assert!(
            serde_json::from_value::<ArtifactAccessSource>(
                serde_json::json!({"kind":"unknown", "subject":"human", "level":"read"})
            )
            .is_err()
        );
        assert!(serde_json::from_value::<ArtifactGrantSummary>(serde_json::json!({"subjectKind":"unknown", "subject":"human", "permission":"read", "labels":[], "createdAt":"2026-10-05T00:00:00Z"})).is_err());
    }
}
