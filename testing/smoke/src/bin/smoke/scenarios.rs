use super::support::*;
use super::*;

#[path = "scenarios/agent_kernel.rs"]
mod agent_kernel;
#[path = "scenarios/basic.rs"]
mod basic;
#[path = "scenarios/bioma.rs"]
mod bioma;
#[path = "scenarios/datasheet.rs"]
mod datasheet;
#[path = "scenarios/frames.rs"]
mod frames;
#[path = "scenarios/gateway.rs"]
mod gateway;
#[path = "scenarios/map.rs"]
mod map;
#[path = "scenarios/media.rs"]
mod media;
#[path = "scenarios/perception.rs"]
mod perception;
#[path = "scenarios/recording_ingest.rs"]
mod recording_ingest;
#[path = "scenarios/secrets.rs"]
mod secrets;
#[path = "scenarios/sumo.rs"]
mod sumo;
#[path = "scenarios/uav_sim.rs"]
mod uav_sim;
#[path = "scenarios/view.rs"]
mod view;

pub(crate) use agent_kernel::*;
pub(crate) use basic::*;
pub(crate) use bioma::*;
pub(crate) use datasheet::*;
pub(crate) use frames::*;
pub(crate) use gateway::*;
pub(crate) use map::*;
pub(crate) use media::*;
pub(crate) use perception::*;
pub(crate) use recording_ingest::*;
pub(crate) use secrets::*;
pub(crate) use sumo::*;
pub(crate) use uav_sim::*;
pub(crate) use view::*;

pub(crate) async fn gateway_suite(control_plane: &Path, smoke_control_plane: &Path) -> Result<()> {
    let conformance = Path::new("target/debug/conformance");
    let gateway = Path::new("target/debug/gateway");
    let media = Path::new("target/debug/server");
    let artifact_service = Path::new("target/debug/artifact-service");

    suite_step("workspace contract and gateway tests");
    run_checked(
        Path::new("cargo"),
        [
            "test".into(),
            "-p".into(),
            "veoveo-mcp-contract".into(),
            "-p".into(),
            "veoveo-mcp-gateway".into(),
        ],
        [],
    )?;

    suite_step("smoke binary dependencies");
    run_checked(
        Path::new("cargo"),
        [
            "build".into(),
            "-p".into(),
            "veoveo-mcp-conformance".into(),
            "--bin".into(),
            "conformance".into(),
            "-p".into(),
            "veoveo-mcp-gateway".into(),
            "--bin".into(),
            "gateway".into(),
            "-p".into(),
            "veoveo-recording-hub".into(),
            "--bin".into(),
            "spooler".into(),
            "-p".into(),
            "veoveo-media-mcp".into(),
            "--bin".into(),
            "server".into(),
        ],
        [],
    )?;

    suite_step("contract schema export");
    contract_schemas(conformance)?;

    suite_step("gateway control-plane validation");
    run_checked(
        gateway,
        [
            "validate".into(),
            "--control-plane".into(),
            control_plane.as_os_str().to_os_string(),
        ],
        [],
    )?;
    run_checked(
        gateway,
        [
            "validate".into(),
            "--control-plane".into(),
            smoke_control_plane.as_os_str().to_os_string(),
        ],
        [],
    )?;

    suite_step("gateway SurrealDB platform bootstrap");
    gateway_platform_store(gateway, smoke_control_plane).await?;

    suite_step("self-hosted deployment validation");
    run_checked(
        conformance,
        [
            "deployment-validate".into(),
            "--file".into(),
            "configs/deployments.json".into(),
        ],
        [],
    )?;

    suite_step("Helm deployment configuration");
    helm_config().await?;

    suite_step("gateway HTTP and OAuth boundary");
    gateway_http(conformance, gateway, smoke_control_plane).await?;

    suite_step("authenticated recording ingest boundary");
    recording_ingest(
        conformance,
        gateway,
        Path::new("target/debug/spooler"),
        smoke_control_plane,
    )
    .await?;

    suite_step("gateway OpenTelemetry export");
    otel(conformance, gateway, smoke_control_plane).await?;

    suite_step("gateway Vault secret resolution");
    gateway_vault_secrets(gateway, smoke_control_plane).await?;

    suite_step("media MCP auth boundary");
    media_mcp_auth(conformance, media, artifact_service).await?;

    suite_step("direct media task run");
    media_task_run(conformance, media, artifact_service).await?;

    suite_step("authenticated gateway forwarding and policy");
    gateway_authenticated(
        conformance,
        media,
        gateway,
        smoke_control_plane,
        artifact_service,
    )
    .await?;

    suite_step("gateway with two hosted servers");
    gateway_two_servers(conformance, gateway, smoke_control_plane).await?;

    suite_step("gateway chart resource projection");
    gateway_chart_projection(conformance, gateway, smoke_control_plane).await?;
    gateway_console_stream(conformance, gateway, smoke_control_plane).await?;

    suite_step("gateway task run with artifacts and usage");
    gateway_task_run(
        conformance,
        media,
        gateway,
        smoke_control_plane,
        artifact_service,
    )
    .await?;

    println!("gateway smoke suite ok");
    Ok(())
}

fn suite_step(name: &str) {
    println!("==> {name}");
}
