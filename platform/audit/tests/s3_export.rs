//! Real S3 delivery from a disposable Store, using an isolated provider prefix.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use futures::{FutureExt, TryStreamExt};
use object_store::{ObjectStore, ObjectStoreExt, aws::AmazonS3Builder, path::Path};
use serde::Deserialize;
use std::{
    collections::BTreeMap, num::NonZeroU32, panic::AssertUnwindSafe, sync::Arc, time::Duration,
};
use veoveo_audit::{
    export::{AuditExportConfig, AuditExporter, ObjectLock},
    integrity::{AuditKeyRing, AuditSigningKey, AuditVerifier},
    *,
};

#[derive(Deserialize)]
struct ExportedEvent {
    unmapped: Source,
}

#[derive(Deserialize)]
struct Source {
    veoveo: AuditRecord,
}

async fn delivered(
    store: &veoveo_platform_store::PlatformStore,
    destination: &AuditDestinationId,
    health: &AuditHealth,
) {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            assert_ne!(
                health.state(),
                AuditHealthState::Failed,
                "export worker failed"
            );
            if store
                .audit_export_candidate(destination)
                .await
                .unwrap()
                .is_none()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("S3 export acknowledgement exceeded 30 seconds");
}

async fn qualify(exports: AuditExportConfig, s3: &dyn ObjectStore, prefix: &Path) {
    let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
    let destinations = AuditExporter::new(exports.clone())
        .unwrap()
        .destination_ids();
    let key = Arc::new(AuditSigningKey::from_seed(&[23; 32]));
    let mut keys = AuditKeyRing::default();
    keys.insert(key.public_key()).unwrap();
    let service = AuditService::start(
        db.a.clone(),
        key.clone(),
        NonZeroU32::new(1).unwrap(),
        exports.clone(),
    )
    .unwrap();
    let writer = AuditWriter::start(db.a.clone());
    let scope = AuditReadScope::new(None, true);
    let partition = AuditPartition::Installation;
    let mut expected_ids = Vec::new();
    // Waiting for each delivery makes the second write exercise an idle worker's
    // LIVE wakeup, and produces two independently verifiable signed blocks.
    for method in [AuditReadMethod::ResourceRead, AuditReadMethod::AuditView] {
        let draft = AuditDraft::builder(
            AuditRequest::background(),
            AuditTarget::Installation,
            AuditDetail::Read { method },
            AuditOutcome::Allowed,
            AuditReason::Accepted,
        )
        .build()
        .unwrap();
        expected_ids.push(draft.id());
        writer.record(draft.clone()).await.unwrap();
        tokio::time::timeout(
            Duration::from_secs(30),
            db.a.audit_wait_sealed(&scope, &partition, draft.id()),
        )
        .await
        .expect("fixture record was not sealed")
        .unwrap();
        for destination in &destinations {
            delivered(&db.b, destination, &service.health()).await;
        }
    }
    writer.shutdown(Duration::from_secs(10)).await.unwrap();
    service.shutdown(Duration::from_secs(15)).await.unwrap();
    let expected_blocks =
        db.b.audit_blocks(&scope, &partition, None, 10)
            .await
            .unwrap();
    assert_eq!(expected_blocks.len(), 2);

    let mut objects: Vec<_> = s3
        .list(Some(prefix))
        .try_collect()
        .await
        .unwrap_or_else(|_| panic!("cannot list the owned S3 fixture prefix"));
    objects.sort_by(|left, right| left.location.cmp(&right.location));
    assert_eq!(
        objects.len(),
        4,
        "one JSON Lines object and one seal per block"
    );
    let mut records = BTreeMap::new();
    let mut blocks = Vec::new();
    for object in &objects {
        assert!(object.size < 1_048_576, "fixture object exceeds 1 MiB");
        let bytes = s3
            .get(&object.location)
            .await
            .unwrap_or_else(|_| panic!("cannot retrieve the fixture object"))
            .bytes()
            .await
            .unwrap_or_else(|_| panic!("cannot read the fixture object"));
        let name = object.location.filename().unwrap();
        if name.ends_with(".ocsf.jsonl") {
            assert_eq!(bytes.last(), Some(&b'\n'));
            for line in bytes
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
            {
                let event: ExportedEvent = serde_json::from_slice(line).unwrap();
                assert!(
                    records
                        .insert(event.unmapped.veoveo.draft.id(), event.unmapped.veoveo)
                        .is_none()
                );
            }
        } else {
            assert!(name.ends_with(".seal.json"));
            blocks.push(serde_json::from_slice::<AuditBlock>(&bytes).unwrap());
        }
    }
    blocks.sort_by_key(|block| block.head.sequence);
    assert_eq!(blocks, expected_blocks);
    expected_ids.sort_unstable();
    assert_eq!(records.keys().copied().collect::<Vec<_>>(), expected_ids);
    let mut verifier =
        AuditVerifier::new(&keys, partition, None, chrono::TimeDelta::minutes(3)).unwrap();
    for block in &blocks {
        let members = block
            .head
            .members
            .iter()
            .map(|member| records[&member.id].clone())
            .collect::<Vec<_>>();
        assert!(verifier.verify(block, &members).unwrap().is_empty());
    }
    verifier
        .finish(&blocks.last().unwrap().checkpoint())
        .unwrap();

    // A different Store connection and worker reuse the persisted receipts.
    let restarted =
        AuditService::start(db.b.clone(), key, NonZeroU32::new(1).unwrap(), exports).unwrap();
    let mut health = restarted.health();
    tokio::time::timeout(Duration::from_secs(15), async {
        while health.state() != AuditHealthState::Active {
            assert_ne!(health.state(), AuditHealthState::Failed);
            health.changed().await.unwrap();
        }
    })
    .await
    .expect("restarted export worker did not become active");
    for destination in &destinations {
        assert!(
            db.a.audit_export_candidate(destination)
                .await
                .unwrap()
                .is_none()
        );
    }
    restarted.shutdown(Duration::from_secs(15)).await.unwrap();
    let mut after: Vec<_> = s3
        .list(Some(prefix))
        .try_collect()
        .await
        .unwrap_or_else(|_| panic!("cannot inspect the fixture objects after restart"));
    after.sort_by(|left, right| left.location.cmp(&right.location));
    assert_eq!(
        after, objects,
        "restart must not replace acknowledged objects"
    );
    println!(
        "{{\"s3_export\":\"passed\",\"blocks\":2,\"records\":2,\"destinations\":{},\"replica_restart\":true}}",
        destinations.len()
    );
}

async fn cleanup(s3: &dyn ObjectStore, prefix: &Path) -> bool {
    let Ok(objects) = s3.list(Some(prefix)).try_collect::<Vec<_>>().await else {
        return false;
    };
    let mut success = true;
    for object in objects {
        success &= s3.delete(&object.location).await.is_ok();
    }
    success
}

#[tokio::test]
#[ignore = "requires VEOVEO_AUDIT_TEST_S3_CONFIG, S3 credentials and Docker; owns an isolated prefix and Store"]
async fn real_s3_delivery_preserves_records_signatures_and_replica_receipts() {
    let path = std::env::var_os("VEOVEO_AUDIT_TEST_S3_CONFIG")
        .expect("set VEOVEO_AUDIT_TEST_S3_CONFIG to a public export configuration");
    let mut exports = AuditExportConfig::load(std::path::Path::new(&path)).unwrap();
    let config = exports.s3.as_mut().expect("S3 destination is required");
    assert!(
        matches!(config.object_lock, ObjectLock::Disabled),
        "fixture cleanup requires Object Lock disabled"
    );
    let prefix = Path::from(config.prefix.as_str()).join(uuid::Uuid::now_v7().to_string());
    config.prefix = prefix.to_string().try_into().unwrap();
    let s3 = AmazonS3Builder::from_env()
        .with_endpoint(config.endpoint.as_str())
        .with_region(&config.region)
        .with_bucket_name(config.bucket.as_str())
        .with_allow_http(config.allow_http)
        .with_virtual_hosted_style_request(false)
        .build()
        .unwrap_or_else(|_| panic!("cannot initialize fixture S3 credentials"));
    let result = AssertUnwindSafe(tokio::time::timeout(
        Duration::from_secs(180),
        qualify(exports, &s3, &prefix),
    ))
    .catch_unwind()
    .await;
    let cleaned = tokio::time::timeout(Duration::from_secs(30), cleanup(&s3, &prefix))
        .await
        .unwrap_or(false);
    if let Err(panic) = result {
        if !cleaned {
            eprintln!("S3 fixture prefix cleanup failed: {prefix}");
        }
        std::panic::resume_unwind(panic);
    }
    assert!(cleaned, "S3 fixture prefix cleanup failed: {prefix}");
    result
        .unwrap()
        .expect("S3 delivery qualification exceeded 180 seconds");
}
