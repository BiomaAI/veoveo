use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Conformance report schema.
pub const CONFORMANCE_REPORT_SCHEMA: &str = "veoveo.io/mcp-conformance-report/v1";

/// Supported conformance report schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ConformanceReportSchema {
    #[serde(rename = "veoveo.io/mcp-conformance-report/v1")]
    V1,
}

/// Outcome of one applicable requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Failed,
    Skipped,
}

/// One stable conformance requirement result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckResult {
    pub requirement_id: String,
    pub status: CheckStatus,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Value>,
}

/// Server identity observed during MCP initialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservedImplementation {
    pub name: String,
    pub version: String,
    pub protocol_version: String,
}

/// Machine-readable result from one hosted-server certification run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConformanceReport {
    pub schema_version: ConformanceReportSchema,
    pub profile_id: String,
    pub contract_revision: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation: Option<ObservedImplementation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_capabilities: Option<Value>,
    pub checks: Vec<CheckResult>,
}

impl ConformanceReport {
    pub fn passed(&self) -> bool {
        self.checks
            .iter()
            .all(|check| check.status != CheckStatus::Failed)
    }
}
