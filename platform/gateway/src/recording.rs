//! Recording-specific admission for the gateway's producer transport adapter.

use veoveo_mcp_contract::{GatewayControlPlane, GatewayControlPlaneError};
use veoveo_recording_mcp::contract::RecordingProducerScope;
use veoveo_types::ScopeDefinition;

pub(crate) fn validate_ingest_scopes(
    control_plane: &GatewayControlPlane,
) -> Result<(), GatewayControlPlaneError> {
    for resource in &control_plane.recording_ingest_resources {
        if !resource
            .required_scopes
            .contains(RecordingProducerScope::Ingest.name())
        {
            return Err(GatewayControlPlaneError::InvalidRecordingIngestResource {
                resource: resource.id.clone(),
                reason: format!(
                    "required_scopes must contain {}",
                    RecordingProducerScope::Ingest
                ),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GatewayCatalog;
    use std::collections::BTreeSet;
    use veoveo_types::ScopeName;

    #[test]
    fn recording_adapter_requires_ingest_and_preserves_additional_scopes() {
        let mut configuration: GatewayControlPlane =
            serde_json::from_str(include_str!("../../../configs/gateway.smoke.json")).unwrap();
        assert_eq!(configuration.recording_ingest_resources.len(), 1);
        let installation_scope = ScopeName::new("installation:producer").unwrap();
        configuration.recording_ingest_resources[0]
            .required_scopes
            .insert(installation_scope.clone());
        for client in &mut configuration.oauth_clients {
            client.allowed_scopes.extend([
                installation_scope.clone(),
                RecordingProducerScope::Publish.into(),
            ]);
        }
        let catalog = GatewayCatalog::from_control_plane(configuration.clone()).unwrap();
        assert!(
            catalog
                .single_recording_ingest_resource()
                .unwrap()
                .required_scopes
                .contains(&installation_scope)
        );

        configuration.recording_ingest_resources[0].required_scopes =
            BTreeSet::from([RecordingProducerScope::Publish.into(), installation_scope]);
        // Generic MCP configuration admits names; the installed transport adapter
        // enforces the producer protocol without a domain dependency in MCP core.
        configuration.validate().unwrap();
        let error = GatewayCatalog::from_control_plane(configuration).unwrap_err();
        assert!(matches!(
            error.downcast_ref(),
            Some(GatewayControlPlaneError::InvalidRecordingIngestResource { .. })
        ));
    }
}
