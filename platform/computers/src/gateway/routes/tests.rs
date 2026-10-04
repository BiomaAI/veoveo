use super::parameters::Route;
use super::*;

#[test]
fn grant_and_pairing_paths_decode_their_owner_types() {
    let valid = "019b7b88-7f03-7123-8123-abcdefabcdef";
    for field in ["grant_id", "access_grant_id", "pairing_id"] {
        assert!(
            serde_json::from_value::<Route>(
                serde_json::json!({"profile": "operator", field: valid})
            )
            .is_ok()
        );
        for invalid in [
            valid.to_uppercase(),
            valid.replace('-', ""),
            uuid::Uuid::nil().to_string(),
            uuid::Uuid::new_v4().to_string(),
        ] {
            assert!(
                serde_json::from_value::<Route>(
                    serde_json::json!({"profile": "operator", field: invalid})
                )
                .is_err()
            );
        }
    }
}
use axum::http::{HeaderMap, Method};
use parameters::Operation;
use veoveo_mcp_contract::{GatewayAction, PolicyTarget};

#[test]
fn maintenance_reads_keep_parent_authority_and_cannot_become_updates() {
    let computer = veoveo_computers_contract::ComputerId::new();
    let task = uuid::Uuid::now_v7();
    let resume = Operation::from_route(
        "/computers/{profile}/{id}/maintenance/{operation_id}/resume",
        &Method::POST,
        Some(computer),
        Some(veoveo_types::TaskId::from_uuid(task)),
        None,
        None,
        None,
    )
    .unwrap();
    let (target, actions) = resume.authorization();
    assert!(matches!(target, PolicyTarget::Tool { tool, .. } if tool.as_str() == "resume_update"));
    assert_eq!(actions, &[GatewayAction::ToolsCall]);
    assert!(resume.requires_json());
    assert!(resume.requires_contributor());
    assert_eq!(
        resume.service_path(),
        format!("computers/{computer}/maintenance/{task}/resume")
    );
    assert!(
        Operation::from_route(
            "/computers/{profile}/{id}/maintenance/{operation_id}/resume",
            &Method::GET,
            Some(computer),
            Some(veoveo_types::TaskId::from_uuid(task)),
            None,
            None,
            None
        )
        .is_err()
    );
    for (path, id, suffix) in [
        (
            "/computers/{profile}/{id}/maintenance",
            None,
            "maintenance".to_owned(),
        ),
        (
            "/computers/{profile}/{id}/maintenance/{operation_id}",
            Some(task),
            format!("maintenance/{task}"),
        ),
    ] {
        let op = Operation::from_route(
            path,
            &Method::GET,
            Some(computer),
            id.map(veoveo_types::TaskId::from_uuid),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            op.authorization(),
            Operation::Read(computer).authorization()
        );
        assert!(!op.requires_contributor());
        assert!(!op.requires_json());
        assert_eq!(op.service_path(), format!("computers/{computer}/{suffix}"));
        assert!(
            Operation::from_route(
                path,
                &Method::POST,
                Some(computer),
                id.map(veoveo_types::TaskId::from_uuid),
                None,
                None,
                None
            )
            .is_err()
        );
    }
}

#[test]
fn automation_mutations_require_named_tools_and_reads_keep_exact_resource_identity() {
    let computer = veoveo_computers_contract::ComputerId::new();
    let grant = veoveo_computers_contract::AutomationGrantId::new();
    for (path, grant_id, tool) in [
        (
            "/computers/{profile}/{id}/automation",
            None,
            "grant_automation",
        ),
        (
            "/computers/{profile}/{id}/automation/{grant_id}/revoke",
            Some(grant),
            "revoke_automation",
        ),
    ] {
        let op = Operation::from_route(
            path,
            &Method::POST,
            Some(computer),
            None,
            grant_id,
            None,
            None,
        )
        .unwrap();
        let (target, actions) = op.authorization();
        assert!(matches!(target, PolicyTarget::Tool { tool: name, .. } if name.as_str() == tool));
        assert_eq!(actions, &[GatewayAction::ToolsCall]);
        assert!(op.requires_contributor());
        assert!(op.requires_json());
        assert!(
            Operation::from_route(
                path,
                &Method::GET,
                Some(computer),
                Some(veoveo_types::TaskId::from_uuid(grant.into_uuid())),
                grant_id,
                None,
                None
            )
            .is_err()
        );
    }
    let op = Operation::from_route(
        "/computers/{profile}/{id}/automation/{grant_id}",
        &Method::GET,
        Some(computer),
        None,
        Some(grant),
        None,
        None,
    )
    .unwrap();
    let (target, actions) = op.authorization();
    assert!(
        matches!(target, PolicyTarget::Resource { uri, .. } if uri == veoveo_computers_contract::automation_grant_uri(computer, grant))
    );
    assert_eq!(actions, &[GatewayAction::ResourcesRead]);
    assert!(!op.requires_contributor());
}

#[test]
fn pairing_requires_attachment_authority_and_exact_confirmation_identity() {
    let computer = veoveo_computers_contract::ComputerId::new();
    let pairing = veoveo_computers_contract::CliPairingId::new();
    let path = "/computers/{profile}/{id}/cli-pairings/{pairing_id}/confirm";
    let confirmed = Operation::from_route(
        path,
        &Method::POST,
        Some(computer),
        None,
        None,
        None,
        Some(pairing),
    )
    .unwrap();
    assert_eq!(
        confirmed.service_path(),
        format!("computers/{computer}/cli-pairings/{pairing}/confirm")
    );
    for operation in [confirmed, Operation::Pairing(computer)] {
        assert!(
            operation.is_attachment()
                && operation.requires_json()
                && operation.requires_contributor()
        );
        assert_eq!(
            operation.authorization(),
            Operation::Terminal(computer).authorization()
        );
    }
    assert!(
        Operation::from_route(
            path,
            &Method::GET,
            Some(computer),
            None,
            None,
            None,
            Some(pairing)
        )
        .is_err()
    );
    assert!(
        Operation::from_route(
            path,
            &Method::POST,
            Some(computer),
            None,
            Some(veoveo_computers_contract::AutomationGrantId::new()),
            None,
            Some(pairing)
        )
        .is_err()
    );
    assert!(veoveo_computers_contract::CliPairingId::try_from(uuid::Uuid::nil()).is_err());
}

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
    let id = veoveo_computers_contract::ComputerId::new();
    let (target, actions) = Operation::Terminal(id).authorization();
    assert_eq!(
        actions,
        &[GatewayAction::ComputerAttach, GatewayAction::ResourcesRead]
    );
    let PolicyTarget::Resource { server, uri } = target else {
        panic!("resource target")
    };
    assert_eq!(server.as_str(), "computers");
    assert_eq!(
        uri.as_str(),
        veoveo_computers_contract::computer_uri(id).as_str()
    );
    for (operation, name) in [
        (Operation::Create, "create"),
        (Operation::Start(id), "start"),
        (Operation::Stop(id), "stop"),
        (Operation::UpdateTemplate(id), "update_template"),
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
            Some(veoveo_computers_contract::ComputerId::new()),
            None,
            None,
            None,
            None
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<parameters::Route>(serde_json::json!({
            "profile": "operator", "id": uuid::Uuid::nil()
        }))
        .is_err()
    );
    assert!(
        Operation::from_route(
            "/computers/{profile}/{id}/provider-admin",
            &Method::POST,
            Some(veoveo_computers_contract::ComputerId::new()),
            None,
            None,
            None,
            None
        )
        .is_err()
    );
    assert_eq!(
        veoveo_mcp_gateway::http::profile_from_route("/computers/{profile}", "/computers/operator")
            .unwrap()
            .as_str(),
        "operator"
    );
}

#[test]
fn receipt_route_uses_the_parent_computers_read_authority() {
    let computer = veoveo_computers_contract::ComputerId::new();
    let receipt = uuid::Uuid::new_v4();
    let path = "/computers/{profile}/{id}/operations/{operation_id}";
    let operation = Operation::from_route(
        path,
        &Method::GET,
        Some(computer),
        Some(veoveo_types::TaskId::from_uuid(receipt)),
        None,
        None,
        None,
    )
    .unwrap();
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
        veoveo_computers_contract::computer_uri(computer).as_str()
    );
    assert!(
        Operation::from_route(
            path,
            &Method::POST,
            Some(computer),
            Some(veoveo_types::TaskId::from_uuid(receipt)),
            None,
            None,
            None
        )
        .is_err()
    );
    assert!(
        Operation::from_route(
            path,
            &Method::GET,
            Some(computer),
            Some(veoveo_types::TaskId::from_uuid(uuid::Uuid::nil())),
            None,
            None,
            None
        )
        .is_err()
    );
}

#[test]
fn self_revocation_is_a_json_mutation_bound_to_the_parents_read_authority() {
    let computer = veoveo_computers_contract::ComputerId::new();
    let grant = veoveo_computers_contract::AccessGrantId::new();
    let path = "/computers/{profile}/{id}/access/{access_grant_id}/revoke";
    let operation = Operation::from_route(
        path,
        &Method::POST,
        Some(computer),
        None,
        None,
        Some(grant),
        None,
    )
    .unwrap();
    assert!(operation.requires_json());
    assert!(!operation.requires_contributor());
    assert_eq!(
        operation.service_path(),
        format!("computers/{computer}/access/{grant}/revoke")
    );
    let (target, actions) = operation.authorization();
    assert_eq!(actions, &[GatewayAction::ResourcesRead]);
    let PolicyTarget::Resource { uri, .. } = target else {
        panic!("parent read target required")
    };
    assert_eq!(
        uri.as_str(),
        veoveo_computers_contract::computer_uri(computer).as_str()
    );
    assert!(
        Operation::from_route(
            path,
            &Method::GET,
            Some(computer),
            None,
            None,
            Some(grant),
            None
        )
        .is_err()
    );
    assert!(
        Operation::from_route(
            path,
            &Method::POST,
            Some(computer),
            Some(veoveo_types::TaskId::from_uuid(grant.into_uuid())),
            None,
            Some(grant),
            None
        )
        .is_err()
    );
    assert!(veoveo_computers_contract::AccessGrantId::try_from(uuid::Uuid::nil()).is_err());
}
