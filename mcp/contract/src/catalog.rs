use rmcp::model::MetaObject;
use veoveo_gateway_contract::{
    GATEWAY_DISCOVERY_DEGRADATION_META_KEY, GatewayDiscoveryDegradation,
};

/// MCP metadata conversion for the protocol-independent Gateway declaration.
pub trait GatewayDiscoveryMetadata: Sized {
    fn into_meta(self) -> Option<MetaObject>;
    fn from_meta(meta: Option<&MetaObject>) -> Result<Self, serde_json::Error>;
}
impl GatewayDiscoveryMetadata for GatewayDiscoveryDegradation {
    fn into_meta(self) -> Option<MetaObject> {
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

    fn from_meta(meta: Option<&MetaObject>) -> Result<Self, serde_json::Error> {
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
    use veoveo_gateway_contract::{
        GatewayDiscoveryFailure, GatewayDiscoveryFailureCode, GatewayDiscoverySurface,
    };

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
