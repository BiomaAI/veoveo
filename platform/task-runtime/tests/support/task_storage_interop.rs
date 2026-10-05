//! The existing Python fixture owns this database and invokes the built Rust test.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeSet, path::PathBuf, time::Duration};
use veoveo_platform_store::{PlatformStore, StoreAuthLevel, StoreConfig, StoreCredentials};
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskRuntime};
use veoveo_types::TaskId;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExchangeConfig {
    endpoint: String,
    namespace: String,
    database: String,
    username: String,
    password: String,
    server: String,
    python_tasks: Vec<ExchangeTask>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExchangeTask {
    task_id: TaskId,
    owner: TaskOwner,
    request: Value,
    poll_interval_ms: u64,
}
#[derive(Serialize)]
struct ExchangeOutput {
    rust_tasks: Vec<ExchangeTask>,
}

pub async fn exchange() -> Result<()> {
    let config_path = PathBuf::from(
        std::env::var("VEOVEO_TEST_TASK_INTEROP_CONFIG")
            .context("SDK-driven Task interoperability requires explicit fixture configuration")?,
    );
    let output_path = PathBuf::from(
        std::env::var("VEOVEO_TEST_TASK_INTEROP_OUTPUT")
            .context("SDK-driven Task interoperability requires an explicit output path")?,
    );
    let config: ExchangeConfig = serde_json::from_slice(&std::fs::read(config_path)?)
        .context("invalid Task interoperability fixture configuration")?;
    let store = PlatformStore::connect(
        StoreConfig::builder(
            &config.endpoint,
            config.namespace,
            config.database,
            StoreCredentials::new(StoreAuthLevel::Database, config.username, config.password),
        )
        .build()?,
    )
    .await?;
    let runtime = TaskRuntime::new(store, config.server.clone(), "rust-interop");
    ensure!(
        config.python_tasks.len() == 3,
        "interop fixture requires three provenance controls"
    );
    let mut rust_tasks = Vec::new();
    for expected in config.python_tasks {
        let observed = runtime
            .get(expected.task_id)
            .await?
            .context("Python-written Task missing")?;
        ensure!(
            observed.owner == expected.owner,
            "Python-written owner snapshot differs"
        );
        ensure!(
            observed.request == expected.request,
            "Python-written opaque input differs"
        );
        ensure!(
            observed.poll_interval_ms == Some(expected.poll_interval_ms),
            "Python-written metadata differs"
        );
        let claimed = runtime
            .claim(expected.task_id, Duration::from_secs(60))
            .await?;
        ensure!(
            claimed.snapshot.owner == expected.owner,
            "cross-writer claim changed owner snapshot"
        );
        ensure!(
            claimed.snapshot.request == expected.request,
            "cross-writer claim changed opaque input"
        );
        ensure!(
            claimed.snapshot.poll_interval_ms == Some(expected.poll_interval_ms),
            "cross-writer claim changed metadata"
        );
        let created = runtime
            .create(CreateTask {
                task_id: TaskId::new(),
                owner: expected.owner,
                server: config.server.clone(),
                task_type: "interop".parse()?,
                request: expected.request,
                recovery_class: RecoveryClass::Resume,
                idempotency_key: None,
                ttl_ms: None,
                poll_interval_ms: Some(expected.poll_interval_ms),
                retention_pins: BTreeSet::new(),
            })
            .await?
            .snapshot;
        rust_tasks.push(ExchangeTask {
            task_id: created.task_id,
            owner: created.owner,
            request: created.request,
            poll_interval_ms: created
                .poll_interval_ms
                .context("Rust-created polling metadata missing")?,
        });
    }
    std::fs::write(
        output_path,
        serde_json::to_vec(&ExchangeOutput { rust_tasks })?,
    )?;
    Ok(())
}
