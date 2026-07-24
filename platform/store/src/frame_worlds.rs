use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    FrameWorldRecord, FrameWorldRecordId, FrameWorldRevisionRecord, FrameWorldRevisionRecordId,
    OpenObject, OutboxDraft, PlatformIdentity, PlatformStore, StoreError, TenantId,
};

const FRAME_WORLD_EVENT_SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Debug)]
pub struct FrameWorldDraft {
    pub identity: PlatformIdentity,
    pub world_key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub classification: String,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct FrameWorldRevisionDraft {
    pub identity: PlatformIdentity,
    pub world_key: String,
    pub expected_head_revision_key: Option<String>,
    pub revision_key: String,
    pub spec_sha256: String,
    pub root_frame_key: String,
    pub definition: OpenObject,
}

#[derive(Clone, Debug)]
pub struct FrameWorldPublication {
    pub world: FrameWorldRecord,
    pub revision: FrameWorldRevisionRecord,
    pub created: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct FrameWorldContent {
    tenant: RecordId,
    owner: RecordId,
    world_key: String,
    display_name: String,
    description: Option<String>,
    head_revision: Option<RecordId>,
    head_revision_key: Option<String>,
    revision: i64,
    classification: String,
    labels: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct FrameWorldRevisionContent {
    tenant: RecordId,
    owner: RecordId,
    world: RecordId,
    world_key: String,
    revision_key: String,
    revision: i64,
    spec_sha256: String,
    root_frame_key: String,
    definition: OpenObject,
    created_at: DateTime<Utc>,
}

impl PlatformStore {
    pub async fn create_frame_world(
        &self,
        mut draft: FrameWorldDraft,
    ) -> Result<FrameWorldRecord, StoreError> {
        validate_text("world_key", &draft.world_key, 128)?;
        validate_text("display_name", &draft.display_name, 512)?;
        validate_optional_text("description", draft.description.as_deref(), 2_048)?;
        validate_text("classification", &draft.classification, 256)?;
        normalize_labels(&mut draft.labels)?;
        if self
            .frame_world_by_key(draft.identity.tenant_id, &draft.world_key)
            .await?
            .is_some()
        {
            return Err(StoreError::FrameWorldConflict(draft.world_key));
        }

        let world_id = FrameWorldRecordId::new();
        let now = Utc::now();
        let content = FrameWorldContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            world_key: draft.world_key.clone(),
            display_name: draft.display_name,
            description: draft.description,
            head_revision: None,
            head_revision_key: None,
            revision: 0,
            classification: draft.classification,
            labels: draft.labels,
            created_at: now,
            updated_at: now,
        };
        let outbox = frame_world_event(
            &draft.identity,
            "frame_world",
            &draft.world_key,
            "frame.world.created",
        );
        let result = self
            .client()
            .query("BEGIN TRANSACTION; CREATE ONLY $world CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("world", world_id.record_id()))
            .bind(("content", content))
            .bind(("outbox", outbox))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if self
                .frame_world_by_key(draft.identity.tenant_id, &draft.world_key)
                .await?
                .is_some()
            {
                return Err(StoreError::FrameWorldConflict(draft.world_key));
            }
            return Err(error.into());
        }
        self.frame_world_by_key(draft.identity.tenant_id, &draft.world_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "frame world creation readback",
            })
    }

    pub async fn frame_world_by_key(
        &self,
        tenant_id: TenantId,
        world_key: &str,
    ) -> Result<Option<FrameWorldRecord>, StoreError> {
        validate_text("world_key", world_key, 128)?;
        let mut response = self
            .client()
            .query(
                "SELECT * FROM frame_world WHERE tenant = $tenant AND world_key = $world_key LIMIT 1;",
            )
            .bind(("tenant", tenant_id.record_id()))
            .bind(("world_key", world_key.to_owned()))
            .await?
            .check()?;
        let records: Vec<FrameWorldRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }

    pub async fn list_frame_worlds(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<FrameWorldRecord>, StoreError> {
        let mut response = self
            .client()
            .query("SELECT * FROM frame_world WHERE tenant = $tenant ORDER BY world_key ASC;")
            .bind(("tenant", tenant_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn frame_world_revision_by_key(
        &self,
        tenant_id: TenantId,
        world_key: &str,
        revision_key: &str,
    ) -> Result<Option<FrameWorldRevisionRecord>, StoreError> {
        validate_text("world_key", world_key, 128)?;
        validate_text("revision_key", revision_key, 128)?;
        let mut response = self
            .client()
            .query("SELECT * FROM frame_world_revision WHERE tenant = $tenant AND world_key = $world_key AND revision_key = $revision_key LIMIT 1;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("world_key", world_key.to_owned()))
            .bind(("revision_key", revision_key.to_owned()))
            .await?
            .check()?;
        let records: Vec<FrameWorldRevisionRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }

    pub async fn frame_world_head_revision(
        &self,
        tenant_id: TenantId,
        world_key: &str,
    ) -> Result<Option<FrameWorldRevisionRecord>, StoreError> {
        let Some(world) = self.frame_world_by_key(tenant_id, world_key).await? else {
            return Ok(None);
        };
        let Some(revision_key) = world.head_revision_key else {
            return Ok(None);
        };
        self.frame_world_revision_by_key(tenant_id, world_key, &revision_key)
            .await
    }

    pub async fn publish_frame_world_revision(
        &self,
        draft: FrameWorldRevisionDraft,
    ) -> Result<FrameWorldPublication, StoreError> {
        validate_text("world_key", &draft.world_key, 128)?;
        validate_optional_text(
            "expected_head_revision_key",
            draft.expected_head_revision_key.as_deref(),
            128,
        )?;
        validate_text("revision_key", &draft.revision_key, 128)?;
        validate_sha256(&draft.spec_sha256)?;
        validate_text("root_frame_key", &draft.root_frame_key, 128)?;
        let world = self
            .frame_world_by_key(draft.identity.tenant_id, &draft.world_key)
            .await?
            .ok_or_else(|| StoreError::FrameWorldNotFound(draft.world_key.clone()))?;
        if world.owner != draft.identity.principal_id.record_id() {
            return Err(StoreError::FrameWorldConflict(draft.world_key));
        }
        if let Some(head_key) = &world.head_revision_key {
            let head = self
                .frame_world_revision_by_key(draft.identity.tenant_id, &draft.world_key, head_key)
                .await?
                .ok_or(StoreError::MissingRecord {
                    operation: "frame world head revision lookup",
                })?;
            if head.spec_sha256 == draft.spec_sha256 {
                return Ok(FrameWorldPublication {
                    world,
                    revision: head,
                    created: false,
                });
            }
        }
        if world.head_revision_key != draft.expected_head_revision_key {
            return Err(StoreError::FrameWorldConflict(draft.world_key));
        }

        let revision_id = FrameWorldRevisionRecordId::new();
        let next_revision = world.revision + 1;
        let now = Utc::now();
        let spec_sha256 = draft.spec_sha256.clone();
        let content = FrameWorldRevisionContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            world: world.id.clone(),
            world_key: draft.world_key.clone(),
            revision_key: draft.revision_key.clone(),
            revision: next_revision,
            spec_sha256: draft.spec_sha256,
            root_frame_key: draft.root_frame_key,
            definition: draft.definition,
            created_at: now,
        };
        let revision_outbox = frame_world_event(
            &draft.identity,
            "frame_world_revision",
            &draft.revision_key,
            "frame.world.revision.published",
        );
        let world_outbox = frame_world_event(
            &draft.identity,
            "frame_world",
            &draft.world_key,
            "frame.world.head.changed",
        );
        let result = self
            .client()
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $world); IF $current.revision != $expected_revision { THROW 'frame_world_revision_conflict'; }; CREATE ONLY $revision_record CONTENT $content RETURN NONE; UPDATE ONLY $world SET head_revision = $revision_record, head_revision_key = $revision_key, revision = $next_revision, updated_at = $now RETURN NONE; CREATE outbox_event CONTENT $revision_outbox RETURN NONE; CREATE outbox_event CONTENT $world_outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("world", world.id.clone()))
            .bind(("expected_revision", world.revision))
            .bind(("revision_record", revision_id.record_id()))
            .bind(("content", content))
            .bind(("revision_key", draft.revision_key.clone()))
            .bind(("next_revision", next_revision))
            .bind(("now", now))
            .bind(("revision_outbox", revision_outbox))
            .bind(("world_outbox", world_outbox))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            let current = self
                .frame_world_by_key(draft.identity.tenant_id, &draft.world_key)
                .await?
                .ok_or_else(|| StoreError::FrameWorldNotFound(draft.world_key.clone()))?;
            if let Some(head_key) = &current.head_revision_key {
                let head = self
                    .frame_world_revision_by_key(
                        draft.identity.tenant_id,
                        &draft.world_key,
                        head_key,
                    )
                    .await?
                    .ok_or(StoreError::MissingRecord {
                        operation: "frame world publication recovery",
                    })?;
                if head.spec_sha256 == spec_sha256 {
                    return Ok(FrameWorldPublication {
                        world: current,
                        revision: head,
                        created: false,
                    });
                }
            }
            if current.revision != world.revision {
                return Err(StoreError::FrameWorldConflict(draft.world_key));
            }
            return Err(error.into());
        }
        let world = self
            .frame_world_by_key(draft.identity.tenant_id, &draft.world_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "frame world publication readback",
            })?;
        let revision = self
            .frame_world_revision_by_key(
                draft.identity.tenant_id,
                &draft.world_key,
                &draft.revision_key,
            )
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "frame world revision publication readback",
            })?;
        Ok(FrameWorldPublication {
            world,
            revision,
            created: true,
        })
    }
}

fn frame_world_event(
    identity: &PlatformIdentity,
    aggregate_type: &str,
    aggregate_id: &str,
    event_type: &str,
) -> OutboxDraft {
    OutboxDraft::now(
        Some(identity.tenant_id.record_id()),
        aggregate_type,
        aggregate_id,
        event_type,
        FRAME_WORLD_EVENT_SCHEMA_VERSION,
        OpenObject::new(BTreeMap::from([
            ("tenant_key".into(), serde_json::json!(identity.tenant_key)),
            (
                "principal_key".into(),
                serde_json::json!(identity.principal_key),
            ),
        ])),
    )
}

fn normalize_labels(labels: &mut Vec<String>) -> Result<(), StoreError> {
    for label in labels.iter() {
        validate_text("labels", label, 256)?;
    }
    labels.sort();
    labels.dedup();
    Ok(())
}

fn validate_optional_text(
    field: &'static str,
    value: Option<&str>,
    max: usize,
) -> Result<(), StoreError> {
    if let Some(value) = value {
        validate_text(field, value, max)?;
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), StoreError> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(StoreError::InvalidCoordinateField {
            field,
            reason: "must be non-empty, bounded, and contain no control characters",
        });
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), StoreError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(StoreError::InvalidCoordinateField {
            field: "spec_sha256",
            reason: "must be a lowercase hexadecimal SHA-256 digest",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_validation_is_strict() {
        assert!(validate_sha256(&"a".repeat(64)).is_ok());
        assert!(validate_sha256("abc").is_err());
        assert!(validate_sha256(&"g".repeat(64)).is_err());
        assert!(validate_sha256(&"A".repeat(64)).is_err());
    }
}
