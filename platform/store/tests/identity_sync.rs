#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use fixture::TestDb;
use veoveo_platform_store::{
    EnterpriseRecord, PlatformIdentity, PlatformStore, PrincipalKind, PrincipalRecord, StoreError,
    TenantRecord, deterministic_enterprise_id,
};

const TENANT: &str = "identity-sync";
const KEY: &str = "https://issuer.example#alice";
const ISSUER: &str = "https://issuer.example";

async fn ensure(store: &PlatformStore, name: &str) -> PlatformIdentity {
    store
        .ensure_named_identity(TENANT, KEY, ISSUER, "alice", PrincipalKind::User, name)
        .await
        .unwrap()
}

#[tokio::test]
async fn stale_identity_projection_cannot_restore_revoked_authority() {
    let db = TestDb::new().await;
    let identity = ensure(&db.a, "Alice").await;
    let enterprise_id = deterministic_enterprise_id().record_id();
    let tenant_id = identity.tenant_id.record_id();
    let principal_id = identity.principal_id.record_id();
    // Stage the precise race: the metadata writer has already read enabled
    // snapshots when another replica commits disablement and policy changes.
    let enterprise: EnterpriseRecord =
        db.a.client()
            .select(enterprise_id.clone())
            .await
            .unwrap()
            .unwrap();
    let tenant: TenantRecord =
        db.a.client()
            .select(tenant_id.clone())
            .await
            .unwrap()
            .unwrap();
    let principal: PrincipalRecord =
        db.a.client()
            .select(principal_id.clone())
            .await
            .unwrap()
            .unwrap();
    db.b.client().query("BEGIN; UPDATE ONLY $enterprise SET enabled = false, name = 'Operator name'; UPDATE ONLY $tenant SET enabled = false, classification_ceiling = 'restricted'; UPDATE ONLY $principal SET enabled = false, email = 'verified@example.test', claims_hash = 'committed-claims', display_name = 'Current name'; COMMIT;")
        .bind(("enterprise", enterprise_id.clone()))
        .bind(("tenant", tenant_id.clone()))
        .bind(("principal", principal_id.clone()))
        .await.unwrap().check().unwrap();
    // Exercise the actual transaction with stale pre-read content, including
    // the read-none/concurrent-create case: existing records follow the same arm.
    for display_name in [None, Some("New display name".to_owned())] {
        db.a.client()
            .query(include_str!("../src/identity/ensure.surql"))
            .bind(("enterprise", enterprise_id.clone()))
            .bind(("enterprise_content", enterprise.clone()))
            .bind(("tenant", tenant_id.clone()))
            .bind(("tenant_content", tenant.clone()))
            .bind(("principal", principal_id.clone()))
            .bind(("principal_content", principal.clone()))
            .bind(("principal_key", KEY))
            .bind(("display_name", display_name.clone()))
            .bind(("fallback_display_name", "alice"))
            .await
            .unwrap()
            .check()
            .unwrap();
        let current: PrincipalRecord =
            db.b.client()
                .select(principal_id.clone())
                .await
                .unwrap()
                .unwrap();
        assert!(!current.enabled);
        assert_eq!(
            current.display_name,
            display_name.as_deref().unwrap_or("Current name")
        );
        assert_eq!(current.email.as_deref(), Some("verified@example.test"));
        assert_eq!(current.claims_hash, "committed-claims");
        assert_eq!(current.created_at, principal.created_at);
    }
    let current_enterprise: EnterpriseRecord =
        db.b.client().select(enterprise_id).await.unwrap().unwrap();
    assert!(!current_enterprise.enabled);
    assert_eq!(current_enterprise.name, "Operator name");
    let current_tenant: TenantRecord = db.b.client().select(tenant_id).await.unwrap().unwrap();
    assert!(!current_tenant.enabled);
    assert_eq!(current_tenant.classification_ceiling, "restricted");
    assert_eq!(
        ensure(&db.a, "Still disabled").await.principal_id,
        identity.principal_id
    );
    let current: PrincipalRecord = db.b.client().select(principal_id).await.unwrap().unwrap();
    assert!(!current.enabled);
}

#[tokio::test]
async fn concurrent_discovery_converges_and_cannot_rebind_identity() {
    let db = TestDb::new().await;
    let mut callers = tokio::task::JoinSet::new();
    for i in 0..12 {
        let store = if i % 2 == 0 {
            db.a.clone()
        } else {
            db.b.clone()
        };
        callers.spawn(async move { ensure(&store, &format!("Alice {i}")).await });
    }
    let identity = callers.join_next().await.unwrap().unwrap();
    while let Some(result) = callers.join_next().await {
        assert_eq!(result.unwrap().principal_id, identity.principal_id);
    }
    for (issuer, subject, kind) in [
        ("https://foreign.example", "alice", PrincipalKind::User),
        (ISSUER, "mallory", PrincipalKind::User),
        (ISSUER, "alice", PrincipalKind::Service),
    ] {
        assert!(matches!(
            db.b.ensure_named_identity(TENANT, KEY, issuer, subject, kind, "Wrong identity")
                .await,
            Err(StoreError::IdentityConflict {
                entity: "principal",
                ..
            })
        ));
    }
    ensure(&db.b, "Trusted display name").await;
    db.a.ensure_identity(TENANT, KEY, ISSUER, "alice", PrincipalKind::User)
        .await
        .unwrap();
    let current: PrincipalRecord =
        db.b.client()
            .select(identity.principal_id.record_id())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(current.display_name, "Trusted display name");
    db.a.client()
        .query("UPDATE ONLY $principal SET display_name = $key;")
        .bind(("principal", identity.principal_id.record_id()))
        .bind(("key", KEY))
        .await
        .unwrap()
        .check()
        .unwrap();
    db.b.ensure_identity(TENANT, KEY, ISSUER, "alice", PrincipalKind::User)
        .await
        .unwrap();
    let current: PrincipalRecord =
        db.a.client()
            .select(identity.principal_id.record_id())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(current.display_name, "alice");
}
