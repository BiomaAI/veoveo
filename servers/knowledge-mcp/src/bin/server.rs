use clap::Parser;
use secrecy::SecretString;
use std::{path::PathBuf, sync::Arc};
use veoveo_embedding_client::{EmbeddingClient, EmbeddingClientConfig, EmbeddingEndpoint};
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
    let embeddings = EmbeddingClient::connect(EmbeddingClientConfig::new(
        EmbeddingEndpoint::parse(&args.embedding_endpoint)?,
        args.embedding_api_key,
        space,
    ))
    .await?;
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
    let server = veoveo_knowledge_mcp::mcp::KnowledgeMcp::new(store, Arc::new(embeddings));
    let router = axum::Router::new().nest(
        endpoint.mount_path(),
        veoveo_knowledge_mcp::host::router(server, verifier, allowed_hosts, cancel.child_token()),
    );
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, args.port)).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancel.cancel();
        })
        .await?;
    Ok(())
}
