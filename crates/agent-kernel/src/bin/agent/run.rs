use anyhow::{Context, Result};
use rig_core::tool::server::ToolServer;
use tokio::sync::mpsc;
use veoveo_agent_kernel::{
    connection::GatewayConnection,
    episode::EpisodeDriver,
    ledger::KernelLedger,
    llm,
    manifest::AgentManifest,
    tasks::{TaskSettled, arm_watcher},
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

    let tool_server_handle = ToolServer::new().run();
    let agent = llm::build_agent(&manifest, tool_server_handle.clone())?;
    let (mut connection, epoch_rx) =
        GatewayConnection::connect(manifest.clone(), tool_server_handle)
            .await
            .context("connecting to the gateway")?;
    let driver = EpisodeDriver::new(manifest, agent, ledger.clone());

    if let Some(prompt) = args.prompt {
        driver
            .run_episode(&mut connection, "boot_prompt", prompt)
            .await?;
    }
    if args.halt_after_episode {
        tracing::info!("halt-after-episode set; detached tasks resume on next boot");
        return Ok(());
    }

    let (wake_tx, mut wake_rx) = mpsc::channel::<TaskSettled>(64);

    // Results that settled in a previous life but were never consumed become
    // immediate wakes; unsettled tasks get watchers on the fresh connection.
    for resolved in ledger.unconsumed_results()? {
        let _ = wake_tx
            .send(TaskSettled {
                task_id: resolved.task_id,
            })
            .await;
    }
    for task in ledger.tasks_to_watch()? {
        tracing::info!(
            task_id = task.task_id,
            tool_name = task.tool_name,
            "arming watcher"
        );
        arm_watcher(ledger.clone(), wake_tx.clone(), epoch_rx.clone(), task);
    }

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("shutdown requested");
                return Ok(());
            }
            settled = wake_rx.recv() => {
                let Some(settled) = settled else { return Ok(()) };
                if let Err(err) = handle_task_wake(&driver, &mut connection, &ledger, &settled).await {
                    tracing::error!(task_id = settled.task_id, %err, "task wake episode failed");
                }
            }
        }
    }
}

async fn handle_task_wake(
    driver: &EpisodeDriver,
    connection: &mut GatewayConnection,
    ledger: &KernelLedger,
    settled: &TaskSettled,
) -> Result<()> {
    let results = ledger.unconsumed_results()?;
    let Some(result) = results
        .into_iter()
        .find(|result| result.task_id == settled.task_id)
    else {
        tracing::debug!(
            task_id = settled.task_id,
            "task already consumed; wake dropped"
        );
        return Ok(());
    };

    let status = if result.result_is_error {
        "failed"
    } else {
        "completed"
    };
    let prompt = format!(
        "Background task update: your earlier `{tool}` dispatch (task {task_id}) has {status} \
         with this result:\n\n{result}\n\nContinue your objective using this result. If nothing \
         actionable remains, summarize the outcome and stop.",
        tool = result.tool_name,
        task_id = result.task_id,
        result = result.result_json,
    );
    let report = driver
        .run_episode(connection, &format!("task:{}", result.task_id), prompt)
        .await?;
    ledger.mark_task_consumed(&result.task_id, report.episode_id)?;
    tracing::info!(
        task_id = result.task_id,
        episode_id = %report.episode_id,
        "task result consumed"
    );
    Ok(())
}
