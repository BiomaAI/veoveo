use std::collections::{BTreeMap, BTreeSet};
pub(crate) use veoveo_console_bff::contract::installation::*;

use chrono::{DateTime, Utc};
use veoveo_agent_runtime::persistence::{AgentRecord, WakeRecord};
use veoveo_artifact_contract::{ArtifactId, Grant};
use veoveo_mcp_contract::{
    AccessDecision, AccessRequest, Exposure, GatewayControlPlane, GroupMembership, GroupRole,
    ResourceSelector, ServerManifest,
};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayServerHealth, GatewayServerHealthState};
use veoveo_platform_store::{
    ArtifactBlobRecord, ArtifactGrantEdge, ArtifactOccurrenceRecord, PrincipalRecord, RecordId,
    RecordIdKey, ShareLinkRecord, TaskRecord,
};
use veoveo_recording_store::{RecordingLayerRecord, RecordingRecord};
use veoveo_types::{AccessLevel, WorkContextMembershipLevel};
use veoveo_types::{
    AccessSubject, DataLabelId, InvocationMode, PrincipalId, TenantId, WorkContextId,
};

use crate::runtime::AdminState;

pub(crate) const SNAPSHOT_LIMIT: i64 = 200;
const LAYER_SNAPSHOT_LIMIT: i64 = 10_000;

pub(crate) struct Projection {
    pub(crate) principals: Vec<PrincipalRecord>,
    pub(crate) tasks: Vec<TaskRecord>,
    pub(crate) artifacts: Vec<ArtifactOccurrenceRecord>,
    pub(crate) blobs: Vec<ArtifactBlobRecord>,
    pub(crate) share_links: Vec<ShareLinkRecord>,
    pub(crate) grants: Vec<ArtifactGrantEdge>,
    pub(crate) agents: Vec<AgentRecord>,
    pub(crate) wakes: Vec<WakeRecord>,
    pub(crate) recordings: Vec<RecordingRecord>,
    pub(crate) layers: Vec<RecordingLayerRecord>,
}

pub(crate) fn principal_summary(principal: &PrincipalRecord) -> PrincipalSummary {
    PrincipalSummary {
        id: format!("{}#{}", principal.issuer, principal.subject),
        display_name: principal.display_name.clone(),
    }
}

pub(crate) async fn load_projection(
    state: &AdminState,
    tenant: &RecordId,
) -> anyhow::Result<Projection> {
    let mut response = state
        .control_store
        .platform_store()
        .client()
        .query(
            include_str!("../../../../queries/bin/gateway/admin/console/projection/load_projection/statement_1.surql"),
        )
        .bind(("tenant", tenant.clone()))
        .bind(("limit", SNAPSHOT_LIMIT))
        .bind(("layer_limit", LAYER_SNAPSHOT_LIMIT))
        .await?
        .check()?;
    let artifacts: Vec<ArtifactOccurrenceRecord> = response.take(2)?;
    let mut blob_ids: Vec<RecordId> = artifacts.iter().map(|row| row.blob.clone()).collect();
    blob_ids.sort();
    blob_ids.dedup();
    // Read at most the selected occurrences' exact blob keys. A WHERE ... IN
    // subquery scans the blob table and can reevaluate the occurrence query per row.
    let blobs =
        load_referenced_blobs(state.control_store.platform_store(), tenant, blob_ids).await?;
    Ok(Projection {
        principals: response.take(0)?,
        tasks: response.take(1)?,
        artifacts,
        blobs,
        share_links: response.take(3)?,
        grants: response.take(4)?,
        agents: response.take(5)?,
        wakes: response.take(6)?,
        recordings: response.take(7)?,
        layers: response.take(8)?,
    })
}

async fn load_referenced_blobs(
    store: &veoveo_platform_store::PlatformStore,
    tenant: &RecordId,
    blob_ids: Vec<RecordId>,
) -> anyhow::Result<Vec<ArtifactBlobRecord>> {
    if blob_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(store
        .client()
        .query(include_str!("../../../../queries/bin/gateway/admin/console/projection/load_referenced_blobs/statement_1.surql"))
        .bind(("blobs", blob_ids))
        .bind(("tenant", tenant.clone()))
        .await?
        .check()?
        .take(0)?)
}

#[derive(Clone)]
pub(crate) struct ArtifactAccessContext {
    actor: PrincipalId,
    tenant: TenantId,
    clearance: BTreeSet<DataLabelId>,
    groups: BTreeSet<GroupMembership>,
    work_context: WorkContextId,
    membership: WorkContextMembershipLevel,
}

impl ArtifactAccessContext {
    pub(super) fn matches_upload_scope(&self, tenant: &str, actor: &str, context: &str) -> bool {
        self.tenant.as_str() == tenant
            && self.actor.as_str() == actor
            && self.work_context.as_str() == context
    }

    pub(crate) fn from_subject(
        subject: &AuthenticatedSubject,
        tenant_key: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            actor: subject.actor.id.clone(),
            tenant: TenantId::parse(tenant_key)?,
            clearance: subject.actor.data_labels.clone(),
            groups: subject.actor.group_memberships(),
            work_context: subject.authority.work_context.clone(),
            membership: subject.authority.membership,
        })
    }
}

fn recording_artifact_summary(metadata: serde_json::Value) -> Option<ArtifactRecordingSummary> {
    use veoveo_recording_contract::{RecordingArtifactMetadata, RecordingArtifactProvenance};
    let metadata: RecordingArtifactMetadata = serde_json::from_value(metadata).ok()?;
    let kind = metadata.provenance.kind();
    let (recording_id, layer_id) = match metadata.provenance {
        RecordingArtifactProvenance::RecordingLayer {
            recording_id,
            layer_id,
            ..
        } => (recording_id, Some(layer_id)),
        RecordingArtifactProvenance::RecordingBlueprint { recording_id, .. } => {
            (recording_id, None)
        }
        RecordingArtifactProvenance::RecordingManifest { recording_id, .. } => (recording_id, None),
    };
    Some(ArtifactRecordingSummary {
        recording_id,
        kind,
        layer_id,
        // The owner metadata does not attest a layer ordinal.
        ordinal: None,
    })
}

pub(crate) fn task_summary(
    task: TaskRecord,
    principal_names: &BTreeMap<String, String>,
) -> anyhow::Result<TaskSummary> {
    Ok(TaskSummary {
        id: veoveo_types::TaskId::parse(record_key(&task.id)?)?,
        r#type: task.task_type,
        server: veoveo_types::ServerSlug::parse(record_key(&task.server)?)?,
        owner: display_record(principal_names, &task.owner)?,
        state: task.status,
        recovery_class: task.recovery_class,
        progress: task.progress,
        created_at: task.created_at,
        updated_at: task.updated_at,
        result_artifact_id: task
            .result_artifact
            .as_ref()
            .map(|id| Ok::<_, anyhow::Error>(ArtifactId::parse(record_key(id)?)?))
            .transpose()?,
        message: task.error.as_ref().map(|error| error.message.clone()),
    })
}

pub(crate) fn artifact_grant_summary(
    grant: &ArtifactGrantEdge,
) -> anyhow::Result<ArtifactGrantSummary> {
    let subject = match grant.subject_kind {
        veoveo_platform_store::ArtifactGrantSubjectKind::Principal => {
            AccessSubject::Principal(PrincipalId::parse(grant.subject_key.clone())?)
        }
        veoveo_platform_store::ArtifactGrantSubjectKind::Group => {
            AccessSubject::Group(veoveo_types::GroupId::parse(grant.subject_key.clone())?)
        }
    };
    Ok(ArtifactGrantSummary::new(
        subject,
        contract_access_level(grant.permission),
        grant
            .labels
            .iter()
            .map(|label| DataLabelId::parse(label.clone()))
            .collect::<Result<_, _>>()?,
        grant.expires_at,
        grant.created_at,
    ))
}

pub(crate) fn share_link_summary(
    link: &ShareLinkRecord,
    now: DateTime<Utc>,
) -> anyhow::Result<ArtifactShareLinkSummary> {
    let active = link.revoked_at.is_none()
        && link.expires_at > now
        && link
            .max_downloads
            .is_none_or(|max| link.download_count < max);
    Ok(ArtifactShareLinkSummary {
        id: veoveo_artifact_contract::ArtifactShareLinkId::parse(record_key(&link.id)?)?,
        permission: contract_access_level(link.permission),
        expires_at: link.expires_at,
        max_downloads: link.max_downloads,
        download_count: link.download_count,
        revoked_at: link.revoked_at,
        created_at: link.created_at,
        active,
    })
}

pub(crate) fn artifact_summary(
    artifact: ArtifactOccurrenceRecord,
    byte_length: Option<i64>,
    grants: Vec<ArtifactGrantSummary>,
    share_links: Vec<ArtifactShareLinkSummary>,
    principal_names: &BTreeMap<String, String>,
    access: &ArtifactAccessContext,
) -> anyhow::Result<ArtifactSummary> {
    let byte_length = browser_byte_length(byte_length)?;
    let effective_access = effective_artifact_access(&artifact, &grants, access)?;
    let recording = recording_artifact_summary(serde_json::Value::Object(
        artifact
            .metadata
            .as_map()
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    ));
    Ok(ArtifactSummary {
        id: ArtifactId::parse(record_key(&artifact.id)?)?,
        filename: artifact.filename.unwrap_or_else(|| "artifact".to_owned()),
        media_type: artifact.media_type,
        byte_length,
        owner: display_record(principal_names, &artifact.owner)?,
        output_owner: ArtifactOutputOwnerSummary(match artifact.owner_kind {
            veoveo_platform_store::ArtifactGrantSubjectKind::Principal => {
                AccessSubject::Principal(PrincipalId::parse(artifact.owner_key.clone())?)
            }
            veoveo_platform_store::ArtifactGrantSubjectKind::Group => {
                AccessSubject::Group(veoveo_types::GroupId::parse(artifact.owner_key.clone())?)
            }
        }),
        provenance: ArtifactGovernanceSummary {
            work_context: WorkContextId::parse(artifact.authority.context_key.clone())?,
            producer: PrincipalId::parse(artifact.producer_key.clone())?,
            invocation_mode: contract_invocation_mode(artifact.invocation_mode),
            initiator: artifact
                .initiator_key
                .clone()
                .map(PrincipalId::parse)
                .transpose()?,
            delegation_id: artifact
                .delegation_id
                .clone()
                .map(veoveo_types::DelegationId::parse)
                .transpose()?,
            policy_revision: veoveo_types::PolicyVersion::parse(artifact.policy_revision.clone())?,
        },
        effective_access,
        task_id: artifact
            .task
            .as_ref()
            .map(|id| Ok::<_, anyhow::Error>(veoveo_types::TaskId::parse(record_key(id)?)?))
            .transpose()?,
        classification: artifact.classification,
        labels: artifact
            .labels
            .into_iter()
            .map(DataLabelId::parse)
            .collect::<Result<_, _>>()?,
        release_state: contract_release_state(artifact.release_state),
        authorized_grants: grants.len(),
        active_links: share_links.iter().filter(|link| link.active).count(),
        grants,
        share_links,
        retention_expires_at: artifact.retention_expires_at,
        created_at: artifact.created_at,
        recording,
    })
}

fn browser_byte_length(value: Option<i64>) -> anyhow::Result<Option<ArtifactByteLength>> {
    value
        .map(|value| Ok(ArtifactByteLength::new(u64::try_from(value)?)?))
        .transpose()
}

const fn contract_release_state(
    state: veoveo_platform_store::ArtifactReleaseState,
) -> veoveo_artifact_contract::ArtifactReleaseState {
    match state {
        veoveo_platform_store::ArtifactReleaseState::Private => {
            veoveo_artifact_contract::ArtifactReleaseState::Private
        }
        veoveo_platform_store::ArtifactReleaseState::Released => {
            veoveo_artifact_contract::ArtifactReleaseState::Released
        }
        veoveo_platform_store::ArtifactReleaseState::Releasable => {
            veoveo_artifact_contract::ArtifactReleaseState::Releasable
        }
    }
}

const fn contract_invocation_mode(mode: veoveo_platform_store::InvocationMode) -> InvocationMode {
    match mode {
        veoveo_platform_store::InvocationMode::Direct => InvocationMode::Direct,
        veoveo_platform_store::InvocationMode::Delegated => InvocationMode::Delegated,
        veoveo_platform_store::InvocationMode::Automated => InvocationMode::Automated,
    }
}

fn effective_artifact_access(
    artifact: &ArtifactOccurrenceRecord,
    summaries: &[ArtifactGrantSummary],
    context: &ArtifactAccessContext,
) -> anyhow::Result<ArtifactEffectiveAccessSummary> {
    let artifact_id = ArtifactId::parse(record_key(&artifact.id)?)?;
    let now = Utc::now();
    let labels = artifact
        .labels
        .iter()
        .map(|label| DataLabelId::parse(label.clone()))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let grants = summaries
        .iter()
        .map(|grant| Grant {
            artifact: artifact_id,
            subject: grant.subject(),
            level: grant.permission(),
            tenant: context.tenant.clone(),
            data_labels: grant.labels().iter().cloned().collect(),
            retention_expires_at: grant.expires_at(),
        })
        .collect::<Vec<_>>();
    let context_membership = (artifact.authority.context_key == context.work_context.as_str())
        .then_some(context.membership);
    let decision = |requested| {
        veoveo_mcp_contract::decide(&AccessRequest {
            now,
            caller_id: &context.actor,
            caller_tenant: Some(&context.tenant),
            caller_labels: &context.clearance,
            memberships: &context.groups,
            resource_tenant: &context.tenant,
            resource_labels: &labels,
            grants: &grants,
            context_membership,
            requested,
        })
    };
    let read_decision = decision(AccessLevel::Read);
    let read = read_decision.is_allowed();
    let write = decision(AccessLevel::Write).is_allowed();
    let admin = decision(AccessLevel::Admin).is_allowed();
    let level = if admin {
        Some(AccessLevel::Admin)
    } else if write {
        Some(AccessLevel::Write)
    } else if read {
        Some(AccessLevel::Read)
    } else {
        None
    };
    let mut sources = summaries
        .iter()
        .filter(|grant| grant.expires_at().is_none_or(|expires| expires > now))
        .filter_map(|grant| match grant.subject() {
            AccessSubject::Principal(subject) if subject == context.actor => Some(
                ArtifactAccessSourceSummary(ArtifactAccessSource::PrincipalGrant {
                    subject,
                    level: grant.permission(),
                }),
            ),
            AccessSubject::Group(subject) => context
                .groups
                .iter()
                .find(|membership| membership.group == subject)
                .map(|membership| {
                    ArtifactAccessSourceSummary(ArtifactAccessSource::GroupGrant {
                        subject,
                        level: contract_group_role_level(membership.role).min(grant.permission()),
                    })
                }),
            AccessSubject::Principal(_) => None,
        })
        .collect::<Vec<_>>();
    if let Some(membership) = context_membership {
        sources.push(ArtifactAccessSourceSummary(
            ArtifactAccessSource::WorkContext {
                subject: context.work_context.clone(),
                level: membership.artifact_access(),
            },
        ));
    }
    // Preserve the existing lexical kind/subject presentation order.
    sources.sort_by_key(|source| match &source.0 {
        ArtifactAccessSource::PrincipalGrant { subject, .. } => {
            ("principal_grant", subject.to_string())
        }
        ArtifactAccessSource::GroupGrant { subject, .. } => ("group_grant", subject.to_string()),
        ArtifactAccessSource::WorkContext { subject, .. } => ("work_context", subject.to_string()),
    });
    Ok(ArtifactEffectiveAccessSummary {
        level,
        read,
        write,
        admin,
        clearance_satisfied: labels.is_subset(&context.clearance),
        requestable: read_decision == AccessDecision::DenyNeedToKnow,
        denial_reason: match read_decision {
            AccessDecision::Allow => None,
            AccessDecision::DenyTenant => Some(ArtifactAccessDenialReason::TenantBoundary),
            AccessDecision::DenyClearance => Some(ArtifactAccessDenialReason::Clearance),
            AccessDecision::DenyNeedToKnow => Some(ArtifactAccessDenialReason::NeedToKnow),
        },
        sources,
    })
}

const fn contract_group_role_level(role: GroupRole) -> AccessLevel {
    match role {
        GroupRole::Read => AccessLevel::Read,
        GroupRole::Write => AccessLevel::Write,
        GroupRole::Admin => AccessLevel::Admin,
    }
}

const fn contract_access_level(level: veoveo_platform_store::GrantPermission) -> AccessLevel {
    match level {
        veoveo_platform_store::GrantPermission::Read => AccessLevel::Read,
        veoveo_platform_store::GrantPermission::Write => AccessLevel::Write,
        veoveo_platform_store::GrantPermission::Admin => AccessLevel::Admin,
    }
}

pub(crate) fn agent_summary(
    agent: AgentRecord,
    pending_wakes: usize,
) -> anyhow::Result<AgentSummary> {
    let runner_lease_expires_at = agent.lease_owner.as_ref().and(agent.lease_expires_at);
    let detail = match agent.last_episode.as_ref() {
        Some(episode) => format!("Episode {}", record_key(episode)?),
        None => "No completed episode".to_owned(),
    };
    Ok(AgentSummary {
        id: agent_public_key(&agent).to_owned(),
        name: agent.display_name,
        profile: veoveo_types::GatewayProfileId::parse(record_key(&agent.profile)?)?,
        state: agent.state,
        runner_lease_expires_at,
        pending_wakes,
        last_episode_at: agent.last_episode.as_ref().map(|_| agent.updated_at),
        detail,
    })
}

pub(crate) fn agent_public_key(agent: &AgentRecord) -> &str {
    &agent.agent_key
}

const fn contract_recording_state(
    state: veoveo_recording_store::RecordingState,
) -> veoveo_recording_contract::RecordingState {
    match state {
        veoveo_recording_store::RecordingState::Live => {
            veoveo_recording_contract::RecordingState::Live
        }
        veoveo_recording_store::RecordingState::Ready => {
            veoveo_recording_contract::RecordingState::Ready
        }
        veoveo_recording_store::RecordingState::Sealing => {
            veoveo_recording_contract::RecordingState::Sealing
        }
        veoveo_recording_store::RecordingState::Sealed => {
            veoveo_recording_contract::RecordingState::Sealed
        }
        veoveo_recording_store::RecordingState::Interrupted => {
            veoveo_recording_contract::RecordingState::Interrupted
        }
        veoveo_recording_store::RecordingState::Failed => {
            veoveo_recording_contract::RecordingState::Failed
        }
    }
}

pub(crate) fn recording_summary(
    recording: RecordingRecord,
    layer_count: usize,
    committed_layer_count: usize,
    committed_byte_length: i64,
) -> anyhow::Result<RecordingSummary> {
    Ok(RecordingSummary {
        id: veoveo_recording_contract::RecordingId::parse(record_key(&recording.id)?)?,
        application: recording.application_id,
        recording_key: recording.recording_key,
        state: contract_recording_state(recording.state),
        layer_count,
        committed_layer_count,
        committed_byte_length,
        started_at: recording.started_at,
        last_data_at: recording.last_data_at,
        ended_at: recording.ended_at,
        sealed_at: recording.sealed_at,
    })
}

pub(crate) fn server_summary(
    server: &ServerManifest,
    control: &GatewayControlPlane,
    health: Option<&GatewayServerHealth>,
    now: DateTime<Utc>,
) -> ServerSummary {
    let resources = control
        .profiles
        .iter()
        .filter_map(|profile| {
            profile
                .servers
                .iter()
                .find(|item| item.server == server.slug)
        })
        .flat_map(|exposure| match &exposure.resources {
            Exposure::All => vec![format!("{}://**", server.uri_scheme)],
            Exposure::Listed(selectors) => selectors
                .iter()
                .map(resource_selector_label)
                .collect::<Vec<_>>(),
            Exposure::None => Vec::new(),
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    ServerSummary {
        id: server.slug.clone(),
        name: server.slug.to_string(),
        uri_scheme: server.uri_scheme.clone(),
        transport: server.upstream.transport,
        endpoint: server.upstream.url.clone(),
        state: health.map_or(GatewayServerHealthState::Offline, |health| health.state),
        checked_at: health.map_or(now, |health| health.checked_at),
        capabilities: ServerCapabilitiesSummary {
            tools: server.capabilities.tools,
            resources: server.capabilities.resources,
            resource_templates: server.capabilities.resource_templates,
            resource_subscriptions: server.capabilities.resource_subscriptions,
            prompts: server.capabilities.prompts,
            completions: server.capabilities.completions,
            tasks: server.capabilities.tasks,
            tools_list_changed: server.capabilities.tools_list_changed,
            prompts_list_changed: server.capabilities.prompts_list_changed,
            resources_list_changed: server.capabilities.resources_list_changed,
        },
        tools: server.tools.clone(),
        compatibility_helpers: server
            .compatibility_helpers
            .iter()
            .map(ToString::to_string)
            .collect(),
        resources,
        prompts: server.prompts.clone(),
        required_scopes: server.required_scopes.clone(),
        owned_routes: server
            .owned_routes
            .iter()
            .map(|route| ServerRouteSummary {
                path: route.path.to_string(),
                purpose: route.purpose,
            })
            .collect(),
        profiles: control
            .profiles
            .iter()
            .filter(|profile| {
                profile
                    .servers
                    .iter()
                    .any(|item| item.server == server.slug)
            })
            .map(|profile| profile.id.clone())
            .collect(),
    }
}

fn resource_selector_label(selector: &ResourceSelector) -> String {
    match selector {
        ResourceSelector::Scheme { scheme } => format!("{scheme}://**"),
        ResourceSelector::UriPrefix { prefix } => format!("{prefix}**"),
        ResourceSelector::Template { uri_template } => uri_template.to_string(),
    }
}

pub(crate) fn display_record(
    names: &BTreeMap<String, String>,
    record: &RecordId,
) -> Result<String, UnsupportedRecordKey> {
    let key = record_key(record)?;
    Ok(names.get(&key).cloned().unwrap_or(key))
}

#[derive(Debug)]
pub(crate) struct UnsupportedRecordKey {
    table: String,
    key_kind: &'static str,
}

impl std::fmt::Display for UnsupportedRecordKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "console projection does not support {} record keys on table {}",
            self.key_kind, self.table,
        )
    }
}

impl std::error::Error for UnsupportedRecordKey {}

pub(crate) fn record_key(record: &RecordId) -> Result<String, UnsupportedRecordKey> {
    match &record.key {
        RecordIdKey::String(value) => Ok(value.clone()),
        RecordIdKey::Uuid(value) => Ok(value.to_string()),
        RecordIdKey::Number(value) => Ok(value.to_string()),
        RecordIdKey::Array(_) => Err(unsupported_record_key(record, "array")),
        RecordIdKey::Object(_) => Err(unsupported_record_key(record, "object")),
        RecordIdKey::Range(_) => Err(unsupported_record_key(record, "range")),
    }
}

fn unsupported_record_key(record: &RecordId, key_kind: &'static str) -> UnsupportedRecordKey {
    UnsupportedRecordKey {
        table: record.table.as_str().to_owned(),
        key_kind,
    }
}

#[cfg(test)]
mod tests {
    use veoveo_agent_runtime::persistence::AgentState;
    use veoveo_platform_store::{
        ArtifactGrantSubjectKind, InvocationAuthorityRecord, InvocationMode as StoreInvocationMode,
        OpenObject, WorkContextMembershipLevel as StoreWorkContextMembershipLevel,
    };

    use super::*;

    #[test]
    fn recording_projection_requires_complete_current_owner_metadata() {
        use veoveo_recording_contract::{
            RecordingArtifactMetadata, RecordingArtifactProvenance, RecordingDatasetId,
            RecordingId, RecordingLayerId, RecordingLayerKind,
        };
        let recording_id = RecordingId::new();
        let layer_id = RecordingLayerId::new();
        let produced = serde_json::to_value(RecordingArtifactMetadata {
            provenance: RecordingArtifactProvenance::RecordingLayer {
                layer_kind: RecordingLayerKind::Capture,
                dataset_id: RecordingDatasetId::new(),
                recording_id,
                layer_id,
                sha256: veoveo_types::Sha256Digest::from_bytes([7; 32]),
            },
        })
        .unwrap();
        let projected = recording_artifact_summary(produced.clone()).unwrap();
        assert_eq!(projected.recording_id, recording_id);
        assert_eq!(projected.layer_id, Some(layer_id));
        assert_eq!(
            projected.kind,
            veoveo_recording_contract::RecordingArtifactProvenanceKind::RecordingLayer
        );
        assert!(projected.ordinal.is_none());
        for keep_current in [false, true] {
            for (current, retired) in [
                ("recordingId", "recording_id"),
                ("layerId", "layer_id"),
                ("datasetId", "dataset_id"),
                ("layerKind", "layer_kind"),
            ] {
                let mut bad = produced.clone();
                let fields = bad["provenance"].as_object_mut().unwrap();
                let value = fields.get(current).unwrap().clone();
                if !keep_current {
                    fields.remove(current);
                }
                fields.insert(retired.to_owned(), value);
                assert!(
                    recording_artifact_summary(bad).is_none(),
                    "retired {retired}"
                );
            }
        }
        let mut malformed = produced;
        malformed["provenance"]["sha256"] = "invalid".into();
        assert!(recording_artifact_summary(malformed).is_none());
        assert!(
            recording_artifact_summary(serde_json::json!({"provider": {"anything":true}}))
                .is_none()
        );
    }

    #[test]
    fn upload_events_require_the_exact_actor_tenant_and_context() {
        let access = ArtifactAccessContext {
            actor: PrincipalId::parse("alice").unwrap(),
            tenant: TenantId::parse("example").unwrap(),
            work_context: WorkContextId::parse("operations").unwrap(),
            clearance: BTreeSet::new(),
            groups: BTreeSet::new(),
            membership: WorkContextMembershipLevel::Owner,
        };
        assert!(access.matches_upload_scope("example", "alice", "operations"));
        assert!(!access.matches_upload_scope("foreign", "alice", "operations"));
        assert!(!access.matches_upload_scope("example", "bob", "operations"));
        assert!(!access.matches_upload_scope("example", "alice", "other"));
    }

    #[tokio::test]
    #[ignore = "requires VEOVEO_SURREAL_BINARY pointing to the pinned native server"]
    async fn referenced_blob_reads_preserve_exact_sizes_and_tenant_scope() {
        use std::{
            net::TcpListener,
            process::{Child, Command, Stdio},
            time::Duration,
        };
        use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials};
        struct Server(Child);
        impl Drop for Server {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let binary = std::env::var("VEOVEO_SURREAL_BINARY").unwrap();
        let version = Command::new(&binary).arg("version").output().unwrap();
        assert!(
            version.status.success()
                && String::from_utf8_lossy(&version.stdout).starts_with("3.3.0")
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let password = uuid::Uuid::now_v7().to_string();
        let mut server = Server(
            Command::new(binary)
                .args([
                    "start",
                    "--no-banner",
                    "--bind",
                    &address.to_string(),
                    "memory",
                ])
                .env("SURREAL_USER", "fixture")
                .env("SURREAL_PASS", &password)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let client = reqwest::Client::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while !client
            .get(format!("http://{address}/ready"))
            .timeout(Duration::from_secs(1))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            assert!(
                server.0.try_wait().unwrap().is_none() && tokio::time::Instant::now() < deadline
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let config = StoreConfig::builder(
            format!("ws://{address}"),
            "projection_fixture",
            "fixture",
            StoreCredentials::root("fixture", password),
        )
        .build()
        .unwrap();
        let store = PlatformStore::connect(config).await.unwrap();
        let tenant = RecordId::new("tenant", "selected");
        let requested = RecordId::new("artifact_blob", "old_blob");
        let foreign = RecordId::new("artifact_blob", "foreign");
        let rows = [
            (requested.clone(), tenant.clone(), 14_288_899_i64),
            (
                foreign.clone(),
                RecordId::new("tenant", "other"),
                10_737_418_240_i64,
            ),
            (
                RecordId::new("artifact_blob", "unselected"),
                tenant.clone(),
                0_i64,
            ),
        ]
        .map(|(id, tenant, byte_len)| ArtifactBlobRecord {
            id,
            tenant,
            byte_len,
            sha256: "a".repeat(64),
            object_key: "fixture".into(),
            content_type: "application/octet-stream".into(),
            encryption: OpenObject::default(),
            created_at: Utc::now(),
        });
        store
            .client()
            .query(include_str!(
                "../../../../queries/bin/gateway/admin/console/projection/drop/statement_1.surql"
            ))
            .bind(("rows", rows.to_vec()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let missing = RecordId::new("artifact_blob", "missing");
        let blobs =
            load_referenced_blobs(&store, &tenant, vec![requested.clone(), foreign, missing])
                .await
                .unwrap();
        assert_eq!(blobs.len(), 1);
        assert_eq!(blobs[0].id, requested);
        assert_eq!(blobs[0].byte_len, 14_288_899);
        assert!(
            load_referenced_blobs(&store, &tenant, Vec::new())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn artifact_sizes_distinguish_missing_zero_and_large_exact_values() {
        assert_eq!(browser_byte_length(None).unwrap(), None);
        assert_eq!(
            browser_byte_length(Some(0))
                .unwrap()
                .map(ArtifactByteLength::get),
            Some(0)
        );
        assert_eq!(
            browser_byte_length(Some(14_288_899))
                .unwrap()
                .map(ArtifactByteLength::get),
            Some(14_288_899)
        );
        assert_eq!(
            browser_byte_length(Some(10_737_418_240))
                .unwrap()
                .map(ArtifactByteLength::get),
            Some(10_737_418_240)
        );
        assert!(browser_byte_length(Some(-1)).is_err());
        assert!(browser_byte_length(Some(1_i64 << 53)).is_err());
    }

    #[test]
    fn share_link_snapshot_never_serializes_bearer_hash_material() {
        let now = Utc::now();
        let value = serde_json::to_value(ArtifactShareLinkSummary {
            id: veoveo_artifact_contract::ArtifactShareLinkId::parse(
                "0197f78e-f2f0-7a6e-8a5d-f41c691e4471",
            )
            .unwrap(),
            permission: AccessLevel::Read,
            expires_at: now,
            max_downloads: Some(3),
            download_count: 1,
            revoked_at: None,
            created_at: now,
            active: true,
        })
        .expect("share-link summary serializes");
        let encoded = serde_json::to_string(&value).expect("JSON serializes");

        assert!(!encoded.contains("token_hash"));
        assert!(!encoded.contains("tokenHash"));
        assert!(!encoded.contains("url"));
        assert_eq!(value["active"], true);
    }

    #[test]
    fn agent_snapshot_uses_the_symbolic_control_key_and_runner_lease() {
        let now = Utc::now();
        let lease_expires_at = now + chrono::Duration::seconds(30);
        let record_id = "019fd9bc-e7d1-7fff-bfff-ffffffffffff";
        let agent = AgentRecord {
            id: RecordId::new("agent", record_id),
            tenant: RecordId::new("tenant", "example"),
            agent_key: "mission-supervisor".to_owned(),
            display_name: "Mission Supervisor".to_owned(),
            profile: RecordId::new("profile", "operator"),
            work_context: RecordId::new("work_context", "operations"),
            policy_revision: "r1".to_owned(),
            authority: InvocationAuthorityRecord {
                context_key: "operations".to_owned(),
                membership: StoreWorkContextMembershipLevel::Contributor,
                policy_revision: "r1".to_owned(),
                owner_kind: ArtifactGrantSubjectKind::Principal,
                owner_key: "mission-supervisor".to_owned(),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: Vec::new(),
                invocation_mode: StoreInvocationMode::Automated,
                initiator_key: None,
                delegation_id: None,
            },
            state: AgentState::Running,
            manifest: OpenObject::default(),
            memory_database: "supervisor.duckdb".to_owned(),
            last_episode: None,
            managed_ready: None,
            next_episode_sequence: 1,
            lease_owner: Some("runner-instance".to_owned()),
            lease_expires_at: Some(lease_expires_at),
            heartbeat_at: Some(now),
            fence: 1,
            revision: 1,
            created_at: now,
            updated_at: now,
        };

        let value = serde_json::to_value(agent_summary(agent, 2).unwrap()).unwrap();

        assert_eq!(value["id"], "mission-supervisor");
        assert_eq!(
            value["runnerLeaseExpiresAt"],
            serde_json::json!(lease_expires_at)
        );
        assert_eq!(value["pendingWakes"], 2);
        assert!(!value.to_string().contains(record_id));
    }
}
