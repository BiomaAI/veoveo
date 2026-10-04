#![cfg(feature = "contract")]
use schemars::JsonSchema;
use serde::{Deserialize, de::Visitor};
use std::any::TypeId;
use veoveo_types::Identity;
use veoveo_workspace::contract::{ChatId, MessageId, PersonId};

#[test]
fn workspace_ids_keep_uuid_versions_aliases_and_binary_admission() {
    let v4 = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    for uuid in [uuid::Uuid::nil(), v4, uuid::Uuid::now_v7()] {
        let alias = format!("urn:uuid:{}", uuid.hyphenated().to_string().to_uppercase());
        let chat = alias.parse::<ChatId>().unwrap();
        assert_eq!(chat.0, uuid);
        assert_eq!(chat.identity_text(), uuid.to_string());
        assert_eq!(serde_json::to_value(chat).unwrap(), uuid.to_string());
        assert_eq!(
            serde_json::from_value::<MessageId>(serde_json::json!(alias))
                .unwrap()
                .0,
            uuid
        );
        assert_eq!(
            ChatId::deserialize(UuidBytes(uuid.as_bytes())).unwrap().0,
            uuid
        );
        assert_eq!(
            PersonId::deserialize(UuidBytes(uuid.as_bytes())).unwrap().0,
            uuid
        );
    }
    assert!("invalid".parse::<ChatId>().is_err());
    assert!(serde_json::from_str::<ChatId>(r#""invalid""#).is_err());
    assert!(ChatId::deserialize(UuidBytes(&[0; 15])).is_err());
    assert_ne!(TypeId::of::<ChatId>(), TypeId::of::<MessageId>());
    let schema = serde_json::to_value(schemars::schema_for!(ChatId)).unwrap();
    assert_eq!(schema["title"], "Uuid");
    assert_eq!(schema["type"], "string");
    assert_eq!(schema["format"], "uuid");
    assert_eq!(ChatId::schema_id(), uuid::Uuid::schema_id());
    assert_eq!(MessageId::schema_id(), uuid::Uuid::schema_id());
    assert_eq!(ChatId::inline_schema(), uuid::Uuid::inline_schema());
}

// Exercise UUID's non-human-readable Serde profile without another codec dependency.
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
