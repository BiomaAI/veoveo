//! Upgrade existing storage accounting from the deployed schema, then repeat migration.

use super::native_database::Database;
use veoveo_platform_store as platform;

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.2.4 process"]
async fn upload_migration_counts_existing_blobs_and_replay_preserves_reservations() {
    let mut database = Database::start();
    let store = database.connect_unmigrated().await;
    for migration in platform::migrations().iter().take(50) {
        let sql = format!(
            "BEGIN TRANSACTION; {} CREATE platform_schema_migration:{} CONTENT {{version: {}, name: $name, checksum: $checksum, applied_at: time::now()}}; COMMIT TRANSACTION;",
            migration.sql, migration.version, migration.version
        );
        store
            .client()
            .query(sql)
            .bind(("name", migration.name))
            .bind(("checksum", migration.checksum()))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    let owner = store
        .ensure_identity(
            "populated",
            "owner",
            "https://fixture",
            "owner",
            platform::PrincipalKind::User,
        )
        .await
        .unwrap();
    let empty = store
        .ensure_identity(
            "empty",
            "owner",
            "https://fixture",
            "owner",
            platform::PrincipalKind::User,
        )
        .await
        .unwrap();
    let blob = platform::ArtifactBlobRecord {
        id: platform::ArtifactBlobId::new().record_id(),
        tenant: owner.tenant_id.record_id(),
        sha256: "c".repeat(64),
        byte_len: 10 * 1024 * 1024 * 1024,
        object_key: "retained/opaque".into(),
        content_type: "application/octet-stream".into(),
        encryption: platform::OpenObject::default(),
        created_at: chrono::Utc::now(),
    };
    store
        .client()
        .query("CREATE ONLY $blob.id CONTENT $blob;")
        .bind(("blob", blob))
        .await
        .unwrap()
        .check()
        .unwrap();
    let migration = store.migrate().await.unwrap();
    assert_eq!(migration.applied_versions, vec![50]);
    let usage = platform::artifact_storage_usage_id(owner.tenant_id);
    let mut response = store
        .client()
        .query("SELECT * FROM ONLY $usage; SELECT * FROM ONLY $empty;")
        .bind(("usage", usage.clone()))
        .bind((
            "empty",
            platform::artifact_storage_usage_id(empty.tenant_id),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    let record: Option<platform::ArtifactStorageUsage> = response.take(0).unwrap();
    assert_eq!(record.unwrap().committed_bytes, 10 * 1024 * 1024 * 1024);
    let record: Option<platform::ArtifactStorageUsage> = response.take(1).unwrap();
    assert_eq!(record.unwrap().committed_bytes, 0);
    store
        .client()
        .query("UPDATE ONLY $usage SET reserved_bytes = 8192, active_uploads = 1;")
        .bind(("usage", usage.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(store.migrate().await.unwrap().applied_versions.is_empty());
    let mut response = store
        .client()
        .query("SELECT * FROM ONLY $usage;")
        .bind(("usage", usage))
        .await
        .unwrap()
        .check()
        .unwrap();
    let record: Option<platform::ArtifactStorageUsage> = response.take(0).unwrap();
    assert_eq!(record.unwrap().reserved_bytes, 8192);
    database.finish();
}
