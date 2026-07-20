use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use rig_core::tool::server::ToolServer;
use veoveo_agent_kernel::{
    connection::{ConnectionEpoch, GatewayConnection, KernelHandlers},
    episode::EpisodeDriver,
    llm,
    manifest::AgentManifest,
    memory::MemoryStore,
    rrd::RrdRecorder,
    tasks::arm_watcher,
    tools::{MemoryQueryTool, MemoryWriteTool, TimelineQueryTool},
    wake::{WakeBatch, WakeBus, WakeKindExt, WakeReceiver, heartbeat, is_priority},
};
use veoveo_agent_runtime::{
    AgentInstanceId, AgentRuntime, AgentSpec, DEFAULT_AGENT_LEASE, DEFAULT_CLAIM_LEASE, json_object,
};
use veoveo_platform_store::{
    AgentElicitationId, AgentTaskId, PlatformStore, StoreConfig, StoreCredentials, WakeKind,
};

use crate::cli::RunArgs;

pub(crate) async fn cmd_run(args: RunArgs) -> Result<()> {
    if args.surreal_auth_level != "database" {
        bail!("agent requires VEOVEO_SURREAL_AUTH_LEVEL=database");
    }
    let seed_manifest = AgentManifest::load(&args.manifest)?;
    let store_config = StoreConfig::builder(
        &args.surreal_endpoint,
        &args.surreal_namespace,
        &args.surreal_database,
        StoreCredentials::database(args.surreal_username, args.surreal_password),
    )
    .build()?;
    let store = PlatformStore::connect(store_config).await?;
    let authority = store
        .automated_authority_for_oauth_client(
            &seed_manifest.agent.tenant,
            &seed_manifest.gateway.work_context,
            &seed_manifest.gateway.client_id,
        )
        .await?
        .with_context(|| {
            format!(
                "OAuth client `{}` has no membership in Work Context `{}`",
                seed_manifest.gateway.client_id, seed_manifest.gateway.work_context
            )
        })?;
    let manifest_object = json_object(serde_json::to_value(&seed_manifest)?, "agent manifest")?;
    let runtime = AgentRuntime::register(
        store,
        AgentSpec {
            tenant_key: seed_manifest.agent.tenant.clone(),
            agent_key: seed_manifest.agent.id.clone(),
            display_name: seed_manifest.agent.display_name.clone(),
            profile: seed_manifest.gateway.profile.clone(),
            authority,
            manifest: manifest_object,
            memory_database: "memory.duckdb".to_owned(),
        },
        AgentInstanceId::new(),
    )
    .await?;
    let manifest: AgentManifest = serde_json::from_value(serde_json::Value::Object(
        runtime
            .active_manifest()
            .clone()
            .into_map()
            .into_iter()
            .collect(),
    ))?;
    manifest.validate()?;
    let Some(lease) = runtime.acquire_lease(DEFAULT_AGENT_LEASE).await? else {
        bail!("another replica holds the scheduler lease for this agent");
    };
    tracing::info!(
        agent_id = manifest.agent.id,
        instance_id = %runtime.instance_id(),
        fence = lease.fence,
        "agent scheduler lease acquired"
    );

    let memory = MemoryStore::open(&args.data_dir.join("memory.duckdb"))?;
    if let Some(dir) = &manifest.migrations_dir {
        let applied = memory.run_migrations(dir)?;
        tracing::info!(applied, "domain memory migrations applied");
    }
    let rrd = Arc::new(RrdRecorder::open(
        &args.data_dir,
        &manifest.memory.rrd_dir,
        manifest.memory.segment_max_bytes,
        &manifest.agent.id,
        &memory,
        args.viewer_tee.clone(),
    )?);

    let (bus, wake_rx) = WakeBus::channel(runtime.clone(), 256);
    let waiters = Arc::new(Mutex::new(HashMap::new()));
    let handlers = KernelHandlers {
        bus: bus.clone(),
        runtime: runtime.clone(),
        waiters,
        elicitation_grace: Duration::from_secs(manifest.schedule.elicitation_grace_s),
    };

    let tool_server_handle = ToolServer::new().run();
    tool_server_handle
        .add_tool(MemoryQueryTool::new(memory.clone()))
        .await;
    tool_server_handle
        .add_tool(MemoryWriteTool::new(
            memory.clone(),
            rrd.clone(),
            manifest.memory.memory_write_tables.clone(),
        ))
        .await;
    tool_server_handle
        .add_tool(TimelineQueryTool::new(rrd.clone()))
        .await;

    let agent = llm::build_agent(&manifest, tool_server_handle.clone())?;
    let (mut connection, epoch_rx) =
        GatewayConnection::connect(manifest.clone(), tool_server_handle, handlers)
            .await
            .context("connecting to the gateway")?;
    let schedule = manifest.schedule.clone();
    let budgets = manifest.budgets.clone();
    let driver = EpisodeDriver::new(manifest, agent, runtime.clone(), memory.clone(), rrd);

    if let Some(prompt) = args.prompt {
        driver
            .run_episode(&mut connection, "boot_prompt", &prompt, &[])
            .await?;
    }
    if args.halt_after_episode {
        runtime.release_lease().await?;
        return Ok(());
    }

    spawn_heartbeat(bus.clone(), schedule.heartbeat_interval_s);
    let mut receiver = WakeReceiver::new(
        wake_rx,
        runtime.clone(),
        Duration::from_millis(schedule.wake_coalesce_window_ms),
        Duration::from_secs(schedule.min_wake_interval_s),
        DEFAULT_CLAIM_LEASE,
    );
    let mut task_scan = tokio::time::interval(Duration::from_secs(1));
    task_scan.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut watchers = HashMap::new();
    arm_available_tasks(&runtime, &bus, &epoch_rx, &mut watchers).await?;

    let (lease_lost_tx, mut lease_lost_rx) = tokio::sync::watch::channel(false);
    {
        let runtime = runtime.clone();
        tokio::spawn(async move {
            let mut renew = tokio::time::interval(Duration::from_secs(10));
            renew.tick().await;
            loop {
                renew.tick().await;
                if let Err(error) = runtime.renew_lease(DEFAULT_AGENT_LEASE).await {
                    tracing::error!(%error, "agent scheduler lease renewal failed");
                    lease_lost_tx.send_replace(true);
                    break;
                }
            }
        });
    }

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                for (_, watcher) in watchers.drain() {
                    watcher.abort();
                }
                runtime.release_lease().await?;
                return Ok(());
            }
            changed = lease_lost_rx.changed() => {
                if changed.is_err() || *lease_lost_rx.borrow() {
                    for (_, watcher) in watchers.drain() {
                        watcher.abort();
                    }
                    bail!("agent scheduler lease was lost");
                }
            }
            _ = task_scan.tick() => {
                arm_available_tasks(&runtime, &bus, &epoch_rx, &mut watchers).await?;
            }
            batch = receiver.next_batch() => {
                let batch = batch?;
                if let Some(max) = budgets.hourly_max_episodes
                    && !batch.wakes.iter().any(is_priority)
                {
                    let started = runtime
                        .episodes_started_since(chrono::Utc::now() - chrono::TimeDelta::hours(1))
                        .await?;
                    if started as u64 >= max {
                        receiver
                            .defer_batch(&batch, Duration::from_secs(30), "hourly episode budget reached")
                            .await?;
                        continue;
                    }
                }
                match run_batch_episode(&driver, &mut connection, &runtime, &batch).await {
                    Ok(()) => receiver.note_episode_finished(),
                    Err(error) => {
                        tracing::error!(%error, "wake episode failed; durable wakes requeued");
                        receiver.retry_batch(&batch, &error.to_string()).await?;
                    }
                }
                arm_available_tasks(&runtime, &bus, &epoch_rx, &mut watchers).await?;
            }
        }
    }
}

fn spawn_heartbeat(bus: WakeBus, interval_s: u64) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_s));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Err(error) = bus.send(heartbeat()).await {
                tracing::error!(%error, "persisting heartbeat wake failed");
            }
        }
    });
}

async fn arm_available_tasks(
    runtime: &AgentRuntime,
    bus: &WakeBus,
    epoch_rx: &tokio::sync::watch::Receiver<ConnectionEpoch>,
    watchers: &mut HashMap<AgentTaskId, tokio::task::JoinHandle<()>>,
) -> Result<()> {
    watchers.retain(|_, watcher| !watcher.is_finished());
    for task in runtime.claim_tasks(64, DEFAULT_CLAIM_LEASE).await? {
        let id = task.agent_task_id;
        let watcher = arm_watcher(runtime.clone(), bus.clone(), epoch_rx.clone(), task);
        watchers.insert(id, watcher);
    }
    Ok(())
}

async fn run_batch_episode(
    driver: &EpisodeDriver,
    connection: &mut GatewayConnection,
    runtime: &AgentRuntime,
    batch: &WakeBatch,
) -> Result<()> {
    let wake_note = batch
        .wakes
        .iter()
        .map(|wake| wake.kind.note_name())
        .collect::<Vec<_>>()
        .join("+");
    let wake_body = render_wake_body(runtime, batch).await?;
    driver
        .run_episode(connection, &wake_note, &wake_body, &batch.ids())
        .await?;
    Ok(())
}

async fn render_wake_body(runtime: &AgentRuntime, batch: &WakeBatch) -> Result<String> {
    let mut parts = Vec::new();
    let results = runtime.unconsumed_task_results().await?;
    for wake in &batch.wakes {
        match wake.kind {
            WakeKind::TaskResult => {
                let Some(task_id) = wake
                    .payload
                    .as_map()
                    .get("task_id")
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let Some(result) = results
                    .iter()
                    .find(|result| result.task_id.to_string() == task_id)
                else {
                    continue;
                };
                parts.push(format!(
                    "Background task update: `{}` task {} {} with result:\n\n{}",
                    result.tool_name,
                    result.task_id,
                    if result.is_error {
                        "failed"
                    } else {
                        "completed"
                    },
                    serde_json::to_string(&result.result)?,
                ));
            }
            WakeKind::ResourceChanged => {
                if let Some(uri) = wake
                    .payload
                    .as_map()
                    .get("uri")
                    .and_then(serde_json::Value::as_str)
                {
                    parts.push(format!("Resource updated: {uri}"));
                }
            }
            WakeKind::Timer => {
                let name = wake
                    .payload
                    .as_map()
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("timer");
                if wake
                    .payload
                    .as_map()
                    .get("timer_kind")
                    .and_then(serde_json::Value::as_str)
                    == Some("heartbeat")
                {
                    parts.push(
                        "Scheduled heartbeat. Review pending work and act only when needed."
                            .to_owned(),
                    );
                } else {
                    parts.push(format!("Scheduled timer `{name}` fired."));
                }
            }
            WakeKind::OperatorMessage => {
                if let Some(text) = wake
                    .payload
                    .as_map()
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                {
                    parts.push(format!("Operator message: {text}"));
                }
            }
            WakeKind::Elicitation => {
                if wake
                    .payload
                    .as_map()
                    .get("phase")
                    .and_then(serde_json::Value::as_str)
                    == Some("answered")
                {
                    if let Some(id) = wake
                        .payload
                        .as_map()
                        .get("elicitation_id")
                        .and_then(serde_json::Value::as_str)
                        && let Ok(id) = AgentElicitationId::from_str(id)
                    {
                        let elicitation = runtime.elicitation(id).await?;
                        parts.push(format!(
                            "Operator answered elicitation `{id}`: {}",
                            serde_json::to_string(&elicitation.answer)?
                        ));
                    }
                } else {
                    parts.push("A tool requested operator input. The durable elicitation is awaiting an authorized answer.".to_owned());
                }
            }
        }
    }
    Ok(parts.join("\n\n"))
}
