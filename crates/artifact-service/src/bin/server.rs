//! Artifact plane service binary.

use anyhow::Context;
use tracing_subscriber::EnvFilter;
use veoveo_artifact_service::config::Config;
use veoveo_artifact_service::crypto::TenantCipher;
use veoveo_artifact_service::http::{AppState, router};
use veoveo_artifact_service::{
    ArtifactService, EncryptedObjectStore, PlaneAuthenticator, PostgresGrantLedger,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    let config = Config::from_env().context("loading configuration")?;

    let object_store = config
        .object_store
        .build()
        .context("building object store")?;
    let cipher = TenantCipher::new(config.master_key.clone());
    let store = EncryptedObjectStore::new(object_store, cipher);

    let ledger =
        PostgresGrantLedger::connect(&config.database_url, config.db_max_connections).await?;
    ledger.migrate().await.context("running migrations")?;
    tracing::info!("grant ledger migrations applied");

    let service = ArtifactService::new(ledger, store);
    let auth = PlaneAuthenticator::new(
        config.internal_token_issuer.clone(),
        config.allowed_audiences.clone(),
        config.internal_token_secret.clone(),
    );

    let app = router(AppState::new(service, auth));
    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("binding {}", config.bind))?;
    tracing::info!(bind = %config.bind, "artifact service listening");
    axum::serve(listener, app).await.context("serving")?;
    Ok(())
}
