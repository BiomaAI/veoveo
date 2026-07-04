use super::*;

pub(crate) async fn compose_config() -> Result<()> {
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let compose_output = run_checked(
        Path::new("docker"),
        [
            "compose".into(),
            "-f".into(),
            "compose.yaml".into(),
            "-f".into(),
            "compose.tunnel.yaml".into(),
            "--profile".into(),
            "dev".into(),
            "--profile".into(),
            "tunnel".into(),
            "config".into(),
        ],
        [
            ("MEDIA_PROVIDER_API_KEY", "dummy".into()),
            (
                "MEDIA_PROVIDER_WEBHOOK_SECRET",
                "whsec_0Wn4SW+lD1zrRtFhb1r4fGHt6XZLSkX5y2EK+lSbA+E=".into(),
            ),
            (
                "VEOVEO_INTERNAL_TOKEN_SECRET",
                "local-development-secret-at-least-32-bytes".into(),
            ),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                "dummy".into(),
            ),
            ("VEOVEO_IDP_OIDC_CLIENT_SECRET", "dummy".into()),
            ("PUBLIC_BASE_URL", PUBLIC_BASE_URL.into()),
            ("CLOUDFLARED_TUNNEL_TOKEN", "dummy".into()),
        ],
    )?;
    let host_ip_count = compose_output.matches("host_ip: 127.0.0.1").count();
    if host_ip_count < 7 {
        bail!("compose config had {host_ip_count} loopback port bindings; expected at least 7");
    }
    for expected in [
        "image: caddy:2.11.2",
        "image: postgres:18-alpine",
        "image: rustfs/rustfs:1.0.0-beta.8",
        "target: /etc/caddy/Caddyfile",
        "target: /var/lib/postgresql",
        "--control-plane-source",
        "postgresql://veoveo_gateway:veoveo_gateway@gateway-control-db:5432/veoveo_gateway",
        "http://rustfs:9000",
        "target: 8080",
        "published: \"8780\"",
        "edge:",
        "gateway-control-db:",
        "rustfs:",
    ] {
        contains(&compose_output, expected)?;
    }
    if compose_output.to_ascii_lowercase().contains("minio") {
        bail!("compose config must use RustFS/S3-compatible storage, not MinIO");
    }

    let gateway_dockerfile = fs::read_to_string("crates/mcp-gateway/Dockerfile")?;
    contains(&gateway_dockerfile, "find /app/target -name 'libduckdb.so'")?;
    contains(
        &gateway_dockerfile,
        "COPY --from=builder /out/lib/libduckdb.so /usr/local/lib/libduckdb.so",
    )?;

    let caddyfile = env::current_dir()?.join("configs/Caddyfile");
    let caddyfile_text = fs::read_to_string(&caddyfile)?;
    contains(&caddyfile_text, "respond /media/mcp* 404")?;
    contains(&caddyfile_text, "reverse_proxy mcp-gateway:8788")?;
    contains(&caddyfile_text, "reverse_proxy media-mcp:8787")?;
    run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "--rm".into(),
            "-v".into(),
            format!("{}:/etc/caddy/Caddyfile:ro", caddyfile.display()).into(),
            "caddy:2.11.2".into(),
            "caddy".into(),
            "validate".into(),
            "--config".into(),
            "/etc/caddy/Caddyfile".into(),
            "--adapter".into(),
            "caddyfile".into(),
        ],
        [],
    )?;

    cleanup.remove_on_drop();
    println!("compose config smoke ok");
    Ok(())
}

pub(crate) async fn gateway_control_db(gateway: &Path, control_plane: &Path) -> Result<()> {
    assert_executable(gateway)?;
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let host_port = reserve_local_port()?;
    let container_name = format!("veoveo-smoke-postgres-{}", uuid::Uuid::new_v4());
    let _container = ContainerGuard::new(container_name.clone());
    run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "-d".into(),
            "--name".into(),
            container_name.clone().into(),
            "-e".into(),
            "POSTGRES_DB=veoveo_gateway".into(),
            "-e".into(),
            "POSTGRES_USER=veoveo_gateway".into(),
            "-e".into(),
            "POSTGRES_PASSWORD=veoveo_gateway".into(),
            "-p".into(),
            format!("127.0.0.1:{host_port}:5432").into(),
            "postgres:18-alpine".into(),
        ],
        [],
    )?;
    wait_for_postgres_container(&container_name).await?;

    let control_db_url =
        format!("postgresql://veoveo_gateway:veoveo_gateway@127.0.0.1:{host_port}/veoveo_gateway");
    let seed_output = run_checked(
        gateway,
        [
            "control-plane-seed".into(),
            "--control-db-url".into(),
            control_db_url.clone().into(),
            "--control-plane".into(),
            control_plane.as_os_str().to_os_string(),
            "--applied-by".into(),
            "smoke#control-db".into(),
        ],
        [],
    )?;
    let seed: Value = serde_json::from_str(&seed_output)?;
    if seed.get("status").and_then(Value::as_str) != Some("seeded") {
        bail!("gateway control-plane seed did not report seeded: {seed}");
    }
    if seed.get("revisions").and_then(Value::as_u64) != Some(1) {
        bail!("gateway control-plane seed did not record one revision: {seed}");
    }
    if seed
        .get("active_objects")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        == 0
    {
        bail!("gateway control-plane seed recorded no queryable objects: {seed}");
    }

    let validate = run_checked(
        gateway,
        [
            "validate-db".into(),
            "--control-db-url".into(),
            control_db_url.into(),
        ],
        [],
    )?;
    contains(&validate, "ok: revision")?;
    contains(&validate, "1 server(s), 1 profile(s)")?;

    cleanup.remove_on_drop();
    println!("gateway control DB smoke ok");
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
        &schemas.join("resource-authorization-server.schema.json"),
        "ResourceAuthorizationServer",
    )?;
    assert_schema_title(
        &schemas.join("oauth-client-registration.schema.json"),
        "OAuthClientRegistration",
    )?;
    assert_schema_title(
        &schemas.join("gateway-task-mapping.schema.json"),
        "GatewayTaskMapping",
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
    for property in ["service_to_service", "state_stores", "telemetry_sinks"] {
        if !deployment_profile
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("deployment profile schema has no object `{property}` property");
        }
    }
    let network_boundary = assert_schema_title(
        &schemas.join("network-boundary-rule.schema.json"),
        "NetworkBoundaryRule",
    )?;
    for property in ["target_kind", "target", "ports", "tls_required"] {
        if !network_boundary
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("network boundary schema has no object `{property}` property");
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

fn reserve_local_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

async fn wait_for_postgres_container(container_name: &str) -> Result<()> {
    for _ in 0..150 {
        let output = run_raw(
            Path::new("docker"),
            [
                "exec".into(),
                container_name.into(),
                "pg_isready".into(),
                "-U".into(),
                "veoveo_gateway".into(),
                "-d".into(),
                "veoveo_gateway".into(),
            ],
            [],
        )?;
        if output.status.success() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("timed out waiting for Postgres container {container_name}");
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
    let state_db = tmpdir.join("gateway-state.duckdb");

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
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, control_plane, &state_db),
        [
            ("OTEL_EXPORTER_OTLP_ENDPOINT", otlp_base.into()),
            ("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
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
