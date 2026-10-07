use schemars::JsonSchema;
use serde::{Deserialize, de::Visitor};
use veoveo_recording_contract::{
    RecordingDatasetId, RecordingId, RecordingLayerId, RecordingProjectionId, RecordingReadGrantId,
};
use veoveo_types::{
    Identity, NamingProfile, NamingRole, NamingSchemaContext, ScalarNaming, with_naming_profile,
};
fn check<I: Identity + JsonSchema + serde::Serialize + serde::de::DeserializeOwned>(name: &str)
where
    I::Error: std::fmt::Debug,
{
    let value = "01983da0-0000-7000-8000-000000000001";
    let id = I::parse_identity(value).unwrap();
    assert_eq!(id.identity_text(), value);
    assert_eq!(serde_json::to_value(id).unwrap(), value);
    assert!(I::inline_schema());
    assert_eq!(I::schema_name(), name);
    assert_eq!(I::schema_id(), name);
    let mut generator = schemars::SchemaGenerator::default();
    let string_schema = String::json_schema(&mut generator);
    let expected = with_naming_profile(
        string_schema.clone(),
        NamingProfile::new(NamingRole::Scalar {
            profile: ScalarNaming::owner("veoveo_recording_contract::ids", name).unwrap(),
        })
        .unwrap(),
        NamingSchemaContext::new(&string_schema),
    )
    .unwrap();
    assert_eq!(I::json_schema(&mut generator), expected);
    for invalid in [
        value.to_uppercase(),
        value.replace('-', ""),
        format!("urn:uuid:{value}"),
        "550e8400-e29b-41d4-a716-446655440000".to_owned(),
    ] {
        assert!(I::parse_identity(&invalid).is_err());
        assert!(serde_json::from_value::<I>(serde_json::json!(invalid)).is_err());
    }
}
#[test]
fn every_recording_identity_preserves_inline_schema_and_canonical_admission() {
    check::<RecordingId>("RecordingId");
    check::<RecordingDatasetId>("RecordingDatasetId");
    check::<RecordingLayerId>("RecordingLayerId");
    check::<RecordingReadGrantId>("RecordingReadGrantId");
    check::<RecordingProjectionId>("RecordingProjectionId");
}
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
fn recording_decode_keeps_validating_string_wire() {
    let id = RecordingId::new();
    assert!(RecordingId::deserialize(UuidBytes(id.as_uuid().as_bytes())).is_err());
    assert_eq!(RecordingId::try_from(id.as_uuid()).unwrap(), id);
    assert!(RecordingId::try_from(uuid::Uuid::nil()).is_err());
}
