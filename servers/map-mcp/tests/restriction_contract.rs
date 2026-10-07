use veoveo_map_mcp::contract::{
    MapRestrictionCursor, MapRestrictionPage, MapRestrictionUri, MapRestrictionsUri, RestrictionId,
};
use veoveo_types::{ResourceAddress, ResourceUri};

#[test]
fn addresses_preserve_map_identity_and_reject_ambiguous_spelling() {
    use iri_string::template::simple_context::SimpleContext;
    use veoveo_types::ResourceTemplateUri;
    for id in [
        RestrictionId::new(),
        RestrictionId::from_stable_key(b"fixture"),
    ] {
        let address = MapRestrictionUri::new(id.clone());
        let mut context = SimpleContext::new();
        context.insert("restriction_id", id.to_string());
        assert_eq!(
            ResourceTemplateUri::new(MapRestrictionUri::TEMPLATE)
                .unwrap()
                .expand(&context)
                .unwrap(),
            address.to_uri().unwrap()
        );
        assert_eq!(address.id(), &id);
        assert_eq!(MapRestrictionUri::parse(address.as_str()).unwrap(), address);
        assert_eq!(
            <MapRestrictionUri as ResourceAddress>::parse(&address.to_uri().unwrap()).unwrap(),
            address
        );
        assert_eq!(
            serde_json::from_value::<MapRestrictionUri>(serde_json::json!(address.as_str()))
                .unwrap(),
            address
        );
        assert_eq!(
            address.to_uri().unwrap(),
            ResourceUri::new(format!("map://restriction/{id}")).unwrap()
        );
        for bad in [
            format!("{}?x=y", address.as_str()),
            format!("{}#x", address.as_str()),
            format!("{}/extra", address.as_str()),
            address.as_str().replace("map://", "other://"),
            address.as_str().replace("restriction/", "route/"),
            address.as_str().replace("restriction-", "restriction%2D"),
        ] {
            assert!(MapRestrictionUri::parse(bad).is_err());
        }
    }
    for value in [
        "0195dabe-7777-7abc-0def-000000000001",
        "0195dabe-7777-4abc-8def-000000000001",
        "0195DABE-7777-7ABC-8DEF-000000000001",
        "0195dabe77777abc8def000000000001",
    ] {
        assert!(RestrictionId::parse(format!("restriction-{value}")).is_err());
    }
}

#[test]
fn collection_cursor_checks_version_family_and_canonical_components() {
    use iri_string::template::simple_context::SimpleContext;
    use veoveo_types::ResourceTemplateUri;
    let cursor = MapRestrictionCursor::new(RestrictionId::new());
    for cursor in [None, Some(cursor.clone())] {
        let address = MapRestrictionsUri::new(cursor);
        let mut context = SimpleContext::new();
        if let Some(cursor) = address.cursor() {
            context.insert("cursor", cursor.as_str().to_owned());
        }
        assert_eq!(
            ResourceTemplateUri::new(MapRestrictionsUri::TEMPLATE)
                .unwrap()
                .expand(&context)
                .unwrap(),
            address.to_uri().unwrap()
        );
        assert_eq!(
            MapRestrictionsUri::parse(address.as_str()).unwrap(),
            address
        );
        assert_eq!(
            <MapRestrictionsUri as ResourceAddress>::parse(&address.to_uri().unwrap()).unwrap(),
            address
        );
        assert_eq!(
            serde_json::from_value::<MapRestrictionsUri>(serde_json::json!(address.as_str()))
                .unwrap(),
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
            MapRestrictionCursor::parse(hex::encode(serde_json::to_vec(&invalid).unwrap()))
                .is_err()
        );
    }
    for bad in [
        "map://restrictions/",
        "map://restrictions?",
        "map://restrictions?cursor=",
        "map://restrictions?cursor=a&cursor=a",
        "map://restrictions?ignored=1",
        "map://restrictions#x",
        "map://restrictions?cursor=%",
        "other://restrictions",
    ] {
        assert!(MapRestrictionsUri::parse(bad).is_err());
    }
    assert!(MapRestrictionCursor::parse("a".repeat(1025)).is_err());
}

#[test]
fn empty_page_round_trip_and_invalid_wire_limits_fail() {
    let page = MapRestrictionPage::from_lookahead(vec![]).unwrap();
    let wire = serde_json::to_value(&page).unwrap();
    assert_eq!(
        wire,
        serde_json::json!({"items":[], "limit":100,"nextCursor":null})
    );
    assert_eq!(
        serde_json::from_value::<MapRestrictionPage>(wire.clone()).unwrap(),
        page
    );
    let mut wrong = wire.clone();
    wrong["limit"] = serde_json::json!(99);
    assert!(serde_json::from_value::<MapRestrictionPage>(wrong).is_err());
    let mut wrong = wire;
    wrong["nextCursor"] = serde_json::json!(MapRestrictionCursor::new(RestrictionId::new()));
    assert!(serde_json::from_value::<MapRestrictionPage>(wrong).is_err());
}

#[test]
fn compact_summary_checks_identity_and_metadata_without_geometry() {
    use veoveo_map_mcp::contract::RestrictionSummary;
    let id = RestrictionId::new();
    let wire = serde_json::json!({
        "restrictionId": id, "resourceUri": MapRestrictionUri::new(id.clone()),
        "kind":"navigational_warning", "effectKind":"advise", "affectedMobilityFamilies":["human"],
        "validFrom":"2026-01-01T00:00:00Z", "validUntil":null, "cancelledBy":null, "recordVersion":1,
    });
    let summary: RestrictionSummary = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(summary.restriction_id(), &id);
    assert_eq!(serde_json::to_value(&summary).unwrap(), wire);
    for (key, value) in [
        ("restrictionId", serde_json::json!(RestrictionId::new())),
        ("record_version", serde_json::json!(0)),
        ("affectedMobilityFamilies", serde_json::json!([])),
        ("validUntil", serde_json::json!("2026-01-01T00:00:00Z")),
    ] {
        let mut bad = wire.clone();
        bad[key] = value;
        assert!(serde_json::from_value::<RestrictionSummary>(bad).is_err());
    }
}
