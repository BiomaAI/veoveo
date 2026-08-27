use std::net::TcpListener;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use re_grpc_server::{MemoryLimit, ServerOptions, shutdown};
use veoveo_recording_hub::{DatasetName, DatasetRoute, Spooler, SpoolerConfig, run_blocking};
use veoveo_rrd::projection::{
    ProjectionQuery, ProjectionSampling, ProjectionSparseFill, write_arrow_projection,
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
        capture_layer_max_bytes: 192 * 1024 * 1024,
        capture_layer_max_age_s: 3_600,
        recording_idle_timeout_s: 15,
        flush_interval_ms: 10,
        fsync_on_flush: true,
        live_queue_limit_bytes: 256 * 1024 * 1024,
        blueprint_max_bytes: veoveo_recording_protocol::DEFAULT_MAXIMUM_BLUEPRINT_BYTES,
        blueprint_max_messages: veoveo_recording_protocol::DEFAULT_MAXIMUM_BLUEPRINT_MESSAGES,
        blueprint_max_revisions: veoveo_recording_protocol::DEFAULT_MAXIMUM_BLUEPRINT_REVISIONS,
    };
    let flush_interval = config.flush_interval();
    let max_age = config.capture_layer_max_age();
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

    let mut layers = Vec::new();
    collect_rrd_layers(&spool_dir.join("world"), &mut layers)?;
    layers.sort();
    let output = temp.path().join("sumo-smoke.arrow");
    let query = write_arrow_projection(
        &layers,
        &ProjectionQuery {
            entity_paths: vec!["/world/sumo/vehicle_count".to_owned()],
            component_ids: vec!["Scalars:scalars".to_owned()],
            timeline: "tick".to_owned(),
            sampling: ProjectionSampling::Range {
                start: 0,
                end: i64::from(steps),
            },
            sparse_fill: ProjectionSparseFill::None,
            maximum_entities: 1,
            maximum_columns: 1,
            maximum_samples: usize::try_from(steps)?.saturating_add(1),
            maximum_rows: u64::from(steps).saturating_add(1),
            maximum_bytes: 4 * 1024 * 1024,
        },
        &output,
    )?;
    ensure!(
        query.row_count == u64::from(steps),
        "expected {steps} durable SUMO Arrow rows, got {}",
        query.row_count
    );
    println!("sumo push smoke ok: {steps} typed world frames persisted and projected to Arrow");
    Ok(())
}

pub(crate) async fn sumo_verify(conformance: &Path, context: &str) -> Result<()> {
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
    let layers = run_checked(
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
            "find".into(),
            "/recordings".into(),
            "-type".into(),
            "f".into(),
            "-name".into(),
            "*.rrd".into(),
            "-size".into(),
            "+0c".into(),
            "-print".into(),
        ],
        [],
    )?;
    ensure!(
        layers.lines().any(|path| path.contains("sumo-live")),
        "Recording Hub did not retain a nonempty live SUMO RRD layer: {layers}"
    );

    println!("sumo verify ok: live k3d TraCI, authenticated MCP task/actuation, and durable world");
    Ok(())
}

fn collect_rrd_layers(root: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(root)
        .with_context(|| format!("reading recording layer directory {}", root.display()))?
    {
        let path = entry?.path();
        if path.is_dir() {
            collect_rrd_layers(&path, output)?;
        } else if path.extension().is_some_and(|extension| extension == "rrd") {
            output.push(path);
        }
    }
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
