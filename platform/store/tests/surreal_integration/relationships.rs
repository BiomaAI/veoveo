//! Qualify referential cleanup on the selected kernel schema.
use super::*;
use std::{collections::BTreeSet, time::Duration};
use surrealdb::types::RecordId;

async fn assert_present(store: &PlatformStore, records: &[RecordId], expected: &[RecordId]) {
    let actual = store
        .client()
        .query(include_str!(
            "../queries/surreal_integration/relationships/assert_present.surql"
        ))
        .bind(("records", records.to_vec()))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take::<Vec<RecordId>>(0)
        .unwrap();
    assert_eq!(actual.len(), expected.len());
    for record in expected {
        assert!(actual.contains(record), "missing {record:?}");
    }
}

async fn aborted_delete(store: &PlatformStore, parent: RecordId) {
    assert!(
        store
            .client()
            .query(include_str!(
                "../queries/surreal_integration/relationships/aborted_delete.surql"
            ))
            .bind(("parent", parent))
            .await
            .unwrap()
            .check()
            .is_err()
    );
}

#[tokio::test]
async fn original_rows_are_kept_only_for_deleted_tenant_or_parent_consumers() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let info: surrealdb::types::Value =
            db.a.client()
                .query(include_str!("../queries/surreal_integration/relationships/original_rows_are_kept_only_for_deleted_tenant_or_parent_consumers.surql"))
                .await
                .unwrap()
                .check()
                .unwrap()
                .take(0)
                .unwrap();
        let info = info.into_json_value();
        let tables = info["tables"].as_object().unwrap();
        let actual = tables
            .iter()
            .filter_map(|(name, definition)| {
                definition
                    .as_str()
                    .unwrap()
                    .contains("INCLUDE ORIGINAL")
                    .then_some(name.as_str())
            })
            .collect::<BTreeSet<_>>();
        let expected = BTreeSet::from([
            "principal",
            "task",
            "artifact_blob",
            "artifact_occurrence",
            "artifact_grant",
            "artifact_access_request",
            "share_link",
        ]);
        assert_eq!(actual, expected);
        let feeds = tables
            .values()
            .filter(|definition| definition.as_str().unwrap().contains("CHANGEFEED"))
            .count();
        println!(
            "CHANGEFEED_SCHEMA tables={} feeds={feeds} originals={}",
            tables.len(),
            actual.len()
        );
    })
    .await
    .expect("original-row schema qualification exceeded 60 seconds");
}

#[tokio::test]
async fn occurrence_deletion_cascades_shares_and_graph_grants_with_parent_notifications() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
        let identity =
            db.a.ensure_identity(
                "tenant-relations",
                "alice",
                "https://idp.example.com",
                "alice",
                PrincipalKind::User,
            )
            .await
            .unwrap();
        let mut parents = Vec::new();
        let mut children = Vec::new();
        for token in ['a', 'b'] {
            let artifact_id = ArtifactId::new();
            let artifact =
                db.a.create_artifact_occurrence(ArtifactOccurrenceDraft {
                    artifact_id,
                    identity: identity.clone(),
                    authority: artifact_authority(&identity),
                    owner: identity.principal_id.record_id(),
                    initial_grants: vec![owner_grant(artifact_id, &identity)],
                    sha256: "a".repeat(64),
                    byte_len: 4,
                    object_key: "qualification/shared-blob".into(),
                    media_type: "application/octet-stream".into(),
                    filename: None,
                    classification: String::new(),
                    labels: vec![],
                    metadata: BTreeMap::new(),
                    retention_expires_at: None,
                })
                .await
                .unwrap();
            let link =
                db.a.create_artifact_share_link(ArtifactShareLinkDraft {
                    link_id: ShareLinkId::new(),
                    artifact_id,
                    identity: identity.clone(),
                    token_hash: token.to_string().repeat(64),
                    expires_at: Utc::now() + TimeDelta::minutes(5),
                    max_downloads: None,
                })
                .await
                .unwrap();
            parents.push(artifact.occurrence.id);
            children.push(vec![link.id, artifact.grants[0].id.clone()]);
        }
        let all = parents
            .iter()
            .cloned()
            .chain(children.iter().flatten().cloned())
            .collect::<Vec<_>>();
        aborted_delete(&db.a, parents[0].clone()).await;
        assert_present(&db.b, &all, &all).await;
        db.a.client()
            .query(include_str!("../queries/surreal_integration/relationships/occurrence_deletion_cascades_shares_and_graph_grants_with_parent_notifications.surql"))
            .bind(("parent", parents[0].clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let survivors = std::iter::once(parents[1].clone())
            .chain(children[1].iter().cloned())
            .collect::<Vec<_>>();
        assert_present(&db.b, &all, &survivors).await;
        // Cascaded deletions must still tell the Artifact feed which parent changed.
        let mut cursor = ChangefeedCursor::initial();
        let mut observed = Vec::new();
        let mut created = Vec::new();
        while observed.len() < children[0].len() {
            let batches = db.b.replay_changes(cursor, 1_000).await.unwrap();
            for batch in batches {
                cursor = ChangefeedCursor::from_versionstamp(batch.versionstamp + 1).unwrap();
                for value in batch.changes {
                    let change = decode_changefeed_entry(&value).unwrap();
                    if let Some(record) = change.record_id()
                        && children[0].contains(record)
                    {
                        let decoded = veoveo_platform_store::ArtifactChange::decode(&change)
                            .unwrap()
                            .unwrap();
                        assert_eq!(
                            RecordId::new(
                                "artifact_occurrence",
                                surrealdb::types::Uuid::from(decoded.artifact_id.as_uuid()),
                            ),
                            parents[0]
                        );
                        let seen = if matches!(change, ChangefeedEntry::Delete { .. }) {
                            &mut observed
                        } else {
                            &mut created
                        };
                        if !seen.contains(record) {
                            seen.push(record.clone());
                        }
                    }
                }
            }
            if observed.len() < children[0].len() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
        assert_eq!(created.len(), children[0].len());
    })
    .await
    .expect("Artifact cascade qualification exceeded 90 seconds");
}

#[tokio::test]
async fn refresh_family_deletion_cascades_only_its_tokens_and_rolls_back_together() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
        let mut parents = Vec::new();
        let mut children = Vec::new();
        let now = Utc::now();
        for token_hash in ['a', 'b'] {
            let family = veoveo_platform_store::GatewayRefreshFamilyRecord {
                id: veoveo_platform_store::gateway_refresh_family_record_id(Uuid::now_v7()),
                authorization_server: "veoveo".into(),
                profile: "operator".into(),
                oauth_client_id: "qualification".into(),
                work_context: "operations".into(),
                principal_id: "alice".into(),
                tenant: Some("tenant-relations".into()),
                scopes: vec![],
                principal: serde_json::from_value(serde_json::json!({
                    "principal": {"id":"alice", "kind":"user", "issuer":"https://identity.test", "subject":"alice", "tenant":"tenant-relations", "groups":[], "roles":[], "scopes":[], "data_labels":[], "assurances":[]},
                    "principal_display_name":"Alice"
                })).unwrap(),
                current_generation: 0,
                issued_at: now,
                expires_at: now + TimeDelta::hours(1),
                revoked_at: None,
                revocation_reason: None,
            };
            let token = veoveo_platform_store::GatewayRefreshTokenRecord {
                id: veoveo_platform_store::gateway_refresh_token_record_id(Uuid::now_v7()),
                family: family.id.clone(),
                token_hash: token_hash.to_string().repeat(64),
                generation: 0,
                issued_at: now,
                expires_at: family.expires_at,
                consumed_at: None,
                replacement: None,
                replay_detected_at: None,
                delivery_envelope: None,
                delivery_expires_at: None,
            };
            parents.push(family.id.clone());
            children.push(token.id.clone());
            db.a.create_gateway_refresh_family(family, token)
                .await
                .unwrap();
        }
        let all = parents.iter().chain(&children).cloned().collect::<Vec<_>>();
        aborted_delete(&db.a, parents[0].clone()).await;
        assert_present(&db.b, &all, &all).await;
        db.a.client()
            .query(include_str!("../queries/surreal_integration/relationships/refresh_family_deletion_cascades_only_its_tokens_and_rolls_back_together.surql"))
            .bind(("parent", parents[0].clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_present(&db.b, &all, &[parents[1].clone(), children[1].clone()]).await;
    })
    .await
    .expect("refresh cascade qualification exceeded 60 seconds");
}
