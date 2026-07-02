use std::{collections::BTreeMap, num::NonZeroU32, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, anyhow};
use chrono::{DateTime, TimeDelta, Utc};
use parking_lot::RwLock;
use rmcp::transport::streamable_http_server::{
    StreamableHttpService, session::local::LocalSessionManager,
};
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    CertificateAuthoritySource, GatewayInternalTokenIssuer, GatewayProfileId,
};
use veoveo_mcp_gateway::{GatewayCatalog, GatewayCatalogHandle, GatewayMcp, GatewayState};

const GATEWAY_AUTH_HTTP_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) type SharedCatalog = GatewayCatalogHandle;
pub(super) type SharedHttpClient = Arc<RwLock<reqwest::Client>>;
pub(super) type ProfileMcpService = StreamableHttpService<GatewayMcp, LocalSessionManager>;
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
    pub(super) control_plane: PathBuf,
    pub(super) gateway_state: GatewayState,
}

#[derive(Clone)]
pub(super) struct DynamicMcpState {
    pub(super) catalog: SharedCatalog,
    pub(super) gateway_state: GatewayState,
    pub(super) internal_token_issuer: GatewayInternalTokenIssuer,
    pub(super) allowed_hosts: Arc<Vec<String>>,
    pub(super) cancellation_token: CancellationToken,
    pub(super) services: SharedProfileMcpServices,
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

pub(super) fn profile_id_from_gateway_path(path: &str) -> Option<GatewayProfileId> {
    let mut segments = path.trim_start_matches('/').split('/');
    match segments.next()? {
        "mcp" | "admin" => {}
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

pub(super) fn run_gateway_retention_gc(
    gateway_state: &GatewayState,
    retention: GatewayRetentionPolicy,
) -> anyhow::Result<()> {
    let now = Utc::now();
    let audit_cutoff = gateway_retention_cutoff(now, retention.audit_event_days)?;
    let audit_summary = gateway_state.delete_audit_events_before(audit_cutoff)?;
    let authorization_records_deleted = gateway_state.prune_expired_authorization_records(now)?;
    let jwt_revocations_deleted = gateway_state.prune_expired_jwt_revocations(now)?;
    tracing::info!(
        deleted_auth_audit_events = audit_summary.auth_events_deleted,
        deleted_policy_audit_events = audit_summary.policy_events_deleted,
        deleted_authorization_records = authorization_records_deleted,
        deleted_jwt_revocations = jwt_revocations_deleted,
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
            if let Err(err) = run_gateway_retention_gc(&gateway_state, retention) {
                tracing::error!("gateway retention gc failed: {err}");
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
