//! Work Context prerequisite for fixtures that replay authored Map commits.
use veoveo_platform_store::{
    InvocationAuthorityRecord, PlatformIdentity, PlatformStore, WorkContextOutputPolicyRecord,
    WorkContextRecord, deterministic_work_context_id,
};

pub async fn install(
    store: &PlatformStore,
    identity: &PlatformIdentity,
    authority: &InvocationAuthorityRecord,
) {
    let now = chrono::Utc::now();
    let context = WorkContextRecord {
        id: deterministic_work_context_id(&identity.tenant_key, &authority.context_key)
            .unwrap()
            .record_id(),
        tenant: identity.tenant_id.record_id(),
        context_key: authority.context_key.clone(),
        title: "Map commit fixture".into(),
        policy_revision: authority.policy_revision.clone(),
        output_policy: WorkContextOutputPolicyRecord {
            owner_kind: authority.owner_kind,
            owner_key: authority.owner_key.clone(),
            initial_grants: authority.initial_grants.clone(),
            classification: authority.classification.clone(),
            data_labels: authority.data_labels.clone(),
        },
        memberships: vec![],
        created_at: now,
        updated_at: now,
    };
    store
        .client()
        .query(include_str!(
            "../../src/queries/authoring/projection/recovery_tests/recovery/create_context.surql"
        ))
        .bind(("context", context))
        .await
        .unwrap()
        .check()
        .unwrap();
}
