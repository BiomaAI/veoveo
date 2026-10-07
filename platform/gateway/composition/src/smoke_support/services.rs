#[path = "../../../../../testing/fixtures/module_lanes.rs"]
mod module_lanes;
use super::*;

pub fn spawn_fake_hosted_mcp(
    port: u16,
    server: &str,
    scheme: &str,
    ready_file: &Path,
    log: &Path,
) -> Result<ChildGuard> {
    ChildGuard::spawn(
        &veoveo_testing_support::artifacts::executable(
            "veoveo-gateway-composition",
            "gateway-smoke-support",
        )?,
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
    .map(ChildGuard::with_owner_scope)
    .map(ChildGuard::with_owner_scope)
}

pub fn spawn_fake_media_provider(
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
    ChildGuard::spawn(
        &veoveo_testing_support::artifacts::executable("veoveo-media-mcp", "media-smoke")?,
        args,
        [],
        log,
    )
    .map(ChildGuard::with_owner_scope)
    .map(ChildGuard::with_owner_scope)
}

pub fn spawn_media_s3_smoke(
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

pub fn spawn_media_memory_smoke(
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

pub fn spawn_frames_smoke(
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
            "serve".into(),
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

pub fn spawn_datasheet_smoke(
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

#[allow(
    clippy::too_many_arguments,
    reason = "the smoke process boundary keeps each executable path and runtime input explicit"
)]
pub fn spawn_duckdb_smoke(
    duckdb: &Path,
    spatial_extension: &Path,
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
            "--spatial-extension".into(),
            spatial_extension.as_os_str().to_os_string(),
            "--artifact-service-url".into(),
            artifact_service_url.into(),
        ],
        env,
        log,
    )
}

pub struct OptimizationSmokeConfig<'a> {
    pub optimization: &'a Path,
    pub port: u16,
    pub public_base_url: &'a str,
    pub workspace: &'a Path,
    pub executor_socket: &'a Path,
    pub artifact_service_url: &'a str,
    pub platform: &'a PlatformStoreSmoke,
    pub log: &'a Path,
}

pub fn spawn_optimization_smoke(config: &OptimizationSmokeConfig<'_>) -> Result<ChildGuard> {
    let mut env = config.platform.runtime_env();
    env.push(("VEOVEO_INTERNAL_TRUST_JWKS", INTERNAL_TRUST_JWKS.into()));
    ChildGuard::spawn(
        config.optimization,
        [
            "--port".into(),
            config.port.to_string().into(),
            "--public-base-url".into(),
            config.public_base_url.into(),
            "--allow-loopback-hosts".into(),
            "--optimization-workspace".into(),
            config.workspace.as_os_str().to_os_string(),
            "--executor-socket".into(),
            config.executor_socket.as_os_str().to_os_string(),
            "--artifact-service-url".into(),
            config.artifact_service_url.into(),
        ],
        env,
        config.log,
    )
}

pub struct CuoptExecutorSmoke {
    pub socket: PathBuf,
    _container: ContainerGuard,
}

pub fn spawn_cuopt_executor_smoke(runtime_dir: &Path) -> Result<CuoptExecutorSmoke> {
    fs::create_dir_all(runtime_dir)?;
    let runtime_dir = fs::canonicalize(runtime_dir)?;
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let container_name = format!("veoveo-smoke-cuopt-{suffix}");
    let container = ContainerGuard::new(container_name.clone())?;
    let uid = run_checked(Path::new("id"), ["-u".into()], [])?
        .trim()
        .to_owned();
    let gid = run_checked(Path::new("id"), ["-g".into()], [])?
        .trim()
        .to_owned();
    let image = env::var("VEOVEO_CUOPT_EXECUTOR_IMAGE")
        .unwrap_or_else(|_| "veoveo/cuopt-executor:0.1.0".to_owned());
    run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "--cidfile".into(),
            container.cid_file().as_os_str().to_os_string(),
            "-d".into(),
            "--name".into(),
            container_name.clone().into(),
            "--gpus".into(),
            "all".into(),
            "--read-only".into(),
            "--user".into(),
            format!("{uid}:{gid}").into(),
            "--shm-size".into(),
            "8g".into(),
            "--tmpfs".into(),
            format!("/tmp:rw,nosuid,nodev,noexec,uid={uid},gid={gid}").into(),
            "-e".into(),
            "NVIDIA_DRIVER_CAPABILITIES=compute,utility".into(),
            "-e".into(),
            "VEOVEO_CUOPT_SOCKET=/run/veoveo-cuopt/executor.sock".into(),
            "-v".into(),
            format!("{}:/run/veoveo-cuopt", runtime_dir.display()).into(),
            image.into(),
        ],
        [],
    )?;
    let socket = runtime_dir.join("executor.sock");
    for _ in 0..120 {
        if socket.exists()
            && Command::new("docker")
                .args([
                    "exec",
                    container_name.as_str(),
                    "python",
                    "-m",
                    "veoveo_cuopt_executor.healthcheck",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        {
            return Ok(CuoptExecutorSmoke {
                socket,
                _container: container,
            });
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    bail!("cuOpt executor did not become ready")
}

const SURREAL_ROOT_USER: &str = "root";
const SURREAL_ROOT_PASSWORD: &str = "root";
pub const SURREAL_RUNTIME_USER: &str = "veoveo_runtime";
pub const SURREAL_RUNTIME_PASSWORD: &str = "runtime-secret";

pub struct PlatformStoreSmoke {
    pub endpoint: String,
    pub namespace: String,
    pub database: String,
    _container: ContainerGuard,
    module_directory: tempfile::TempDir,
}

impl PlatformStoreSmoke {
    fn root_env(&self) -> Vec<(&'static str, OsString)> {
        self.env("root", SURREAL_ROOT_USER, SURREAL_ROOT_PASSWORD)
    }

    pub fn runtime_env(&self) -> Vec<(&'static str, OsString)> {
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
            (
                "VEOVEO_MODULE_PLAN",
                self.module_directory
                    .path()
                    .join("plan.json")
                    .into_os_string(),
            ),
            (
                "VEOVEO_MODULE_COMPOSITION",
                format!("sha256:{}", "1".repeat(64)).into(),
            ),
            ("VEOVEO_INSTALLATION_GENERATION", "1".into()),
            ("VEOVEO_CREDENTIAL_REVISION", "smoke-runtime-v1".into()),
            (
                "VEOVEO_SURREAL_RUNTIME_USERNAME",
                SURREAL_RUNTIME_USER.into(),
            ),
        ]
    }
}

async fn spawn_surreal_platform() -> Result<PlatformStoreSmoke> {
    let host_port = reserve_local_port()?;
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let container_name = format!("veoveo-smoke-surreal-{suffix}");
    let container = ContainerGuard::new(container_name.clone())?;
    run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "--cidfile".into(),
            container.cid_file().as_os_str().to_os_string(),
            "-d".into(),
            "--name".into(),
            container_name.into(),
            "-p".into(),
            format!("127.0.0.1:{host_port}:8000").into(),
            "surrealdb/surrealdb:v3.3.0".into(),
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
        module_directory: tempfile::tempdir()?,
    })
}

async fn initialize_surreal_platform(platform: &PlatformStoreSmoke) -> Result<()> {
    use veoveo_modules::*;
    use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials};

    let config = StoreConfig::builder(
        &platform.endpoint,
        &platform.namespace,
        &platform.database,
        StoreCredentials::root(SURREAL_ROOT_USER, SURREAL_ROOT_PASSWORD),
    )
    .build()?;
    let store = PlatformStore::connect(config).await?;
    let registry = module_lanes::registry(Vec::new())?;
    let selection = ModuleSelectionDocument::new(
        Vec::new(),
        InstallationGeneration::new(1)?,
        CredentialRevision::new("smoke-runtime-v1")?,
    )?;
    let plan = ModulePlanDocument::generate(
        &registry,
        &selection,
        CompositionIdentity::new(format!("sha256:{}", "1".repeat(64)))?,
        Vec::new(),
    )?;
    std::fs::write(
        platform.module_directory.path().join("plan.json"),
        serde_json::to_vec(&plan)?,
    )?;
    let prepared = veoveo_modules::runner::prepare(registry.select(Vec::new())?)?;
    let key = preparation_key(&plan, SURREAL_RUNTIME_USER)?;
    prepared.claim_preparation(store.client(), &key).await?;
    prepared
        .complete_preparation(
            store.client(),
            &key,
            veoveo_modules::runner::DatabaseEditorCredentials::new(
                SURREAL_RUNTIME_USER,
                SURREAL_RUNTIME_PASSWORD,
            )?,
        )
        .await?;
    prepared.apply(store.client()).await?;
    Ok(())
}

pub async fn spawn_platform_store_smoke() -> Result<PlatformStoreSmoke> {
    let platform = spawn_surreal_platform().await?;
    initialize_surreal_platform(&platform).await?;
    Ok(platform)
}

pub async fn spawn_gateway_platform_store(
    gateway: &Path,
    control_plane: &Path,
) -> Result<PlatformStoreSmoke> {
    let platform = spawn_surreal_platform().await?;
    bootstrap_gateway_platform_store(gateway, control_plane, &platform).await?;
    Ok(platform)
}

pub async fn bootstrap_gateway_platform_store(
    gateway: &Path,
    control_plane: &Path,
    platform: &PlatformStoreSmoke,
) -> Result<()> {
    // The fixture binding identifies this local process test; installed compilation
    // obtains the digest from the exact locked OCI image instead.
    let selection_path = platform.module_directory.path().join("selection.json");
    std::fs::write(
        &selection_path,
        serde_json::to_vec(&serde_json::json!({
            "format":"veoveo.ai/module-selection/v1", "enabled":["agents","computers","frames","map","media","recordings","time","uav","workspace"],
            "generation":"1", "credentialRevision":"smoke-runtime-v1"
        }))?,
    )?;
    let generated = run_checked(
        gateway,
        [
            "module-plan".into(),
            "--modules".into(),
            selection_path.into_os_string(),
            "--composition".into(),
            format!("sha256:{}", "1".repeat(64)).into(),
        ],
        [],
    )?;
    let plan: veoveo_modules::ModulePlanDocument = serde_json::from_str(&generated)?;
    std::fs::write(
        platform.module_directory.path().join("plan.json"),
        generated,
    )?;
    let mut prepare_env = platform.root_env();
    prepare_env.push((
        "VEOVEO_SURREAL_RUNTIME_PASSWORD",
        SURREAL_RUNTIME_PASSWORD.into(),
    ));
    run_checked(gateway, ["installation-prepare".into()], prepare_env)?;
    for lane in plan.lanes() {
        run_checked(
            gateway,
            [
                "module-migrate".into(),
                "--module".into(),
                lane.module.as_str().into(),
            ],
            platform.root_env(),
        )?;
    }
    run_checked(gateway, ["module-status".into()], platform.runtime_env())?;
    run_checked(
        gateway,
        [
            "control-plane-publish".into(),
            "--control-plane".into(),
            control_plane.as_os_str().to_os_string(),
            "--applied-by".into(),
            "smoke-platform-bootstrap".into(),
        ],
        platform.runtime_env(),
    )?;
    let validation = run_checked(
        gateway,
        ["control-plane-validate".into()],
        platform.runtime_env(),
    )?;
    contains(&validation, "ok: revision")?;
    Ok(())
}

pub fn gateway_serve_args(port: u16, platform: &PlatformStoreSmoke) -> Vec<OsString> {
    gateway_serve_args_for_base(port, platform, PUBLIC_BASE_URL)
}

pub fn gateway_serve_args_for_base(
    port: u16,
    platform: &PlatformStoreSmoke,
    public_base_url: &str,
) -> Vec<OsString> {
    vec![
        "serve".into(),
        "--port".into(),
        port.to_string().into(),
        "--public-base-url".into(),
        public_base_url.into(),
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
        "--audit-signing-key-b64".into(),
        AUDIT_SIGNING_KEY_B64.into(),
        "--audit-retention-days".into(),
        "1".into(),
        "--refresh-delivery-key-b64".into(),
        REFRESH_DELIVERY_KEY_B64.into(),
        "--refresh-delivery-window-seconds".into(),
        REFRESH_DELIVERY_WINDOW_SECONDS.to_string().into(),
        "--allow-loopback-hosts".into(),
    ]
}

pub fn reserve_local_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

/// A running artifact-service backed by the SurrealDB platform store and an
/// in-memory object store. Guards tear both down on drop.
pub struct ArtifactServiceSmoke {
    pub url: String,
    pub platform: PlatformStoreSmoke,
    _child: ChildGuard,
}

pub async fn spawn_artifact_service_smoke(
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

pub fn write_edge_caddyfile(path: &Path, gateway_port: u16, media_port: u16) -> Result<()> {
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
