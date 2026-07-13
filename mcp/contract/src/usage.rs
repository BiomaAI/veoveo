use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Whether a usage row is a pre-run estimate or a provider-confirmed actual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UsageKind {
    Estimate,
    Actual,
}

/// One normalized usage event for a provider-backed task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UsageRecord {
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_job_id: Option<String>,
    pub model_id: String,
    pub kind: UsageKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    pub recorded_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: Value,
}

/// Resource body for `{scheme}://usage/task/{task_id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UsageReport {
    pub task_id: String,
    pub usage_uri: String,
    pub records: Vec<UsageRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_amount: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_kind: Option<UsageKind>,
}

impl UsageReport {
    pub fn new(task_id: impl Into<String>, usage_uri: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            usage_uri: usage_uri.into(),
            records: Vec::new(),
            total_amount: None,
            currency: None,
            total_kind: None,
        }
    }

    pub fn with_records(mut self, records: Vec<UsageRecord>) -> Self {
        let total_kind = if records
            .iter()
            .any(|record| record.kind == UsageKind::Actual)
        {
            Some(UsageKind::Actual)
        } else if records
            .iter()
            .any(|record| record.kind == UsageKind::Estimate)
        {
            Some(UsageKind::Estimate)
        } else {
            None
        };
        let total_records: Vec<&UsageRecord> = records
            .iter()
            .filter(|record| Some(record.kind) == total_kind)
            .collect();
        let currency = common_currency(&total_records);
        let total_amount = currency
            .as_ref()
            .and_then(|currency| sum_amounts(&total_records, currency));
        self.records = records;
        self.total_amount = total_amount;
        self.currency = currency;
        self.total_kind = total_kind;
        self
    }
}

fn common_currency(records: &[&UsageRecord]) -> Option<String> {
    let mut seen = records
        .iter()
        .filter_map(|record| record.currency.as_ref())
        .filter(|currency| !currency.is_empty());
    let first = seen.next()?.clone();
    if seen.all(|currency| currency == &first) {
        Some(first)
    } else {
        None
    }
}

fn sum_amounts(records: &[&UsageRecord], currency: &str) -> Option<f64> {
    let mut total = 0.0;
    let mut saw_amount = false;
    for record in records {
        if record.currency.as_deref() != Some(currency) {
            continue;
        }
        if let Some(amount) = record.amount {
            total += amount;
            saw_amount = true;
        }
    }
    saw_amount.then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_report_totals_common_currency() {
        let records = vec![
            UsageRecord {
                task_id: "task-1".into(),
                source_id: None,
                provider_job_id: None,
                model_id: "model".into(),
                kind: UsageKind::Estimate,
                quantity: Some(1.0),
                unit: Some("run".into()),
                amount: Some(0.25),
                currency: Some("USD".into()),
                recorded_at: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                metadata: Value::Null,
            },
            UsageRecord {
                task_id: "task-1".into(),
                source_id: Some("bill-1".into()),
                provider_job_id: Some("prediction-1".into()),
                model_id: "model".into(),
                kind: UsageKind::Actual,
                quantity: Some(1.0),
                unit: Some("run".into()),
                amount: Some(0.25),
                currency: Some("USD".into()),
                recorded_at: DateTime::parse_from_rfc3339("2026-01-01T00:00:01Z")
                    .unwrap()
                    .with_timezone(&Utc),
                metadata: Value::Null,
            },
        ];

        let report = UsageReport::new("task-1", "media://usage/task/task-1").with_records(records);
        assert_eq!(report.total_kind, Some(UsageKind::Actual));
        assert_eq!(report.total_amount, Some(0.25));
        assert_eq!(report.currency.as_deref(), Some("USD"));
    }
}
