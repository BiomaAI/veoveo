use clap::Parser;
use secrecy::ExposeSecret as _;
use std::{collections::BTreeSet, net::SocketAddr, sync::Arc};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, PublicDeployment, SubscriptionHub, TelemetryGuard,
    init_server_telemetry, public_allowed_hosts,
};
use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials};
use veoveo_recording_hub::{GatewayLayerPublisher, GatewayLayerPublisherConfig};
use veoveo_recording_mcp::{
    RecordingService, mcp_setup::SERVER_SETUP, playback::PlaybackManager,
    service::ProjectionRuntimeLimits,
};
use veoveo_recording_reader::cache::LayerCacheLimits;

#[path = "server/auth.rs"]
mod auth;
#[path = "server/config.rs"]
mod config;
#[path = "server/http.rs"]
mod http;
#[path = "server/mcp.rs"]
mod mcp;
#[path = "server/prompts.rs"]
mod prompts;
#[path = "server/resources.rs"]
mod resources;
#[path = "server/state.rs"]
mod state;

use config::Args;
use state::AppState;
const SERVER_SLUG: &str = "recording";

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-recording-mcp", "info,veoveo_recording_mcp=debug")?;
    let args = Args::parse();
    std::sync::LazyLock::force(&SERVER_SETUP);
    let spool_dir = if args.spool_dir.is_absolute() {
        args.spool_dir.clone()
    } else {
        std::env::current_dir()?.join(&args.spool_dir)
    };
    let store = PlatformStore::connect(
        StoreConfig::builder(
            &args.surreal_endpoint,
            &args.surreal_namespace,
            &args.surreal_database,
            StoreCredentials::database(&args.surreal_username, args.surreal_password.clone()),
        )
        .build()?,
    )
    .await?;
    let state = Arc::new(AppState {
        recordings: RecordingService::new(
            store.clone(),
            HttpArtifactPlane::new(&args.artifact_service_url),
            spool_dir,
        )?
        .with_layer_cache(
            args.catalog_cache_dir,
            LayerCacheLimits {
                managed_bytes: args.catalog_cache_managed_bytes,
                minimum_free_bytes: args.catalog_cache_minimum_free_bytes,
            },
        )?
        .with_projection_runtime(ProjectionRuntimeLimits {
            aggregate_scratch_bytes: args.projection_scratch_bytes,
            minimum_free_bytes: args.projection_minimum_free_bytes,
            concurrent_projections: usize::from(args.projection_concurrency),
            maximum_deadline_ms: args.projection_deadline_ms,
        })?
        .with_layer_publisher(GatewayLayerPublisher::new(GatewayLayerPublisherConfig {
            gateway_url: args.gateway_url,
            gateway_transport_url: args.gateway_transport_url,
            protected_resource: args.publication_protected_resource,
            profile: args.publication_profile,
            client_id: args.publication_client_id,
            private_key_pem_file: args.publication_private_key_pem_file,
            key_id: args.publication_key_id,
            algorithm: args.publication_signing_algorithm,
        })?)
        .with_live_history_seconds(args.live_history_seconds)?,
        playback: PlaybackManager::new(
            args.playback_token_key.expose_secret(),
            &args.playback_public_url,
            store,
        )?,
        subscribers: SubscriptionHub::new(),
    });
    let resource_state = state.clone();
    let _resource_observer = tokio::spawn(async move {
        use futures::StreamExt;
        use veoveo_platform_store::PlatformTable::*;
        let mut changes = resource_state
            .recordings
            .platform_store()
            .resource_changes(vec![
                RecordingDataset,
                Recording,
                RecordingLayer,
                RecordingBlueprint,
            ]);
        while changes.next().await.is_some() {
            resource_state
                .subscribers
                .notify_resource_contents_changed()
                .await;
        }
    });
    let mut allowed_hosts: BTreeSet<String> = args.allowed_hosts.into_iter().collect();
    allowed_hosts.insert(format!("recording-mcp:{}", args.port));
    if args.allow_loopback_hosts {
        allowed_hosts.insert(format!("localhost:{}", args.port));
        allowed_hosts.insert(format!("127.0.0.1:{}", args.port));
    }
    // The public ingress sends Rerun gRPC calls here under the playback host.
    allowed_hosts.extend(public_allowed_hosts(
        &PublicDeployment::new(args.playback_public_url.clone())?,
        false,
    ));
    http::server(
        state,
        allowed_hosts,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    )?
    .serve(SocketAddr::from(([0, 0, 0, 0], args.port)))
    .await
}
