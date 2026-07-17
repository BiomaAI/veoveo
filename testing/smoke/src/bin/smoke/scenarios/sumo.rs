use std::net::TcpListener;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use re_grpc_server::{MemoryLimit, ServerOptions, shutdown};
use veoveo_recording_hub::{
    DatasetName, DatasetRoute, Spooler, SpoolerConfig, query_tree, run_blocking,
};
use veoveo_sumo_mcp::{
    driver::{FakeSimDriver, SimDriver},
    recording::RecordingPublisher,
};

use super::*;

pub(crate) async fn sumo_push(steps: u32) -> Result<()> {
    ensure!(steps > 0, "steps must be positive");
    let temp = tempfile::tempdir()?;
    let spool_dir = temp.path().join("spool");
    let port = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
    let bind = format!("127.0.0.1:{port}").parse()?;
    let config = SpoolerConfig {
        bind,
        spool_dir: spool_dir.clone(),
        datasets: vec![DatasetRoute {
            dataset: DatasetName::new("world")?,
            application_id_prefix: "veoveo-sumo".to_owned(),
        }],
        segment_max_bytes: 192 * 1024 * 1024,
        segment_max_age_s: 3_600,
        recording_idle_timeout_s: 15,
        flush_interval_ms: 10,
        fsync_on_flush: true,
        live_queue_limit_bytes: 256 * 1024 * 1024,
        rerun_bin: None,
    };
    let flush_interval = config.flush_interval();
    let max_age = config.segment_max_age();
    let (shutdown_signal, shutdown_handle) = shutdown::shutdown();
    let options = ServerOptions {
        memory_limit: MemoryLimit::from_bytes(config.live_queue_limit_bytes),
        ..Default::default()
    };
    let (receiver, _server) = re_grpc_server::spawn_with_recv(bind, options, shutdown_handle);
    let stopping = Arc::new(AtomicBool::new(false));
    let drain_stopping = stopping.clone();
    let drain = tokio::task::spawn_blocking(move || {
        run_blocking(
            Spooler::new(config)?,
            receiver,
            drain_stopping,
            flush_interval,
            max_age,
        )
    });

    tokio::time::sleep(Duration::from_millis(150)).await;
    let proxy = format!("rerun+http://127.0.0.1:{port}/proxy");
    let mut publisher = RecordingPublisher::connect(proxy, "sumo-smoke")?;
    let mut driver = FakeSimDriver::new(6, 3, (10, 20));
    publisher.publish_network(&driver.network_geometry()?)?;
    for _ in 0..steps {
        publisher.publish(&driver.state()?)?;
        driver.step(1)?;
    }
    publisher.flush()?;
    drop(publisher);
    tokio::time::sleep(Duration::from_millis(400)).await;

    stopping.store(true, Ordering::SeqCst);
    shutdown_signal.stop();
    drain.await.context("SUMO recording drain panicked")??;

    let query = query_tree(
        &spool_dir.join("world"),
        "/world/sumo/**",
        "tick",
        u64::from(steps) + 1,
    )?;
    ensure!(
        query.rows_by_recording.get("sumo-smoke") == Some(&u64::from(steps)),
        "expected {steps} durable SUMO rows, got {:?}",
        query.rows_by_recording
    );
    println!("sumo push smoke ok: {steps} typed world frames persisted and queried");
    Ok(())
}

pub(crate) async fn sumo_verify(conformance: &Path, context: &str) -> Result<()> {
    if conformance == Path::new("target/debug/conformance") {
        run_checked(
            Path::new("cargo"),
            [
                "build".into(),
                "-p".into(),
                "veoveo-mcp-conformance".into(),
                "--bin".into(),
                "conformance".into(),
            ],
            [],
        )?;
    }
    assert_executable(conformance)?;

    run_checked(
        Path::new("kubectl"),
        ["--context".into(), context.into(), "cluster-info".into()],
        [],
    )
    .context("SUMO verification requires the active k3d cluster")?;

    let mcp_url = "http://127.0.0.1:8895/sumo/mcp";
    let health_url = "http://127.0.0.1:8895/sumo/healthz";
    let client = reqwest::Client::new();
    let mut ready = false;
    for _ in 0..300 {
        if client
            .get(health_url)
            .send()
            .await
            .is_ok_and(|response| response.status() == StatusCode::OK)
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    if !ready {
        let logs = run_checked(
            Path::new("kubectl"),
            [
                "--context".into(),
                context.into(),
                "-n".into(),
                "veoveo".into(),
                "logs".into(),
                "deployment/sumo-mcp".into(),
                "--tail=200".into(),
            ],
            [],
        )
        .unwrap_or_else(|error| error.to_string());
        bail!("SUMO MCP did not become healthy\n{logs}");
    }

    assert_http_get_status(mcp_url, None, StatusCode::UNAUTHORIZED).await?;
    let auth = [(
        "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
        INTERNAL_SIGNING_KEY_DER_B64.into(),
    )];
    let base = [
        "--url",
        mcp_url,
        "--scheme",
        "sumo",
        "--internal-server",
        "sumo",
    ];
    let info = run_conformance(conformance, &base, &["info"], auth.clone())?;
    contains(&info, "run_batch")?;
    let resources = run_conformance(conformance, &base, &["resources"], auth.clone())?;
    contains(&resources, "sumo://congestion")?;

    let state = run_conformance(
        conformance,
        &base,
        &["call", "--tool-name", "query_state", "--arguments", "{}"],
        auth.clone(),
    )?;
    let state = structured_output(&state)?;
    ensure!(
        state.get("vehicle_count").and_then(Value::as_u64).is_some(),
        "query_state did not return a typed vehicle_count: {state}"
    );

    let scenario = run_conformance(
        conformance,
        &base,
        &[
            "call",
            "--tool-name",
            "describe_scenario",
            "--arguments",
            "{}",
        ],
        auth.clone(),
    )?;
    let scenario = structured_output(&scenario)?;
    let edge = scenario
        .get("edges")
        .and_then(Value::as_array)
        .and_then(|edges| edges.first())
        .and_then(Value::as_str)
        .context("live SUMO scenario exposed no edges")?;
    let edge_request = serde_json::json!({"edge_id": edge, "speed_mps": 8.0}).to_string();
    let actuation = run_conformance(
        conformance,
        &base,
        &[
            "call",
            "--tool-name",
            "set_edge_speed",
            "--arguments",
            &edge_request,
        ],
        auth.clone(),
    )?;
    ensure!(
        structured_output(&actuation)?
            .get("applied")
            .and_then(Value::as_bool)
            == Some(true),
        "live SUMO actuation was not applied"
    );

    let task = run_conformance(
        conformance,
        &base,
        &[
            "task-call",
            "--tool-name",
            "run_batch",
            "--arguments",
            r#"{"steps":50}"#,
        ],
        auth,
    )?;
    let task_result = structured_output(&task)?;
    ensure!(
        task_result.get("steps_advanced").and_then(Value::as_u64) == Some(50),
        "run_batch task did not advance 50 steps: {task_result}"
    );

    tokio::time::sleep(Duration::from_secs(2)).await;
    let pod = run_checked(
        Path::new("kubectl"),
        [
            "--context".into(),
            context.into(),
            "-n".into(),
            "veoveo".into(),
            "get".into(),
            "pod".into(),
            "-l".into(),
            "app.kubernetes.io/component=recording".into(),
            "-o".into(),
            "jsonpath={.items[0].metadata.name}".into(),
        ],
        [],
    )?;
    let query = run_checked(
        Path::new("kubectl"),
        [
            "--context".into(),
            context.into(),
            "-n".into(),
            "veoveo".into(),
            "exec".into(),
            pod.trim().into(),
            "-c".into(),
            "recording-hub".into(),
            "--".into(),
            "hub-query".into(),
            "--root".into(),
            "/recordings/world".into(),
            "--entities".into(),
            "/world/sumo/**".into(),
            "--timeline".into(),
            "tick".into(),
            "--max-rows".into(),
            "0".into(),
        ],
        [],
    )?;
    let query: Value = serde_json::from_str(query.trim())?;
    let rows = query
        .get("rows_by_recording")
        .and_then(Value::as_object)
        .context("hub query omitted rows_by_recording")?;
    ensure!(
        rows.iter().any(|(recording, count)| {
            recording.starts_with("sumo-live") && count.as_u64().is_some_and(|count| count > 0)
        }),
        "Recording Hub did not retain the live SUMO world: {rows:?}"
    );

    println!("sumo verify ok: live k3d TraCI, authenticated MCP task/actuation, and durable world");
    Ok(())
}

fn structured_output(output: &str) -> Result<Value> {
    let raw = output
        .lines()
        .find_map(|line| line.strip_prefix("structured: "))
        .context("conformance output omitted structured content")?;
    serde_json::from_str(raw).context("parsing conformance structured content")
}

fn run_conformance<const N: usize>(
    conformance: &Path,
    base: &[&str],
    command: &[&str],
    environment: [(&'static str, OsString); N],
) -> Result<String> {
    let arguments = base
        .iter()
        .chain(command)
        .map(OsString::from)
        .collect::<Vec<_>>();
    run_checked(conformance, arguments, environment)
}
