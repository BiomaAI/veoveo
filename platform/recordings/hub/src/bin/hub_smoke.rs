//! Rust process smoke harness for the Recording Hub durability boundary.

use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use re_grpc_server::{MemoryLimit, ServerOptions, shutdown};
use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::Scalars;
use veoveo_recording_hub::{
    DatasetName, DatasetRoute, SegmentReadScope, Spooler, SpoolerConfig, collect_segments,
    inspect_segment, query_tree, run_blocking,
};

#[derive(Parser)]
#[command(name = "hub-smoke", about = "Recording Hub Rust smoke scenarios")]
struct Args {
    #[command(subcommand)]
    command: SmokeCommand,
}

#[derive(Subcommand)]
enum SmokeCommand {
    All,
    RestartKill,
    CatalogRebuild,
    AgentWorld,
    RolloverBurst {
        #[arg(long, default_value_t = 1_500)]
        messages: usize,
    },
    #[command(hide = true)]
    ChildSpooler {
        #[arg(long)]
        bind: SocketAddr,
        #[arg(long)]
        spool_dir: PathBuf,
        #[arg(long)]
        ready_file: PathBuf,
        #[arg(long)]
        stop_file: PathBuf,
        #[arg(long, default_value_t = 192 * 1024 * 1024)]
        segment_max_bytes: u64,
        #[arg(long = "route")]
        routes: Vec<String>,
    },
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    match Args::parse().command {
        SmokeCommand::All => {
            restart_kill().await?;
            catalog_rebuild().await?;
            agent_world().await?;
            rollover_burst(1_500).await
        }
        SmokeCommand::RestartKill => restart_kill().await,
        SmokeCommand::CatalogRebuild => catalog_rebuild().await,
        SmokeCommand::AgentWorld => agent_world().await,
        SmokeCommand::RolloverBurst { messages } => rollover_burst(messages).await,
        SmokeCommand::ChildSpooler {
            bind,
            spool_dir,
            ready_file,
            stop_file,
            segment_max_bytes,
            routes,
        } => {
            child_spooler(
                bind,
                spool_dir,
                ready_file,
                stop_file,
                segment_max_bytes,
                routes,
            )
            .await
        }
    }
}

async fn restart_kill() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let spool = temp.path().join("spool");
    let mut first = spawn_child(&spool, 192 * 1024 * 1024, &["world="])?;
    push(&first.proxy, "sensor-suite", "restart-run", 30, false).await?;
    std::thread::sleep(Duration::from_millis(300));
    first.process.kill().context("kill -9 first spooler")?;
    first.process.wait()?;

    let mut second = spawn_child(&spool, 192 * 1024 * 1024, &["world="])?;
    push(&second.proxy, "sensor-suite", "restart-run", 20, false).await?;
    second.stop()?;
    let result = query_tree(&spool, "/**", "tick", 10_000, SegmentReadScope::Frozen)?;
    ensure!(result.rows_by_recording.get("restart-run") == Some(&50));
    let files = collect_segments(&spool, SegmentReadScope::Frozen)?;
    ensure!(
        files.len() == 2,
        "expected crash sibling pair, got {files:?}"
    );
    println!(
        "restart-kill: 50 rows recovered across {} segments",
        files.len()
    );
    Ok(())
}

async fn catalog_rebuild() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let spool = temp.path().join("spool");
    let mut child = spawn_child(&spool, 192 * 1024 * 1024, &["world="])?;
    push(&child.proxy, "sensor-suite", "catalog-run", 40, false).await?;
    child.stop()?;
    let valid = collect_segments(&spool, SegmentReadScope::Frozen)?;
    ensure!(valid.len() == 1);
    inspect_segment(&valid[0])?;

    let corrupt = valid[0].with_file_name("corrupt.rrd");
    std::fs::write(&corrupt, b"not-an-rrd")?;
    ensure!(inspect_segment(&corrupt).is_err());
    let rebuild = collect_segments(&spool, SegmentReadScope::Frozen)?
        .iter()
        .map(|path| inspect_segment(path))
        .collect::<Result<Vec<_>>>();
    ensure!(rebuild.is_err(), "catalog rebuild did not fail closed");
    std::fs::remove_file(&corrupt)?;
    let rebuilt = query_tree(&spool, "/**", "tick", 10_000, SegmentReadScope::Frozen)?;
    ensure!(rebuilt.rows_by_recording.get("catalog-run") == Some(&40));
    println!("catalog-rebuild: corruption rejected and 40 rows rebuilt");
    Ok(())
}

async fn agent_world() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let spool = temp.path().join("spool");
    let mut child = spawn_child(
        &spool,
        192 * 1024 * 1024,
        &["agents=veoveo-agent", "world="],
    )?;
    push(&child.proxy, "veoveo-sim-gnss", "world-run", 25, false).await?;
    push(&child.proxy, "veoveo-agent-pilot", "agent-run", 15, false).await?;
    child.stop()?;
    let world = query_tree(
        &spool.join("world"),
        "/**",
        "tick",
        10_000,
        SegmentReadScope::Frozen,
    )?;
    let agents = query_tree(
        &spool.join("agents"),
        "/**",
        "tick",
        10_000,
        SegmentReadScope::Frozen,
    )?;
    ensure!(world.rows_by_recording.get("world-run") == Some(&25));
    ensure!(!world.rows_by_recording.contains_key("agent-run"));
    ensure!(agents.rows_by_recording.get("agent-run") == Some(&15));
    ensure!(!agents.rows_by_recording.contains_key("world-run"));
    println!("agent-world: routed 25 world and 15 agent rows without leakage");
    Ok(())
}

async fn rollover_burst(messages: usize) -> Result<()> {
    ensure!(messages >= 100, "messages must be at least 100");
    let temp = tempfile::tempdir()?;
    let spool = temp.path().join("spool");
    let mut child = spawn_child(&spool, 4_096, &["world="])?;
    push(&child.proxy, "burst-suite", "burst-run", messages, true).await?;
    child.stop()?;
    let result = query_tree(
        &spool,
        "/**",
        "tick",
        messages as u64 + 1,
        SegmentReadScope::Frozen,
    )?;
    ensure!(result.rows_by_recording.get("burst-run") == Some(&(messages as u64)));
    let segments = collect_segments(&spool, SegmentReadScope::Frozen)?;
    ensure!(
        segments.len() > 1,
        "burst did not exercise segment rollover"
    );
    println!(
        "rollover-burst: {messages} rows preserved across {} segments",
        segments.len()
    );
    Ok(())
}

struct ChildSpooler {
    process: Child,
    proxy: String,
    stop_file: PathBuf,
}

impl ChildSpooler {
    fn stop(&mut self) -> Result<()> {
        std::fs::write(&self.stop_file, b"stop")?;
        let status = self.process.wait()?;
        ensure!(status.success(), "child spooler exited with {status}");
        Ok(())
    }
}

impl Drop for ChildSpooler {
    fn drop(&mut self) {
        if self.process.try_wait().ok().flatten().is_none() {
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }
}

fn spawn_child(spool: &Path, segment_max_bytes: u64, routes: &[&str]) -> Result<ChildSpooler> {
    let port = free_port()?;
    let ready = spool.join(format!("ready-{port}"));
    let stop = spool.join(format!("stop-{port}"));
    std::fs::create_dir_all(spool)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("child-spooler")
        .arg("--bind")
        .arg(format!("127.0.0.1:{port}"))
        .arg("--spool-dir")
        .arg(spool)
        .arg("--ready-file")
        .arg(&ready)
        .arg("--stop-file")
        .arg(&stop)
        .arg("--segment-max-bytes")
        .arg(segment_max_bytes.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    for route in routes {
        command.arg("--route").arg(route);
    }
    let process = command.spawn().context("spawn child spooler")?;
    wait_for_file(&ready, Duration::from_secs(10))?;
    Ok(ChildSpooler {
        process,
        proxy: format!("rerun+http://127.0.0.1:{port}/proxy"),
        stop_file: stop,
    })
}

async fn push(
    proxy: &str,
    application_id: &str,
    recording_id: &str,
    messages: usize,
    paced: bool,
) -> Result<()> {
    let stream = RecordingStreamBuilder::new(application_id)
        .recording_id(recording_id)
        .connect_grpc_opts(proxy.to_owned())?;
    for index in 0..messages {
        stream.set_time_sequence("tick", index as i64);
        stream.log("/world/smoke", &Scalars::new([index as f64]))?;
        if paced {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
    stream.flush_blocking()?;
    drop(stream);
    tokio::time::sleep(Duration::from_millis(400)).await;
    Ok(())
}

async fn child_spooler(
    bind: SocketAddr,
    spool_dir: PathBuf,
    ready_file: PathBuf,
    stop_file: PathBuf,
    segment_max_bytes: u64,
    routes: Vec<String>,
) -> Result<()> {
    let datasets = routes
        .iter()
        .map(|raw| {
            let (dataset, prefix) = raw
                .split_once('=')
                .with_context(|| format!("invalid route {raw}"))?;
            Ok(DatasetRoute {
                dataset: DatasetName::new(dataset)?,
                application_id_prefix: prefix.to_owned(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let config = SpoolerConfig {
        bind,
        spool_dir,
        datasets,
        segment_max_bytes,
        segment_max_age_s: 3_600,
        recording_idle_timeout_s: 3_600,
        flush_interval_ms: 10,
        fsync_on_flush: true,
        live_queue_limit_bytes: 256 * 1024 * 1024,
    };
    let flush = config.flush_interval();
    let (signal, shutdown) = shutdown::shutdown();
    let options = ServerOptions {
        memory_limit: MemoryLimit::from_bytes(config.live_queue_limit_bytes),
        ..Default::default()
    };
    let (receiver, _handle) = re_grpc_server::spawn_with_recv(bind, options, shutdown);
    let stopping = Arc::new(AtomicBool::new(false));
    let stopping_drain = stopping.clone();
    let drain = tokio::task::spawn_blocking(move || {
        run_blocking(
            Spooler::new(config)?,
            receiver,
            stopping_drain,
            flush,
            Duration::from_secs(3_600),
        )
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    std::fs::write(&ready_file, b"ready")?;
    while !stop_file.exists() {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    stopping.store(true, Ordering::SeqCst);
    signal.stop();
    drain.await.context("child drain panicked")??;
    Ok(())
}

fn wait_for_file(path: &Path, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    while !path.exists() {
        ensure!(
            started.elapsed() < timeout,
            "timed out waiting for {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn free_port() -> Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}
