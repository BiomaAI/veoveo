use super::*;

pub(crate) fn spawn_fake_hosted_mcp(
    conformance: &Path,
    port: u16,
    server: &str,
    scheme: &str,
    ready_file: &Path,
    log: &Path,
) -> Result<ChildGuard> {
    ChildGuard::spawn(
        conformance,
        [
            "fake-hosted-mcp".into(),
            "--port".into(),
            port.to_string().into(),
            "--server".into(),
            server.into(),
            "--scheme".into(),
            scheme.into(),
            "--ready-file".into(),
            ready_file.as_os_str().to_os_string(),
        ],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
        log,
    )
}

pub(crate) fn spawn_fake_media_provider(
    conformance: &Path,
    port: u16,
    ready_file: &Path,
    log: &Path,
    completion_delay_ms: Option<u64>,
) -> Result<ChildGuard> {
    let mut args = vec![
        "fake-media-provider".into(),
        "--port".into(),
        port.to_string().into(),
        "--ready-file".into(),
        ready_file.as_os_str().to_os_string(),
    ];
    if let Some(delay) = completion_delay_ms {
        args.push("--completion-delay-ms".into());
        args.push(delay.to_string().into());
    }
    ChildGuard::spawn(conformance, args, [], log)
}

pub(crate) fn spawn_media_s3_smoke(
    media: &Path,
    port: u16,
    public_base_url: &str,
    state_db: &Path,
    artifact_service_url: &str,
    log: &Path,
) -> Result<ChildGuard> {
    ChildGuard::spawn(
        media,
        [
            "--port".into(),
            port.to_string().into(),
            "--public-base-url".into(),
            public_base_url.into(),
            "--allow-loopback-hosts".into(),
            "--state-db".into(),
            state_db.as_os_str().to_os_string(),
            "--artifact-service-url".into(),
            artifact_service_url.into(),
        ],
        [
            ("MEDIA_PROVIDER_API_KEY", "smoke".into()),
            ("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
        ],
        log,
    )
}

pub(crate) fn spawn_media_memory_smoke(
    media: &Path,
    port: u16,
    public_base_url: &str,
    state_db: &Path,
    provider_base_url: &str,
    artifact_service_url: &str,
    log: &Path,
) -> Result<ChildGuard> {
    ChildGuard::spawn(
        media,
        [
            "--port".into(),
            port.to_string().into(),
            "--public-base-url".into(),
            public_base_url.into(),
            "--allow-loopback-hosts".into(),
            "--state-db".into(),
            state_db.as_os_str().to_os_string(),
            "--artifact-service-url".into(),
            artifact_service_url.into(),
            "--provider-base-url".into(),
            provider_base_url.into(),
        ],
        [
            ("MEDIA_PROVIDER_WEBHOOK_SECRET", "".into()),
            ("MEDIA_PROVIDER_API_KEY", "smoke".into()),
            ("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
        ],
        log,
    )
}

pub(crate) fn spawn_coordinates_smoke(
    coordinates: &Path,
    port: u16,
    public_base_url: &str,
    artifact_service_url: &str,
    log: &Path,
) -> Result<ChildGuard> {
    ChildGuard::spawn(
        coordinates,
        [
            "--port".into(),
            port.to_string().into(),
            "--public-base-url".into(),
            public_base_url.into(),
            "--allow-loopback-hosts".into(),
            "--artifact-service-url".into(),
            artifact_service_url.into(),
        ],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
        log,
    )
}

pub(crate) struct GatewayControlDbSmoke {
    pub(crate) url: String,
    _container: ContainerGuard,
}

pub(crate) async fn spawn_gateway_control_db(
    gateway: &Path,
    control_plane: &Path,
) -> Result<GatewayControlDbSmoke> {
    let host_port = reserve_local_port()?;
    let container_name = format!("veoveo-smoke-postgres-{}", uuid::Uuid::new_v4());
    let container = ContainerGuard::new(container_name.clone());
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
    wait_for_postgres_container(&container_name, "veoveo_gateway", "veoveo_gateway").await?;

    let url =
        format!("postgresql://veoveo_gateway:veoveo_gateway@127.0.0.1:{host_port}/veoveo_gateway");
    run_checked(
        gateway,
        [
            "control-plane-seed".into(),
            "--control-db-url".into(),
            url.clone().into(),
            "--control-plane".into(),
            control_plane.as_os_str().to_os_string(),
            "--applied-by".into(),
            "smoke#control-db".into(),
        ],
        [],
    )?;
    run_checked(
        gateway,
        [
            "validate-db".into(),
            "--control-db-url".into(),
            url.clone().into(),
        ],
        [],
    )?;

    Ok(GatewayControlDbSmoke {
        url,
        _container: container,
    })
}

pub(crate) fn write_edge_caddyfile(path: &Path, gateway_port: u16, media_port: u16) -> Result<()> {
    let caddyfile = format!(
        r#"{{
    admin off
    auto_https off
}}

:8080 {{
    handle /mcp* {{
        reverse_proxy host.docker.internal:{gateway_port}
    }}
    handle /oauth* {{
        reverse_proxy host.docker.internal:{gateway_port}
    }}
    handle /.well-known/oauth-* {{
        reverse_proxy host.docker.internal:{gateway_port}
    }}
    handle /admin* {{
        reverse_proxy host.docker.internal:{gateway_port}
    }}
    handle /healthz {{
        reverse_proxy host.docker.internal:{gateway_port}
    }}
    handle /readyz {{
        reverse_proxy host.docker.internal:{gateway_port}
    }}
    handle /media/webhooks* {{
        reverse_proxy host.docker.internal:{media_port}
    }}
    handle /media/files* {{
        reverse_proxy host.docker.internal:{media_port}
    }}
    handle /media/artifacts* {{
        reverse_proxy host.docker.internal:{media_port}
    }}
    handle /media/healthz {{
        reverse_proxy host.docker.internal:{media_port}
    }}
    respond /media/mcp* 404
    respond 404
}}
"#
    );
    fs::write(path, caddyfile)?;
    Ok(())
}

pub(crate) fn gateway_serve_args(
    port: u16,
    control_db_url: &str,
    state_db: &Path,
) -> Vec<OsString> {
    vec![
        "serve".into(),
        "--port".into(),
        port.to_string().into(),
        "--public-base-url".into(),
        PUBLIC_BASE_URL.into(),
        "--control-db-url".into(),
        control_db_url.into(),
        "--state-db".into(),
        state_db.as_os_str().to_os_string(),
        "--allow-loopback-hosts".into(),
    ]
}

pub(crate) fn reserve_local_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

async fn wait_for_postgres_container(container_name: &str, user: &str, db: &str) -> Result<()> {
    for _ in 0..150 {
        let output = run_raw(
            Path::new("docker"),
            [
                "exec".into(),
                container_name.into(),
                "pg_isready".into(),
                "-U".into(),
                user.into(),
                "-d".into(),
                db.into(),
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

/// A running artifact-service backed by a throwaway Postgres (grant ledger) and
/// an in-memory object store. Domain servers spawned in smoke point their
/// `--artifact-service-url` here. Guards tear both down on drop.
pub(crate) struct ArtifactServiceSmoke {
    pub(crate) url: String,
    _container: ContainerGuard,
    _child: ChildGuard,
}

pub(crate) async fn spawn_artifact_service_smoke(
    artifact_service: &Path,
    log: &Path,
) -> Result<ArtifactServiceSmoke> {
    // Postgres grant ledger (the service auto-migrates it on boot).
    let db_port = reserve_local_port()?;
    let container_name = format!("veoveo-smoke-artifact-db-{}", uuid::Uuid::new_v4());
    let container = ContainerGuard::new(container_name.clone());
    run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "-d".into(),
            "--name".into(),
            container_name.clone().into(),
            "-e".into(),
            "POSTGRES_DB=veoveo_artifact".into(),
            "-e".into(),
            "POSTGRES_USER=veoveo_artifact".into(),
            "-e".into(),
            "POSTGRES_PASSWORD=veoveo_artifact".into(),
            "-p".into(),
            format!("127.0.0.1:{db_port}:5432").into(),
            "postgres:18-alpine".into(),
        ],
        [],
    )?;
    wait_for_postgres_container(&container_name, "veoveo_artifact", "veoveo_artifact").await?;

    let database_url =
        format!("postgresql://veoveo_artifact:veoveo_artifact@127.0.0.1:{db_port}/veoveo_artifact");
    let bind_port = reserve_local_port()?;
    let url = format!("http://127.0.0.1:{bind_port}");
    let child = ChildGuard::spawn(
        artifact_service,
        Vec::<OsString>::new(),
        [
            (
                "ARTIFACT_SERVICE_BIND",
                format!("127.0.0.1:{bind_port}").into(),
            ),
            ("DATABASE_URL", database_url.into()),
            ("INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
            ("ARTIFACT_MASTER_KEY", ARTIFACT_MASTER_KEY.into()),
            ("ARTIFACT_STORE", "memory".into()),
            (
                "ARTIFACT_ALLOWED_AUDIENCES",
                "media,timeseries,optimization,duckdb,coordinates".into(),
            ),
        ],
        log,
    )?;
    wait_for_http(&format!("{url}/healthz")).await?;
    Ok(ArtifactServiceSmoke {
        url,
        _container: container,
        _child: child,
    })
}
