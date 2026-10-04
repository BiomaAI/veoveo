use std::borrow::Cow;

use schemars::JsonSchema;
use serde::Deserialize;
use veoveo_types::{Identity, PrincipalId, Sha256Digest, TaskId};

fn admit(value: &str) -> Result<(), &'static str> {
    if value.starts_with("owner:") && value.len() > 6 {
        Ok(())
    } else {
        Err("missing owner identity")
    }
}

/// Identity declared by an independent owner.
#[veoveo_types::id(prefixed(ExternalNames, "owner:"))]
#[schemars(rename = "ExternalIdentity", !try_from, !into)]
struct ExternalId(#[schemars(regex(pattern = "^owner:.+$"))] String);

#[derive(Debug, PartialEq)]
struct ManualIdentity(String);
impl Identity for ManualIdentity {
    type Error = &'static str;
    fn parse_identity(value: &str) -> Result<Self, Self::Error> {
        admit(value)?;
        Ok(Self(value.to_owned()))
    }
    fn identity_text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.0)
    }
}

#[test]
fn ordinary_trait_and_attribute_admit_the_same_owner_values() {
    for value in ["owner:α", "owner:item", "", "owner:", "other:item"] {
        let derived = ExternalId::parse_identity(value);
        let manual = ManualIdentity::parse_identity(value);
        assert_eq!(derived.is_ok(), manual.is_ok());
        assert_eq!(derived.is_ok(), ExternalId::parse(value).is_ok());
        assert_eq!(derived.is_ok(), value.parse::<ExternalId>().is_ok());
        assert_eq!(
            derived.is_ok(),
            ExternalId::try_from(value.to_owned()).is_ok()
        );
        assert_eq!(
            derived.is_ok(),
            serde_json::from_value::<ExternalId>(serde_json::json!(value)).is_ok()
        );
        if let Ok(derived) = derived {
            assert!(matches!(derived.identity_text(), Cow::Borrowed(_)));
            assert_eq!(derived.identity_text(), manual.unwrap().identity_text());
            assert_eq!(serde_json::to_value(&derived).unwrap(), value);
            assert_eq!(String::from(derived), value);
        }
    }
}

#[test]
fn standard_schema_derive_keeps_owner_metadata_and_identity() {
    let schema = serde_json::to_value(schemars::schema_for!(ExternalId)).unwrap();
    assert_eq!(schema["title"], "ExternalIdentity");
    assert_eq!(
        schema["description"],
        "Identity declared by an independent owner."
    );
    assert_eq!(schema["pattern"], "^owner:.+$");
    assert_eq!(ExternalId::schema_id(), "id::ExternalIdentity");
    assert!(!ExternalId::inline_schema());
}

#[test]
fn native_task_admits_aliases_versions_and_preserves_const_uuid_access() {
    const NIL: TaskId = TaskId::from_uuid(uuid::Uuid::nil());
    const UUID: uuid::Uuid = NIL.as_uuid();
    assert_eq!(UUID, uuid::Uuid::nil());
    for value in [
        "00000000000000000000000000000000",
        "{550E8400-E29B-41D4-A716-446655440000}",
        "urn:uuid:550e8400-e29b-41d4-a716-446655440000",
    ] {
        let expected = uuid::Uuid::parse_str(value).unwrap();
        let id = TaskId::parse_identity(value).unwrap();
        assert_eq!(id.as_uuid(), expected);
        assert_eq!(id.to_string(), expected.to_string());
        assert_eq!(
            serde_json::from_value::<TaskId>(serde_json::json!(value)).unwrap(),
            id
        );
        assert_eq!(serde_json::to_value(id).unwrap(), expected.to_string());
    }
    assert_eq!(TaskId::new().as_uuid().get_version_num(), 7);
    assert_eq!(TaskId::schema_id(), "veoveo_types::task::TaskId");
}

#[test]
fn foundational_unicode_and_digest_profiles_do_not_narrow() {
    let text = " Unicode actor α ";
    assert_eq!(PrincipalId::parse_identity(text).unwrap().as_str(), text);
    let digest = Sha256Digest::from_bytes([0; 32]);
    assert_eq!(
        Sha256Digest::parse_identity(digest.as_str()).unwrap(),
        digest
    );
    let alpha_hex = Sha256Digest::from_bytes([0xab; 32]);
    assert!(
        Sha256Digest::parse_identity(&format!("sha256:{}", alpha_hex.hex().to_uppercase()))
            .is_err()
    );
    assert!(Sha256Digest::inline_schema());
}

#[test]
fn formatting_preserves_owner_string_and_uuid_formatter_profiles() {
    let id = ExternalId::parse("owner:item").unwrap();
    assert_eq!(format!("{id:>20}"), "owner:item");
    let agent = veoveo_types::AgentDefinitionId::parse("agent").unwrap();
    assert_eq!(format!("{agent:>8}"), "   agent");
    let uuid = uuid::Uuid::nil();
    let task = TaskId::from_uuid(uuid);
    assert_eq!(format!("{task:>40}"), format!("{uuid:>40}"));
}

// A generator failure must not require Debug or expose an owner's error.
struct SecretFailure;
#[veoveo_types::id(custom(string, error = SecretFailure,
    validate = |_| Err(SecretFailure), generate = || "distinctive-generator-secret".to_owned()))]
struct FailedGenerator(String);

#[test]
fn generator_failure_uses_a_value_free_diagnostic() {
    let failure = std::panic::catch_unwind(FailedGenerator::new)
        .err()
        .unwrap();
    let message = failure
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| failure.downcast_ref::<&str>().copied())
        .unwrap();
    assert_eq!(message, "owner generator must produce an admitted identity");
    assert!(!message.contains("distinctive-generator-secret"));
}

struct OpaqueInner(u32);
fn custom_text(value: &OpaqueInner) -> Cow<'_, str> {
    Cow::Owned(format!("owner:{}", value.0))
}
fn admit_custom(value: &str) -> Result<OpaqueInner, ()> {
    value
        .strip_prefix("owner:")
        .and_then(|value| value.parse().ok())
        .map(OpaqueInner)
        .ok_or(())
}
#[veoveo_types::id(custom(error = (), admit = admit_custom, text = custom_text,
    generate = || OpaqueInner(7)))]
struct ProjectedIdentity(OpaqueInner);

#[test]
fn generation_reapplies_owner_projection_without_requiring_inner_display() {
    let id = ProjectedIdentity::new();
    assert_eq!(id.identity_text(), "owner:7");
    assert_eq!(id.to_string(), "owner:7");
    assert_eq!(
        ProjectedIdentity::parse("owner:8").unwrap().identity_text(),
        "owner:8"
    );
}

use serde::de::Visitor;
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
fn task_uuid_binary_decoder_profile_is_unchanged() {
    let uuid = uuid::Uuid::now_v7();
    assert_eq!(
        TaskId::deserialize(UuidBytes(uuid.as_bytes()))
            .unwrap()
            .as_uuid(),
        uuid
    );
    assert!(TaskId::deserialize(UuidBytes(&[0; 15])).is_err());
}

struct ExternalNames;
impl veoveo_types::IdProfile for ExternalNames {
    type Error = &'static str;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| admit(value));
}
