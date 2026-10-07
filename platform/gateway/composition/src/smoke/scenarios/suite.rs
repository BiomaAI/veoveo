use super::*;
pub(crate) async fn gateway_suite(control_plane: &Path, smoke_control_plane: &Path) -> Result<()> {
    let conformance =
        veoveo_testing_support::artifacts::executable("veoveo-mcp-conformance", "conformance")?;
    let conformance = conformance.as_path();
    let gateway =
        veoveo_testing_support::artifacts::executable("veoveo-gateway-composition", "gateway")?;
    let gateway = gateway.as_path();
    let media = veoveo_testing_support::artifacts::executable("veoveo-media-mcp", "media-mcp")?;
    let _media = media.as_path();
    let artifact_service = veoveo_testing_support::artifacts::executable(
        "veoveo-artifact-service",
        "artifact-service",
    )?;
    let artifact_service = artifact_service.as_path();

    suite_step("workspace contract and gateway tests");
    run_checked(
        Path::new("cargo"),
        [
            "test".into(),
            "-p".into(),
            "veoveo-mcp-contract".into(),
            "-p".into(),
            "veoveo-mcp-gateway".into(),
            "-p".into(),
            "veoveo-gateway-composition".into(),
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
        &veoveo_testing_support::artifacts::executable(
            "veoveo-deployment-smoke",
            "deployment-fixtures",
        )?,
        [
            "deployment-validate".into(),
            "--file".into(),
            "configs/deployments.json".into(),
        ],
        [],
    )?;

    suite_step("Helm deployment configuration");
    run_checked(
        &veoveo_testing_support::artifacts::executable(
            "veoveo-deployment-smoke",
            "deployment-fixtures",
        )?,
        ["helm-config".into()],
        [],
    )?;

    suite_step("gateway HTTP and OAuth boundary");
    gateway_http(conformance, gateway, smoke_control_plane).await?;

    suite_step("authenticated recording ingest boundary");
    run_checked(
        &veoveo_testing_support::artifacts::executable(
            "veoveo-bioma-acceptance",
            "installation-smoke",
        )?,
        [
            "recording-ingest".into(),
            "--control-plane".into(),
            smoke_control_plane.as_os_str().to_owned(),
        ],
        [],
    )?;

    suite_step("gateway OpenTelemetry export");
    otel(conformance, gateway, smoke_control_plane).await?;

    suite_step("gateway Vault secret resolution");
    gateway_vault_secrets(gateway, smoke_control_plane).await?;

    suite_step("media MCP auth boundary");
    run_checked(
        &veoveo_testing_support::artifacts::executable(
            "veoveo-bioma-acceptance",
            "installation-smoke",
        )?,
        ["media-mcp-auth".into()],
        [],
    )?;

    suite_step("direct media task run");
    run_checked(
        &veoveo_testing_support::artifacts::executable(
            "veoveo-bioma-acceptance",
            "installation-smoke",
        )?,
        ["media-task-run".into()],
        [],
    )?;

    suite_step("authenticated gateway forwarding and policy");
    gateway_authenticated(
        conformance,
        _media,
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
        _media,
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
