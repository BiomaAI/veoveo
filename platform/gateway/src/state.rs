use anyhow::{Context, Result};
use veoveo_platform_store::{PlatformStore, StoreConfig};

mod audit;
mod auth_state;
mod refresh_tokens;
mod session;
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
pub(crate) use task_routes::{GatewayTaskOwnership, GatewayTaskRouteDraft};

/// Shared, installation-wide gateway correctness state.
///
/// Clones may be used by independent gateway replicas; SurrealDB remains the
/// sole authority for replay, OAuth, revocation, subscription, and audit data.
#[derive(Debug, Clone)]
pub struct GatewayState {
    pub(super) platform: PlatformStore,
    pub(crate) managed_templates: std::sync::Arc<crate::managed_agents::ManagedTemplateCatalog>,
}

impl GatewayState {
    pub fn new(platform: PlatformStore) -> Self {
        Self {
            platform,
            managed_templates: Default::default(),
        }
    }

    pub fn with_managed_templates(
        mut self,
        templates: crate::managed_agents::ManagedTemplateCatalog,
    ) -> Self {
        self.managed_templates = std::sync::Arc::new(templates);
        self
    }

    pub fn managed_templates(&self) -> &crate::managed_agents::ManagedTemplateCatalog {
        &self.managed_templates
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
