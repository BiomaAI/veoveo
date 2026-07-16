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
        [("VEOVEO_INTERNAL_TRUST_JWKS", INTERNAL_TRUST_JWKS.into())],
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
    platform: &PlatformStoreSmoke,
    artifact_service_url: &str,
    log: &Path,
) -> Result<ChildGuard> {
    let mut env = platform.runtime_env();
    env.extend([
        ("MEDIA_PROVIDER_API_KEY", "smoke".into()),
        (
            "MEDIA_PROVIDER_WEBHOOK_SECRET",
            "whsec_smoke-webhook-secret".into(),
        ),
        ("VEOVEO_INTERNAL_TRUST_JWKS", INTERNAL_TRUST_JWKS.into()),
    ]);
    ChildGuard::spawn(
        media,
        [
            "--port".into(),
            port.to_string().into(),
            "--public-base-url".into(),
            public_base_url.into(),
            "--allow-loopback-hosts".into(),
            "--artifact-service-url".into(),
            artifact_service_url.into(),
        ],
        env,
        log,
    )
}

pub(crate) fn spawn_media_memory_smoke(
    media: &Path,
    port: u16,
    public_base_url: &str,
    platform: &PlatformStoreSmoke,
    provider_base_url: &str,
    artifact_service_url: &str,
    log: &Path,
) -> Result<ChildGuard> {
    let mut env = platform.runtime_env();
    env.extend([
        (
            "MEDIA_PROVIDER_WEBHOOK_SECRET",
            "whsec_smoke-webhook-secret".into(),
        ),
        ("MEDIA_PROVIDER_API_KEY", "smoke".into()),
        ("VEOVEO_INTERNAL_TRUST_JWKS", INTERNAL_TRUST_JWKS.into()),
    ]);
    ChildGuard::spawn(
        media,
        [
            "--port".into(),
            port.to_string().into(),
            "--public-base-url".into(),
            public_base_url.into(),
            "--allow-loopback-hosts".into(),
            "--artifact-service-url".into(),
            artifact_service_url.into(),
            "--provider-base-url".into(),
            provider_base_url.into(),
        ],
        env,
        log,
    )
}

pub(crate) fn spawn_frames_smoke(
    frames: &Path,
    port: u16,
    public_base_url: &str,
    artifact_service_url: &str,
    platform: &PlatformStoreSmoke,
    log: &Path,
) -> Result<ChildGuard> {
    let mut env = platform.runtime_env();
    env.push(("VEOVEO_INTERNAL_TRUST_JWKS", INTERNAL_TRUST_JWKS.into()));
    ChildGuard::spawn(
        frames,
        [
            "--port".into(),
            port.to_string().into(),
            "--public-base-url".into(),
            public_base_url.into(),
            "--allow-loopback-hosts".into(),
            "--artifact-service-url".into(),
            artifact_service_url.into(),
        ],
        env,
        log,
    )
}

pub(crate) fn spawn_datasheet_smoke(
    datasheet: &Path,
    port: u16,
    public_base_url: &str,
    artifact_service_url: &str,
    platform: &PlatformStoreSmoke,
    log: &Path,
) -> Result<ChildGuard> {
    let mut env = platform.runtime_env();
    env.push(("VEOVEO_INTERNAL_TRUST_JWKS", INTERNAL_TRUST_JWKS.into()));
    ChildGuard::spawn(
        datasheet,
        [
            "--port".into(),
            port.to_string().into(),
            "--public-base-url".into(),
            public_base_url.into(),
            "--allow-loopback-hosts".into(),
            "--artifact-service-url".into(),
            artifact_service_url.into(),
        ],
        env,
        log,
    )
}

pub(crate) fn spawn_duckdb_smoke(
    duckdb: &Path,
    port: u16,
    public_base_url: &str,
    data_dir: &Path,
    artifact_service_url: &str,
    platform: &PlatformStoreSmoke,
    log: &Path,
) -> Result<ChildGuard> {
    let mut env = platform.runtime_env();
    env.push(("VEOVEO_INTERNAL_TRUST_JWKS", INTERNAL_TRUST_JWKS.into()));
    ChildGuard::spawn(
        duckdb,
        [
            "--port".into(),
            port.to_string().into(),
            "--public-base-url".into(),
            public_base_url.into(),
            "--allow-loopback-hosts".into(),
            "--database-dir".into(),
            data_dir.join("databases").as_os_str().to_os_string(),
            "--exchange-dir".into(),
            data_dir.join("exchange").as_os_str().to_os_string(),
            "--spill-dir".into(),
            data_dir.join("spill").as_os_str().to_os_string(),
            "--artifact-service-url".into(),
            artifact_service_url.into(),
        ],
        env,
        log,
    )
}

pub(crate) fn spawn_optimization_smoke(
    optimization: &Path,
    port: u16,
    public_base_url: &str,
    state_db: &Path,
    artifact_service_url: &str,
    log: &Path,
) -> Result<ChildGuard> {
    ChildGuard::spawn(
        optimization,
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
        [("VEOVEO_INTERNAL_TRUST_JWKS", INTERNAL_TRUST_JWKS.into())],
        log,
    )
}

const SURREAL_ROOT_USER: &str = "root";
const SURREAL_ROOT_PASSWORD: &str = "root";
pub(crate) const SURREAL_RUNTIME_USER: &str = "veoveo_runtime";
pub(crate) const SURREAL_RUNTIME_PASSWORD: &str = "runtime-secret";

pub(crate) struct PlatformStoreSmoke {
    pub(crate) endpoint: String,
    pub(crate) namespace: String,
    pub(crate) database: String,
    _container: ContainerGuard,
}

impl PlatformStoreSmoke {
    fn root_env(&self) -> Vec<(&'static str, OsString)> {
        self.env("root", SURREAL_ROOT_USER, SURREAL_ROOT_PASSWORD)
    }

    pub(crate) fn runtime_env(&self) -> Vec<(&'static str, OsString)> {
        self.env("database", SURREAL_RUNTIME_USER, SURREAL_RUNTIME_PASSWORD)
    }

    fn env(
        &self,
        auth_level: &str,
        username: &str,
        password: &str,
    ) -> Vec<(&'static str, OsString)> {
        vec![
            ("VEOVEO_SURREAL_ENDPOINT", self.endpoint.clone().into()),
            ("VEOVEO_SURREAL_NAMESPACE", self.namespace.clone().into()),
            ("VEOVEO_SURREAL_DATABASE", self.database.clone().into()),
            ("VEOVEO_SURREAL_AUTH_LEVEL", auth_level.into()),
            ("VEOVEO_SURREAL_USERNAME", username.into()),
            ("VEOVEO_SURREAL_PASSWORD", password.into()),
        ]
    }
}

async fn spawn_surreal_platform() -> Result<PlatformStoreSmoke> {
    let host_port = reserve_local_port()?;
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let container_name = format!("veoveo-smoke-surreal-{suffix}");
    let container = ContainerGuard::new(container_name.clone());
    run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "-d".into(),
            "--name".into(),
            container_name.into(),
            "-p".into(),
            format!("127.0.0.1:{host_port}:8000").into(),
            "surrealdb/surrealdb:v3.2.1".into(),
            "start".into(),
            "--log".into(),
            "warn".into(),
            "--user".into(),
            SURREAL_ROOT_USER.into(),
            "--pass".into(),
            SURREAL_ROOT_PASSWORD.into(),
            "memory".into(),
        ],
        [],
    )?;
    let endpoint = format!("ws://127.0.0.1:{host_port}");
    wait_for_http(&format!("http://127.0.0.1:{host_port}/ready")).await?;
    Ok(PlatformStoreSmoke {
        endpoint,
        namespace: "veoveo_smoke".to_owned(),
        database: format!("platform_{suffix}"),
        _container: container,
    })
}

async fn initialize_surreal_platform(platform: &PlatformStoreSmoke) -> Result<()> {
    use secrecy::SecretString;
    use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials};

    let config = StoreConfig::builder(
        &platform.endpoint,
        &platform.namespace,
        &platform.database,
        StoreCredentials::root(SURREAL_ROOT_USER, SURREAL_ROOT_PASSWORD),
    )
    .migrate_on_connect(true)
    .build()?;
    let store = PlatformStore::connect(config).await?;
    store
        .replace_database_editor(
            SURREAL_RUNTIME_USER,
            &SecretString::from(SURREAL_RUNTIME_PASSWORD),
        )
        .await?;
    Ok(())
}

pub(crate) async fn spawn_platform_store_smoke() -> Result<PlatformStoreSmoke> {
    let platform = spawn_surreal_platform().await?;
    initialize_surreal_platform(&platform).await?;
    Ok(platform)
}

pub(crate) async fn spawn_gateway_platform_store(
    gateway: &Path,
    control_plane: &Path,
) -> Result<PlatformStoreSmoke> {
    let platform = spawn_surreal_platform().await?;
    let mut bootstrap_env = platform.root_env();
    bootstrap_env.extend([
        (
            "VEOVEO_SURREAL_RUNTIME_USERNAME",
            SURREAL_RUNTIME_USER.into(),
        ),
        (
            "VEOVEO_SURREAL_RUNTIME_PASSWORD",
            SURREAL_RUNTIME_PASSWORD.into(),
        ),
    ]);
    run_checked(
        gateway,
        [
            "installation-bootstrap".into(),
            "--control-plane".into(),
            control_plane.as_os_str().to_os_string(),
            "--applied-by".into(),
            "smoke-platform-bootstrap".into(),
        ],
        bootstrap_env,
    )?;
    let validation = run_checked(
        gateway,
        ["control-plane-validate".into()],
        platform.runtime_env(),
    )?;
    contains(&validation, "ok: revision")?;
    Ok(platform)
}

pub(crate) fn gateway_serve_args(port: u16, platform: &PlatformStoreSmoke) -> Vec<OsString> {
    vec![
        "serve".into(),
        "--port".into(),
        port.to_string().into(),
        "--public-base-url".into(),
        PUBLIC_BASE_URL.into(),
        "--surreal-endpoint".into(),
        platform.endpoint.clone().into(),
        "--surreal-namespace".into(),
        platform.namespace.clone().into(),
        "--surreal-database".into(),
        platform.database.clone().into(),
        "--surreal-auth-level".into(),
        "database".into(),
        "--surreal-username".into(),
        SURREAL_RUNTIME_USER.into(),
        "--surreal-password".into(),
        SURREAL_RUNTIME_PASSWORD.into(),
        "--refresh-delivery-key-b64".into(),
        REFRESH_DELIVERY_KEY_B64.into(),
        "--refresh-delivery-window-seconds".into(),
        REFRESH_DELIVERY_WINDOW_SECONDS.to_string().into(),
        "--allow-loopback-hosts".into(),
    ]
}

pub(crate) fn reserve_local_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

/// A running artifact-service backed by the SurrealDB platform store and an
/// in-memory object store. Guards tear both down on drop.
pub(crate) struct ArtifactServiceSmoke {
    pub(crate) url: String,
    pub(crate) platform: PlatformStoreSmoke,
    _child: ChildGuard,
}

pub(crate) async fn spawn_artifact_service_smoke(
    artifact_service: &Path,
    log: &Path,
) -> Result<ArtifactServiceSmoke> {
    let platform = spawn_platform_store_smoke().await?;
    let bind_port = reserve_local_port()?;
    let url = format!("http://127.0.0.1:{bind_port}");
    let mut service_env = platform.runtime_env();
    service_env.extend([
        (
            "ARTIFACT_SERVICE_BIND",
            format!("127.0.0.1:{bind_port}").into(),
        ),
        ("ARTIFACT_PUBLIC_BASE_URL", url.clone().into()),
        ("VEOVEO_INTERNAL_TRUST_JWKS", INTERNAL_TRUST_JWKS.into()),
        ("ARTIFACT_STORE", "memory".into()),
        (
            "ARTIFACT_ALLOWED_AUDIENCES",
            "media,timeseries,optimization,duckdb,frames,map,datasheet".into(),
        ),
    ]);
    let child = ChildGuard::spawn(artifact_service, Vec::<OsString>::new(), service_env, log)?;
    wait_for_http(&format!("{url}/healthz")).await?;
    Ok(ArtifactServiceSmoke {
        url,
        platform,
        _child: child,
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
