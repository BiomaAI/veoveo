use std::future::Future;

use anyhow::Result;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

/// Provider adapter contract used by MCP generation servers.
///
/// Implementations own provider-specific API details while the server can rely
/// on common concepts: catalog models, predictions, terminal states, and output
/// URLs. The trait is intentionally small; higher-level semantics such as MCP
/// task status and artifact policy live outside individual providers.
pub trait Provider: Clone + Send + Sync + 'static {
    type Model: Clone + Send + Sync + Serialize + 'static;
    type Prediction: Clone + Send + Sync + Serialize + DeserializeOwned + 'static;

    fn name(&self) -> &'static str;
    fn uri_scheme(&self) -> &'static str;

    fn list_models(&self) -> impl Future<Output = Result<Vec<Self::Model>>> + Send;
    fn submit_prediction(
        &self,
        model_id: &str,
        input: &Value,
        webhook_url: Option<&str>,
    ) -> impl Future<Output = Result<Self::Prediction>> + Send;

    fn model_id<'a>(&self, model: &'a Self::Model) -> &'a str;
    fn model_type<'a>(&self, model: &'a Self::Model) -> &'a str;
    fn model_description<'a>(&self, model: &'a Self::Model) -> &'a str;
    fn model_base_price(&self, model: &Self::Model) -> Option<f64>;
    fn model_request_schema<'a>(&self, model: &'a Self::Model) -> Option<&'a Value>;

    fn prediction_id<'a>(&self, prediction: &'a Self::Prediction) -> &'a str;
    fn prediction_model<'a>(&self, prediction: &'a Self::Prediction) -> &'a str;
    fn prediction_status<'a>(&self, prediction: &'a Self::Prediction) -> &'a str;
    fn prediction_outputs<'a>(&self, prediction: &'a Self::Prediction) -> &'a [String];
    fn prediction_error<'a>(&self, prediction: &'a Self::Prediction) -> Option<&'a str>;
    fn prediction_execution_time_ms(&self, prediction: &Self::Prediction) -> Option<f64>;

    fn prediction_is_terminal(&self, prediction: &Self::Prediction) -> bool {
        matches!(self.prediction_status(prediction), "completed" | "failed")
    }
}
