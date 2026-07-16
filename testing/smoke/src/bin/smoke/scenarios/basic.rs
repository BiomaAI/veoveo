use anyhow::ensure;

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
            "surrealdb/surrealdb:v3.2.1".into(),
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
        bail!("timed out waiting for SurrealDB 3.2.1 at {ready_url}");
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

pub(crate) async fn helm_config() -> Result<()> {
    for chart in ["deploy/helm/veoveo", "showcase/sumo/deploy/helm"] {
        run_checked(Path::new("helm"), ["lint".into(), chart.into()], [])
            .with_context(|| format!("linting Helm chart {chart}"))?;
    }

    let platform = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "veoveo".into(),
            "deploy/helm/veoveo".into(),
            "--namespace".into(),
            "veoveo".into(),
            "--values".into(),
            "deploy/local/k3d/values.yaml".into(),
            "--values".into(),
            "showcase/sumo/deploy/platform-values.yaml".into(),
        ],
        [],
    )?;
    for expected in [
        "image: surrealdb/surrealdb:v3.2.1",
        "image: rustfs/rustfs:1.0.0-beta.8",
        "image: amazon/aws-cli:2.35.23",
        "name: mcp-gateway",
        "name: artifact-service",
        "name: recording-ingest",
        "name: console-bff",
        "nodePort: 30877",
        "host: localhost",
        "path: /s",
        "mountPath: /etc/veoveo/gateway",
        "runAsUser: 65532",
        "runAsUser: 10001",
    ] {
        contains(&platform, expected)?;
    }
    for forbidden in ["minio", "caddy", "compose", "OTEL_EXPORTER_OTLP_ENDPOINT"] {
        if platform.to_ascii_lowercase().contains(forbidden) {
            bail!("canonical Helm render must not contain `{forbidden}`");
        }
    }

    let bioma = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "bioma".into(),
            "deploy/helm/veoveo".into(),
            "--namespace".into(),
            "veoveo".into(),
            "--values".into(),
            "examples/bioma/values.yaml".into(),
            "--values".into(),
            "examples/bioma/k3d-values.yaml".into(),
        ],
        [],
    )?;
    for expected in [
        "host: veoveo.bioma.ai",
        "host: objects-veoveo.bioma.ai",
        "https://veoveo.bioma.ai",
        "name: bioma-gateway-control-plane",
        "name: recording-ingest",
    ] {
        contains(&bioma, expected)?;
    }
    for forbidden in ["name: otel-collector", "secretName: bioma-ingress-tls"] {
        if bioma.contains(forbidden) {
            bail!("Bioma k3d render must not contain `{forbidden}`");
        }
    }

    let sumo_cluster = fs::read_to_string("deploy/local/k3d/cluster.yaml")?;
    contains(&sumo_cluster, "name: veoveo-sumo")?;
    contains(&sumo_cluster, "127.0.0.1:8780:80")?;

    let bioma_cluster = fs::read_to_string("examples/bioma/k3d.yaml")?;
    contains(&bioma_cluster, "name: veoveo-bioma")?;
    contains(&bioma_cluster, "127.0.0.1:8781:80")?;
    let tunnel: Value = serde_json::from_str(&fs::read_to_string(
        "examples/bioma/cloudflare-tunnel.json",
    )?)?;
    let ingress = tunnel
        .pointer("/config/ingress")
        .and_then(Value::as_array)
        .context("Bioma Cloudflare configuration omitted ingress")?;
    ensure!(
        ingress.iter().any(|route| {
            route.get("hostname").and_then(Value::as_str) == Some("veoveo.bioma.ai")
                && route.get("service").and_then(Value::as_str)
                    == Some("http://traefik.kube-system.svc.cluster.local:80")
        }),
        "Bioma tunnel must route the public hostname to in-cluster Traefik"
    );

    let sumo = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "sumo".into(),
            "showcase/sumo/deploy/helm".into(),
            "--namespace".into(),
            "veoveo".into(),
        ],
        [],
    )?;
    for expected in [
        "image: veoveo/sumo-sim:1.27.1",
        "image: veoveo/sumo-mcp:0.1.0",
        "nodePort: 30895",
        "value: sumo-mcp:8795",
        "rerun+http://recording-ingest:9876/proxy",
        "runAsUser: 10001",
    ] {
        contains(&sumo, expected)?;
    }
    if sumo.contains("tcpSocket:") {
        bail!("SUMO chart must not probe the single-client TraCI socket");
    }
    if sumo.contains("OTEL_EXPORTER_OTLP_ENDPOINT") {
        bail!("SUMO chart must not export telemetry when its profile disables telemetry");
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

    println!("helm config smoke ok");
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
