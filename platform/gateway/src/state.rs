use anyhow::{Context, Result};
use veoveo_platform_store::{PlatformStore, StoreConfig};

mod audit;
mod auth_state;
mod refresh_tokens;
mod subscriptions;
mod task_routes;

pub use audit::{
    GatewayAuditCounts, GatewayAuditRetentionSummary, GatewayAuthAuditMetadataSummary,
    GatewayAuthAuditMethodSummary, GatewayAuthAuditReasonSummary,
    GatewayPolicyAuditMetadataSummary, GatewayPolicyAuditMethodSummary,
    GatewayPolicyAuditReasonSummary, GatewayToolCallAuditEvent, GatewayToolCallResultKind,
};
pub use auth_state::GatewayReplayRetentionSummary;
pub use refresh_tokens::{
    GatewayRefreshDeliveryWindow, GatewayRefreshExchange, GatewayRefreshIssueRequest,
    GatewayRefreshRotationRequest, IssuedGatewayRefreshToken, REFRESH_TOKEN_TTL_SECONDS,
    RefreshTokenDeliveryCipher,
};
pub(crate) use task_routes::GatewayTaskRouteDraft;

/// Shared, installation-wide gateway correctness state.
///
/// Clones may be used by independent gateway replicas; SurrealDB remains the
/// sole authority for replay, OAuth, revocation, subscription, and audit data.
#[derive(Debug, Clone)]
pub struct GatewayState {
    pub(super) platform: PlatformStore,
}

impl GatewayState {
    pub fn new(platform: PlatformStore) -> Self {
        Self { platform }
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
