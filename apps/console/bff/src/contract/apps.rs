//! Browser App catalog transport. Provider tool input schemas stay open.
use schemars::JsonSchema;
use serde::Serialize;
use veoveo_gateway_contract::{AppResourceDependency, AppToolDependency, GatewayDiscoveryFailure};

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppCatalog {
    pub apps: Vec<AppDescriptor>,
    pub degradations: Vec<GatewayDiscoveryFailure>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppDescriptor {
    pub server: String,
    pub resource_uri: String,
    pub standalone_path: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Self-contained `data:` icon sources only — the console shell's CSP
    /// does not fetch remote images, and apps are self-contained by contract.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[schemars(default)]
    pub icons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefers_border: Option<bool>,
    pub tools: Vec<AppToolDescriptor>,
    pub resource_dependencies: Vec<AppResourceDependency>,
    pub tool_dependencies: Vec<AppToolDependency>,
    pub agent_message_targets: Vec<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppToolDescriptor {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

pub fn schema_bundle() -> schemars::Schema {
    schemars::schema_for!(AppCatalog)
}
