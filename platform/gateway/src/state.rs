use anyhow::{Context, Result};
use veoveo_platform_store::{PlatformStore, StoreConfig};

mod auth_state;
mod refresh_tokens;
mod session;
mod subscriptions;
mod task_routes;

pub use auth_state::GatewayReplayRetentionSummary;
pub use refresh_tokens::{
    GatewayRefreshDeliveryWindow, GatewayRefreshExchange, GatewayRefreshIssueRequest,
    GatewayRefreshRotationRequest, IssuedGatewayRefreshToken, REFRESH_TOKEN_TTL_SECONDS,
    RefreshTokenDeliveryCipher,
};
pub(crate) use task_routes::{GatewayTaskOwnership, GatewayTaskRouteDraft};

/// Shared, installation-wide gateway correctness state.
///
/// Clones may be used by independent gateway replicas; SurrealDB remains the
/// sole authority for replay, OAuth, revocation, subscription, and audit data.
#[derive(Debug, Clone)]
pub struct GatewayState {
    pub(super) platform: PlatformStore,
    pub(crate) audit_writer: std::sync::Arc<tokio::sync::OnceCell<veoveo_audit::AuditWriter>>,
    pub(crate) audit_health: std::sync::Arc<std::sync::OnceLock<veoveo_audit::AuditHealth>>,
    pub(crate) oauth_client_resolver:
        Option<std::sync::Arc<dyn crate::oauth_clients::OAuthClientResolver>>,
}

impl GatewayState {
    pub fn new(platform: PlatformStore) -> Self {
        Self {
            platform,
            audit_writer: Default::default(),
            audit_health: Default::default(),
            oauth_client_resolver: None,
        }
    }

    pub async fn connect(config: StoreConfig) -> Result<Self> {
        let platform = PlatformStore::connect(config)
            .await
            .context("failed to connect gateway runtime state to SurrealDB")?;
        Ok(Self::new(platform))
    }

    pub fn platform_store(&self) -> &PlatformStore {
        &self.platform
    }
}
