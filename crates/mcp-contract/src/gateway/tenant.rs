use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::TenantId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TenantDefinition {
    pub id: TenantId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}
