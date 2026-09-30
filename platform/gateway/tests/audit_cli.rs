//! Public verification against root-tampered records in an owned RocksDB database.
#[path = "../../../testing/fixtures/store/container.rs"]
mod container;

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{TimeDelta, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::{num::NonZeroU32, process::Output, sync::Arc, time::Duration};
use surrealdb::types::{Array, RecordId, SurrealValue};
use tokio::process::Command;
use veoveo_audit::{integrity::AuditSigningKey, *};
use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials, StoreError};

#[derive(Clone, Copy, Debug)]
enum Attack {
    ChangedRecord,
    DeletedRecord,
    DeletedBlock,
    ForgedSignature,
    BackdatedInsert,
}

#[derive(Deserialize)]
struct VerificationReport {
    checkpoint: AuditCheckpoint,
    blocks: u64,
    records: u64,
    clock_findings: u64,
}

async fn connect(endpoint: &str, password: &SecretString) -> PlatformStore {
    let config = StoreConfig::builder(
        endpoint,
        "audit_cli_fixture",
        format!("case_{}", uuid::Uuid::now_v7().simple()),
        StoreCredentials::root("fixture_admin", password.clone()),
    )
    .migrate_on_connect(true)
    .build()
    .unwrap();
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            match PlatformStore::connect(config.clone()).await {
                Ok(store) => return store,
                Err(
                    error @ (StoreError::MigrationExecution { .. }
                    | StoreError::Migration(_)
                    | StoreError::Config(_)),
                ) => {
                    panic!("audit CLI fixture initialization failed: {error}");
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    })
    .await
    .expect("audit CLI fixture initialization exceeded 45 seconds")
}

async fn verify(store: &PlatformStore, password: &SecretString, key: &AuditSigningKey) -> Output {
    let config = store.config();
    let mut command = Command::new(env!("CARGO_BIN_EXE_gateway"));
    // A fixture must neither load the worktree .env nor inherit installation credentials.
    command
        .env_clear()
        .current_dir("/")
        .kill_on_drop(true)
        .args([
            "audit",
            "verify",
            "--installation",
            "--public-key",
            &STANDARD.encode(key.public_key()),
        ])
        .env("VEOVEO_SURREAL_ENDPOINT", config.endpoint().as_str())
        .env("VEOVEO_SURREAL_NAMESPACE", config.namespace())
        .env("VEOVEO_SURREAL_DATABASE", config.database())
        .env("VEOVEO_SURREAL_AUTH_LEVEL", "root")
        .env("VEOVEO_SURREAL_USERNAME", "fixture_admin")
        .env("VEOVEO_SURREAL_PASSWORD", password.expose_secret());
    if let Some(path) = std::env::var_os("LD_LIBRARY_PATH") {
        command.env("LD_LIBRARY_PATH", path);
    }
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("audit verify exceeded 30 seconds")
        .expect("cannot execute gateway");
    assert!(
        output.stdout.len() <= 65_536 && output.stderr.len() <= 65_536,
        "audit CLI fixture output exceeded 64 KiB"
    );
    output
}

fn diagnostic(output: &Output, password: &SecretString) -> String {
    String::from_utf8_lossy(&output.stderr).replace(password.expose_secret(), "[REDACTED]")
}

async fn qualify(store: &PlatformStore, password: &SecretString, attack: Attack) {
    let key = Arc::new(AuditSigningKey::from_seed(&[37; 32]));
    let service = AuditService::start(
        store.clone(),
        key.clone(),
        NonZeroU32::new(7).unwrap(),
        Default::default(),
    )
    .unwrap();
    let scope = AuditReadScope::new(None, true);
    let partition = AuditPartition::Installation;
    let mut ids = Vec::new();
    for method in [AuditReadMethod::ResourceRead, AuditReadMethod::Status] {
        let mut builder = AuditDraft::builder(
            AuditRequest::background(),
            AuditTarget::Installation,
            AuditDetail::Read { method },
            AuditOutcome::Allowed,
            AuditReason::Accepted,
        );
        if matches!(attack, Attack::BackdatedInsert) && !ids.is_empty() {
            builder = builder.occurred_at(Utc::now() - TimeDelta::days(1));
        }
        let draft = builder.build().unwrap();
        ids.push(draft.id());
        store.append_audit_records(&[draft]).await.unwrap();
        tokio::time::timeout(
            Duration::from_secs(20),
            store.audit_wait_sealed(&scope, &partition, *ids.last().unwrap()),
        )
        .await
        .expect("fixture record was not sealed")
        .unwrap();
    }
    service.shutdown(Duration::from_secs(15)).await.unwrap();
    let blocks = store
        .audit_blocks(&scope, &partition, None, 10)
        .await
        .unwrap();
    assert_eq!(blocks.len(), 2);

    if !matches!(attack, Attack::BackdatedInsert) {
        let output = verify(store, password, &key).await;
        assert!(
            output.status.success(),
            "pristine audit CLI verification failed: {}",
            diagnostic(&output, password)
        );
        let report: VerificationReport = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            (report.blocks, report.records, report.clock_findings),
            (2, 2, 0)
        );
        assert_eq!(report.checkpoint, blocks[1].checkpoint());
    }

    let record = veoveo_platform_store::audit::record_id(&partition, ids[0]);
    let block = RecordId::new(
        "audit_block",
        Array::from(vec![
            partition.storage_key().into_value(),
            (blocks[0].head.sequence.get() as i64).into_value(),
        ]),
    );
    let statement = match attack {
        Attack::ChangedRecord => {
            "DEFINE FIELD OVERWRITE draft ON audit_record TYPE object FLEXIBLE;
            DEFINE FIELD OVERWRITE recorded_at ON audit_record TYPE datetime DEFAULT ALWAYS time::now();
            UPDATE $record SET draft.latency_ms = 17;"
        }
        Attack::DeletedRecord => "DELETE $record;",
        Attack::DeletedBlock => "DELETE $block;",
        Attack::ForgedSignature => {
            "DEFINE FIELD OVERWRITE block ON audit_block TYPE object FLEXIBLE;
            UPDATE $block SET block.signature = $signature;"
        }
        Attack::BackdatedInsert => "RETURN NONE;",
    };
    // Only the owned root client weakens READONLY fields to model a database administrator.
    store
        .client()
        .query(statement)
        .bind(("record", record))
        .bind(("block", block))
        .bind((
            "signature",
            serde_json::to_value(AuditSignature::from_bytes([0; 64])).unwrap(),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    let output = verify(store, password, &key).await;
    assert!(
        !output.status.success(),
        "{attack:?} was accepted by audit verify"
    );
    let error = diagnostic(&output, password);
    let expected = match attack {
        Attack::ChangedRecord => "record hash does not match",
        Attack::DeletedRecord => "audit row identity or document is inconsistent",
        Attack::DeletedBlock => "missing or mismatched link",
        Attack::ForgedSignature => "signature is invalid",
        Attack::BackdatedInsert => "BackdatedRecord",
    };
    assert!(
        error.contains(expected),
        "{attack:?}: expected {expected:?}, received {error}"
    );
    if matches!(attack, Attack::BackdatedInsert) {
        let report: VerificationReport = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            (report.blocks, report.records, report.clock_findings),
            (2, 2, 1)
        );
    }
    println!("{{\"audit_cli\":\"{attack:?}\",\"detected\":true}}");
}

#[tokio::test]
async fn public_verify_detects_root_tampering_and_backdated_inserts() {
    tokio::time::timeout(Duration::from_secs(240), async {
        let password = SecretString::from(format!(
            "{}{}",
            uuid::Uuid::now_v7().simple(),
            uuid::Uuid::now_v7().simple()
        ));
        let (_container, endpoint) = container::Container::start(
            container::Docker::default(),
            "rocksdb:/tmp/veoveo-test.db",
            password.expose_secret(),
        )
        .await
        .unwrap();
        for attack in [
            Attack::ChangedRecord,
            Attack::DeletedRecord,
            Attack::DeletedBlock,
            Attack::ForgedSignature,
            Attack::BackdatedInsert,
        ] {
            let store = connect(&endpoint, &password).await;
            qualify(&store, &password, attack).await;
        }
    })
    .await
    .expect("audit CLI acceptance exceeded 240 seconds");
}
