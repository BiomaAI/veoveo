use super::{
    config::{Config, PLUGIN, PROVIDER_PORT, RUN, RegistryTransport, SOCKET, STATE},
    files,
    process::{self, Process},
};
use anyhow::{Context, Result, ensure};
use std::{fs, path::Path, time::Duration};
use tokio::{
    net::TcpStream,
    signal::unix::{SignalKind, signal},
    time::{Instant, sleep},
};

#[derive(Default)]
struct Children {
    daemon: Option<Process>,
    storage: Option<Process>,
    provider: Option<Process>,
}
impl Children {
    fn check(&mut self) -> Result<()> {
        for process in [&mut self.daemon, &mut self.storage, &mut self.provider]
            .into_iter()
            .flatten()
        {
            process.check()?;
        }
        Ok(())
    }
    async fn shutdown(&mut self) {
        let _ = fs::remove_file(format!("{RUN}/ready"));
        // Keep the helper serving volume callbacks throughout daemon shutdown.
        for (process, seconds) in [
            (&mut self.provider, 10),
            (&mut self.daemon, 20),
            (&mut self.storage, 5),
        ] {
            if let Some(mut child) = process.take() {
                child.stop(seconds).await;
            }
        }
    }
}
pub async fn run(config: Config) -> Result<()> {
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    files::directory(Path::new(STATE))?;
    ensure!(
        nix::sys::statfs::statfs(STATE)?.filesystem_type() == nix::sys::statfs::EXT4_SUPER_MAGIC,
        "compute host requires its persistent ext4 volume"
    );
    let _lock = files::lock(&Path::new(STATE).join("host.lock"))?;
    files::directory(Path::new(RUN))?;
    let _ = fs::remove_file(format!("{RUN}/ready"));
    for directory in ["docker", "provider"] {
        // Docker owns traversal permissions below the enclosing private state
        // directory. Preserve the daemon's normal data-root permissions.
        files::owned_directory(&Path::new(STATE).join(directory))?;
    }
    let journal = veoveo_computer_storage::Journal::reopen(
        Path::new(STATE).join("retained"),
        config.provider_id,
        &config.namespace,
    )?;
    let retained = journal.is_some();
    drop(journal);
    files::trust()?;
    config
        .storage_config()
        .tls
        .load()
        .context("validate retained storage TLS")?;
    veoveo_computer_storage::transport::TlsConfig {
        worker_ca: format!("{RUN}/trust/provider-ca.pem").into(),
        certificate: format!("{RUN}/trust/provider-server.pem").into(),
        private_key: format!("{RUN}/trust/provider-server-key.pem").into(),
    }
    .load()
    .context("validate provider TLS")?;
    ensure!(
        files::read(&Path::new(RUN).join("trust/provider-ca.pem"))?
            != files::read(&Path::new(RUN).join("trust/storage-ca.pem"))?,
        "provider and storage require separate trust roots"
    );
    files::write(
        &Path::new(RUN).join("storage.json"),
        &serde_json::to_vec(&config.storage_config())?,
    )?;
    files::write(
        &Path::new(RUN).join("provider.toml"),
        config.provider_config()?.as_bytes(),
    )?;
    let mut children = Children::default();
    let outcome = tokio::select! {
        result = serve(&config, retained, &mut children) => result,
        _ = terminate.recv() => Ok(()),
        _ = interrupt.recv() => Ok(()),
    };
    children.shutdown().await;
    outcome
}
fn register_plugin() -> Result<()> {
    fs::create_dir_all("/etc/docker/plugins")?;
    files::write(
        Path::new("/etc/docker/plugins/veoveo-retained.spec"),
        format!("unix://{PLUGIN}").as_bytes(),
    )
}
fn docker_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .unix_socket(SOCKET)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()?)
}
async fn api_ready() -> Result<()> {
    let reply = docker_client()?
        .get("http://localhost/_ping")
        .send()
        .await?;
    ensure!(
        reply.status().is_success(),
        "private Docker API is unavailable"
    );
    Ok(())
}
async fn wait_api(children: &mut Children) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        children.check()?;
        if api_ready().await.is_ok() {
            return Ok(());
        }
        ensure!(Instant::now() < deadline, "private Docker startup deadline");
        sleep(Duration::from_millis(100)).await;
    }
}
async fn serve(config: &Config, retained: bool, children: &mut Children) -> Result<()> {
    if retained {
        register_plugin()?;
    }
    let mut daemon = process::command("/usr/local/bin/dockerd");
    daemon.args([
        "--host",
        &format!("unix://{SOCKET}"),
        "--data-root",
        &format!("{STATE}/docker"),
        "--exec-root",
        &format!("{RUN}/docker-exec"),
        "--pidfile",
        &format!("{RUN}/docker.pid"),
        "--storage-driver=overlay2",
        "--iptables=true",
        "--ip6tables=false",
        "--ip-forward=true",
        "--ip-masq=true",
        "--live-restore=false",
        "--log-level=warn",
        "--group=root",
        "--log-driver=local",
        "--log-opt=max-size=10m",
        "--log-opt=max-file=3",
        "--bip",
        &format!("{}/24", config.bridge_address),
        "--default-address-pool",
        &format!("base={}/16,size=24", config.network_pool),
    ]);
    if config.registry.transport == RegistryTransport::DevelopmentHttp {
        daemon
            .arg("--insecure-registry")
            .arg(&config.registry.authority);
    }
    children.daemon = Some(Process::spawn("Docker", &mut daemon)?);
    if !retained {
        wait_api(children).await?;
    }
    children.storage = Some(Process::spawn(
        "storage",
        process::command("/usr/local/bin/veoveo-computer-storage")
            .args(["--config", &format!("{RUN}/storage.json")]),
    )?);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        children.check()?;
        if tokio::net::UnixStream::connect(PLUGIN).await.is_ok() {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "retained plugin startup deadline"
        );
        sleep(Duration::from_millis(100)).await;
    }
    if !retained {
        register_plugin()?;
    }
    wait_api(children).await?;
    // Preload only the declared installation registry closure. The provider
    // never downloads an image in response to a user's lifecycle request.
    for image in &config.images {
        let url = format!("http://localhost/v1.53/images/{image}/json");
        let present = docker_client()?.get(url).send().await?;
        if present.status() == reqwest::StatusCode::NOT_FOUND {
            process::finite(
                process::command("/usr/local/bin/docker").args([
                    "--host",
                    &format!("unix://{SOCKET}"),
                    "pull",
                    image,
                ]),
                300,
            )
            .await?;
        } else {
            ensure!(
                present.status().is_success(),
                "private image inventory unavailable"
            );
        }
        children.check()?;
    }
    let mut provider = process::command("/usr/local/bin/openshell-gateway");
    provider
        .env("XDG_STATE_HOME", format!("{STATE}/provider/state"))
        .env("XDG_DATA_HOME", format!("{STATE}/provider/data"))
        .env("XDG_CONFIG_HOME", format!("{STATE}/provider/config"))
        .args([
            "--config",
            &format!("{RUN}/provider.toml"),
            "--db-url",
            &format!("sqlite://{STATE}/provider/gateway.sqlite?mode=rwc"),
            "--tls-cert",
            &format!("{RUN}/trust/provider-server.pem"),
            "--tls-key",
            &format!("{RUN}/trust/provider-server-key.pem"),
            "--tls-client-ca",
            &format!("{RUN}/trust/provider-ca.pem"),
            "--enable-mtls-auth",
            "true",
            "--enable-loopback-service-http",
            "false",
        ]);
    children.provider = Some(Process::spawn("provider", &mut provider)?);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        children.check()?;
        if TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, PROVIDER_PORT))
            .await
            .is_ok()
        {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "provider listener startup deadline"
        );
        sleep(Duration::from_millis(100)).await;
    }
    files::write(
        &Path::new(RUN).join("ready"),
        b"veoveo.io/computer-host/v1\n",
    )?;
    eprintln!("compute-host: private runtime processes are ready");
    loop {
        children.check()?;
        sleep(Duration::from_millis(250)).await;
    }
}
pub async fn health() -> Result<()> {
    ensure!(
        Path::new(&format!("{RUN}/ready")).is_file(),
        "compute host is starting"
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        api_ready().await?;
        let _ = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, PROVIDER_PORT)).await?;
        let _ = tokio::net::UnixStream::connect(PLUGIN).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("compute host health deadline")?
}
