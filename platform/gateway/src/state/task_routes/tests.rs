use super::*;
#[path = "../../../../../testing/fixtures/store.rs"]
mod fixture;

fn draft() -> GatewayTaskRouteDraft {
    GatewayTaskRouteDraft {
        tenant_key: "task-route-fixture".into(),
        owner_key: "https://tasks.test#agent".into(),
        owner_issuer: "https://tasks.test".into(),
        owner_subject: "agent".into(),
        owner_kind: PrincipalKind::Service,
        work_context: "operations".into(),
        profile: "admin".into(),
        server: "computers".into(),
        source_task_id: "opaque-provider-task/one".into(),
        source_task: None,
        authority_digest: "a".repeat(64),
        ttl_ms: Some(60_000),
    }
}

#[tokio::test]
async fn concurrent_projection_and_retry_reuse_one_route_without_extending_retention() {
    let db = fixture::TestDb::new().await;
    let left = GatewayState::new(db.a.clone());
    let right = GatewayState::new(db.b.clone());
    let input = draft();
    // Gateway authentication has already enrolled the principal before projection.
    db.a.ensure_identity(
        &input.tenant_key,
        &input.owner_key,
        &input.owner_issuer,
        &input.owner_subject,
        input.owner_kind,
    )
    .await
    .unwrap();
    let replies = futures::future::join_all((0..12).map(|i| {
        let state = if i % 2 == 0 { &left } else { &right };
        state.create_task_route(input.clone())
    }))
    .await
    .into_iter()
    .map(Result::unwrap)
    .collect::<Vec<_>>();
    let (canonical, route) = &replies[0];
    for (id, row) in &replies {
        assert_eq!(id, canonical);
        assert_eq!(row.created_at, route.created_at);
        assert_eq!(row.expires_at, route.expires_at);
    }
    let mut retry = input.clone();
    retry.ttl_ms = Some(120_000);
    let (same, retained) = right.create_task_route(retry).await.unwrap();
    assert_eq!(&same, canonical);
    assert_eq!(retained.expires_at, route.expires_at);
    assert!(left.task_route(canonical).await.unwrap().is_some());
    let mut read =
        db.a.client()
            .query("SELECT * FROM gateway_task_route;")
            .await
            .unwrap();
    let rows: Vec<GatewayTaskRouteRecord> = read.take(0).unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn changed_authority_or_expired_routes_cannot_be_rebound_by_an_upstream_retry() {
    let db = fixture::TestDb::new().await;
    let state = GatewayState::new(db.a.clone());
    let input = draft();
    let (canonical, original) = state.create_task_route(input.clone()).await.unwrap();
    let mut work_context = input.clone();
    work_context.work_context = "other".into();
    let mut authority = input.clone();
    authority.authority_digest = "b".repeat(64);
    let mut source_link = input.clone();
    source_link.source_task = Some(uuid::Uuid::now_v7().to_string().parse().unwrap());
    for altered in [work_context, authority, source_link] {
        assert!(state.create_task_route(altered).await.is_err());
        let current = state.task_route(&canonical).await.unwrap().unwrap();
        assert_eq!(current.authority_digest, original.authority_digest);
        assert_eq!(current.expires_at, original.expires_at);
    }
    db.a.client()
        .query("UPDATE ONLY $route SET expires_at = time::now() - 1s;")
        .bind(("route", original.id))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(state.create_task_route(input.clone()).await.is_err());
    assert!(state.task_route(&canonical).await.unwrap().is_none());
    let mut another = input.clone();
    another.server = "another-server".into();
    let (independent, _) = state.create_task_route(another).await.unwrap();
    assert_ne!(canonical, independent);
    let mut overflow = input;
    overflow.source_task_id = "another-task".into();
    overflow.ttl_ms = Some(i64::MAX as u64);
    assert!(state.create_task_route(overflow).await.is_err());
}
