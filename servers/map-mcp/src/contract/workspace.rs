use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Caller-specific capabilities for the single Map MCP workspace App.
///
/// The App reads this resource before rendering controls. Tool and resource
/// handlers remain the authorization boundary; these booleans only let the
/// view present the surface the current caller can actually use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MapWorkspaceAccess {
    pub administration: bool,
    pub feature_read: bool,
    pub feature_write: bool,
    pub feature_publish: bool,
}
