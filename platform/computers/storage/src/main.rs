use clap::Parser;
use serde::Deserialize;
use std::{
    net::SocketAddr,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
};
use veoveo_computer_storage::{
    Docker, Filesystem, HostIdentity, Journal, Service, StorageError, Template, plugin,
    transport::{self, TlsConfig},
};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    config: PathBuf,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    provider_id: uuid::Uuid,
    namespace: String,
    root: PathBuf,
    reserve_bytes: u64,
    templates: Vec<Template>,
    docker_socket: PathBuf,
    plugin_name: String,
    plugin_socket: PathBuf,
    listen: SocketAddr,
    tls: TlsConfig,
}
#[tokio::main]
async fn main() -> Result<(), StorageError> {
    let args = Args::parse();
    let file = std::fs::File::open(args.config).map_err(|_| StorageError::InvalidIdentity)?;
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| StorageError::InvalidIdentity)?;
    if bytes.len() > 65536 {
        return Err(StorageError::InvalidIdentity);
    }
    let config: Config =
        serde_json::from_slice(&bytes).map_err(|_| StorageError::InvalidIdentity)?;
    let tls = config.tls.load()?;
    if !config.plugin_socket.is_absolute() || config.plugin_socket.as_os_str().len() > 100 {
        return Err(StorageError::InvalidIdentity);
    }
    let parent = config
        .plugin_socket
        .parent()
        .ok_or(StorageError::InvalidIdentity)?;
    let metadata = std::fs::symlink_metadata(parent).map_err(|_| StorageError::InvalidIdentity)?;
    if !metadata.is_dir() || metadata.uid() != 0 || metadata.mode() & 0o777 != 0o700 {
        return Err(StorageError::InvalidIdentity);
    }
    let (journal, docker) =
        match Journal::reopen(config.root.clone(), config.provider_id, &config.namespace)? {
            Some(journal) => {
                let docker = Docker::new(
                    &config.docker_socket,
                    journal.identity().engine_id,
                    config.plugin_name,
                )?;
                (journal, docker)
            }
            None => {
                let docker = Docker::discover(&config.docker_socket, config.plugin_name).await?;
                let identity = HostIdentity {
                    provider_id: config.provider_id,
                    engine_id: docker.verify_engine().await?,
                    namespace: config.namespace,
                };
                (Journal::open(config.root, identity)?, docker)
            }
        };
    let service = Service::new(
        Filesystem::new(journal, config.reserve_bytes)?,
        docker,
        config.templates,
    )?;
    // The journal lock is held before replacing a socket left by a stopped
    // helper. Never unlink a listener owned by a live process.
    if config
        .plugin_socket
        .try_exists()
        .map_err(|_| StorageError::Unavailable)?
    {
        use std::os::unix::fs::FileTypeExt;
        let metadata = std::fs::symlink_metadata(&config.plugin_socket)
            .map_err(|_| StorageError::Unavailable)?;
        if !metadata.file_type().is_socket()
            || metadata.uid() != 0
            || tokio::net::UnixStream::connect(&config.plugin_socket)
                .await
                .is_ok()
        {
            return Err(StorageError::Busy);
        }
        std::fs::remove_file(&config.plugin_socket).map_err(|_| StorageError::Unavailable)?;
    }
    let plugin_listener = tokio::net::UnixListener::bind(&config.plugin_socket)
        .map_err(|_| StorageError::Unavailable)?;
    std::fs::set_permissions(
        &config.plugin_socket,
        std::fs::Permissions::from_mode(0o600),
    )
    .map_err(|_| StorageError::Unavailable)?;
    let worker_listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .map_err(|_| StorageError::Unavailable)?;
    tokio::select! {
        result = axum::serve(plugin_listener, plugin::router(service.clone())) => result.map_err(|_| StorageError::Unavailable),
        result = transport::serve(worker_listener, tls, service) => result,
    }
}
