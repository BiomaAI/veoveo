//! Shared typed publication content for ordinary writes and upload commits.

use super::*;

pub(crate) struct PreparedPublication {
    pub blob: ArtifactBlobRecord,
    pub occurrence: ArtifactOccurrenceRecord,
    pub grants: Vec<ArtifactGrantEdge>,
    pub outbox: OutboxDraft,
}

pub(crate) fn prepare_publication(
    draft: ArtifactOccurrenceDraft,
) -> Result<PreparedPublication, StoreError> {
    let work_context =
        deterministic_work_context_id(&draft.identity.tenant_key, &draft.authority.context_key)?;
    let initiator = draft
        .authority
        .initiator_key
        .as_deref()
        .map(|principal| deterministic_principal_id(&draft.identity.tenant_key, principal))
        .transpose()?
        .map(|principal| principal.record_id());
    let blob_id = ArtifactBlobId::from_uuid(Uuid::new_v5(
        &PLATFORM_ID_NAMESPACE,
        format!("blob:{}:{}", draft.identity.tenant_key, draft.sha256).as_bytes(),
    ));
    let now = Utc::now();
    let blob = ArtifactBlobRecord {
        id: blob_id.record_id(),
        tenant: draft.identity.tenant_id.record_id(),
        sha256: draft.sha256,
        byte_len: draft.byte_len,
        object_key: draft.object_key,
        content_type: draft.media_type.clone(),
        encryption: OpenObject::default(),
        created_at: now,
    };
    let occurrence = ArtifactOccurrenceRecord {
        id: draft.artifact_id.record_id(),
        tenant: draft.identity.tenant_id.record_id(),
        blob: blob_id.record_id(),
        owner: draft.owner,
        owner_kind: draft.authority.owner_kind,
        owner_key: draft.authority.owner_key.clone(),
        work_context: work_context.record_id(),
        producer: draft.identity.principal_id.record_id(),
        producer_key: draft.identity.principal_key.clone(),
        initiator,
        initiator_key: draft.authority.initiator_key.clone(),
        invocation_mode: draft.authority.invocation_mode,
        delegation_id: draft.authority.delegation_id.clone(),
        policy_revision: draft.authority.policy_revision.clone(),
        authority: draft.authority.clone(),
        task: None,
        filename: draft.filename,
        media_type: draft.media_type,
        classification: draft.classification,
        labels: draft.labels.clone(),
        metadata: OpenObject::new(draft.metadata),
        release_state: ArtifactReleaseState::Private,
        retention_expires_at: draft.retention_expires_at,
        created_at: now,
        updated_at: now,
        search_text: String::new(),
    };
    let grants = draft
        .initial_grants
        .into_iter()
        .map(|grant| ArtifactGrantEdge {
            id: deterministic_relation_id(
                "artifact-grant",
                draft.artifact_id.to_string(),
                format!("{:?}:{}", grant.subject_kind, grant.subject_key),
            ),
            r#in: draft.artifact_id.record_id(),
            out: grant.subject,
            subject_kind: grant.subject_kind,
            subject_key: grant.subject_key,
            permission: grant.permission,
            labels: grant.labels,
            expires_at: grant.expires_at,
            created_by: grant.created_by.record_id(),
            created_at: now,
        })
        .collect::<Vec<_>>();
    let outbox = OutboxDraft::now(
        Some(draft.identity.tenant_id.record_id()),
        "artifact",
        draft.artifact_id.to_string(),
        "artifact.created",
        1,
        OpenObject::new(BTreeMap::from([(
            "artifact_id".into(),
            serde_json::json!(draft.artifact_id.to_string()),
        )])),
    );
    Ok(PreparedPublication {
        blob,
        occurrence,
        grants,
        outbox,
    })
}
