use axum::{
    Router,
    routing::{get, post},
};
use veoveo_mcp_contract::{GatewayInternalTokenIssuer, ServerSlug};
use veoveo_mcp_gateway::{
    GatewayCatalogHandle, GatewayState, GatewayUpstreamHttpClientPool,
    http::{GatewayHttpContext, GatewayModuleRoutes, ModuleTaskScope, SharedHttpClient},
};
type SharedCatalog = GatewayCatalogHandle;
mod ingest;
mod layer_publication;
mod playback;
#[derive(Clone)]
pub struct RecordingPlaybackState {
    pub catalog: SharedCatalog,
    pub gateway_state: GatewayState,
    pub internal_token_issuer: GatewayInternalTokenIssuer,
    pub upstream_http: GatewayUpstreamHttpClientPool,
    pub artifact_server: ServerSlug,
}

#[derive(Clone)]
pub struct RecordingLayerPublicationState {
    pub catalog: SharedCatalog,
    pub gateway_state: GatewayState,
    pub http: SharedHttpClient,
    pub internal_token_issuer: GatewayInternalTokenIssuer,
    pub artifact_server: ServerSlug,
    pub artifact_service_url: String,
}

#[derive(Clone)]
pub struct RecordingIngestGatewayState {
    pub catalog: SharedCatalog,
    pub gateway_state: GatewayState,
    pub http: SharedHttpClient,
    pub internal_token_issuer: GatewayInternalTokenIssuer,
    pub public_base_url: String,
}

pub fn build(
    context: GatewayHttpContext,
    _scope: ModuleTaskScope,
    artifact_service_url: String,
) -> anyhow::Result<GatewayModuleRoutes> {
    let playback_state = RecordingPlaybackState {
        catalog: context.catalog.clone(),
        gateway_state: context.gateway_state.clone(),
        internal_token_issuer: context.internal_token_issuer.clone(),
        upstream_http: context.upstream_http.clone(),
        artifact_server: ServerSlug::parse("artifact")?,
    };
    let publication = RecordingLayerPublicationState {
        catalog: context.catalog.clone(),
        gateway_state: context.gateway_state.clone(),
        http: context.auth_http.clone(),
        internal_token_issuer: context.internal_token_issuer.clone(),
        artifact_server: ServerSlug::parse("artifact")?,
        artifact_service_url,
    };
    let ingest = RecordingIngestGatewayState {
        catalog: context.catalog,
        gateway_state: context.gateway_state,
        http: context.auth_http,
        internal_token_issuer: context.internal_token_issuer,
        public_base_url: context.deployment.base_url().to_owned(),
    };
    let profile = Router::new()
        .route(
            "/recordings/{profile}/layers",
            post(layer_publication::publish_recording_layer),
        )
        .with_state(publication)
        .merge(
            Router::new()
                .route(
                    "/recordings/{profile}/catalog-grants",
                    post(playback::catalog_grant),
                )
                .route(
                    "/recordings/{profile}/{recording_id}/playback",
                    get(playback::playback_manifest),
                )
                .route(
                    "/recordings/{profile}/{recording_id}/live/rrd-stream",
                    get(playback::playback_live_recording),
                )
                .route(
                    "/recordings/{profile}/{recording_id}/blueprints/{revision}/data.rrd",
                    get(playback::playback_blueprint),
                )
                .route(
                    "/recordings/{profile}/{recording_id}/projections/{projection_id}/data.arrow",
                    get(playback::projection_data),
                )
                .with_state(playback_state),
        );
    Ok(GatewayModuleRoutes::new(
        profile,
        ingest::recording_ingest_router(ingest),
    ))
}
