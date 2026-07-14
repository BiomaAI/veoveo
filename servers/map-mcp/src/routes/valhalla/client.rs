use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures::StreamExt;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use url::Url;

const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ValhallaClientConfig {
    pub base_url: Url,
    pub timeout: Duration,
}

#[derive(Clone, Debug)]
pub struct ValhallaClient {
    base_url: Url,
    client: reqwest::Client,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct RouteRequest {
    pub locations: Vec<Location>,
    pub costing: String,
    pub costing_options: CostingOptions,
    pub units: &'static str,
    pub language: &'static str,
    pub shape_format: &'static str,
    pub alternates: u16,
    pub admin_crossings: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_polygons: Vec<Vec<[f64; 2]>>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct IsochroneRequest {
    pub locations: Vec<Location>,
    pub costing: String,
    pub costing_options: CostingOptions,
    pub contours: Vec<IsochroneContour>,
    pub polygons: bool,
    pub denoise: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generalize: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_polygons: Vec<Vec<[f64; 2]>>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct IsochroneContour {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct Location {
    pub lat: f64,
    pub lon: f64,
    pub r#type: &'static str,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct CostingOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto: Option<MotorizedOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bus: Option<MotorizedOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truck: Option<MotorizedOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motorcycle: Option<MotorcycleOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bicycle: Option<BicycleOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pedestrian: Option<PedestrianOptions>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct MotorizedOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axle_load: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axle_count: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hazmat: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_unpaved: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortest: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_distance: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct MotorcycleOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortest: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct BicycleOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycling_speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortest: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct PedestrianOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walking_speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_penalty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<&'static str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortest: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct RouteResponse {
    pub trip: Trip,
    #[serde(default)]
    pub alternates: Vec<Alternate>,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Alternate {
    pub trip: Trip,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Trip {
    pub status: u32,
    pub status_message: String,
    pub summary: Summary,
    pub legs: Vec<Leg>,
    #[serde(default)]
    pub admins: Vec<Admin>,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Summary {
    pub time: f64,
    pub length: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Admin {
    #[serde(default)]
    pub country_code: String,
    #[serde(default)]
    pub state_code: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Leg {
    pub shape: String,
    pub summary: Summary,
    #[serde(default)]
    pub maneuvers: Vec<Maneuver>,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Maneuver {
    pub instruction: String,
    pub begin_shape_index: usize,
    #[serde(default)]
    pub begin_heading: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
struct ErrorResponse {
    #[serde(default)]
    error_code: Option<u32>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    status_message: Option<String>,
}

impl ValhallaClient {
    pub fn new(config: ValhallaClientConfig) -> Result<Self> {
        validate_base_url(&config.base_url)?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(config.timeout)
            .build()
            .context("building loopback Valhalla client")?;
        Ok(Self {
            base_url: config.base_url,
            client,
        })
    }

    pub async fn health(&self) -> Result<()> {
        let url = self.base_url.join("status")?;
        let response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            bail!("Valhalla status endpoint returned {}", response.status());
        }
        Ok(())
    }

    pub(super) async fn route(&self, request: &RouteRequest) -> Result<RouteResponse> {
        let url = self.base_url.join("route")?;
        let response = self.client.post(url).json(request).send().await?;
        let status = response.status();
        let bytes = bounded_bytes(response).await?;
        if !status.is_success() {
            let error: ErrorResponse = serde_json::from_slice(&bytes).unwrap_or(ErrorResponse {
                error_code: None,
                error: None,
                status_message: None,
            });
            bail!(
                "Valhalla route failed with HTTP {} and code {:?}: {}",
                status,
                error.error_code,
                error
                    .error
                    .or(error.status_message)
                    .unwrap_or_else(|| "redacted engine error".to_owned())
            );
        }
        decode_json(&bytes, "Valhalla route response")
    }

    pub(super) async fn isochrone(&self, request: &IsochroneRequest) -> Result<serde_json::Value> {
        let url = self.base_url.join("isochrone")?;
        let response = self.client.post(url).json(request).send().await?;
        let status = response.status();
        let bytes = bounded_bytes(response).await?;
        if !status.is_success() {
            let error: ErrorResponse = serde_json::from_slice(&bytes).unwrap_or(ErrorResponse {
                error_code: None,
                error: None,
                status_message: None,
            });
            bail!(
                "Valhalla isochrone failed with HTTP {} and code {:?}: {}",
                status,
                error.error_code,
                error
                    .error
                    .or(error.status_message)
                    .unwrap_or_else(|| "redacted engine error".to_owned())
            );
        }
        decode_json(&bytes, "Valhalla isochrone response")
    }
}

fn validate_base_url(url: &Url) -> Result<()> {
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("Valhalla base URL must be an uncredentialed 127.0.0.1 HTTP URL");
    }
    Ok(())
}

async fn bounded_bytes(response: reqwest::Response) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        bail!("Valhalla response exceeds the configured byte limit");
    }
    let mut result = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if result.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            bail!("Valhalla response exceeds the configured byte limit");
        }
        result.extend_from_slice(&chunk);
    }
    Ok(result)
}

fn decode_json<T: DeserializeOwned>(bytes: &[u8], kind: &str) -> Result<T> {
    serde_json::from_slice(bytes).with_context(|| format!("decoding {kind}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valhalla_url_is_strictly_loopback() {
        assert!(validate_base_url(&Url::parse("http://127.0.0.1:8002/").unwrap()).is_ok());
        assert!(validate_base_url(&Url::parse("http://valhalla:8002/").unwrap()).is_err());
        assert!(validate_base_url(&Url::parse("https://127.0.0.1:8002/").unwrap()).is_err());
    }
}
