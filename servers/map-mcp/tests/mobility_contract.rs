use iri_string::template::simple_context::SimpleContext;
use veoveo_map_mcp::contract::*;
use veoveo_types::{ResourceAddress, ResourceTemplateUri};

fn profile(version: u64) -> MobilityProfile {
    let mut wire: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/mobility.json")).unwrap();
    wire["profile"]["metadata"]["version"] = serde_json::json!(version);
    serde_json::from_value(wire).unwrap()
}

#[test]
fn versions_preserve_numeric_wire_values_and_reject_out_of_range_or_aliases() {
    for value in [1, 10, i64::MAX as u64] {
        let version = MobilityProfileVersion::new(value).unwrap();
        assert_eq!(version.get(), value);
        assert_eq!(
            version
                .to_string()
                .parse::<MobilityProfileVersion>()
                .unwrap(),
            version
        );
        assert_eq!(
            serde_json::to_value(version).unwrap(),
            serde_json::json!(value)
        );
        assert_eq!(
            serde_json::from_value::<MobilityProfileVersion>(serde_json::json!(value)).unwrap(),
            version
        );
    }
    for value in [0, i64::MAX as u64 + 1, u64::MAX] {
        assert!(MobilityProfileVersion::new(value).is_err());
        assert!(
            serde_json::from_value::<MobilityProfileVersion>(serde_json::json!(value)).is_err()
        );
    }
    for value in ["01", "+1", "0", "-1", "1.0", "1e1", "1/2", " 1"] {
        assert!(value.parse::<MobilityProfileVersion>().is_err());
    }
    assert!(MobilityProfileVersion::try_from(-1i64).is_err());
    let schema = serde_json::to_value(schemars::schema_for!(MobilityProfileVersion)).unwrap();
    assert_eq!(schema["minimum"], 1);
    assert_eq!(schema["maximum"], i64::MAX);
}

#[test]
fn addresses_bind_both_profile_identity_and_version_and_match_their_templates() {
    for id in [
        MobilityProfileId::new(),
        MobilityProfileId::from_stable_key(b"profile"),
    ] {
        let address = MapMobilityProfileUri::new(id.clone(), MobilityProfileVersion::FIRST);
        assert_eq!(
            MapMobilityProfileUri::parse(address.as_str()).unwrap(),
            address
        );
        assert_eq!(
            <MapMobilityProfileUri as ResourceAddress>::parse(&address.to_uri().unwrap()).unwrap(),
            address
        );
        let mut context = SimpleContext::new();
        context.insert("profile_id", id.to_string());
        context.insert("profile_version", "1");
        assert_eq!(
            ResourceTemplateUri::new(MapMobilityProfileUri::TEMPLATE)
                .unwrap()
                .expand(&context)
                .unwrap(),
            address.to_uri().unwrap()
        );
        for bad in [
            format!("{}?x=y", address.as_str()),
            format!("{}#x", address.as_str()),
            format!("{}/extra", address.as_str()),
            address.as_str().replace("map://", "other://"),
            address.as_str().replace("mobility-", "mobility%2D"),
            address.as_str().replace("/1", "/01"),
            address.as_str().replace("/1", "/0"),
            address.as_str().replace("/1", "/9223372036854775808"),
        ] {
            assert!(MapMobilityProfileUri::parse(bad).is_err());
        }
    }
    for id in [
        "mobility-0195DABE-7777-7ABC-8DEF-000000000001",
        "mobility-0195dabe77777abc8def000000000001",
        "mobility-0195dabe-7777-7abc-0def-000000000001",
    ] {
        assert!(MobilityProfileId::parse(id).is_err());
    }
}

#[test]
fn collection_cursor_checks_both_positions_version_and_collection_identity() {
    let id = MobilityProfileId::new();
    let cursor =
        MapMobilityProfileCursor::new(id.clone(), MobilityProfileVersion::new(100).unwrap());
    assert_eq!(cursor.after_id(), &id);
    assert_eq!(cursor.after_version().get(), 100);
    for cursor in [None, Some(cursor.clone())] {
        let address = MapMobilityProfilesUri::new(cursor);
        assert_eq!(
            MapMobilityProfilesUri::parse(address.as_str()).unwrap(),
            address
        );
        let mut context = SimpleContext::new();
        if let Some(cursor) = address.cursor() {
            context.insert("cursor", cursor.as_str());
        }
        assert_eq!(
            ResourceTemplateUri::new(MapMobilityProfilesUri::TEMPLATE)
                .unwrap()
                .expand(&context)
                .unwrap(),
            address.to_uri().unwrap()
        );
        assert_eq!(
            serde_json::from_value::<MapMobilityProfilesUri>(serde_json::json!(address.as_str()))
                .unwrap(),
            address
        );
    }
    let wire: serde_json::Value =
        serde_json::from_slice(&hex::decode(cursor.as_str()).unwrap()).unwrap();
    for (field, value) in [
        ("version", serde_json::json!(2)),
        ("collection", serde_json::json!("map://sources")),
        ("after_id", serde_json::json!(MapSourceId::new())),
        ("after_version", serde_json::json!(0)),
        ("after_version", serde_json::json!("100")),
        ("extra", serde_json::json!(true)),
    ] {
        let mut bad = wire.clone();
        bad[field] = value;
        assert!(
            MapMobilityProfileCursor::parse(hex::encode(serde_json::to_vec(&bad).unwrap()))
                .is_err()
        );
    }
    for bad in [
        "map://mobility-profiles/",
        "map://mobility-profiles?",
        "map://mobility-profiles?cursor=",
        "map://mobility-profiles?cursor=a&cursor=a",
        "map://mobility-profiles?unknown=1",
        "map://mobility-profiles#x",
    ] {
        assert!(MapMobilityProfilesUri::parse(bad).is_err());
    }
    assert!(MapMobilityProfileCursor::parse("a".repeat(1025)).is_err());
}

#[test]
fn pages_preserve_complete_profiles_and_require_numeric_order_and_consistent_continuation() {
    let page = MapMobilityProfilePage::from_lookahead((1..=101).map(profile).collect()).unwrap();
    assert_eq!(page.items().len(), 100);
    assert_eq!(page.items()[9].metadata().version.get(), 10);
    assert_eq!(page.next_cursor().unwrap().after_version().get(), 100);
    let wire = serde_json::to_value(&page).unwrap();
    assert_eq!(
        serde_json::from_value::<MapMobilityProfilePage>(wire.clone()).unwrap(),
        page
    );
    assert_eq!(wire["items"][0], serde_json::to_value(profile(1)).unwrap());
    for items in [
        vec![profile(10), profile(2)],
        vec![profile(1), profile(1)],
        (1..=102).map(profile).collect(),
    ] {
        assert!(MapMobilityProfilePage::from_lookahead(items).is_err());
    }
    let mut bad = wire.clone();
    bad["limit"] = serde_json::json!(99);
    assert!(serde_json::from_value::<MapMobilityProfilePage>(bad).is_err());
    let mut bad = wire.clone();
    bad["items"].as_array_mut().unwrap().remove(99);
    assert!(serde_json::from_value::<MapMobilityProfilePage>(bad).is_err());
    let mut bad = wire.clone();
    bad["next_cursor"] = serde_json::json!(MapMobilityProfileCursor::new(
        MobilityProfileId::new(),
        MobilityProfileVersion::new(100).unwrap()
    ));
    assert!(serde_json::from_value::<MapMobilityProfilePage>(bad).is_err());
    let mut bad = wire;
    bad["items"][0]["profile"]["preferred_speed"] = serde_json::json!(30);
    assert!(serde_json::from_value::<MapMobilityProfilePage>(bad).is_err());
    assert!(
        MapMobilityProfilePage::from_lookahead(vec![])
            .unwrap()
            .next_cursor()
            .is_none()
    );
}

#[test]
fn profile_creation_closes_request_tagged_profile_and_nested_components() {
    let profile: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/mobility.json")).unwrap();
    let wire = serde_json::json!({"profile": profile, "idempotency_key": "fixture"});
    let schema = serde_json::to_value(schemars::schema_for!(CreateMobilityProfileRequest)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&wire));
    assert!(serde_json::from_value::<CreateMobilityProfileRequest>(wire.clone()).is_ok());
    for pointer in [
        "",
        "/profile",
        "/profile/profile",
        "/profile/profile/metadata",
        "/profile/profile/planning",
    ] {
        let mut invalid = wire.clone();
        invalid.pointer_mut(pointer).unwrap()["unexpected"] = serde_json::json!(true);
        assert!(
            serde_json::from_value::<CreateMobilityProfileRequest>(invalid.clone()).is_err(),
            "{pointer}"
        );
        assert!(!validator.is_valid(&invalid), "{pointer}");
    }
}
