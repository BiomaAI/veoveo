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
            "surrealdb/surrealdb:v3.2.0".into(),
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
        bail!("timed out waiting for SurrealDB 3.2.0 at {ready_url}");
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
    println!("surreal integration smoke ok");
    Ok(())
}

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
            "config".into(),
        ],
        [
            ("MEDIA_PROVIDER_API_KEY", "dummy".into()),
            (
                "MEDIA_PROVIDER_WEBHOOK_SECRET",
                "whsec_0Wn4SW+lD1zrRtFhb1r4fGHt6XZLSkX5y2EK+lSbA+E=".into(),
            ),
            (
                "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
                INTERNAL_SIGNING_KEY_DER_B64.into(),
            ),
            (
                "VEOVEO_REFRESH_DELIVERY_KEY_B64",
                REFRESH_DELIVERY_KEY_B64.into(),
            ),
            ("VEOVEO_INTERNAL_TRUST_JWKS", INTERNAL_TRUST_JWKS.into()),
            ("VEOVEO_SURREAL_ADMIN_PASSWORD", "admin-secret".into()),
            ("VEOVEO_SURREAL_RUNTIME_USERNAME", "veoveo-runtime".into()),
            ("VEOVEO_SURREAL_RUNTIME_PASSWORD", "runtime-secret".into()),
            ("VEOVEO_OBJECT_STORE_ACCESS_KEY", "rustfs-access".into()),
            ("VEOVEO_OBJECT_STORE_SECRET_KEY", "rustfs-secret".into()),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                "dummy".into(),
            ),
            ("VEOVEO_IDP_OIDC_CLIENT_SECRET", "dummy".into()),
            (
                "VEOVEO_CONSOLE_SESSION_KEY",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
            ),
            (
                "VEOVEO_CONSOLE_OAUTH_RESOURCE",
                "https://veoveo.enterprise.example/mcp/admin".into(),
            ),
            (
                "PERCEPTION_CONFIG_DIR",
                tmpdir
                    .join("perception-config")
                    .display()
                    .to_string()
                    .into(),
            ),
            (
                "PERCEPTION_MODEL_DIR",
                tmpdir
                    .join("perception-models")
                    .display()
                    .to_string()
                    .into(),
            ),
            ("PUBLIC_BASE_URL", PUBLIC_BASE_URL.into()),
        ],
    )?;
    let host_ip_count = compose_output.matches("host_ip: 127.0.0.1").count();
    if host_ip_count < 8 {
        bail!("compose config had {host_ip_count} loopback port bindings; expected at least 8");
    }
    for expected in [
        "image: caddy:2.11.2",
        "image: surrealdb/surrealdb:v3.2.0",
        "image: rustfs/rustfs:1.0.0-beta.8",
        "gateway.local.json",
        "rocksdb:/data/veoveo.db",
        "VEOVEO_SURREAL_AUTH_LEVEL: database",
        "VEOVEO_SURREAL_ENDPOINT: ws://surrealdb:8000",
        "VEOVEO_REFRESH_DELIVERY_KEY_B64:",
        "VEOVEO_REFRESH_DELIVERY_WINDOW_SECONDS: \"5\"",
        "ARTIFACT_S3_PUBLIC_ENDPOINT",
        "target: /etc/caddy/Caddyfile",
        "http://rustfs:9000",
        "target: 8080",
        "published: \"8780\"",
        "edge:",
        "installation-bootstrap:",
        "console-bff:",
        "rustfs:",
        "chart-mcp:",
        "perception-mcp:",
        "time-mcp:",
        "target: /etc/veoveo/perception",
        "target: /models",
        "target: 8795",
        "published: \"8795\"",
    ] {
        contains(&compose_output, expected)?;
    }
    if compose_output.to_ascii_lowercase().contains("minio") {
        bail!("compose config must use RustFS/S3-compatible storage, not MinIO");
    }
    for forbidden in ["postgres", "cloudflared", "artifact_master_key"] {
        if compose_output.to_ascii_lowercase().contains(forbidden) {
            bail!("canonical compose config must not contain `{forbidden}`");
        }
    }

    let gateway_dockerfile = fs::read_to_string("platform/gateway/Dockerfile")?;
    contains(&gateway_dockerfile, "find /app/target -name 'libduckdb.so'")?;
    contains(
        &gateway_dockerfile,
        "COPY --from=builder /out/lib/libduckdb.so /usr/local/lib/libduckdb.so",
    )?;
    for dockerfile in [
        "agents/kernel/Dockerfile",
        "apps/console/bff/Dockerfile",
        "mcp/bridges/stdio/Dockerfile",
        "platform/artifacts/service/Dockerfile",
        "platform/gateway/Dockerfile",
        "platform/recordings/hub/Dockerfile",
        "servers/artifact-mcp/Dockerfile",
        "servers/frames-mcp/Dockerfile",
        "servers/duckdb-mcp/Dockerfile",
        "servers/media-mcp/Dockerfile",
        "servers/optimization-mcp/Dockerfile",
        "servers/perception-mcp/Dockerfile",
        "servers/recording-mcp/Dockerfile",
        "servers/timeseries-mcp/Dockerfile",
        "servers/time-mcp/Dockerfile",
        "showcase/sumo/sumo-mcp/Dockerfile",
    ] {
        let contents = fs::read_to_string(dockerfile)?;
        for workspace_root in [
            "COPY agents ./agents",
            "COPY apps/console/bff ./apps/console/bff",
            "COPY mcp ./mcp",
            "COPY platform ./platform",
            "COPY servers ./servers",
            "COPY testing ./testing",
            "COPY showcase/sumo/sumo-mcp ./showcase/sumo/sumo-mcp",
        ] {
            contains(&contents, workspace_root)
                .with_context(|| format!("{dockerfile} must copy every Cargo workspace root"))?;
        }
    }
    let dockerignore = fs::read_to_string(".dockerignore")?;
    contains(&dockerignore, "**/.venv")?;
    contains(&dockerignore, "**/node_modules")?;
    contains(&dockerignore, "**/dist")?;

    let caddyfile = env::current_dir()?.join("configs/Caddyfile");
    let caddyfile_text = fs::read_to_string(&caddyfile)?;
    contains(&caddyfile_text, "respond /media/mcp* 404")?;
    contains(&caddyfile_text, "respond /frames/mcp* 404")?;
    contains(&caddyfile_text, "respond /map/mcp* 404")?;
    contains(&caddyfile_text, "respond /time/mcp* 404")?;
    contains(&caddyfile_text, "respond /perception/mcp* 404")?;
    contains(&caddyfile_text, "respond /charts/mcp* 404")?;
    contains(&caddyfile_text, "reverse_proxy mcp-gateway:8788")?;
    contains(&caddyfile_text, "reverse_proxy media-mcp:8787")?;
    contains(&caddyfile_text, "reverse_proxy console-bff:8786")?;
    contains(&caddyfile_text, "reverse_proxy artifact-service:8790")?;
    let public_share_route = caddyfile_text
        .split_once("handle /s/* {")
        .and_then(|(_, route)| route.split_once('}').map(|(route, _)| route))
        .context("Caddy config is missing a dedicated /s/* route")?;
    contains(public_share_route, "log_skip")?;
    contains(public_share_route, "reverse_proxy artifact-service:8790")?;
    for forbidden in [
        "/media/artifacts",
        "/timeseries/artifacts",
        "/optimization/artifacts",
        "/frames/artifacts",
        "/duckdb/artifacts",
    ] {
        if caddyfile_text.contains(forbidden) {
            bail!("edge config must not expose obsolete domain byte route `{forbidden}`");
        }
    }
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
    let frame = assert_schema_title(
        &schemas.join("rrd-frame-definition.schema.json"),
        "RrdFrameDefinition",
    )?;
    for property in ["frame_id", "kind", "view_coordinates"] {
        if !frame
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("frame definition schema has no object `{property}` property");
        }
    }
    assert_schema_title(
        &schemas.join("coordinate-point.schema.json"),
        "CoordinatePoint",
    )?;
    assert_schema_title(
        &schemas.join("coordinate-operation-provenance.schema.json"),
        "CoordinateOperationProvenance",
    )?;
    assert_schema_title(
        &schemas.join("rrd-geofence-geometry.schema.json"),
        "RrdGeofenceGeometry",
    )?;
    let batch_transform = assert_schema_title(
        &schemas.join("batch-transform-output.schema.json"),
        "BatchTransformOutput",
    )?;
    for property in ["result", "artifact"] {
        if !batch_transform
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("batch transform schema has no object `{property}` property");
        }
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
