use rmcp::model::Meta;
use serde::{Deserialize, Serialize};

use crate::ServerSlug;

/// MCP result metadata carrying failures isolated from a federated catalog.
pub const GATEWAY_DISCOVERY_DEGRADATION_META_KEY: &str = "veoveo.io/gateway-discovery-degradation";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayDiscoverySurface {
    Resources,
    ResourceTemplates,
    Tools,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayDiscoveryFailureCode {
    UpstreamUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayDiscoveryFailure {
    pub server: ServerSlug,
    pub surface: GatewayDiscoverySurface,
    pub code: GatewayDiscoveryFailureCode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayDiscoveryDegradation {
    pub failures: Vec<GatewayDiscoveryFailure>,
}

impl GatewayDiscoveryDegradation {
    pub fn new(failures: impl IntoIterator<Item = GatewayDiscoveryFailure>) -> Self {
        let mut failures: Vec<_> = failures.into_iter().collect();
        failures.sort();
        failures.dedup();
        Self { failures }
    }

    pub fn is_empty(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn merge(&mut self, other: Self) {
        self.failures.extend(other.failures);
        self.failures.sort();
        self.failures.dedup();
    }

    pub fn into_meta(self) -> Option<Meta> {
        if self.is_empty() {
            return None;
        }
        let mut meta = Meta::new();
        meta.0.insert(
            GATEWAY_DISCOVERY_DEGRADATION_META_KEY.to_owned(),
            serde_json::to_value(self).expect("gateway discovery degradation serializes"),
        );
        Some(meta)
    }

    pub fn from_meta(meta: Option<&Meta>) -> Result<Self, serde_json::Error> {
        let Some(value) =
            meta.and_then(|meta| meta.0.get(GATEWAY_DISCOVERY_DEGRADATION_META_KEY).cloned())
        else {
            return Ok(Self::default());
        };
        serde_json::from_value(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degradation_metadata_is_typed_sorted_and_deduplicated() {
        let resources = GatewayDiscoveryFailure {
            server: ServerSlug::new("recording").unwrap(),
            surface: GatewayDiscoverySurface::Resources,
            code: GatewayDiscoveryFailureCode::UpstreamUnavailable,
        };
        let tools = GatewayDiscoveryFailure {
            server: ServerSlug::new("artifact").unwrap(),
            surface: GatewayDiscoverySurface::Tools,
            code: GatewayDiscoveryFailureCode::UpstreamUnavailable,
        };
        let degradation =
            GatewayDiscoveryDegradation::new([resources.clone(), tools.clone(), resources.clone()]);
        let meta = degradation.clone().into_meta().unwrap();
        assert_eq!(
            GatewayDiscoveryDegradation::from_meta(Some(&meta)).unwrap(),
            degradation
        );
        assert_eq!(degradation.failures, vec![tools, resources]);
    }
}
