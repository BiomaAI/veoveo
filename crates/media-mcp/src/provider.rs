//! Minimal provider API client: model registry and prediction submit.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_BASE_URL: &str = "https://api.wavespeed.ai";

/// Every v3 response is wrapped in `{code, message, data}`.
#[derive(Debug, Clone, Deserialize)]
pub struct Envelope<T> {
    pub code: i64,
    #[serde(default)]
    pub message: String,
    pub data: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionUrls {
    pub get: String,
}

/// A prediction as returned by submit, result fetch, and webhook callbacks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    pub id: String,
    pub model: String,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub urls: Option<PredictionUrls>,
    pub status: String,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(rename = "executionTime", default)]
    pub execution_time: Option<f64>,
    #[serde(default)]
    pub timings: Option<Value>,
    /// Original request input; present in webhook payloads.
    #[serde(default)]
    pub input: Option<Value>,
}

impl Prediction {
    pub fn is_terminal(&self) -> bool {
        matches!(self.status.as_str(), "completed" | "failed")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BillingSearchResult {
    #[serde(default)]
    pub items: Vec<BillingRecord>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BillingRecord {
    pub uuid: String,
    pub billing_type: String,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub order: Option<BillingOrder>,
    #[serde(default)]
    pub prediction: Option<BillingPrediction>,
}

impl BillingRecord {
    pub fn signed_amount(&self) -> Option<f64> {
        let price = self.price?;
        match self.billing_type.as_str() {
            "deduct" => Some(price),
            "refund" => Some(-price),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BillingOrder {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BillingPrediction {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub model_uuid: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

/// One entry from `GET /api/v3/models`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub model_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub model_type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub base_price: Option<f64>,
    #[serde(default)]
    pub formula: Option<String>,
    #[serde(default)]
    pub api_schema: Option<Value>,
}

impl ModelEntry {
    /// The JSON Schema for this model's run input, if published.
    pub fn request_schema(&self) -> Option<&Value> {
        self.api_schema
            .as_ref()?
            .get("api_schemas")?
            .as_array()?
            .iter()
            .find(|s| s.get("type").and_then(Value::as_str) == Some("model_run"))?
            .get("request_schema")
    }
}

#[derive(Debug)]
pub enum ProviderError {
    Http(reqwest::Error),
    Api { code: i64, message: String },
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Http(e) => write!(f, "http error: {e}"),
            ProviderError::Api { code, message } => {
                write!(f, "provider api error (code {code}): {message}")
            }
        }
    }
}
impl std::error::Error for ProviderError {}
impl From<reqwest::Error> for ProviderError {
    fn from(e: reqwest::Error) -> Self {
        ProviderError::Http(e)
    }
}

#[derive(Clone)]
pub struct ProviderClient {
    http: reqwest::Client,
    base: String,
    api_key: String,
}

impl ProviderClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: DEFAULT_BASE_URL.to_string(),
            api_key: api_key.into(),
        }
    }

    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }

    async fn unwrap_envelope<T: serde::de::DeserializeOwned>(
        resp: reqwest::Response,
    ) -> Result<T, ProviderError> {
        let status = resp.status();
        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                code: status.as_u16() as i64,
                message,
            });
        }
        let env: Envelope<T> = resp.json().await?;
        if env.code != 200 {
            return Err(ProviderError::Api {
                code: env.code,
                message: env.message,
            });
        }
        Ok(env.data)
    }

    /// Fetch the full model registry (988+ models with JSON schemas).
    pub async fn list_models(&self) -> Result<Vec<ModelEntry>, ProviderError> {
        let resp = self
            .http
            .get(format!("{}/api/v3/models", self.base))
            .bearer_auth(&self.api_key)
            .send()
            .await?;
        Self::unwrap_envelope(resp).await
    }

    /// Submit a run for `model_id`. `webhook_url` is registered via the
    /// `?webhook=` query parameter; the provider POSTs the terminal prediction
    /// state through that callback.
    pub async fn submit(
        &self,
        model_id: &str,
        input: &Value,
        webhook_url: Option<&str>,
    ) -> Result<Prediction, ProviderError> {
        let mut url = format!("{}/api/v3/{}", self.base, model_id);
        if let Some(hook) = webhook_url {
            url = format!("{url}?webhook={}", urlencode(hook));
        }
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.api_key)
            .json(input)
            .send()
            .await?;
        Self::unwrap_envelope(resp).await
    }

    /// Fetch billing rows related to one completed prediction. This is billing
    /// reconciliation, not task-status retrieval.
    pub async fn billing_records(
        &self,
        prediction_id: &str,
    ) -> Result<Vec<BillingRecord>, ProviderError> {
        let resp = self
            .http
            .post(format!("{}/api/v3/billings/search", self.base))
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({
                "page": 1,
                "page_size": 100,
                "prediction_uuids": [prediction_id],
                "sort": "created_at ASC",
            }))
            .send()
            .await?;
        let result: BillingSearchResult = Self::unwrap_envelope(resp).await?;
        Ok(result.items)
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencode_reserves() {
        assert_eq!(
            urlencode("https://x.io/hook?a=1"),
            "https%3A%2F%2Fx.io%2Fhook%3Fa%3D1"
        );
    }

    #[test]
    fn request_schema_extraction() {
        let entry: ModelEntry = serde_json::from_value(serde_json::json!({
            "model_id": "m/x",
            "type": "text-to-image",
            "api_schema": {"api_schemas": [
                {"type": "other"},
                {"type": "model_run", "request_schema": {"type": "object"}}
            ]}
        }))
        .unwrap();
        assert_eq!(
            entry.request_schema(),
            Some(&serde_json::json!({"type": "object"}))
        );
    }

    #[test]
    fn billing_record_amounts_are_signed_by_type() {
        let deduct = BillingRecord {
            uuid: "bill-1".into(),
            billing_type: "deduct".into(),
            price: Some(0.06),
            created_at: None,
            updated_at: None,
            order: None,
            prediction: None,
        };
        let refund = BillingRecord {
            uuid: "bill-2".into(),
            billing_type: "refund".into(),
            price: Some(0.06),
            created_at: None,
            updated_at: None,
            order: None,
            prediction: None,
        };
        let unknown = BillingRecord {
            uuid: "bill-3".into(),
            billing_type: "adjustment".into(),
            price: Some(0.06),
            created_at: None,
            updated_at: None,
            order: None,
            prediction: None,
        };

        assert_eq!(deduct.signed_amount(), Some(0.06));
        assert_eq!(refund.signed_amount(), Some(-0.06));
        assert_eq!(unknown.signed_amount(), None);
    }
}
