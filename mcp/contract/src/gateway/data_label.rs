use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use veoveo_types::DataLabelId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct DataLabelDefinition {
    pub id: DataLabelId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub regulated: bool,
    #[serde(default)]
    pub metadata: Value,
}
