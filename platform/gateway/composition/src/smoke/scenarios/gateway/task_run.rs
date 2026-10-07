use super::*;
use SmokeAuditSelection as Select;
use veoveo_mcp_contract::audit::{AuditOutcome, AuditReadMethod, DiscoveryKind, TaskActivity};
use veoveo_media_mcp::contract::{
    ArtifactArgs, ArtifactOutput, MediaArtifactUri, MediaGenerationResult, ModelCatalogOutput,
    ModelSchemaOutput,
};
pub(crate) async fn gateway_task_run(
    conformance: &Path,
    media: &Path,
    gateway: &Path,
    control_plane: &Path,
    artifact_service: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;
    assert_executable(gateway)?;
    assert_executable(artifact_service)?;
    let seed: veoveo_mcp_contract::GatewayControlPlane =
        serde_json::from_slice(&fs::read(control_plane)?)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let media_port = 18801u16;
    let gateway_port = 18802u16;
    let provider_port = 18806u16;
    let media_base = format!("http://127.0.0.1:{media_port}");
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let provider_base = format!("http://127.0.0.1:{provider_port}");
    let provider_log = tmpdir.join("provider.log");
    let media_log = tmpdir.join("media.log");
    let gateway_log = tmpdir.join("gateway.log");
    let provider_ready = tmpdir.join("provider.ready");
    let output_dir = tmpdir.join("outputs");

    let mut provider =
        spawn_fake_media_provider(provider_port, &provider_ready, &provider_log, Some(4000))?;
    wait_for_file_and_http(&provider_ready, &format!("{provider_base}/api/v3/models")).await?;

    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let mut media_child = spawn_media_memory_smoke(
        media,
        media_port,
        &media_base,
        &plane.platform,
        &provider_base,
        &plane.url,
        &media_log,
    )?;
    wait_for_http(&format!("{media_base}/media/healthz")).await?;

    let auth_private_key = run_checked(
        &veoveo_testing_support::artifacts::executable(
            "veoveo-gateway-composition",
            "gateway-smoke-support",
        )?,
        ["gateway-private-key-der-b64".into()],
        [],
    )?;
    let platform_store = &plane.platform;
    bootstrap_gateway_platform_store(gateway, control_plane, platform_store).await?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, platform_store),
        [
            (
                "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
                INTERNAL_SIGNING_KEY_DER_B64.into(),
            ),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.trim().into(),
            ),
        ],
        &gateway_log,
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;
    assert_ready_profiles(&gateway_base, seed.profiles.len().try_into()?).await?;

    let token = gateway_id_jag_token(
        &gateway_base,
        &[
            "--id-jag-scope",
            "operator:use",
            "--group",
            "engineering",
            "--role",
            "operator",
            "--data-label",
            "cui",
        ],
    )?;
    let token = token.trim();

    let cancel_output = run_mcp(
        &veoveo_testing_support::artifacts::executable("veoveo-media-mcp", "media-smoke")?,
        &gateway_base,
        token,
        [
            "run".into(),
            "fake/image".into(),
            "--tool-name".into(),
            "media__run".into(),
            "--input".into(),
            r#"{"prompt":"cancel"}"#.into(),
            "--cancel".into(),
        ],
    )?;
    let cancel_task_id = task_id_from_output(&cancel_output)?;
    contains(
        &cancel_output,
        &format!("cancelled task {cancel_task_id} (status Cancelled)"),
    )?;

    let complete_output = run_mcp(
        &veoveo_testing_support::artifacts::executable("veoveo-media-mcp", "media-smoke")?,
        &gateway_base,
        token,
        ["complete".into(), "fake".into()],
    )?;
    contains(&complete_output, "fake/image")?;

    let run_result = run_raw(
        &veoveo_testing_support::artifacts::executable("veoveo-media-mcp", "media-smoke")?,
        [
            "--url".into(),
            format!("{gateway_base}/mcp/operator").into(),
            "run".into(),
            "fake/image".into(),
            "--tool-name".into(),
            "media__run".into(),
            "--input".into(),
            r#"{"prompt":"smoke"}"#.into(),
            "--output-dir".into(),
            output_dir.as_os_str().to_os_string(),
        ],
        [("MCP_BEARER_TOKEN", token.into())],
    )?;
    let run_output = String::from_utf8(run_result.stdout)?;
    let notifications = String::from_utf8(run_result.stderr)?;
    if !run_result.status.success() {
        bail!(
            "gateway Media run failed: {}\nstdout:\n{run_output}\nstderr:\n{notifications}",
            run_result.status
        );
    }
    contains(&notifications, "[resource updated] media://prediction/")?;
    let task_id = task_id_from_output(&run_output)?;
    for expected in [
        "poll: Working — submitted; prediction".to_string(),
        "subscribed to media://prediction/".to_string(),
        "poll: Completed — Generation completed.".to_string(),
        "subscription cancelled".to_string(),
    ] {
        contains(&run_output, &expected)?;
    }

    let structured: MediaGenerationResult = structured_from_output(&run_output)?;
    let native_task_id = structured.task_id().to_string();
    if native_task_id == task_id {
        bail!("gateway exposed the native Media Task as its public handle");
    }
    if structured.artifacts().is_empty() {
        bail!("run output had no artifacts: {run_output}");
    }
    for artifact in structured.artifacts() {
        if artifact.metadata.get("taskId").and_then(Value::as_str) != Some(native_task_id.as_str())
        {
            bail!("artifact metadata did not use native task id `{native_task_id}`: {artifact:?}");
        }
        if artifact
            .compliance
            .tenant_id
            .as_ref()
            .map(|tenant| tenant.as_str())
            != Some("tenant-a")
            || !artifact
                .compliance
                .data_labels
                .iter()
                .any(|label| label.as_str() == "cui")
        {
            bail!("artifact compliance labels were not propagated: {artifact:?}");
        }
    }
    assert_output_file(&output_dir, "png")?;

    let usage = wait_for_actual_usage(
        conformance,
        &format!("{gateway_base}/mcp/operator"),
        &native_task_id,
        Some(token),
    )?;
    assert_usage_report(&usage, "media", &native_task_id)?;

    let full_session = connect_mcp_client(&format!("{gateway_base}/mcp/operator"), token).await?;
    let full_resources = full_session
        .list_resources(Default::default())
        .await
        .context("full-MCP resource discovery")?;
    if full_resources
        .resources
        .iter()
        .any(|resource| resource.uri == veoveo_media_mcp::uris::STUDIO_APP_URI)
    {
        bail!("operator resource discovery exposed the excluded Studio App");
    }
    let full_tools = full_session
        .list_tools(Default::default())
        .await
        .context("full-MCP tool discovery")?;
    if full_tools.tools.iter().any(|tool| {
        matches!(
            tool.name.as_ref(),
            "media__artifact" | "media__models" | "media__model_schema"
        )
    }) {
        bail!("full-MCP client unexpectedly saw compatibility helpers: {full_tools:?}");
    }
    if !full_tools
        .tools
        .iter()
        .any(|tool| tool.name.as_ref() == "media__run")
    {
        bail!("full-MCP client did not see media__run: {full_tools:?}");
    }

    let compat_token = gateway_hosted_public_id_jag_token(
        &gateway_base,
        &[
            "--id-jag-scope",
            "operator:use",
            "--group",
            "engineering",
            "--role",
            "operator",
            "--data-label",
            "cui",
        ],
    )?;
    let compat_token = compat_token.trim();
    let session =
        connect_tools_only_mcp_client(&format!("{gateway_base}/mcp/operator"), compat_token)
            .await?;
    let listed_tools = session
        .list_tools(Default::default())
        .await
        .context("compatibility tool discovery")?;
    for expected_tool in [
        "media__artifact",
        "media__models",
        "media__model_schema",
        "media__run",
    ] {
        if !listed_tools
            .tools
            .iter()
            .any(|tool| tool.name.as_ref() == expected_tool)
        {
            bail!("gateway did not list {expected_tool}: {listed_tools:?}");
        }
    }
    let models_result = session
        .call_tool(
            CallToolRequestParams::new("media__models").with_arguments(
                serde_json::json!({
                    "query": "fake",
                    "type": "image-to-image",
                    "limit": 5
                })
                .as_object()
                .cloned()
                .unwrap(),
            ),
        )
        .await
        .context("Media model compatibility helper")?;
    if models_result.is_error == Some(true) {
        bail!("gateway media__models returned an error: {models_result:?}");
    }
    let models_structured = models_result
        .structured_content
        .clone()
        .ok_or_else(|| anyhow!("media__models returned no structured content"))?;
    let models: ModelCatalogOutput = serde_json::from_value(models_structured)?;
    if !models
        .models
        .iter()
        .any(|model| model.model_id.as_str() == "fake/image")
    {
        bail!("media__models did not return fake/image: {models:?}");
    }
    let schema_result = session
        .call_tool(
            CallToolRequestParams::new("media__model_schema").with_arguments(
                serde_json::json!({ "model": "fake/image" })
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        )
        .await
        .context("Media model schema compatibility helper")?;
    if schema_result.is_error == Some(true) {
        bail!("gateway media__model_schema returned an error: {schema_result:?}");
    }
    let schema_structured = schema_result
        .structured_content
        .clone()
        .ok_or_else(|| anyhow!("media__model_schema returned no structured content"))?;
    let schema: ModelSchemaOutput = serde_json::from_value(schema_structured)?;
    if schema.model_id.as_str() != "fake/image"
        || !schema
            .request_schema
            .as_ref()
            .and_then(|schema| schema.get("required"))
            .and_then(Value::as_array)
            .is_some_and(|required| required.iter().any(|field| field == "prompt"))
    {
        bail!("media__model_schema did not return fake/image prompt schema: {schema:?}");
    }
    let direct_result = session
        .call_tool(
            CallToolRequestParams::new("media__run").with_arguments(
                serde_json::json!({
                    "model": "fake/image",
                    "input": { "prompt": "direct-call smoke" }
                })
                .as_object()
                .cloned()
                .unwrap(),
            ),
        )
        .await
        .context("Media direct-call Task adapter")?;
    if direct_result.is_error == Some(true) {
        bail!("direct gateway tools/call returned an error: {direct_result:?}");
    }
    let direct_task_id = direct_result
        .meta
        .as_ref()
        .and_then(|meta| meta.0.get(RELATED_TASK_META_KEY))
        .and_then(|value| value.get("taskId"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("direct tools/call returned no gateway task id: {direct_result:?}"))?
        .to_string();
    let direct_structured: MediaGenerationResult = serde_json::from_value(
        direct_result
            .structured_content
            .clone()
            .ok_or_else(|| anyhow!("direct tools/call returned no structured output"))?,
    )?;
    if direct_structured.artifacts().is_empty() {
        bail!("direct tools/call returned no artifacts: {direct_result:?}");
    }
    if direct_structured
        .artifacts()
        .iter()
        .any(|artifact| artifact.download_url.is_some())
    {
        bail!("direct tools/call leaked artifact download_url: {direct_structured:?}");
    }
    let artifact_result = session
        .call_tool(
            CallToolRequestParams::new("media__artifact").with_arguments(
                serde_json::to_value(ArtifactArgs {
                    artifact_uri: MediaArtifactUri::new(
                        direct_structured.artifacts()[0].artifact_id(),
                    ),
                })?
                .as_object()
                .cloned()
                .unwrap(),
            ),
        )
        .await
        .context("Media Artifact compatibility helper")?;
    if artifact_result.is_error == Some(true) {
        bail!("media__artifact returned an error: {artifact_result:?}");
    }
    if !artifact_result
        .content
        .iter()
        .any(|block| block.as_image().is_some())
    {
        bail!("media__artifact did not return image content: {artifact_result:?}");
    }
    let artifact_structured = artifact_result
        .structured_content
        .clone()
        .ok_or_else(|| anyhow!("media__artifact returned no structured content"))?;
    let artifact: ArtifactOutput = serde_json::from_value(artifact_structured)?;
    if artifact.artifact.download_url.is_some() {
        bail!("media__artifact leaked artifact download URL: {artifact:?}");
    }
    let direct_status: GatewayTaskStatusDocument = serde_json::from_value(
        read_mcp_resource_json(&session, &format!("veoveo://task/{direct_task_id}")).await?,
    )?;
    if direct_status.task.status != GatewayTaskStatusKind::Completed {
        bail!("direct gateway task status was not completed: {direct_status:?}");
    }
    if direct_status.result.is_none() {
        bail!("completed gateway task status resource did not include result");
    }

    let media_mcp_url = format!("{media_base}/media/mcp");
    {
        let mut identity = fixture_identity(veoveo_mcp_contract::ServerSlug::parse("media")?)?;
        identity.profile = veoveo_mcp_contract::GatewayProfileId::parse("admin")?;
        let bearer = fixture_bearer(identity)?;
        assert_direct_mcp_denied(
            &veoveo_testing_support::artifacts::executable("veoveo-media-mcp", "media-smoke")?,
            &media_mcp_url,
            ["usage".into(), native_task_id.clone().into()],
            [("MCP_BEARER_TOKEN", bearer.into())],
            veoveo_mcp_conformance::client::failure::ObservedFailure::mcp(
                -32602,
                "unknown task usage",
            ),
        )
    }?;

    {
        let mut identity = fixture_identity(veoveo_mcp_contract::ServerSlug::parse("media")?)?;
        identity.profile = veoveo_mcp_contract::GatewayProfileId::parse("admin")?;
        let bearer = fixture_bearer(identity)?;
        assert_direct_mcp_denied(
            &veoveo_testing_support::artifacts::executable(
                "veoveo-artifact-service",
                "artifact-smoke",
            )?,
            &media_mcp_url,
            [
                "artifact".into(),
                structured.artifacts()[0].artifact_id().to_string().into(),
                "--output-dir".into(),
                tmpdir
                    .join("denied-gateway-artifacts")
                    .as_os_str()
                    .to_os_string(),
            ],
            [("MCP_BEARER_TOKEN", bearer.into())],
            veoveo_mcp_conformance::client::failure::ObservedFailure::mcp(
                -32602,
                format!(
                    "unknown artifact '{}'",
                    structured.artifacts()[0].artifact_id()
                ),
            ),
        )
    }?;

    gateway_child.drain(Duration::from_secs(90)).await?;
    let audit = SmokeAudit::connect(platform_store, control_plane).await?;
    // One aggregate discovery record describes the excluded Studio App.
    audit
        .at_least(
            Select::DiscoveryDenials {
                collection: DiscoveryKind::Resources,
                denied: 1,
            },
            Some(AuditOutcome::Allowed),
            1,
        )
        .await?;
    audit
        .at_least(
            Select::Read(AuditReadMethod::Completion),
            Some(AuditOutcome::Allowed),
            1,
        )
        .await?;
    audit
        .at_least(Select::ToolAdmission, Some(AuditOutcome::Allowed), 6)
        .await?;
    audit.at_least(Select::ToolCompletion, None, 6).await?;
    audit
        .at_least(
            Select::Task(TaskActivity::Cancel),
            Some(AuditOutcome::Allowed),
            1,
        )
        .await?;
    audit
        .exact(
            Select::Read(AuditReadMethod::Status),
            Some(AuditOutcome::Allowed),
            0,
        )
        .await?;
    audit
        .at_least(
            Select::Read(AuditReadMethod::Subscription),
            Some(AuditOutcome::Allowed),
            1,
        )
        .await?;
    audit
        .at_least(
            Select::Read(AuditReadMethod::ResourceRead),
            Some(AuditOutcome::Allowed),
            2,
        )
        .await?;
    audit.assert_cli(gateway, platform_store)?;

    media_child.stop_checked().await?;
    provider.stop_checked().await?;
    cleanup.remove_on_drop();
    println!("gateway task run smoke ok");
    Ok(())
}
