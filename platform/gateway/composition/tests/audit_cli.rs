//! Public verification against root-tampered records in an owned RocksDB database.
#[path = "../../../../testing/fixtures/store/container.rs"]
mod container;
#[path = "../../../../testing/fixtures/module_lanes.rs"]
mod module_lanes;

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{TimeDelta, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::{num::NonZeroU32, process::Output, sync::Arc, time::Duration};
use surrealdb::types::{Array, RecordId, SurrealValue};
use tokio::process::Command;
use veoveo_audit::{integrity::AuditSigningKey, *};
use veoveo_computers_contract::{ComputerAuditTarget, ComputerId};
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
    .audit_targets(veoveo_gateway_catalog::audit_target_registry().unwrap())
    .build()
    .unwrap();
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            match PlatformStore::connect(config.clone()).await {
                Ok(store) => {
                    module_lanes::install(&store, Vec::new()).await.unwrap();
                    return store;
                }
                Err(error @ (StoreError::FreshInstallationRequired | StoreError::Config(_))) => {
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
    audit_command(
        store,
        password,
        &[
            "verify",
            "--installation",
            "--public-key",
            &STANDARD.encode(key.public_key()),
        ],
    )
    .await
}

async fn audit_command(store: &PlatformStore, password: &SecretString, args: &[&str]) -> Output {
    let config = store.config();
    let mut command = Command::new(env!("CARGO_BIN_EXE_gateway"));
    // A fixture must neither load the worktree .env nor inherit installation credentials.
    command
        .env_clear()
        .current_dir("/")
        .kill_on_drop(true)
        .arg("audit")
        .args(args)
        // Match the hosted environment without borrowing installation services.
        // CLI commands must not initialize server exporters or put logs on stdout.
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:9")
        .env("RUST_LOG", "info")
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
        .expect("audit command exceeded 30 seconds")
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

async fn qualify_export(store: &PlatformStore, password: &SecretString) {
    let service = AuditService::start(
        store.clone(),
        Arc::new(AuditSigningKey::from_seed(&[38; 32])),
        NonZeroU32::new(7).unwrap(),
        Default::default(),
    )
    .unwrap();
    let output = audit_command(store, password, &["export", "--installation"]).await;
    service.shutdown(Duration::from_secs(15)).await.unwrap();
    assert!(
        output.status.success(),
        "audit export failed: {}",
        diagnostic(&output, password)
    );
    let lines: Vec<AuditExportLine> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            veoveo_gateway_catalog::audit_target_registry()
                .unwrap()
                .decoder()
                .from_str(line)
                .expect("every stdout line must be audit JSONL")
        })
        .collect();
    let Some(AuditExportLine::Header {
        checkpoint: first, ..
    }) = lines.first()
    else {
        panic!("export must begin with its header")
    };
    let Some(AuditExportLine::Complete {
        records,
        checkpoint: last,
    }) = lines.last()
    else {
        panic!("export must end with its completion footer")
    };
    assert_eq!(first, last);
    assert!(*records > 0, "export includes its committed access record");
    assert_eq!(*records as usize, lines.len() - 2);
    assert!(
        lines[1..lines.len() - 1]
            .iter()
            .all(|line| matches!(line, AuditExportLine::Record { .. }))
    );
}

async fn qualify_owner_targets(store: &PlatformStore, password: &SecretString) {
    let mut response = store
        .client()
        .query(include_str!(
            "queries/audit_cli/owner_targets/computer_schema_present.surql"
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    let computer_schema_present = response.take::<Option<bool>>(1).unwrap().unwrap();
    assert!(
        !computer_schema_present,
        "read codecs must not install Computer schema"
    );

    let service = AuditService::start(
        store.clone(),
        Arc::new(AuditSigningKey::from_seed(&[39; 32])),
        NonZeroU32::new(7).unwrap(),
        Default::default(),
    )
    .unwrap();
    let registry = store.audit_targets();
    let registration = registry.registration::<ComputerAuditTarget>().unwrap();
    let computers = [ComputerId::new(), ComputerId::new()];
    let scope = AuditReadScope::new(None, true);
    let partition = AuditPartition::Installation;
    let mut records = Vec::new();
    for computer in computers {
        let target = registry.target(ComputerAuditTarget { computer }).unwrap();
        let draft = AuditDraft::builder(
            AuditRequest::background(),
            target,
            AuditDetail::Read {
                method: AuditReadMethod::ResourceRead,
            },
            AuditOutcome::Allowed,
            AuditReason::Accepted,
        )
        .build()
        .unwrap();
        store
            .append_audit_records(std::slice::from_ref(&draft))
            .await
            .unwrap();
        tokio::time::timeout(
            Duration::from_secs(20),
            store.audit_wait_sealed(&scope, &partition, draft.id()),
        )
        .await
        .expect("Computer target fixture record was not sealed")
        .unwrap();
        records.push(draft);
    }

    // Both targets have the same codec and partition; only the complete target differs.
    for (computer, draft) in computers.into_iter().zip(&records) {
        let target = serde_json::to_string(draft.target()).unwrap();
        let output = audit_command(
            store,
            password,
            &["list", "--installation", "--target", &target],
        )
        .await;
        assert!(
            output.status.success(),
            "Computer target list failed: {}",
            diagnostic(&output, password)
        );
        let page: AuditPage = registry
            .decoder()
            .from_str(std::str::from_utf8(&output.stdout).unwrap())
            .unwrap();
        assert!(page.next.is_none());
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].draft.id(), draft.id());
        assert_eq!(page.records[0].draft.target(), draft.target());
        assert_eq!(
            registration
                .get(registry, page.records[0].draft.target())
                .unwrap()
                .computer,
            computer
        );
    }

    let target = serde_json::to_string(records[0].target()).unwrap();
    let output = audit_command(
        store,
        password,
        &["export", "--installation", "--target", &target],
    )
    .await;
    assert!(
        output.status.success(),
        "Computer target export failed: {}",
        diagnostic(&output, password)
    );
    let lines: Vec<AuditExportLine> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| registry.decoder().from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    let AuditExportLine::Header {
        query,
        checkpoint: first,
        ..
    } = &lines[0]
    else {
        panic!("Computer target export must begin with a header")
    };
    assert_eq!(query.target.as_ref(), Some(records[0].target()));
    let AuditExportLine::Record { record } = &lines[1] else {
        panic!("Computer target export must contain its selected record")
    };
    assert_eq!(record.draft.id(), records[0].id());
    assert_eq!(record.draft.target(), records[0].target());
    assert_eq!(
        registration
            .get(registry, record.draft.target())
            .unwrap()
            .computer,
        computers[0]
    );
    let AuditExportLine::Complete {
        records: count,
        checkpoint: last,
    } = &lines[2]
    else {
        panic!("Computer target export must end with a completion footer")
    };
    assert_eq!(*count, 1);
    assert!(first.is_some());
    assert_eq!(first, last);

    for (case, target) in [
        ("malformed JSON", String::from(r#"{"kind":"computer""#)),
        (
            "malformed owner ID",
            String::from(r#"{"kind":"computer","computer":"invalid"}"#),
        ),
        (
            "unknown codec",
            String::from(r#"{"kind":"unregistered_owner"}"#),
        ),
        (
            "duplicate owner field",
            format!(
                r#"{{"kind":"computer","computer":"{}","computer":"{}"}}"#,
                computers[0], computers[1]
            ),
        ),
    ] {
        for command in ["list", "export"] {
            let output = audit_command(
                store,
                password,
                &[command, "--installation", "--target", &target],
            )
            .await;
            assert!(!output.status.success(), "{command} accepted {case}");
            assert!(
                output.stdout.is_empty(),
                "{command} emitted an audit result for {case}"
            );
            assert!(
                !diagnostic(&output, password).is_empty(),
                "{command} omitted its {case} diagnostic"
            );
        }
    }
    service.shutdown(Duration::from_secs(15)).await.unwrap();
    println!("{{\"audit_cli\":\"computer_targets\",\"qualified\":true}}");
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
            include_str!("queries/audit_cli/qualify/statement_1.surql")
        }
        Attack::DeletedRecord => include_str!("queries/audit_cli/qualify/statement_2.surql"),
        Attack::DeletedBlock => include_str!("queries/audit_cli/qualify/statement_3.surql"),
        Attack::ForgedSignature => {
            include_str!("queries/audit_cli/qualify/statement_4.surql")
        }
        Attack::BackdatedInsert => include_str!("queries/audit_cli/qualify/statement_5.surql"),
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
        let store = connect(&endpoint, &password).await;
        qualify_export(&store, &password).await;
    })
    .await
    .expect("audit CLI acceptance exceeded 240 seconds");
}

#[tokio::test]
async fn public_cli_filters_and_exports_registered_computer_targets_without_owner_schema() {
    tokio::time::timeout(Duration::from_secs(420), async {
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
        let store = connect(&endpoint, &password).await;
        qualify_owner_targets(&store, &password).await;
    })
    .await
    .expect("Computer target CLI acceptance exceeded 420 seconds");
}
