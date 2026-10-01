use super::*;
use veoveo_mcp_contract::access::{GroupMembership, GroupRole};

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.3.0 process"]
async fn discovery_sql_admits_context_grants_clearance_and_retention_before_limits() {
    let mut database = native_database::Database::start();
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let store = database.connect().await;
        let service = ArtifactService::new(
            crate::SurrealArtifactRepository::new(store.clone()),
            InMemoryBlobStore::default(),
        );
        let alice = caller("alice", "acme", &["secret"]);
        let mut bob = caller("bob", "acme", &[]);
        store.ensure_identity("acme", "bob", bob.identity.actor.issuer.as_str(),
            bob.identity.actor.subject.as_str(), veoveo_platform_store::PrincipalKind::User)
            .await.unwrap();
        let mut visible = BTreeSet::new();
        for _ in 0..3 {
            visible.insert(service.put(&alice, PutArtifactRequest::default(), b"visible".to_vec())
                .await.unwrap().artifact_id());
        }

        // These newer rows would occupy the first page if admission ran after LIMIT.
        let classified = service.put(&alice, PutArtifactRequest {
            classification: Some("secret".parse().unwrap()),
            ..Default::default()
        }, b"classified".to_vec()).await.unwrap();
        let expired = service.put(&alice, PutArtifactRequest::default(), b"expired".to_vec())
            .await.unwrap();
        let mut private = alice.clone();
        private.identity.authority.work_context = "private-work".parse().unwrap();
        bind_request_context(&mut private.identity);
        let unshared = service.put(&private, PutArtifactRequest::default(), b"private".to_vec())
            .await.unwrap();
        let expired_grant = service.put(&private, PutArtifactRequest::default(), b"grant".to_vec())
            .await.unwrap();
        service.grant(&private, &expired_grant.artifact_id(),
            AccessSubject::Principal(bob.identity.actor.id.clone()), AccessLevel::Read)
            .await.unwrap();

        for id in [classified.artifact_id(), expired.artifact_id(), unshared.artifact_id(), expired_grant.artifact_id()] {
            store.client().query("UPDATE $artifact SET producer_key = ''; ")
                .bind(("artifact", veoveo_platform_store::ArtifactId::from_uuid(id.as_uuid()).record_id()))
                .await.unwrap().check().unwrap();
        }
        store.client().query("UPDATE $artifact SET retention_expires_at = time::now() - 1s; UPDATE artifact_grant SET expires_at = time::now() - 1s WHERE in = $granted AND subject_key = 'bob';")
            .bind(("artifact", veoveo_platform_store::ArtifactId::from_uuid(expired.artifact_id().as_uuid()).record_id()))
            .bind(("granted", veoveo_platform_store::ArtifactId::from_uuid(expired_grant.artifact_id().as_uuid()).record_id()))
            .await.unwrap().check().unwrap();

        let first = service.list(&bob, ListArtifactsRequest { cursor: None, limit: Some(2) })
            .await.unwrap();
        assert_eq!(first.artifacts.len(), 2);
        let second = service.list(&bob, ListArtifactsRequest { cursor: first.next_cursor, limit: Some(2) })
            .await.unwrap();
        assert_eq!(second.artifacts.len(), 1);
        assert!(second.next_cursor.is_none());
        let actual = first.artifacts.into_iter().chain(second.artifacts)
            .map(|item| item.artifact_id()).collect::<BTreeSet<_>>();
        assert_eq!(actual, visible);

        bob.identity.authority.work_context = "other-work".parse().unwrap();
        bind_request_context(&mut bob.identity);
        assert!(service.list(&bob, ListArtifactsRequest::default()).await.unwrap().artifacts.is_empty());
        let ids: Vec<_> = visible.iter().copied().collect();
        service.grant(&alice, &ids[0], AccessSubject::Principal(bob.identity.actor.id.clone()), AccessLevel::Read)
            .await.unwrap();
        service.grant(&alice, &ids[1], AccessSubject::Group("operators".parse().unwrap()), AccessLevel::Read)
            .await.unwrap();
        bob.memberships.insert(GroupMembership { group: "operators".parse().unwrap(), role: GroupRole::Read });
        let granted = service.list(&bob, ListArtifactsRequest { cursor: None, limit: Some(2) }).await.unwrap();
        assert_eq!(granted.artifacts.len(), 2);
        assert!(granted.next_cursor.is_none(), "an exactly full final page has no continuation");
        assert_eq!(granted.artifacts.iter().map(|item| item.artifact_id()).collect::<BTreeSet<_>>(), BTreeSet::from([ids[0], ids[1]]));
        assert!(service.list(&caller("bob", "other-tenant", &["secret"]), ListArtifactsRequest::default())
            .await.unwrap().artifacts.is_empty());
        let mut cleared = bob;
        cleared.identity.actor.data_labels.insert("secret".parse().unwrap());
        cleared.identity.authority.work_context = alice.identity.authority.work_context.clone();
        bind_request_context(&mut cleared.identity);
        assert!(service.list(&cleared, ListArtifactsRequest::default()).await.is_err(),
            "an admitted corrupt row must fail explicitly");
    }).await.expect("Artifact admission qualification timed out");
    database.finish();
}
