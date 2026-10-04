use rmcp::model::MetaObject;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use veoveo_gateway_contract::{GATEWAY_DISCOVERY_DEGRADATION_META_KEY, GatewayDiscoveryFailure};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

    pub fn into_meta(self) -> Option<MetaObject> {
        if self.is_empty() {
            return None;
        }
        let mut meta = MetaObject::new();
        meta.0.insert(
            GATEWAY_DISCOVERY_DEGRADATION_META_KEY.to_owned(),
            serde_json::to_value(self).expect("gateway discovery degradation serializes"),
        );
        Some(meta)
    }

    pub fn from_meta(meta: Option<&MetaObject>) -> Result<Self, serde_json::Error> {
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
    use crate::ServerSlug;
    use veoveo_gateway_contract::{GatewayDiscoveryFailureCode, GatewayDiscoverySurface};

    #[test]
    fn degradation_metadata_is_typed_sorted_and_deduplicated() {
        let resources = GatewayDiscoveryFailure {
            server: ServerSlug::parse("recording").unwrap(),
            surface: GatewayDiscoverySurface::Resources,
            code: GatewayDiscoveryFailureCode::UpstreamUnavailable,
        };
        let tools = GatewayDiscoveryFailure {
            server: ServerSlug::parse("artifact").unwrap(),
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
