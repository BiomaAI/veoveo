use super::*;

pub(crate) async fn surreal_integration() -> Result<()> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let name = format!("veoveo-surreal-smoke-{}", uuid::Uuid::new_v4().simple());
    run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "--detach".into(),
            "--rm".into(),
            "--name".into(),
            name.clone().into(),
            "--publish".into(),
            format!("127.0.0.1:{port}:8000").into(),
            "--tmpfs".into(),
            "/data:rw,size=1073741824,uid=65532,gid=65532,mode=0700".into(),
            "surrealdb/surrealdb:v3.2.4".into(),
            "start".into(),
            "--bind".into(),
            "0.0.0.0:8000".into(),
            "--user".into(),
            "root".into(),
            "--pass".into(),
            "root".into(),
            "rocksdb:/data/veoveo.db".into(),
        ],
        [],
    )?;
    let _container = ContainerGuard::new(name);
    let ready_url = format!("http://127.0.0.1:{port}/ready");
    let mut ready = false;
    for _ in 0..120 {
        if http_ok(&ready_url).await? {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    if !ready {
        bail!("timed out waiting for SurrealDB 3.2.4 at {ready_url}");
    }

    let endpoint = format!("ws://127.0.0.1:{port}");
    let environment = [
        ("VEOVEO_SURREAL_INTEGRATION", "1".into()),
        ("VEOVEO_SURREAL_URL", endpoint.clone().into()),
        ("VEOVEO_SURREAL_ENDPOINT", endpoint.into()),
        ("VEOVEO_SURREAL_USER", "root".into()),
        ("VEOVEO_SURREAL_USERNAME", "root".into()),
        ("VEOVEO_SURREAL_PASSWORD", "root".into()),
    ];
    for (package, test) in [
        ("veoveo-platform-store", "surreal_integration"),
        ("veoveo-task-runtime", "surreal_integration"),
        ("veoveo-agent-runtime", "surreal_integration"),
        ("veoveo-mcp-gateway", "control_store"),
        ("veoveo-mcp-gateway", "gateway_state"),
        ("veoveo-media-mcp", "surreal_integration"),
    ] {
        println!("==> live SurrealDB test: {package}/{test}");
        run_checked(
            Path::new("cargo"),
            [
                "test".into(),
                "-p".into(),
                package.into(),
                "--test".into(),
                test.into(),
                "--".into(),
                "--nocapture".into(),
                "--test-threads=1".into(),
            ],
            environment.clone(),
        )?;
    }
    let _ = environment;
    println!("surreal integration smoke ok");
    Ok(())
}

pub(crate) async fn gateway_platform_store(gateway: &Path, control_plane: &Path) -> Result<()> {
    assert_executable(gateway)?;
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let platform_store = spawn_gateway_platform_store(gateway, control_plane).await?;
    let validate = run_checked(
        gateway,
        ["control-plane-validate".into()],
        platform_store.runtime_env(),
    )?;
    contains(&validate, "ok: revision")?;
    contains(&validate, "1 server(s), 2 profile(s)")?;

    cleanup.remove_on_drop();
    println!("gateway platform store smoke ok");
    Ok(())
}

pub(crate) fn contract_schemas(conformance: &Path) -> Result<()> {
    assert_executable(conformance)?;
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());
    let schemas = tmpdir.join("schemas");

    run_checked(
        conformance,
        [
            "contract-schemas".into(),
            "--output-dir".into(),
            schemas.as_os_str().to_os_string(),
        ],
        [],
    )?;

    assert_schema_title(
        &schemas.join("gateway-control-plane.schema.json"),
        "GatewayControlPlane",
    )?;
    let control_plane_revision = assert_schema_title(
        &schemas.join("gateway-control-plane-revision.schema.json"),
        "GatewayControlPlaneRevision",
    )?;
    for property in ["revision_id", "sha256", "source", "control_plane"] {
        if !control_plane_revision
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("control-plane revision schema has no object `{property}` property");
        }
    }
    assert_schema_title(
        &schemas.join("gateway-server-fragment.schema.json"),
        "GatewayServerFragment",
    )?;
    assert_schema_title(
        &schemas.join("gateway-binding.schema.json"),
        "GatewayBinding",
    )?;
    assert_schema_title(
        &schemas.join("gateway-composition-provenance.schema.json"),
        "GatewayCompositionProvenance",
    )?;
    assert_schema_title(
        &schemas.join("resource-authorization-server.schema.json"),
        "ResourceAuthorizationServer",
    )?;
    assert_schema_title(
        &schemas.join("oauth-client-registration.schema.json"),
        "OAuthClientRegistration",
    )?;
    assert_schema_title(
        &schemas.join("gateway-resource-subscription.schema.json"),
        "GatewayResourceSubscription",
    )?;
    assert_schema_title(
        &schemas.join("gateway-internal-identity.schema.json"),
        "GatewayInternalIdentity",
    )?;
    assert_schema_title(
        &schemas.join("principal-audit-attributes.schema.json"),
        "PrincipalAuditAttributes",
    )?;
    assert_schema_title(
        &schemas.join("data-label-definition.schema.json"),
        "DataLabelDefinition",
    )?;
    assert_schema_title(
        &schemas.join("tenant-definition.schema.json"),
        "TenantDefinition",
    )?;
    let auth_audit = assert_schema_title(
        &schemas.join("auth-audit-event.schema.json"),
        "AuthAuditEvent",
    )?;
    for property in ["outcome", "reason", "method", "protected_resource"] {
        if !auth_audit
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("auth audit schema has no object `{property}` property");
        }
    }
    let deployment = assert_schema_title(
        &schemas.join("self-hosted-deployment-plan.schema.json"),
        "SelfHostedDeploymentPlan",
    )?;
    if !deployment
        .get("properties")
        .and_then(|properties| properties.get("profiles"))
        .is_some_and(Value::is_object)
    {
        bail!("deployment plan schema has no object profiles property");
    }
    let deployment_profile = assert_schema_title(
        &schemas.join("self-hosted-deployment-profile.schema.json"),
        "SelfHostedDeploymentProfile",
    )?;
    for property in [
        "service_to_service",
        "platform_store",
        "analytical_runtime",
        "telemetry",
        "tenant_model",
    ] {
        if !deployment_profile
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("deployment profile schema has no object `{property}` property");
        }
    }
    let platform_store = assert_schema_title(
        &schemas.join("platform-store-deployment.schema.json"),
        "PlatformStoreDeployment",
    )?;
    for property in [
        "engine",
        "version",
        "storage_engine",
        "topology",
        "database_ha",
        "changefeed_source_of_truth",
    ] {
        if !platform_store
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("platform store schema has no object `{property}` property");
        }
    }
    let artifact = assert_schema_title(
        &schemas.join("artifact-metadata.schema.json"),
        "ArtifactMetadata",
    )?;
    if !artifact
        .get("properties")
        .and_then(|properties| properties.get("compliance"))
        .is_some_and(Value::is_object)
    {
        bail!("artifact metadata schema has no object compliance property");
    }
    assert_schema_title(
        &schemas.join("coordinate-operation-provenance.schema.json"),
        "CoordinateOperationProvenance",
    )?;
    let usage = assert_schema_title(&schemas.join("usage-report.schema.json"), "UsageReport")?;
    if !usage
        .get("properties")
        .and_then(|properties| properties.get("records"))
        .is_some_and(Value::is_object)
    {
        bail!("usage report schema has no object records property");
    }

    cleanup.remove_on_drop();
    println!("contract schemas smoke ok");
    Ok(())
}

pub(crate) async fn otel(conformance: &Path, gateway: &Path, control_plane: &Path) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(gateway)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let gateway_port = 18804u16;
    let otlp_port = 18805u16;
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let otlp_base = format!("http://127.0.0.1:{otlp_port}");
    let gateway_log = tmpdir.join("gateway.log");
    let otlp_log = tmpdir.join("otlp.log");
    let otlp_ready = tmpdir.join("otlp.ready");
    let otlp_hits = tmpdir.join("otlp.hits");

    let mut otlp = ChildGuard::spawn(
        conformance,
        [
            "otlp-http-sink".into(),
            "--port".into(),
            otlp_port.to_string().into(),
            "--ready-file".into(),
            otlp_ready.as_os_str().to_os_string(),
            "--hits-file".into(),
            otlp_hits.as_os_str().to_os_string(),
        ],
        [],
        &otlp_log,
    )?;
    wait_for_file(&otlp_ready).await?;

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let platform_store = spawn_gateway_platform_store(gateway, control_plane).await?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, &platform_store),
        [
            ("OTEL_EXPORTER_OTLP_ENDPOINT", otlp_base.into()),
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
    let ready: Value = reqwest::get(format!("{gateway_base}/readyz"))
        .await?
        .error_for_status()?
        .json()
        .await?;
    if ready.get("profiles").and_then(Value::as_u64) != Some(1) {
        bail!("gateway readyz did not report one profile: {ready}");
    }

    wait_for_file_contains(&otlp_hits, "logs ", "traces ").await?;

    gateway_child.stop();
    otlp.stop();
    cleanup.remove_on_drop();
    println!("otel smoke ok");
    Ok(())
}
