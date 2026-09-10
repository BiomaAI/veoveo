use super::*;
use axum::http::{HeaderMap, Method};
use routes::Operation;
use veoveo_mcp_contract::{GatewayAction, PolicyTarget};

#[test]
fn origin_requires_one_exact_value() {
    let expected = HeaderValue::from_static("https://veoveo.example");
    let mut headers = HeaderMap::new();
    assert!(exact_origin(&headers, &expected).is_err());
    for invalid in [
        "null",
        "https://veoveo.example/",
        "https://foreign.example",
        "https://veoveo.example, https://foreign.example",
    ] {
        headers.insert(header::ORIGIN, HeaderValue::from_str(invalid).unwrap());
        assert!(exact_origin(&headers, &expected).is_err());
    }
    headers.insert(header::ORIGIN, expected.clone());
    assert!(exact_origin(&headers, &expected).is_ok());
    headers.append(header::ORIGIN, expected.clone());
    assert!(exact_origin(&headers, &expected).is_err());
}

#[test]
fn native_routes_use_domain_actions_and_exact_resources() {
    let id = uuid::Uuid::new_v4();
    let (target, actions) = Operation::Terminal(id).authorization();
    assert_eq!(
        actions,
        &[GatewayAction::ComputerAttach, GatewayAction::ResourcesRead]
    );
    let PolicyTarget::Resource { server, uri } = target else {
        panic!("resource target")
    };
    assert_eq!(server.as_str(), "computers");
    assert_eq!(uri.as_str(), veoveo_computers_contract::computer_uri(id));
    for (operation, name) in [
        (Operation::Create, "create"),
        (Operation::Start(id), "start"),
        (Operation::Stop(id), "stop"),
    ] {
        let (target, actions) = operation.authorization();
        assert_eq!(actions, &[GatewayAction::ToolsCall]);
        let PolicyTarget::Tool { tool, .. } = target else {
            panic!("tool target")
        };
        assert_eq!(tool.as_str(), name);
        assert!(operation.requires_contributor());
    }
    assert!(!Operation::List.requires_contributor());
    assert_eq!(
        Operation::List.authorization().1,
        &[GatewayAction::ResourcesRead]
    );
}

#[test]
fn route_selection_cannot_forward_arbitrary_paths_or_nil_computers() {
    assert!(
        Operation::from_route(
            "/computers/{profile}/{id}/start",
            &Method::GET,
            Some(uuid::Uuid::new_v4()),
            None,
        )
        .is_err()
    );
    assert!(
        Operation::from_route(
            "/computers/{profile}/{id}",
            &Method::GET,
            Some(uuid::Uuid::nil()),
            None,
        )
        .is_err()
    );
    assert!(
        Operation::from_route(
            "/computers/{profile}/{id}/provider-admin",
            &Method::POST,
            Some(uuid::Uuid::new_v4()),
            None,
        )
        .is_err()
    );
    assert_eq!(
        crate::runtime::profile_id_from_gateway_path("/computers/operator")
            .unwrap()
            .as_str(),
        "operator"
    );
}

#[test]
fn receipt_route_uses_the_parent_computers_read_authority() {
    let computer = uuid::Uuid::new_v4();
    let receipt = uuid::Uuid::new_v4();
    let path = "/computers/{profile}/{id}/operations/{operation_id}";
    let operation =
        Operation::from_route(path, &Method::GET, Some(computer), Some(receipt)).unwrap();
    assert_eq!(
        operation.service_path(),
        format!("computers/{computer}/operations/{receipt}")
    );
    assert!(!operation.requires_contributor());
    let (target, actions) = operation.authorization();
    assert_eq!(actions, &[GatewayAction::ResourcesRead]);
    let PolicyTarget::Resource { uri, .. } = target else {
        panic!("Computer resource required")
    };
    assert_eq!(
        uri.as_str(),
        veoveo_computers_contract::computer_uri(computer)
    );
    assert!(Operation::from_route(path, &Method::POST, Some(computer), Some(receipt)).is_err());
    assert!(
        Operation::from_route(path, &Method::GET, Some(computer), Some(uuid::Uuid::nil())).is_err()
    );
}
