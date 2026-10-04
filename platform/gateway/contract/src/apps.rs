use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use veoveo_types::{
    DataLabelId, LocalToolName, ResourceScheme, ResourceUri, ResourceUriPrefix, ScopeName,
    ServerSlug,
};

pub const APP_RESOURCE_DEPENDENCIES_META_KEY: &str = "ai.veoveo/app-resource-dependencies";
pub const APP_TOOL_DEPENDENCIES_META_KEY: &str = "ai.veoveo/app-tool-dependencies";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AppResourceOperation {
    Read,
    Subscribe,
}

/// One exact cross-server resource family admitted to one owning MCP App.
///
/// The gateway filters these declarations by the active profile, scopes, and
/// data labels before projecting them to a host. The eventual resource read
/// still passes through ordinary gateway resource exposure and policy.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct AppResourceDependency {
    pub app_resource: ResourceUri,
    pub server: ServerSlug,
    pub scheme: ResourceScheme,
    pub uri_prefix: ResourceUriPrefix,
    pub required_scope: ScopeName,
    pub operations: BTreeSet<AppResourceOperation>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub data_labels: BTreeSet<DataLabelId>,
}

/// One exact cross-server tool set admitted to one owning MCP App.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct AppToolDependency {
    pub app_resource: ResourceUri,
    pub server: ServerSlug,
    pub required_scope: ScopeName,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub data_labels: BTreeSet<DataLabelId>,
    pub tools: BTreeSet<AppToolImport>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct AppToolImport {
    /// Stable name exposed to the App frame.
    pub name: LocalToolName,
    pub target_tool: LocalToolName,
}
