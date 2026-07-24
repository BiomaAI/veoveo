use std::{collections::BTreeMap, num::NonZeroU32, sync::Arc, time::Duration};

use anyhow::{Context, anyhow};
use axum::Router;
use chrono::{DateTime, TimeDelta, Utc};
use parking_lot::RwLock;
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    CertificateAuthoritySource, GatewayInternalTokenIssuer, GatewayProfileId,
    ResourceAuthorizationServer, ServerSlug,
};
use veoveo_mcp_gateway::{
    GatewayCatalog, GatewayCatalogHandle, GatewayControlStore, GatewayRefreshDeliveryWindow,
    GatewayState, GatewayUpstreamHttpClientPool, RefreshTokenDeliveryCipher,
};
use veoveo_platform_store::PlatformStore;

const GATEWAY_AUTH_HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const REFRESH_DELIVERY_GC_INTERVAL: Duration = Duration::from_secs(60);

pub(super) type SharedCatalog = GatewayCatalogHandle;
pub(super) type SharedHttpClient = Arc<RwLock<reqwest::Client>>;
pub(super) type ProfileMcpService = Router;
pub(super) type SharedProfileMcpServices =
    Arc<RwLock<BTreeMap<GatewayProfileId, ProfileMcpService>>>;

#[derive(Debug, Clone, Copy)]
pub(super) struct GatewayRetentionPolicy {
    pub(super) audit_event_days: NonZeroU32,
}

#[derive(Clone)]
pub(super) struct AppState {
    pub(super) catalog: SharedCatalog,
    pub(super) gateway_state: GatewayState,
    pub(super) http: SharedHttpClient,
    pub(super) public_base_url: String,
    pub(super) refresh_delivery_cipher: RefreshTokenDeliveryCipher,
    pub(super) refresh_delivery_window: GatewayRefreshDeliveryWindow,
}

#[derive(Clone)]
pub(super) struct ProfileAuthState {
    pub(super) catalog: SharedCatalog,
    pub(super) gateway_state: GatewayState,
    pub(super) public_base_url: String,
    pub(super) http: SharedHttpClient,
}

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
    pub(super) server_health: crate::admin::ServerHealthMonitor,
    pub(super) console_stream: crate::admin::ConsoleStreamRuntime,
}

#[derive(Clone)]
pub(super) struct DynamicMcpState {
    pub(super) catalog: SharedCatalog,
    pub(super) gateway_state: GatewayState,
    pub(super) platform_store: PlatformStore,
    pub(super) internal_token_issuer: GatewayInternalTokenIssuer,
    pub(super) upstream_http: GatewayUpstreamHttpClientPool,
    pub(super) allowed_hosts: Arc<Vec<String>>,
    pub(super) cancellation_token: CancellationToken,
    pub(super) services: SharedProfileMcpServices,
}

#[derive(Clone)]
pub(super) struct ArtifactDownloadState {
    pub(super) catalog: SharedCatalog,
    pub(super) gateway_state: GatewayState,
    pub(super) http: SharedHttpClient,
    pub(super) internal_token_issuer: GatewayInternalTokenIssuer,
    pub(super) artifact_server: ServerSlug,
    pub(super) artifact_service_url: String,
}

#[derive(Clone)]
pub(super) struct RecordingPlaybackState {
    pub(super) catalog: SharedCatalog,
    pub(super) gateway_state: GatewayState,
    pub(super) internal_token_issuer: GatewayInternalTokenIssuer,
    pub(super) upstream_http: GatewayUpstreamHttpClientPool,
}

#[derive(Clone)]
pub(super) struct RecordingIngestGatewayState {
    pub(super) catalog: SharedCatalog,
    pub(super) gateway_state: GatewayState,
    pub(super) http: SharedHttpClient,
    pub(super) internal_token_issuer: GatewayInternalTokenIssuer,
    pub(super) public_base_url: String,
}

#[derive(Debug, Serialize)]
pub(super) struct Readiness {
    pub(super) status: &'static str,
    pub(super) servers: usize,
    pub(super) profiles: usize,
}

pub(super) fn current_catalog(catalog: &SharedCatalog) -> Arc<GatewayCatalog> {
    catalog.current()
}

pub(super) fn current_http_client(http: &SharedHttpClient) -> reqwest::Client {
    http.read().clone()
}

pub(super) fn public_oauth_issuer(public_base_url: &str) -> String {
    format!("{}/oauth", public_base_url.trim_end_matches('/'))
}

pub(super) fn public_authorization_server<'a>(
    catalog: &'a GatewayCatalog,
    public_base_url: &str,
) -> Option<&'a ResourceAuthorizationServer> {
    catalog.authorization_server_by_issuer(&public_oauth_issuer(public_base_url))
}

pub(super) fn profile_id_from_gateway_path(path: &str) -> Option<GatewayProfileId> {
    let mut segments = path.trim_start_matches('/').split('/');
    match segments.next()? {
        "mcp" | "admin" | "artifacts" | "recordings" => {}
        _ => return None,
    }
    let profile = segments.next()?;
    if profile.is_empty() {
        return None;
    }
    GatewayProfileId::new(profile).ok()
}

pub(super) fn replace_catalog(catalog: &SharedCatalog, new_catalog: Arc<GatewayCatalog>) {
    catalog.replace(new_catalog);
}

pub(super) fn replace_http_client(http: &SharedHttpClient, new_client: reqwest::Client) {
    *http.write() = new_client;
}

pub(super) fn gateway_retention_cutoff(
    now: DateTime<Utc>,
    days: NonZeroU32,
) -> anyhow::Result<DateTime<Utc>> {
    now.checked_sub_signed(TimeDelta::days(i64::from(days.get())))
        .ok_or_else(|| anyhow!("gateway retention cutoff overflow for {days} day window"))
}

pub(super) async fn run_gateway_retention_gc(
    gateway_state: &GatewayState,
    retention: GatewayRetentionPolicy,
) -> anyhow::Result<()> {
    let now = Utc::now();
    let audit_cutoff = gateway_retention_cutoff(now, retention.audit_event_days)?;
    let audit_summary = gateway_state
        .delete_audit_events_before(audit_cutoff)
        .await?;
    let authorization_records_deleted = gateway_state
        .prune_expired_authorization_records(now)
        .await?;
    let jwt_revocations_deleted = gateway_state.prune_expired_jwt_revocations(now).await?;
    let replay_summary = gateway_state.prune_expired_replay_ids(now).await?;
    let refresh_summary = gateway_state.prune_expired_refresh_tokens(now).await?;
    tracing::info!(
        deleted_auth_audit_events = audit_summary.auth_events_deleted,
        deleted_policy_audit_events = audit_summary.policy_events_deleted,
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

pub(super) fn spawn_gateway_retention_gc_loop(
    gateway_state: GatewayState,
    retention: GatewayRetentionPolicy,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
            if let Err(err) = run_gateway_retention_gc(&gateway_state, retention).await {
                tracing::error!("gateway retention gc failed: {err}");
            }
        }
    });
}

pub(super) fn spawn_refresh_delivery_gc_loop(gateway_state: GatewayState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(REFRESH_DELIVERY_GC_INTERVAL).await;
            match gateway_state
                .clear_expired_refresh_delivery_envelopes(Utc::now())
                .await
            {
                Ok(cleared) => tracing::info!(
                    deleted_refresh_delivery_envelopes = cleared,
                    "gateway refresh delivery-envelope gc completed"
                ),
                Err(err) => {
                    tracing::error!("gateway refresh delivery-envelope gc failed: {err}");
                }
            }
        }
    });
}

pub(super) fn build_http_client(catalog: &GatewayCatalog) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(GATEWAY_AUTH_HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none());

    for identity_provider in catalog.identity_providers() {
        for trust_anchor in &identity_provider.trusted_certificate_authorities {
            match trust_anchor {
                CertificateAuthoritySource::File { path } => {
                    let bytes = std::fs::read(path.as_str()).with_context(|| {
                        format!(
                            "failed to read trusted CA certificate `{path}` for identity provider `{}`",
                            identity_provider.id
                        )
                    })?;
                    let certificate = reqwest::Certificate::from_pem(&bytes).with_context(|| {
                        format!(
                            "failed to parse trusted CA certificate `{path}` for identity provider `{}`",
                            identity_provider.id
                        )
                    })?;
                    builder = builder.add_root_certificate(certificate);
                }
            }
        }
    }

    builder
        .build()
        .context("failed to build gateway HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_envelope_gc_cadence_is_bounded_for_the_max_window() {
        assert!(REFRESH_DELIVERY_GC_INTERVAL <= Duration::from_secs(2 * 30));
    }
}
