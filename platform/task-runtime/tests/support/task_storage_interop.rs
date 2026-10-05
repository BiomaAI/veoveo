//! The existing Python fixture owns this database and invokes the built Rust test.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeSet, path::PathBuf, time::Duration};
use veoveo_platform_store::{PlatformStore, StoreAuthLevel, StoreConfig, StoreCredentials};
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskFailure, TaskInputRequest, TaskOwner, TaskRuntime,
    TaskTransition,
};
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
    python_failures: Vec<FailureTask>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExchangeTask {
    task_id: TaskId,
    owner: TaskOwner,
    request: Value,
    poll_interval_ms: u64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FailureTask {
    task_id: TaskId,
    owner: TaskOwner,
    failure: TaskFailure,
}
#[derive(Serialize)]
struct ExchangeOutput {
    rust_tasks: Vec<ExchangeTask>,
    rust_failures: Vec<FailureTask>,
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
    let mut rust_failures = Vec::new();
    for expected in config.python_failures {
        let observed = runtime
            .get(expected.task_id)
            .await?
            .context("Python failure Task missing")?;
        ensure!(
            observed.error.as_ref() == Some(&expected.failure),
            "Python failure envelope differs"
        );
        let created = runtime
            .create(CreateTask {
                task_id: TaskId::new(),
                owner: expected.owner,
                server: config.server.clone(),
                task_type: "interop".parse()?,
                request: Value::Null,
                recovery_class: RecoveryClass::Resume,
                idempotency_key: None,
                ttl_ms: None,
                poll_interval_ms: None,
                retention_pins: BTreeSet::new(),
            })
            .await?
            .snapshot;
        let claimed = runtime
            .claim(created.task_id, Duration::from_secs(60))
            .await?
            .snapshot;
        runtime
            .transition_if_current(&claimed, TaskTransition::Failed(expected.failure.clone()))
            .await?;
        rust_failures.push(FailureTask {
            task_id: created.task_id,
            owner: created.owner,
            failure: expected.failure,
        });
    }
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
        let inputs = runtime.outstanding_inputs(expected.task_id).await?;
        ensure!(
            inputs
                .get("interop")
                .is_some_and(|input| input.method == "owner/input"
                    && input.params.get("opaque") == Some(&serde_json::json!([null, u64::MAX]))),
            "Python input envelope differs"
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
        runtime
            .claim(created.task_id, Duration::from_secs(60))
            .await?;
        runtime
            .request_input(
                created.task_id,
                "interop",
                TaskInputRequest {
                    method: "owner/input".into(),
                    params: std::collections::BTreeMap::from([(
                        "opaque".into(),
                        serde_json::json!([null, u64::MAX]),
                    )]),
                },
            )
            .await?;
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
        serde_json::to_vec(&ExchangeOutput {
            rust_tasks,
            rust_failures,
        })?,
    )?;
    Ok(())
}
