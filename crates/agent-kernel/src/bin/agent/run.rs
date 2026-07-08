use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result};
use rig_core::tool::server::ToolServer;
use veoveo_agent_kernel::{
    connection::{ConnectionEpoch, GatewayConnection, KernelHandlers},
    episode::EpisodeDriver,
    ledger::KernelLedger,
    llm,
    manifest::AgentManifest,
    operator,
    rrd::RrdRecorder,
    tasks::arm_watcher,
    tools::{MemoryQueryTool, MemoryWriteTool, TimelineQueryTool},
    wake::{WakeBatch, WakeBus, WakeEvent, WakeKind, WakeReceiver, mark_batch_handled},
};

use crate::cli::RunArgs;

pub(crate) async fn cmd_run(args: RunArgs) -> Result<()> {
    let manifest = AgentManifest::load(&args.manifest)?;
    let agent_id = manifest.agent.id.clone();
    tracing::info!(agent_id, "agent booting");

    let ledger = KernelLedger::open(&args.data_dir.join("memory.duckdb"))?;
    let crashed = ledger.mark_inflight_episodes_crashed()?;
    if crashed > 0 {
        tracing::warn!(crashed, "recovered from a crash mid-episode");
    }
    if let Some(dir) = &manifest.migrations_dir {
        let applied = ledger.run_migrations(dir)?;
        tracing::info!(applied, "domain migrations applied");
    }
    let rrd = Arc::new(RrdRecorder::open(
        &args.data_dir,
        &manifest.memory.rrd_dir,
        manifest.memory.segment_max_bytes,
        &agent_id,
        &ledger,
        args.viewer_tee.clone(),
    )?);

    let (bus, wake_rx) = WakeBus::channel(256);
    let waiters = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let handlers = KernelHandlers {
        bus: bus.clone(),
        ledger: ledger.clone(),
        waiters: waiters.clone(),
        elicitation_grace: Duration::from_secs(manifest.schedule.elicitation_grace_s),
    };

    let tool_server_handle = ToolServer::new().run();
    tool_server_handle
        .add_tool(MemoryQueryTool::new(ledger.clone()))
        .await
        .context("registering memory_query")?;
    tool_server_handle
        .add_tool(MemoryWriteTool::new(
            ledger.clone(),
            rrd.clone(),
            manifest.memory.memory_write_tables.clone(),
        ))
        .await
        .context("registering memory_write")?;
    tool_server_handle
        .add_tool(TimelineQueryTool::new(rrd.clone()))
        .await
        .context("registering timeline_query")?;

    let agent = llm::build_agent(&manifest, tool_server_handle.clone())?;
    let (mut connection, epoch_rx) =
        GatewayConnection::connect(manifest.clone(), tool_server_handle, handlers)
            .await
            .context("connecting to the gateway")?;
    let schedule = manifest.schedule.clone();
    let budgets = manifest.budgets.clone();
    let driver = EpisodeDriver::new(manifest, agent, ledger.clone(), rrd.clone());

    if let Some(prompt) = args.prompt {
        driver
            .run_episode(&mut connection, "boot_prompt", &prompt)
            .await?;
    }
    if args.halt_after_episode {
        tracing::info!("halt-after-episode set; detached tasks resume on next boot");
        return Ok(());
    }

    operator::serve(
        ledger.clone(),
        bus.clone(),
        waiters,
        &args.data_dir,
        args.operator_port,
    )
    .await?;

    // Heartbeat: bounded silence — every tick wakes an episode.
    {
        let bus = bus.clone();
        let interval = Duration::from_secs(schedule.heartbeat_interval_s);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ticker.tick().await; // the immediate first tick is boot's concern
            loop {
                ticker.tick().await;
                bus.send(WakeEvent::heartbeat()).await;
            }
        });
    }

    let mut armed: HashSet<String> = HashSet::new();
    arm_unwatched(&ledger, &bus, &epoch_rx, &mut armed)?;
    for resolved in ledger.unconsumed_results()? {
        bus.send(WakeEvent::task_settled(&resolved.task_id)).await;
    }

    let mut receiver = WakeReceiver::new(
        wake_rx,
        ledger.clone(),
        Duration::from_millis(schedule.wake_coalesce_window_ms),
        Duration::from_secs(schedule.min_wake_interval_s),
    );

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("shutdown requested");
                return Ok(());
            }
            batch = receiver.next_batch() => {
                let Some(batch) = batch else { return Ok(()) };
                // Window budget: hold low-priority work when the hour is spent.
                if let Some(max) = budgets.hourly_max_episodes
                    && !batch.wakes.iter().any(|event| event.kind.priority())
                {
                    let started = ledger.episodes_started_this_hour()?;
                    if started as u64 >= max {
                        tracing::warn!(started, max, "hourly episode budget reached; wakes deferred");
                        for event in &batch.wakes {
                            bus.send(event.clone()).await;
                        }
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        continue;
                    }
                }
                match run_batch_episode(&driver, &mut connection, &ledger, &batch).await {
                    Ok(episode_id) => {
                        if let Err(err) = mark_batch_handled(&ledger, &batch, episode_id) {
                            tracing::error!(%err, "marking wakes handled failed");
                        }
                    }
                    Err(err) => tracing::error!(%err, "wake episode failed"),
                }
                receiver.note_episode_finished();
                arm_unwatched(&ledger, &bus, &epoch_rx, &mut armed)?;
            }
        }
    }
}

/// Watchers for every ledger task that has none yet — covers boot recovery,
/// detach at episode end, and provisional rows from terminated runs.
fn arm_unwatched(
    ledger: &KernelLedger,
    bus: &WakeBus,
    epoch_rx: &tokio::sync::watch::Receiver<ConnectionEpoch>,
    armed: &mut HashSet<String>,
) -> Result<()> {
    for task in ledger.tasks_to_watch()? {
        if armed.insert(task.task_id.clone()) {
            tracing::info!(
                task_id = task.task_id,
                tool_name = task.tool_name,
                "arming watcher"
            );
            arm_watcher(ledger.clone(), bus.clone(), epoch_rx.clone(), task);
        }
    }
    Ok(())
}

/// One episode answering a batch of wakes; returns the episode id.
async fn run_batch_episode(
    driver: &EpisodeDriver,
    connection: &mut GatewayConnection,
    ledger: &KernelLedger,
    batch: &WakeBatch,
) -> Result<uuid::Uuid> {
    let wake_note = batch
        .wakes
        .iter()
        .map(|event| event.kind.as_str())
        .collect::<Vec<_>>()
        .join("+");
    let wake_body = render_wake_body(ledger, batch)?;
    let report = driver
        .run_episode(connection, &wake_note, &wake_body)
        .await?;
    for event in &batch.wakes {
        if event.kind == WakeKind::TaskSettled
            && let Some(task_id) = event.payload.get("task_id").and_then(|id| id.as_str())
        {
            ledger.mark_task_consumed(task_id, report.episode_id)?;
            tracing::info!(task_id, episode_id = %report.episode_id, "task result consumed");
        }
    }
    Ok(report.episode_id)
}

fn render_wake_body(ledger: &KernelLedger, batch: &WakeBatch) -> Result<String> {
    let mut parts = Vec::new();
    let results = ledger.unconsumed_results()?;
    for event in &batch.wakes {
        match event.kind {
            WakeKind::TaskSettled => {
                let Some(task_id) = event.payload.get("task_id").and_then(|id| id.as_str()) else {
                    continue;
                };
                let Some(result) = results.iter().find(|result| result.task_id == task_id) else {
                    continue;
                };
                let status = if result.result_is_error {
                    "failed"
                } else {
                    "completed"
                };
                parts.push(format!(
                    "Background task update: your earlier `{tool}` dispatch (task {task_id}) \
                     has {status} with this result:\n\n{result}",
                    tool = result.tool_name,
                    result = result.result_json,
                ));
            }
            WakeKind::ResourceUpdated => {
                if let Some(uri) = event.payload.get("uri").and_then(|uri| uri.as_str()) {
                    parts.push(format!("Resource updated: {uri}"));
                }
            }
            WakeKind::Timer => {
                let name = event
                    .payload
                    .get("name")
                    .and_then(|name| name.as_str())
                    .unwrap_or("timer");
                parts.push(format!("Scheduled timer `{name}` fired."));
            }
            WakeKind::Heartbeat => {
                parts.push(
                    "Scheduled heartbeat. Review your state and pending work; act only if \
                     something needs attention, otherwise reply briefly and stop."
                        .to_string(),
                );
            }
            WakeKind::Operator => {
                if let Some(text) = event.payload.get("text").and_then(|text| text.as_str()) {
                    parts.push(format!("Operator message: {text}"));
                }
            }
            WakeKind::ElicitationPending => {
                parts.push(
                    "A background tool asked for operator input; the question is parked for \
                     the operator (see kernel.elicitations). Continue other work — the answer \
                     will wake you."
                        .to_string(),
                );
            }
            WakeKind::ElicitationAnswered => {
                if let Some(id) = event
                    .payload
                    .get("elicitation_id")
                    .and_then(|id| id.as_str())
                {
                    let rows = ledger.query_json(
                        &format!(
                            "SELECT message, state, answer_json FROM kernel.elicitations \
                             WHERE elicitation_id = '{id}'"
                        ),
                        1,
                    )?;
                    parts.push(format!(
                        "The operator answered a parked tool question: {}",
                        rows.first()
                            .map(|row| row.to_string())
                            .unwrap_or_else(|| "(answer row missing)".to_string())
                    ));
                }
            }
        }
    }
    Ok(parts.join("\n\n"))
}
