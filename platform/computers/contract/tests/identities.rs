use veoveo_computers_contract::{ProviderInstanceId, RequestId, TemplateId};

#[test]
fn request_and_provider_profiles_match_their_distinct_sources() {
    let random = "550e8400-e29b-41d4-a716-446655440000";
    assert_eq!(random.parse::<RequestId>().unwrap().to_string(), random);
    assert!(random.parse::<ProviderInstanceId>().is_ok());
    let deterministic = "550e8400-e29b-81d4-a716-446655440000";
    assert!(deterministic.parse::<ProviderInstanceId>().is_ok());
    assert!(deterministic.parse::<RequestId>().is_err());
    for invalid in [
        "00000000-0000-0000-0000-000000000000",
        "550E8400-E29B-41D4-A716-446655440000",
        "550e8400e29b41d4a716446655440000",
        "550e8400-e29b-41d4-1716-446655440000",
        "550e8400-e29b-11d4-a716-446655440000",
    ] {
        assert!(invalid.parse::<RequestId>().is_err(), "{invalid}");
        assert!(invalid.parse::<ProviderInstanceId>().is_err(), "{invalid}");
        assert!(serde_json::from_value::<RequestId>(serde_json::json!(invalid)).is_err());
    }
    let request = RequestId::new();
    assert_eq!(
        serde_json::from_value::<RequestId>(serde_json::to_value(request).unwrap()).unwrap(),
        request
    );
}

#[test]
fn template_names_are_checked_before_catalog_or_sql_use() {
    for valid in ["development", "dev-2", &"a".repeat(64)] {
        assert_eq!(valid.parse::<TemplateId>().unwrap().as_str(), valid);
    }
    for invalid in ["", "-dev", "Dev", "dev/other", "dev\n", &"a".repeat(65)] {
        assert!(invalid.parse::<TemplateId>().is_err(), "{invalid:?}");
        assert!(serde_json::from_value::<TemplateId>(serde_json::json!(invalid)).is_err());
    }
}
