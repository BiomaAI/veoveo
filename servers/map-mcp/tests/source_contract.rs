use veoveo_map_mcp::contract::{
    MapSourceCursor, MapSourceId, MapSourcePage, MapSourceUri, MapSourcesUri,
};
use veoveo_types::{ResourceAddress, ResourceUri};

#[test]
fn addresses_preserve_map_identity_and_reject_ambiguous_spelling() {
    use iri_string::template::simple_context::SimpleContext;
    use veoveo_types::ResourceTemplateUri;
    for id in [MapSourceId::new(), MapSourceId::from_stable_key(b"fixture")] {
        let address = MapSourceUri::new(id.clone());
        let mut context = SimpleContext::new();
        context.insert("source_id", id.to_string());
        assert_eq!(
            ResourceTemplateUri::new(MapSourceUri::TEMPLATE)
                .unwrap()
                .expand(&context)
                .unwrap(),
            address.to_uri().unwrap()
        );
        assert_eq!(address.id(), &id);
        assert_eq!(MapSourceUri::parse(address.as_str()).unwrap(), address);
        assert_eq!(
            <MapSourceUri as ResourceAddress>::parse(&address.to_uri().unwrap()).unwrap(),
            address
        );
        assert_eq!(
            serde_json::from_value::<MapSourceUri>(serde_json::json!(address.as_str())).unwrap(),
            address
        );
        assert_eq!(
            address.to_uri().unwrap(),
            ResourceUri::new(format!("map://source/{id}")).unwrap()
        );
        for bad in [
            format!("{}?x=y", address.as_str()),
            format!("{}#x", address.as_str()),
            format!("{}/extra", address.as_str()),
            address.as_str().replace("map://", "other://"),
            address.as_str().replace("source/", "route/"),
            address.as_str().replace("source-", "source%2D"),
        ] {
            assert!(MapSourceUri::parse(bad).is_err());
        }
    }
    for value in [
        "0195dabe-7777-7abc-0def-000000000001",
        "0195dabe-7777-4abc-8def-000000000001",
        "0195DABE-7777-7ABC-8DEF-000000000001",
        "0195dabe77777abc8def000000000001",
    ] {
        assert!(MapSourceId::parse(format!("source-{value}")).is_err());
    }
}

#[test]
fn collection_cursor_checks_version_family_and_canonical_components() {
    use iri_string::template::simple_context::SimpleContext;
    use veoveo_types::ResourceTemplateUri;
    let cursor = MapSourceCursor::new(MapSourceId::new());
    for cursor in [None, Some(cursor.clone())] {
        let address = MapSourcesUri::new(cursor);
        let mut context = SimpleContext::new();
        if let Some(cursor) = address.cursor() {
            context.insert("cursor", cursor.as_str().to_owned());
        }
        assert_eq!(
            ResourceTemplateUri::new(MapSourcesUri::TEMPLATE)
                .unwrap()
                .expand(&context)
                .unwrap(),
            address.to_uri().unwrap()
        );
        assert_eq!(MapSourcesUri::parse(address.as_str()).unwrap(), address);
        assert_eq!(
            <MapSourcesUri as ResourceAddress>::parse(&address.to_uri().unwrap()).unwrap(),
            address
        );
        assert_eq!(
            serde_json::from_value::<MapSourcesUri>(serde_json::json!(address.as_str())).unwrap(),
            address
        );
    }
    let wire: serde_json::Value =
        serde_json::from_slice(&hex::decode(cursor.as_str()).unwrap()).unwrap();
    for (field, value) in [
        ("version", serde_json::json!(2)),
        ("collection", serde_json::json!("map://routes")),
        (
            "after",
            serde_json::json!("route-0195dabe-7777-7abc-8def-000000000001"),
        ),
        ("extra", serde_json::json!(true)),
    ] {
        let mut invalid = wire.clone();
        invalid[field] = value;
        assert!(
            MapSourceCursor::parse(hex::encode(serde_json::to_vec(&invalid).unwrap())).is_err()
        );
    }
    for bad in [
        "map://sources/",
        "map://sources?",
        "map://sources?cursor=",
        "map://sources?cursor=a&cursor=a",
        "map://sources?ignored=1",
        "map://sources#x",
        "map://sources?cursor=%",
        "other://sources",
    ] {
        assert!(MapSourcesUri::parse(bad).is_err());
    }
    assert!(MapSourceCursor::parse("a".repeat(1025)).is_err());
}

#[test]
fn empty_page_round_trip_and_invalid_wire_limits_fail() {
    let page = MapSourcePage::from_lookahead(vec![]).unwrap();
    let wire = serde_json::to_value(&page).unwrap();
    assert_eq!(
        wire,
        serde_json::json!({"items":[], "limit":100,"next_cursor":null})
    );
    assert_eq!(
        serde_json::from_value::<MapSourcePage>(wire.clone()).unwrap(),
        page
    );
    let mut wrong = wire.clone();
    wrong["limit"] = serde_json::json!(99);
    assert!(serde_json::from_value::<MapSourcePage>(wrong).is_err());
    let mut wrong = wire;
    wrong["next_cursor"] = serde_json::json!(MapSourceCursor::new(MapSourceId::new()));
    assert!(serde_json::from_value::<MapSourcePage>(wrong).is_err());
}

fn summary(n: usize) -> veoveo_map_mcp::contract::SourceSummary {
    serde_json::from_value(serde_json::json!({
        "source_id": format!("source-{n:08x}-0000-7000-8000-000000000000"),
        "dataset_id": "dataset-00000000-0000-7000-8000-000000000000",
        "name":"Fixture", "adapter_kind":"authority_vector", "authority":"synthetic_test",
        "acquisition_model":"snapshot", "map_families":["road_street"],
        "license":{"license_id":"fixture", "source_terms_uri":"https://fixture.local/terms",
            "attribution":"Fixture", "redistribution_allowed":true, "derivatives_allowed":true,
            "offline_bundle_allowed":true},
        "enabled":true, "record_version":1,
        "created_at":"2026-01-01T00:00:00Z", "updated_at":"2026-01-01T00:00:00Z"
    }))
    .unwrap()
}

#[test]
fn public_summary_checks_metadata_and_keeps_the_existing_wire_fields() {
    use veoveo_map_mcp::contract::SourceSummary;
    let admitted = summary(1);
    assert_eq!(admitted.resource_uri().id(), admitted.source_id());
    let wire = serde_json::to_value(&admitted).unwrap();
    let mut keys: Vec<_> = wire
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "acquisition_model",
            "adapter_kind",
            "authority",
            "created_at",
            "dataset_id",
            "enabled",
            "license",
            "map_families",
            "name",
            "record_version",
            "source_id",
            "updated_at"
        ]
    );
    assert_eq!(
        serde_json::from_value::<SourceSummary>(wire.clone()).unwrap(),
        admitted
    );
    for (field, value) in [
        ("name", serde_json::json!("")),
        ("name", serde_json::json!("bad\nname")),
        ("record_version", serde_json::json!(0)),
        ("map_families", serde_json::json!([])),
        ("updated_at", serde_json::json!("2025-01-01T00:00:00Z")),
        ("credential", serde_json::json!("unexpected secret")),
        ("source_id", serde_json::json!(admitted.dataset_id())),
    ] {
        let mut bad = wire.clone();
        bad[field] = value;
        assert!(
            serde_json::from_value::<SourceSummary>(bad).is_err(),
            "{field}"
        );
    }
    let mut bad = wire;
    bad["license"]["attribution"] = serde_json::json!("");
    assert!(serde_json::from_value::<SourceSummary>(bad).is_err());
    let schema = serde_json::to_value(schemars::schema_for!(MapSourcePage)).unwrap();
    let mut pending = vec![&schema];
    while let Some(value) = pending.pop() {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(properties) = object.get("properties").and_then(|v| v.as_object()) {
                    for private_field in ["endpoint", "credential", "publisher_key_refs"] {
                        assert!(
                            !properties.contains_key(private_field),
                            "public source schema exposes {private_field}"
                        );
                    }
                }
                pending.extend(object.values());
            }
            serde_json::Value::Array(array) => pending.extend(array),
            _ => {}
        }
    }
}

#[test]
fn pages_admit_only_sorted_unique_summaries_and_a_matching_full_page_cursor() {
    let page = MapSourcePage::from_lookahead((0..101).map(summary).collect()).unwrap();
    assert_eq!(page.items().len(), 100);
    assert_eq!(page.next_cursor().unwrap().after(), summary(99).source_id());
    let wire = serde_json::to_value(&page).unwrap();
    assert_eq!(
        serde_json::from_value::<MapSourcePage>(wire.clone()).unwrap(),
        page
    );
    assert!(MapSourcePage::from_lookahead((0..102).map(summary).collect()).is_err());
    assert!(MapSourcePage::from_lookahead(vec![summary(1), summary(1)]).is_err());
    assert!(MapSourcePage::from_lookahead(vec![summary(2), summary(1)]).is_err());
    let mut bad = wire.clone();
    bad["next_cursor"] = serde_json::json!(MapSourceCursor::new(summary(98).source_id().clone()));
    assert!(serde_json::from_value::<MapSourcePage>(bad).is_err());
    let mut bad = wire;
    bad["items"].as_array_mut().unwrap().remove(0);
    assert!(serde_json::from_value::<MapSourcePage>(bad).is_err());
}

#[test]
fn source_descriptors_own_building_and_discovery_without_reencoding_cursor_payloads() {
    assert_eq!(
        MapSourceUri::RESOURCE_ROUTES[0]
            .discovery_template()
            .unwrap(),
        MapSourceUri::TEMPLATE
    );
    assert_eq!(
        MapSourcesUri::RESOURCE_ROUTES[0]
            .discovery_template()
            .unwrap(),
        MapSourcesUri::TEMPLATE
    );
    let address = MapSourceUri::new(MapSourceId::new());
    assert_eq!(
        address.resource_components_uri().unwrap(),
        address.to_uri().unwrap()
    );
    let cursor = MapSourceCursor::new(address.id().clone());
    let page = MapSourcesUri::new(Some(cursor.clone()));
    assert_eq!(
        page.resource_components_uri().unwrap(),
        page.to_uri().unwrap()
    );
    assert_eq!(
        MapSourcesUri::parse(page.as_str()).unwrap().cursor(),
        Some(&cursor)
    );
}
