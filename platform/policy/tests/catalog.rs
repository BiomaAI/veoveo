#[path = "../../../testing/fixtures/catalog_registry.rs"]
mod catalog_fixture;

use veoveo_mcp_contract::{GatewayControlPlane, ServerSlug};
use veoveo_policy::{PolicyCatalog, PolicyCatalogView};

#[test]
fn services_cannot_evaluate_an_ambiguous_catalog() {
    let plane: GatewayControlPlane =
        serde_json::from_str(include_str!("../../../configs/gateway.smoke.json")).unwrap();
    let snapshot = PolicyCatalog::new(plane.clone(), catalog_fixture::registry()).unwrap();
    for server in &plane.servers {
        assert_eq!(snapshot.server(&server.slug), Some(server));
    }
    assert!(
        snapshot
            .server(&ServerSlug::parse("unknown-policy-server").unwrap())
            .is_none()
    );
    let mut duplicate = plane.clone();
    duplicate.servers.push(duplicate.servers[0].clone());
    assert!(PolicyCatalog::new(duplicate, catalog_fixture::registry()).is_err());
    let mut duplicate = plane;
    duplicate.policies.push(duplicate.policies[0].clone());
    assert!(PolicyCatalog::new(duplicate, catalog_fixture::registry()).is_err());
}
