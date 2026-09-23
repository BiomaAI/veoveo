//! A failed catalog lookup is not a permission decision. Execution still requires
//! an exact current resource and tool declaration before any upstream dispatch.
use super::*;
use veoveo_mcp_contract::GatewayDiscoverySurface;

pub(super) fn missing_status(
    degradation: &veoveo_mcp_contract::GatewayDiscoveryDegradation,
    server: &str,
    surface: GatewayDiscoverySurface,
) -> StatusCode {
    if degradation
        .failures
        .iter()
        .any(|failure| failure.server.as_str() == server && failure.surface == surface)
    {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::FORBIDDEN
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_mcp_contract::{
        GatewayDiscoveryDegradation, GatewayDiscoveryFailure, GatewayDiscoveryFailureCode,
    };

    #[test]
    fn incomplete_discovery_is_retryable_and_does_not_change_other_permissions() {
        for code in [
            GatewayDiscoveryFailureCode::DiscoveryPending,
            GatewayDiscoveryFailureCode::UpstreamUnavailable,
        ] {
            let degradation = GatewayDiscoveryDegradation::new(vec![GatewayDiscoveryFailure {
                server: "uav-sim".parse().unwrap(),
                surface: GatewayDiscoverySurface::Resources,
                code,
            }]);
            assert_eq!(
                missing_status(&degradation, "uav-sim", GatewayDiscoverySurface::Resources),
                StatusCode::SERVICE_UNAVAILABLE
            );
            assert_eq!(
                missing_status(&degradation, "map", GatewayDiscoverySurface::Resources),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                missing_status(&degradation, "uav-sim", GatewayDiscoverySurface::Tools),
                StatusCode::FORBIDDEN
            );
        }
        assert_eq!(
            missing_status(
                &GatewayDiscoveryDegradation::default(),
                "uav-sim",
                GatewayDiscoverySurface::Resources
            ),
            StatusCode::FORBIDDEN
        );
    }
}
