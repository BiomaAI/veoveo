use std::{collections::BTreeMap, sync::Arc, time::Duration};

use axum::Router;
use chrono::Utc;
use parking_lot::RwLock;
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{GatewayInternalTokenIssuer, GatewayProfileId, ServerSlug};
use veoveo_mcp_gateway::{
    GatewayCatalog, GatewayCatalogHandle, GatewayControlStore, GatewayRefreshDeliveryWindow,
    GatewayState, GatewayUpstreamHttpClientPool, RefreshTokenDeliveryCipher,
};

const REFRESH_DELIVERY_GC_INTERVAL: Duration = Duration::from_secs(60);

pub(super) type SharedCatalog = GatewayCatalogHandle;
pub(super) type SharedHttpClient = Arc<RwLock<reqwest::Client>>;
pub(super) type ProfileMcpService = Router;
pub(super) type SharedProfileMcpServices =
    Arc<RwLock<BTreeMap<GatewayProfileId, ProfileMcpService>>>;

#[derive(Clone)]
pub(super) struct AppState {
    pub(super) catalog: SharedCatalog,
    pub(super) gateway_state: GatewayState,
    pub(super) http: SharedHttpClient,
    pub(super) public_base_url: String,
    pub(super) refresh_delivery_cipher: RefreshTokenDeliveryCipher,
    pub(super) refresh_delivery_window: GatewayRefreshDeliveryWindow,
}

pub(super) use veoveo_mcp_gateway::http::ProfileAuthState;

#[derive(Clone)]
pub(super) struct AdminState {
    pub(super) catalog: SharedCatalog,
    pub(super) http: SharedHttpClient,
    pub(super) control_store: GatewayControlStore,
    pub(super) gateway_state: GatewayState,
    pub(super) internal_token_issuer: GatewayInternalTokenIssuer,
    pub(super) upstream_http: GatewayUpstreamHttpClientPool,
    pub(super) artifact_server: ServerSlug,
    pub(super) artifact_service_url: String,
    pub(super) offline_mode: bool,
    pub(super) module_bindings: Arc<Vec<veoveo_gateway_contract::ModuleBindingSnapshot>>,
    pub(super) server_health: crate::admin::ServerHealthMonitor,
    pub(super) console_stream: crate::admin::ConsoleStreamRuntime,
}

#[derive(Clone)]
pub(super) struct DynamicMcpState {
    pub(super) catalog: SharedCatalog,
    pub(super) gateway_state: GatewayState,
    pub(super) internal_token_issuer: GatewayInternalTokenIssuer,
    pub(super) upstream_http: GatewayUpstreamHttpClientPool,
    pub(super) allowed_hosts: Arc<Vec<String>>,
    pub(super) cancellation_token: CancellationToken,
    pub(super) services: SharedProfileMcpServices,
}

#[derive(Clone)]
pub(super) struct ArtifactHttpState {
    pub(super) catalog: SharedCatalog,
    pub(super) gateway_state: GatewayState,
    pub(super) http: reqwest::Client,
    pub(super) internal_token_issuer: GatewayInternalTokenIssuer,
    pub(super) artifact_server: ServerSlug,
    pub(super) artifact_service_url: String,
}

#[derive(Debug, Serialize)]
pub(super) struct Readiness {
    pub(super) status: &'static str,
    pub(super) servers: usize,
    pub(super) profiles: usize,
}

pub(super) use veoveo_mcp_gateway::http::{
    build_http_client, current_catalog, current_http_client, public_authorization_server,
};

pub(super) fn replace_catalog(
    catalog: &SharedCatalog,
    new_catalog: Arc<GatewayCatalog>,
) -> anyhow::Result<()> {
    catalog.replace(new_catalog)
}

pub(super) fn replace_http_client(http: &SharedHttpClient, new_client: reqwest::Client) {
    *http.write() = new_client;
}

pub(super) async fn run_authorization_retention_gc(
    gateway_state: &GatewayState,
) -> anyhow::Result<()> {
    let now = Utc::now();
    let authorization_records_deleted = gateway_state
        .prune_expired_authorization_records(now)
        .await?;
    let jwt_revocations_deleted = gateway_state.prune_expired_jwt_revocations(now).await?;
    let replay_summary = gateway_state.prune_expired_replay_ids(now).await?;
    let refresh_summary = gateway_state.prune_expired_refresh_tokens(now).await?;
    tracing::info!(
        deleted_authorization_records = authorization_records_deleted,
        deleted_jwt_revocations = jwt_revocations_deleted,
        deleted_client_assertion_replay_ids = replay_summary.client_assertion_jtis_deleted,
        deleted_id_jag_replay_ids = replay_summary.id_jag_jtis_deleted,
        deleted_refresh_tokens = refresh_summary.tokens_deleted,
        deleted_refresh_families = refresh_summary.families_deleted,
        deleted_refresh_delivery_envelopes = refresh_summary.delivery_envelopes_deleted,
        "gateway retention gc completed"
    );
    Ok(())
}

pub(super) fn spawn_authorization_retention_gc_loop(
    gateway_state: GatewayState,
    stop: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if stop.is_cancelled() {
                return;
            }
            let pause = match run_authorization_retention_gc(&gateway_state).await {
                Ok(()) => Duration::from_secs(60 * 60),
                Err(err) => {
                    tracing::error!("gateway authorization retention failed: {err}");
                    Duration::from_secs(60)
                }
            };
            tokio::select! { _ = tokio::time::sleep(pause) => {}, _ = stop.cancelled() => return }
        }
    })
}

pub(super) fn spawn_refresh_delivery_gc_loop(
    gateway_state: GatewayState,
    stop: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! { _ = tokio::time::sleep(REFRESH_DELIVERY_GC_INTERVAL) => {}, _ = stop.cancelled() => return }
            match gateway_state
                .clear_expired_refresh_delivery_envelopes(Utc::now())
                .await
            {
                Ok(cleared) => tracing::info!(
                    deleted_refresh_delivery_envelopes = cleared,
                    "gateway refresh delivery-envelope gc completed"
                ),
                Err(err) => tracing::error!("gateway refresh delivery-envelope gc failed: {err}"),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delivery_envelope_gc_cadence_is_bounded_for_the_max_window() {
        assert!(REFRESH_DELIVERY_GC_INTERVAL <= Duration::from_secs(2 * 30));
    }
}
