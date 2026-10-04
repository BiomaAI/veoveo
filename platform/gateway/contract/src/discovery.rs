use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::ServerSlug;

/// MCP result metadata carrying failures isolated from a federated catalog.
pub const GATEWAY_DISCOVERY_DEGRADATION_META_KEY: &str = "ai.veoveo/gateway-discovery-degradation";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GatewayDiscoverySurface {
    Resources,
    ResourceTemplates,
    Tools,
    Prompts,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GatewayDiscoveryFailureCode {
    DiscoveryPending,
    UpstreamUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GatewayDiscoveryFailure {
    pub server: ServerSlug,
    pub surface: GatewayDiscoverySurface,
    pub code: GatewayDiscoveryFailureCode,
}
