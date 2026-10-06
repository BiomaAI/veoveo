//! Work-context setup for hosted admission and native Task fixtures.
pub async fn register_context(store: &veoveo_platform_store::PlatformStore) -> anyhow::Result<()> {
    use veoveo_platform_store as db;
    let id = db::deterministic_work_context_id("test", "speech-test")?.record_id();
    let value = db::WorkContextRecord {
        id: id.clone(),
        tenant: db::deterministic_tenant_id("test")?.record_id(),
        context_key: "speech-test".into(),
        title: "Speech native test".into(),
        policy_revision: "test-1".into(),
        output_policy: db::WorkContextOutputPolicyRecord {
            owner_kind: db::ArtifactGrantSubjectKind::Principal,
            owner_key: "https://speech.test#alice".into(),
            initial_grants: vec![],
            classification: None,
            data_labels: vec![],
        },
        memberships: vec![db::WorkContextMembershipRuleRecord {
            level: db::WorkContextMembershipLevel::Contributor,
            principals: vec![],
            groups: vec![],
            roles: vec![],
            oauth_clients: vec!["console".into()],
        }],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let _: Option<db::WorkContextRecord> = store.client().create(id).content(value).await?;
    Ok(())
}
