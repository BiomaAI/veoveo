//! Artifact plane service binary.

use anyhow::Context;
use tracing_subscriber::EnvFilter;
use veoveo_artifact_service::config::Config;
use veoveo_artifact_service::http::{AppState, router};
use veoveo_artifact_service::{ArtifactService, PlaneAuthenticator, SurrealArtifactRepository};
use veoveo_platform_store::PlatformStore;

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
    let platform_store = PlatformStore::connect(config.platform_store.clone())
        .await
        .context("connecting platform store")?;
    let repository = SurrealArtifactRepository::new(platform_store);
    let service = ArtifactService::with_options(
        repository,
        object_store,
        &config.public_base_url,
        config.max_internal_read_bytes,
        config.redirect_threshold_bytes,
    );
    let auth = PlaneAuthenticator::new(
        config.internal_token_issuer.clone(),
        config.allowed_audiences.clone(),
        config.internal_trust_bundle.clone(),
    );

    let app = router(AppState::new(service, auth));
    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("binding {}", config.bind))?;
    tracing::info!(bind = %config.bind, "artifact service listening");
    axum::serve(listener, app).await.context("serving")?;
    Ok(())
}
