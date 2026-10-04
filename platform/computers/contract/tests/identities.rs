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

#[test]
fn identity_trait_preserves_canonical_constructor_and_schema_profiles() {
    use schemars::JsonSchema;
    use veoveo_computers_contract::ComputerId;
    use veoveo_types::Identity;
    let raw = "01983da0-0000-7000-8000-000000000001";
    let id = ComputerId::parse_identity(raw).unwrap();
    assert_eq!(id.identity_text(), raw);
    assert_eq!(ComputerId::try_from(id.as_uuid()).unwrap(), id);
    let schema = serde_json::to_value(schemars::schema_for!(ComputerId)).unwrap();
    assert_eq!(
        schema["pattern"],
        "^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
    );
    assert_eq!(schema["minLength"], 36);
    assert_eq!(schema["maxLength"], 36);
    assert_eq!(ComputerId::schema_id(), "ComputerId");
    assert!(!ComputerId::inline_schema());
    for value in [
        raw.to_uppercase(),
        raw.replace('-', ""),
        format!("urn:uuid:{raw}"),
    ] {
        assert!(ComputerId::parse_identity(&value).is_err());
        assert!(serde_json::from_value::<ComputerId>(serde_json::json!(value)).is_err());
    }
}

use serde::{Deserialize, de::Visitor};
struct UuidBytes<'a>(&'a [u8]);

impl<'de> serde::Deserializer<'de> for UuidBytes<'de> {
    type Error = serde::de::value::Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_borrowed_bytes(self.0)
    }

    fn is_human_readable(&self) -> bool {
        false
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum
        identifier ignored_any
    }
}

#[test]
fn computer_decode_preserves_string_only_admission() {
    use veoveo_computers_contract::ComputerId;
    let bytes = uuid::Uuid::now_v7();
    assert!(ComputerId::deserialize(UuidBytes(bytes.as_bytes())).is_err());
    assert!(RequestId::deserialize(UuidBytes(bytes.as_bytes())).is_err());
    assert!(ProviderInstanceId::deserialize(UuidBytes(bytes.as_bytes())).is_err());
}
