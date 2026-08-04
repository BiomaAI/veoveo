use std::collections::BTreeMap;
use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use surrealdb::types::RecordId;

use crate::{
    AuditEventRecord, OpenObject, OutboxDraft, PlatformStore, SimulationViewStateRecord,
    StoreError, deterministic_tenant_id, store::primary_transaction_error,
};

pub const SIMULATION_VIEW_DESIRED_DIGEST_SCHEMA: &str =
    "veoveo.io/simulation-view-desired-digest/v2";
const PREVIOUS_SIMULATION_VIEW_DESIRED_DIGEST_SCHEMAS: [&str; 2] = [
    "veoveo.io/simulation-view-desired-digest/legacy-v1",
    "veoveo.io/simulation-view-desired-digest/v1",
];

#[derive(Clone, Debug, PartialEq)]
pub struct SimulationViewStateDraft {
    pub tenant_key: String,
    pub owner_key: String,
    pub work_context_key: String,
    pub policy_revision: String,
    pub session_id: String,
    pub epoch_id: String,
    pub desired_revision: u64,
    pub realized_revision: u64,
    pub authorization_revision: u64,
    pub revoked: bool,
    pub authorization_expires_at: Option<DateTime<Utc>>,
    pub desired_digest: String,
    pub desired_digest_schema: String,
    pub snapshot: OpenObject,
    pub reconciliation: OpenObject,
    pub updated_at: DateTime<Utc>,
}

pub fn simulation_view_state_record_id(
    tenant_key: &str,
    work_context_key: &str,
    session_id: &str,
) -> RecordId {
    let digest =
        Sha256::digest(format!("{tenant_key}\0{work_context_key}\0{session_id}").as_bytes());
    let mut key = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut key, "{byte:02x}").expect("writing to String cannot fail");
    }
    RecordId::new("simulation_view_state", key)
}

impl PlatformStore {
    pub async fn commit_simulation_view_state(
        &self,
        draft: SimulationViewStateDraft,
    ) -> Result<SimulationViewStateRecord, StoreError> {
        if draft.desired_revision == 0
            || draft.realized_revision > draft.desired_revision
            || draft.desired_digest.len() != 64
            || draft.desired_digest_schema != SIMULATION_VIEW_DESIRED_DIGEST_SCHEMA
        {
            return Err(StoreError::SimulationViewRevisionConflict {
                revision: draft.desired_revision,
            });
        }
        let record = simulation_view_state_record_id(
            &draft.tenant_key,
            &draft.work_context_key,
            &draft.session_id,
        );
        let current = self.simulation_view_state(record.clone()).await?;
        let created_at = current
            .as_ref()
            .map_or(draft.updated_at, |value| value.created_at);
        if let Some(current) = &current {
            let current_revision = u64::try_from(current.desired_revision).unwrap_or_default();
            let upgrades_legacy_digest = PREVIOUS_SIMULATION_VIEW_DESIRED_DIGEST_SCHEMAS
                .contains(&current.desired_digest_schema.as_str())
                && draft.desired_digest_schema == SIMULATION_VIEW_DESIRED_DIGEST_SCHEMA;
            if draft.desired_revision == current_revision {
                if !upgrades_legacy_digest
                    && (draft.desired_digest_schema != current.desired_digest_schema
                        || draft.desired_digest != current.desired_digest)
                {
                    return Err(StoreError::SimulationViewRevisionConflict {
                        revision: draft.desired_revision,
                    });
                }
            } else if draft.desired_revision != current_revision.saturating_add(1) {
                return Err(StoreError::SimulationViewRevisionGap {
                    expected: current_revision.saturating_add(1),
                    actual: draft.desired_revision,
                });
            }
        } else if draft.desired_revision != 1 {
            return Err(StoreError::SimulationViewRevisionGap {
                expected: 1,
                actual: draft.desired_revision,
            });
        }
        let outbox = OutboxDraft::now(
            Some(deterministic_tenant_id(&draft.tenant_key)?.record_id()),
            "simulation_view",
            draft.session_id.clone(),
            "simulation_view.state_committed",
            1,
            OpenObject::new(BTreeMap::from([
                (
                    "sessionId".into(),
                    serde_json::json!(draft.session_id.clone()),
                ),
                (
                    "desiredRevision".into(),
                    serde_json::json!(draft.desired_revision),
                ),
                (
                    "realizedRevision".into(),
                    serde_json::json!(draft.realized_revision),
                ),
            ])),
        );
        let content = SimulationViewStateRecord {
            id: record.clone(),
            tenant_key: draft.tenant_key,
            owner_key: draft.owner_key,
            work_context_key: draft.work_context_key,
            policy_revision: draft.policy_revision,
            session_id: draft.session_id,
            epoch_id: draft.epoch_id,
            desired_revision: i64::try_from(draft.desired_revision).map_err(|_| {
                StoreError::SimulationViewRevisionConflict {
                    revision: draft.desired_revision,
                }
            })?,
            realized_revision: i64::try_from(draft.realized_revision).map_err(|_| {
                StoreError::SimulationViewRevisionConflict {
                    revision: draft.desired_revision,
                }
            })?,
            authorization_revision: i64::try_from(draft.authorization_revision).map_err(|_| {
                StoreError::SimulationViewRevisionConflict {
                    revision: draft.desired_revision,
                }
            })?,
            revoked: draft.revoked,
            authorization_expires_at: draft.authorization_expires_at,
            desired_digest: draft.desired_digest,
            desired_digest_schema: draft.desired_digest_schema,
            snapshot: draft.snapshot,
            reconciliation: draft.reconciliation,
            created_at,
            updated_at: draft.updated_at,
        };
        let mut response = if let Some(current) = current {
            self.db
                .query("BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $record CONTENT $content WHERE desired_revision = $expected_revision AND desired_digest = $expected_digest AND desired_digest_schema = $expected_digest_schema RETURN AFTER); IF $updated = NONE { THROW 'simulation_view_revision_conflict'; }; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
                .bind(("record", record.clone()))
                .bind(("content", content))
                .bind(("expected_revision", current.desired_revision))
                .bind(("expected_digest", current.desired_digest))
                .bind(("expected_digest_schema", current.desired_digest_schema))
                .bind(("outbox", outbox))
                .await?
        } else {
            self.db
                .query("BEGIN TRANSACTION; CREATE ONLY $record CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
                .bind(("record", record.clone()))
                .bind(("content", content))
                .bind(("outbox", outbox))
                .await?
        };
        if let Some(error) = primary_transaction_error(response.take_errors()) {
            if error
                .to_string()
                .contains("simulation_view_revision_conflict")
            {
                return Err(StoreError::SimulationViewRevisionConflict {
                    revision: draft.desired_revision,
                });
            }
            return Err(error.into());
        }
        self.simulation_view_state(record)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "simulation view state commit readback",
            })
    }

    pub async fn simulation_view_state(
        &self,
        record: RecordId,
    ) -> Result<Option<SimulationViewStateRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $record;")
            .bind(("record", record))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn simulation_view_states(
        &self,
    ) -> Result<Vec<SimulationViewStateRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM simulation_view_state ORDER BY tenant_key, work_context_key, session_id;")
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn append_simulation_view_audit(
        &self,
        record: AuditEventRecord,
    ) -> Result<(), StoreError> {
        let outbox = OutboxDraft::now(
            record.tenant.clone(),
            "simulation_view",
            record.resource_id.clone().unwrap_or_default(),
            format!("simulation_view.{}", record.action),
            1,
            OpenObject::new(BTreeMap::from([
                ("action".into(), serde_json::json!(&record.action)),
                ("outcome".into(), serde_json::json!(&record.outcome)),
                ("occurred_at".into(), serde_json::json!(record.occurred_at)),
            ])),
        );
        self.db
            .query("BEGIN TRANSACTION; CREATE ONLY $record CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("record", record.id.clone()))
            .bind(("content", record))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        Ok(())
    }
}
