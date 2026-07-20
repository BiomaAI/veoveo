use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue, Uuid as SurrealUuid};
use uuid::Uuid;

use crate::identity::PLATFORM_ID_NAMESPACE;
use crate::{
    ArtifactAccessRequestId, ArtifactAccessRequestRecord, ArtifactAccessRequestState,
    ArtifactGrantEdge, ArtifactGrantSubjectKind, ArtifactId, GrantPermission, OpenObject,
    OutboxDraft, PlatformIdentity, PlatformStore, PrincipalId, StoreError, TenantId, WorkContextId,
};

const MAX_ACCESS_REQUEST_LIMIT: u32 = 500;
const MAX_JUSTIFICATION_BYTES: usize = 4_096;
const MAX_DECISION_NOTE_BYTES: usize = 4_096;

#[derive(Clone, Debug)]
pub struct ArtifactAccessRequestDraft {
    pub request_id: ArtifactAccessRequestId,
    pub identity: PlatformIdentity,
    pub artifact_id: ArtifactId,
    pub requested_level: GrantPermission,
    pub justification: String,
}

#[derive(Clone, Debug)]
pub struct ArtifactAccessRequestDecisionDraft {
    pub identity: PlatformIdentity,
    pub request_id: ArtifactAccessRequestId,
    pub state: ArtifactAccessRequestState,
    pub note: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ArtifactAccessRequestQuery {
    pub tenant_id: TenantId,
    pub requester_id: Option<PrincipalId>,
    pub work_context_id: Option<WorkContextId>,
    pub state: Option<ArtifactAccessRequestState>,
    pub cursor: Option<ArtifactAccessRequestId>,
    pub limit: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct ArtifactAccessRequestContent {
    tenant: RecordId,
    artifact: RecordId,
    work_context: RecordId,
    work_context_key: String,
    requester: RecordId,
    requester_key: String,
    requested_level: GrantPermission,
    justification: String,
    state: ArtifactAccessRequestState,
    decided_by: Option<RecordId>,
    decided_by_key: Option<String>,
    decision_note: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    decided_at: Option<DateTime<Utc>>,
    revision: i64,
}

impl PlatformStore {
    pub async fn create_or_reopen_artifact_access_request(
        &self,
        draft: ArtifactAccessRequestDraft,
    ) -> Result<ArtifactAccessRequestRecord, StoreError> {
        validate_text(
            "justification",
            &draft.justification,
            MAX_JUSTIFICATION_BYTES,
        )?;
        let artifact = self
            .artifact_aggregate(draft.artifact_id)
            .await?
            .map(|aggregate| aggregate.occurrence)
            .filter(|artifact| artifact.tenant == draft.identity.tenant_id.record_id())
            .ok_or(StoreError::MissingRecord {
                operation: "artifact access request artifact lookup",
            })?;

        for _ in 0..3 {
            if let Some(existing) = self
                .artifact_access_request_for_subject(
                    draft.identity.tenant_id,
                    draft.artifact_id,
                    draft.identity.principal_id,
                )
                .await?
            {
                if existing.state == ArtifactAccessRequestState::Pending {
                    return Ok(existing);
                }
                let now = Utc::now();
                let event = access_request_event(
                    &draft.identity,
                    draft.request_id,
                    draft.artifact_id,
                    "artifact.access_requested",
                    ArtifactAccessRequestState::Pending,
                );
                self.db
                    .query(
                        "BEGIN TRANSACTION; \
                         LET $current = (SELECT * FROM ONLY $request); \
                         IF $current.revision != $revision { THROW 'artifact_access_request_revision_conflict'; }; \
                         UPDATE ONLY $request SET requested_level = $requested_level, \
                           justification = $justification, state = 'pending', decided_by = NONE, \
                           decided_by_key = NONE, decision_note = NONE, decided_at = NONE, created_at = $now, \
                           updated_at = $now, revision += 1 RETURN NONE; \
                         CREATE outbox_event CONTENT $event RETURN NONE; \
                         COMMIT TRANSACTION;",
                    )
                    .bind(("request", existing.id.clone()))
                    .bind(("revision", existing.revision))
                    .bind(("requested_level", draft.requested_level))
                    .bind(("justification", draft.justification.clone()))
                    .bind(("now", now))
                    .bind(("event", event))
                    .await?
                    .check()?;
                return self
                    .artifact_access_request(
                        draft.identity.tenant_id,
                        record_request_id(&existing.id)?,
                    )
                    .await?
                    .ok_or(StoreError::MissingRecord {
                        operation: "artifact access request reopen readback",
                    });
            }

            let now = Utc::now();
            let content = ArtifactAccessRequestContent {
                tenant: draft.identity.tenant_id.record_id(),
                artifact: draft.artifact_id.record_id(),
                work_context: artifact.work_context.clone(),
                work_context_key: artifact.authority.context_key.clone(),
                requester: draft.identity.principal_id.record_id(),
                requester_key: draft.identity.principal_key.clone(),
                requested_level: draft.requested_level,
                justification: draft.justification.clone(),
                state: ArtifactAccessRequestState::Pending,
                decided_by: None,
                decided_by_key: None,
                decision_note: None,
                created_at: now,
                updated_at: now,
                decided_at: None,
                revision: 0,
            };
            let event = access_request_event(
                &draft.identity,
                draft.request_id,
                draft.artifact_id,
                "artifact.access_requested",
                ArtifactAccessRequestState::Pending,
            );
            let created = self
                .db
                .query(
                    "BEGIN TRANSACTION; \
                     CREATE ONLY $request CONTENT $content RETURN NONE; \
                     CREATE outbox_event CONTENT $event RETURN NONE; \
                     COMMIT TRANSACTION;",
                )
                .bind(("request", draft.request_id.record_id()))
                .bind(("content", content))
                .bind(("event", event))
                .await
                .and_then(|response| response.check());
            if created.is_ok() {
                return self
                    .artifact_access_request(draft.identity.tenant_id, draft.request_id)
                    .await?
                    .ok_or(StoreError::MissingRecord {
                        operation: "artifact access request creation readback",
                    });
            }
        }

        Err(StoreError::ArtifactAccessRequestConflict(
            draft.artifact_id.to_string(),
        ))
    }

    pub async fn artifact_access_request(
        &self,
        tenant_id: TenantId,
        request_id: ArtifactAccessRequestId,
    ) -> Result<Option<ArtifactAccessRequestRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $request WHERE tenant = $tenant;")
            .bind(("request", request_id.record_id()))
            .bind(("tenant", tenant_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn list_artifact_access_requests(
        &self,
        query: ArtifactAccessRequestQuery,
    ) -> Result<Vec<ArtifactAccessRequestRecord>, StoreError> {
        if query.limit == 0 || query.limit > MAX_ACCESS_REQUEST_LIMIT {
            return Err(StoreError::InvalidArtifactAccessRequest {
                field: "limit",
                reason: "must be in 1..=500",
            });
        }
        if query.requester_id.is_some() == query.work_context_id.is_some() {
            return Err(StoreError::InvalidArtifactAccessRequest {
                field: "scope",
                reason: "must select one requester or Work Context",
            });
        }
        let scope = if query.requester_id.is_some() {
            "requester = $scope"
        } else {
            "work_context = $scope"
        };
        let state = query
            .state
            .map(|_| " AND state = $state")
            .unwrap_or_default();
        let cursor = query
            .cursor
            .map(|_| " AND id < $cursor")
            .unwrap_or_default();
        let statement = format!(
            "SELECT * FROM artifact_access_request WHERE tenant = $tenant AND {scope}{state}{cursor} \
             ORDER BY id DESC LIMIT $limit;"
        );
        let scope_record = query
            .requester_id
            .map(PrincipalId::record_id)
            .or_else(|| query.work_context_id.map(WorkContextId::record_id))
            .expect("validated access request scope");
        let mut request = self
            .db
            .query(statement)
            .bind(("tenant", query.tenant_id.record_id()))
            .bind(("scope", scope_record))
            .bind(("limit", i64::from(query.limit)));
        if let Some(state) = query.state {
            request = request.bind(("state", state));
        }
        if let Some(cursor) = query.cursor {
            request = request.bind(("cursor", cursor.record_id()));
        }
        let mut response = request.await?.check()?;
        Ok(response.take(0)?)
    }

    pub async fn decide_artifact_access_request(
        &self,
        draft: ArtifactAccessRequestDecisionDraft,
    ) -> Result<ArtifactAccessRequestRecord, StoreError> {
        if draft.state == ArtifactAccessRequestState::Pending {
            return Err(StoreError::InvalidArtifactAccessRequest {
                field: "state",
                reason: "must be approved, denied, or cancelled",
            });
        }
        if let Some(note) = &draft.note {
            validate_optional_text("decision_note", note, MAX_DECISION_NOTE_BYTES)?;
        }
        let existing = self
            .artifact_access_request(draft.identity.tenant_id, draft.request_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "artifact access request decision lookup",
            })?;
        if existing.state != ArtifactAccessRequestState::Pending {
            return Err(StoreError::ArtifactAccessRequestConflict(
                draft.request_id.to_string(),
            ));
        }
        let now = Utc::now();
        let event_type = match draft.state {
            ArtifactAccessRequestState::Approved => "artifact.access_request_approved",
            ArtifactAccessRequestState::Denied => "artifact.access_request_denied",
            ArtifactAccessRequestState::Cancelled => "artifact.access_request_cancelled",
            ArtifactAccessRequestState::Pending => unreachable!(),
        };
        let request_event = access_request_event(
            &draft.identity,
            draft.request_id,
            record_artifact_id(&existing.artifact)?,
            event_type,
            draft.state,
        );
        let mut query = if draft.state == ArtifactAccessRequestState::Approved {
            let grant_id = deterministic_grant_id(
                record_artifact_id(&existing.artifact)?,
                ArtifactGrantSubjectKind::Principal,
                &existing.requester_key,
            );
            let grant = ArtifactGrantEdge {
                id: grant_id.clone(),
                r#in: existing.artifact.clone(),
                out: existing.requester.clone(),
                subject_kind: ArtifactGrantSubjectKind::Principal,
                subject_key: existing.requester_key.clone(),
                permission: existing.requested_level,
                labels: Vec::new(),
                expires_at: None,
                created_by: draft.identity.principal_id.record_id(),
                created_at: now,
            };
            let grant_event = OutboxDraft::now(
                Some(draft.identity.tenant_id.record_id()),
                "artifact",
                record_artifact_id(&existing.artifact)?.to_string(),
                "artifact.grant.updated",
                1,
                OpenObject::new(BTreeMap::from([
                    (
                        "subject_key".to_owned(),
                        serde_json::json!(existing.requester_key),
                    ),
                    (
                        "permission".to_owned(),
                        serde_json::to_value(existing.requested_level).unwrap_or_default(),
                    ),
                    (
                        "access_request_id".to_owned(),
                        serde_json::json!(draft.request_id.to_string()),
                    ),
                ])),
            );
            self.db
                .query(
                    "BEGIN TRANSACTION; \
                     LET $current = (SELECT * FROM ONLY $request); \
                     IF $current.revision != $revision OR $current.state != 'pending' { THROW 'artifact_access_request_state_conflict'; }; \
                     DELETE $grant_id RETURN NONE; \
                     RELATE ONLY $artifact->$grant_id->$requester CONTENT $grant RETURN NONE; \
                     UPDATE ONLY $request SET state = $state, decided_by = $decided_by, \
                       decided_by_key = $decided_by_key, \
                       decision_note = $note, decided_at = $now, updated_at = $now, revision += 1 RETURN NONE; \
                     CREATE outbox_event CONTENT $request_event RETURN NONE; \
                     CREATE outbox_event CONTENT $grant_event RETURN NONE; \
                     COMMIT TRANSACTION;",
                )
                .bind(("grant_id", grant_id))
                .bind(("artifact", existing.artifact.clone()))
                .bind(("requester", existing.requester.clone()))
                .bind(("grant", grant))
                .bind(("grant_event", grant_event))
        } else {
            self.db.query(
                "BEGIN TRANSACTION; \
                 LET $current = (SELECT * FROM ONLY $request); \
                 IF $current.revision != $revision OR $current.state != 'pending' { THROW 'artifact_access_request_state_conflict'; }; \
                 UPDATE ONLY $request SET state = $state, decided_by = $decided_by, \
                   decided_by_key = $decided_by_key, \
                   decision_note = $note, decided_at = $now, updated_at = $now, revision += 1 RETURN NONE; \
                 CREATE outbox_event CONTENT $request_event RETURN NONE; \
                 COMMIT TRANSACTION;",
            )
        };
        query = query
            .bind(("request", draft.request_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("state", draft.state))
            .bind(("decided_by", draft.identity.principal_id.record_id()))
            .bind(("decided_by_key", draft.identity.principal_key.clone()))
            .bind(("note", draft.note))
            .bind(("now", now))
            .bind(("request_event", request_event));
        query.await?.check()?;
        self.artifact_access_request(draft.identity.tenant_id, draft.request_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "artifact access request decision readback",
            })
    }

    async fn artifact_access_request_for_subject(
        &self,
        tenant_id: TenantId,
        artifact_id: ArtifactId,
        requester_id: PrincipalId,
    ) -> Result<Option<ArtifactAccessRequestRecord>, StoreError> {
        let mut response = self
            .db
            .query(
                "SELECT * FROM artifact_access_request \
                 WHERE tenant = $tenant AND artifact = $artifact AND requester = $requester LIMIT 1;",
            )
            .bind(("tenant", tenant_id.record_id()))
            .bind(("artifact", artifact_id.record_id()))
            .bind(("requester", requester_id.record_id()))
            .await?
            .check()?;
        let records: Vec<ArtifactAccessRequestRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }
}

fn access_request_event(
    identity: &PlatformIdentity,
    request_id: ArtifactAccessRequestId,
    artifact_id: ArtifactId,
    event_type: &str,
    state: ArtifactAccessRequestState,
) -> OutboxDraft {
    OutboxDraft::now(
        Some(identity.tenant_id.record_id()),
        "artifact_access_request",
        request_id.to_string(),
        event_type,
        1,
        OpenObject::new(BTreeMap::from([
            (
                "artifact_id".to_owned(),
                serde_json::json!(artifact_id.to_string()),
            ),
            ("state".to_owned(), serde_json::json!(state)),
        ])),
    )
}

fn deterministic_grant_id(
    artifact_id: ArtifactId,
    subject_kind: ArtifactGrantSubjectKind,
    subject_key: &str,
) -> RecordId {
    let id = Uuid::new_v5(
        &PLATFORM_ID_NAMESPACE,
        format!("artifact-grant:{artifact_id}:{subject_kind:?}:{subject_key}").as_bytes(),
    );
    RecordId::new("artifact_grant", SurrealUuid::from(id))
}

fn record_artifact_id(record: &RecordId) -> Result<ArtifactId, StoreError> {
    record_uuid(record, ArtifactId::TABLE).map(ArtifactId::from_uuid)
}

fn record_request_id(record: &RecordId) -> Result<ArtifactAccessRequestId, StoreError> {
    record_uuid(record, ArtifactAccessRequestId::TABLE).map(ArtifactAccessRequestId::from_uuid)
}

fn record_uuid(record: &RecordId, table: &'static str) -> Result<Uuid, StoreError> {
    if record.table.as_str() != table {
        return Err(StoreError::MissingRecord {
            operation: "artifact access request typed identity",
        });
    }
    match &record.key {
        surrealdb::types::RecordIdKey::Uuid(value) => Ok(value.into_inner()),
        _ => Err(StoreError::MissingRecord {
            operation: "artifact access request UUID identity",
        }),
    }
}

fn validate_text(field: &'static str, value: &str, maximum: usize) -> Result<(), StoreError> {
    if value.trim().is_empty()
        || value.trim() != value
        || value.len() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(StoreError::InvalidArtifactAccessRequest {
            field,
            reason: "must be trimmed, non-empty, bounded text",
        });
    }
    Ok(())
}

fn validate_optional_text(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), StoreError> {
    if value.trim() != value || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(StoreError::InvalidArtifactAccessRequest {
            field,
            reason: "must be trimmed, bounded text",
        });
    }
    Ok(())
}
