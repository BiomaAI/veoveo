//! Media's own billing attribution inside the generic Usage metadata object.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(
    tag = "source",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum MediaUsageMetadata {
    ModelRegistry {
        model_type: String,
        formula: Option<String>,
        cost_kind: MediaUsageCostKind,
    },
    BillingRecord {
        billing_type: String,
        source_created_at: Option<DateTime<Utc>>,
        source_updated_at: Option<DateTime<Utc>>,
        order_id: Option<String>,
        order_state: Option<String>,
        order_status: Option<String>,
        job_status: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MediaUsageCostKind {
    Estimate,
}
