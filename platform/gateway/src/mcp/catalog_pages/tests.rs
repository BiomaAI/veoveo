use super::*;
use rmcp::model::ErrorCode;
use veoveo_mcp_knowledge_extension::{DocumentId, docs};
use veoveo_types::ResourceScheme;

fn template(index: usize) -> ResourceTemplate {
    let scheme = ResourceScheme::new(format!("fixture{index:03}")).unwrap();
    ResourceTemplate::new(docs::member_template(&scheme).to_string(), "Document")
}

fn continuation<T>(page: &Page<T>) -> PaginatedRequestParams {
    PaginatedRequestParams::default().with_cursor(page.next_cursor.clone())
}

fn template_ids(page: &Page<ResourceTemplate>) -> Vec<String> {
    page.items
        .iter()
        .map(|item| item.uri_template.clone())
        .collect()
}

#[test]
fn new_servers_before_the_boundary_do_not_repeat_a_declared_collection() {
    let first = page((0..205).map(template).collect(), None).unwrap();
    let request = continuation(&first);
    let mut changed = (0..205).map(template).collect::<Vec<_>>();
    changed.insert(
        0,
        ResourceTemplate::new("aaa://docs/{doc_id}", "Arriving source"),
    );
    // This is the installed failure mechanism: an insertion moves the old
    // offset onto the final member of the preceding page.
    let old_request = PaginatedRequestParams::default().with_cursor(Some("v1:100".into()));
    let old = veoveo_mcp_contract::paginate(changed.clone(), Some(&old_request), 100).unwrap();
    assert_eq!(old.items[0].uri_template, template(99).uri_template);

    let second = page(changed.clone(), Some(&request)).unwrap();
    assert_eq!(
        template_ids(&second),
        (100..200)
            .map(|i| template(i).uri_template)
            .collect::<Vec<_>>()
    );
    let third = page(changed, Some(&continuation(&second))).unwrap();
    assert_eq!(
        template_ids(&third),
        (200..205)
            .map(|i| template(i).uri_template)
            .collect::<Vec<_>>()
    );
    assert!(third.next_cursor.is_none());
}

#[test]
fn disappearing_servers_and_revoked_members_do_not_skip_remaining_entries() {
    let first = page((0..205).map(template).collect(), None).unwrap();
    // Remove entries before the boundary, the boundary itself, and a member
    // whose current policy no longer admits it on the next page.
    let changed = (50..205)
        .filter(|i| ![99, 110].contains(i))
        .map(template)
        .collect();
    let second = page(changed, Some(&continuation(&first))).unwrap();
    let expected = (100..201)
        .filter(|i| *i != 110)
        .map(|i| template(i).uri_template)
        .collect::<Vec<_>>();
    assert_eq!(template_ids(&second), expected);
    let empty = page::<ResourceTemplate>(vec![], Some(&continuation(&second))).unwrap();
    assert!(empty.items.is_empty() && empty.next_cursor.is_none());
}

#[test]
fn each_surface_keeps_its_typed_identity_in_the_cursor() {
    let tools = (0..101)
        .map(|i| {
            let name = GatewayToolName::from_parts(
                &"fixture".parse().unwrap(),
                &format!("tool{i:03}").parse().unwrap(),
            )
            .unwrap();
            Tool::new(name.to_string(), "Fixture", rmcp::model::JsonObject::new())
        })
        .collect();
    let prompts = (0..101)
        .map(|i| Prompt::new(format!("prompt{i:03}"), None::<String>, None))
        .collect();
    let resources = (0..101)
        .map(|i| {
            Resource::new(
                docs::member_uri(
                    &"fixture".parse().unwrap(),
                    &DocumentId::new(format!("d{i:03}")).unwrap(),
                )
                .to_string(),
                "Fixture",
            )
        })
        .collect();
    let tool_page = page::<Tool>(tools, None).unwrap();
    let prompt_page = page::<Prompt>(prompts, None).unwrap();
    let resource_page = page::<Resource>(resources, None).unwrap();
    let template_page = page((0..101).map(template).collect(), None).unwrap();
    assert_eq!(
        decode::<Tool>(tool_page.next_cursor.as_ref().unwrap()).unwrap(),
        "fixture__tool099".parse().unwrap()
    );
    assert_eq!(
        decode::<Prompt>(prompt_page.next_cursor.as_ref().unwrap()).unwrap(),
        "prompt099".parse().unwrap()
    );
    assert_eq!(
        decode::<Resource>(resource_page.next_cursor.as_ref().unwrap()).unwrap(),
        docs::member_uri(&"fixture".parse().unwrap(), &"d099".parse().unwrap())
    );
    assert_eq!(
        decode::<ResourceTemplate>(template_page.next_cursor.as_ref().unwrap())
            .unwrap()
            .as_str(),
        template(99).uri_template
    );
    // A literal resource URI is also a valid URI template, so the surface tag
    // must independently reject moving a resource cursor to template listing.
    assert_eq!(
        page::<ResourceTemplate>(vec![], Some(&continuation(&resource_page)))
            .unwrap_err()
            .code,
        ErrorCode::INVALID_PARAMS
    );
    assert_eq!(
        page::<Prompt>(vec![], Some(&continuation(&tool_page)))
            .unwrap_err()
            .code,
        ErrorCode::INVALID_PARAMS
    );
}

#[test]
fn malformed_versions_keys_and_excessive_cursors_fail_without_echoing_input() {
    let mut invalid = vec![
        "v1:100".into(),
        "!".into(),
        "x".repeat(MAX_CURSOR_BYTES + 1),
    ];
    for value in [
        serde_json::json!({"version":"2","surface":"templates","after":"fixture://docs/{doc_id}"}),
        serde_json::json!({"version":"1","surface":"templates","after":"not a URI"}),
        serde_json::json!({"version":"1","surface":"templates","after":"fixture://docs/{doc_id}","unknown":true}),
    ] {
        invalid.push(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).unwrap()));
    }
    for cursor in invalid {
        let request = PaginatedRequestParams::default().with_cursor(Some(cursor));
        let error = page::<ResourceTemplate>(vec![], Some(&request)).unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
        assert_eq!(
            error.message,
            "invalid catalog cursor; restart enumeration without a cursor"
        );
    }
}

#[test]
fn duplicate_upstream_identities_are_rejected_instead_of_hidden_by_pagination() {
    let error = page(vec![template(1), template(1)], None).unwrap_err();
    assert_eq!(error.code, ErrorCode::INTERNAL_ERROR);
}
