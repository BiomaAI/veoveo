use clap::Parser;
use secrecy::SecretString;
use std::{path::PathBuf, sync::Arc};
use veoveo_embedding_client::{EmbeddingClient, EmbeddingClientConfig, EmbeddingEndpoint};
use veoveo_knowledge_mcp::{
    coordinator::CoordinatorState,
    indexing::{IndexingConfig, IndexingReadiness, IndexingService},
};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    PublicDeployment,
};
use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials};

#[derive(Parser)]
#[command(about = "Knowledge catalog, search and embedding MCP server")]
struct Args {
    #[arg(long, default_value_t = 8800)]
    port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    public_base_url: String,
    #[arg(long = "allowed-host")]
    allowed_hosts: Vec<String>,
    #[arg(long, default_value_t = false)]
    allow_loopback_hosts: bool,
    #[arg(long, env = "VEOVEO_SURREAL_ENDPOINT")]
    surreal_endpoint: String,
    #[arg(long, env = "VEOVEO_SURREAL_NAMESPACE")]
    surreal_namespace: String,
    #[arg(long, env = "VEOVEO_SURREAL_DATABASE")]
    surreal_database: String,
    #[arg(long, env = "VEOVEO_SURREAL_USERNAME")]
    surreal_username: String,
    #[arg(long, env = "VEOVEO_SURREAL_PASSWORD", hide_env_values = true)]
    surreal_password: SecretString,
    #[arg(long, env = "VEOVEO_INTERNAL_TRUST_JWKS", hide_env_values = true)]
    internal_trust_jwks: String,
    #[arg(long, env = "VEOVEO_EMBEDDING_ENDPOINT")]
    embedding_endpoint: String,
    #[arg(long, env = "VEOVEO_EMBEDDING_API_KEY", hide_env_values = true)]
    embedding_api_key: SecretString,
    #[arg(long, env = "VEOVEO_EMBEDDING_SPACE_FILE")]
    embedding_space_file: PathBuf,
    /// One typed machine/source configuration per tenant served by this process.
    #[arg(long, required = true, num_args = 1..)]
    indexing_config: Vec<PathBuf>,
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _telemetry = veoveo_mcp_contract::init_server_telemetry("veoveo-knowledge-mcp", "info")?;
    let args = Args::parse();
    let deployment = PublicDeployment::new(&args.public_base_url)?;
    let endpoint = deployment.server("knowledge")?;
    let verifier = GatewayInternalTokenVerifier::new(
        GATEWAY_INTERNAL_TOKEN_ISSUER.parse()?,
        "knowledge".parse()?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
    let store = PlatformStore::connect(
        StoreConfig::builder(
            &args.surreal_endpoint,
            args.surreal_namespace,
            args.surreal_database,
            StoreCredentials::database(args.surreal_username, args.surreal_password),
        )
        .build()?,
    )
    .await?;
    let space = serde_json::from_slice(&tokio::fs::read(&args.embedding_space_file).await?)?;
    let embeddings = Arc::new(
        EmbeddingClient::connect(EmbeddingClientConfig::new(
            EmbeddingEndpoint::parse(&args.embedding_endpoint)?,
            args.embedding_api_key,
            space,
        ))
        .await?,
    );
    let mut allowed_hosts =
        veoveo_mcp_contract::public_allowed_hosts(&deployment, args.allow_loopback_hosts);
    for host in &args.allowed_hosts {
        anyhow::ensure!(
            veoveo_mcp_contract::parse_allowed_host_authority(host).is_some(),
            "invalid allowed host"
        );
    }
    allowed_hosts.extend(args.allowed_hosts);
    let cancel = tokio_util::sync::CancellationToken::new();
    let mut tenants = std::collections::BTreeSet::new();
    let mut configurations = Vec::new();
    for path in &args.indexing_config {
        let config: IndexingConfig = serde_json::from_slice(&tokio::fs::read(path).await?)?;
        anyhow::ensure!(
            tenants.insert(config.tenant.clone()),
            "duplicate indexing tenant configuration"
        );
        configurations.push(config);
    }
    anyhow::ensure!(
        (1..=128).contains(&configurations.len()),
        "configure 1..128 indexing tenants"
    );
    let mut workers = tokio::task::JoinSet::new();
    let mut states = Vec::new();
    for config in configurations {
        let (status, state) = tokio::sync::watch::channel(CoordinatorState::Starting);
        states.push(state);
        let store = store.clone();
        let embeddings = embeddings.clone();
        let stop = cancel.child_token();
        workers.spawn(async move {
            IndexingService {
                store: &store,
                embeddings: embeddings.as_ref(),
                config: &config,
            }
            .run(stop, status)
            .await
        });
    }
    let readiness = IndexingReadiness::new(states)?;
    let server = veoveo_knowledge_mcp::mcp::KnowledgeMcp::new(store, embeddings);
    let router = axum::Router::new().nest(
        endpoint.mount_path(),
        veoveo_knowledge_mcp::host::router(
            server,
            verifier,
            allowed_hosts,
            cancel.child_token(),
            readiness,
        ),
    );
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, args.port)).await?;
    let shutdown = cancel.clone();
    let mut http = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
    });
    let mut http_ended = false;
    let result = tokio::select! {
        signal = shutdown_signal() => signal,
        worker = workers.join_next() => match worker {
            Some(Ok(Err(error))) => Err(anyhow::Error::from(error)),
            Some(Err(error)) => Err(anyhow::Error::from(error)),
            _ => Err(anyhow::anyhow!("indexing worker ended before shutdown")),
        },
        finished = &mut http => { http_ended = true; finished.map_err(anyhow::Error::from).and_then(|result| result.map_err(anyhow::Error::from)) },
    };
    cancel.cancel();
    if tokio::time::timeout(std::time::Duration::from_secs(20), async {
        while workers.join_next().await.is_some() {}
    })
    .await
    .is_err()
    {
        workers.abort_all();
    }
    if !http_ended
        && tokio::time::timeout(std::time::Duration::from_secs(10), &mut http)
            .await
            .is_err()
    {
        http.abort();
    }
    result
}

async fn shutdown_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result?,
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    Ok(())
}
