use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{Check, Checked};

/// An independent owner's admission and schema profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
#[schemars(rename = "ExternalCheckedIdentity")]
struct ExternalValue(uuid::Uuid);
impl Check for ExternalValue {
    type Error = &'static str;
    fn check(&self) -> Result<(), Self::Error> {
        if self.0.is_nil() {
            Err("nil identity")
        } else {
            Ok(())
        }
    }
}

#[test]
fn independent_owner_preserves_admission_wire_and_schema() {
    let value = ExternalValue(uuid::Uuid::now_v7());
    let wire = serde_json::to_value(&value).unwrap();
    let checked = Checked::new(value.clone()).unwrap();
    assert_eq!(checked.get(), &value);
    assert_eq!(serde_json::to_value(&checked).unwrap(), wire);
    assert_eq!(
        serde_json::from_value::<Checked<ExternalValue>>(wire)
            .unwrap()
            .into_inner(),
        value
    );
    assert!(Checked::new(ExternalValue(uuid::Uuid::nil())).is_err());
    assert!(
        serde_json::from_value::<Checked<ExternalValue>>(serde_json::json!(uuid::Uuid::nil()))
            .is_err()
    );
    assert_eq!(
        Checked::<ExternalValue>::schema_name(),
        ExternalValue::schema_name()
    );
    assert_eq!(
        Checked::<ExternalValue>::schema_id(),
        ExternalValue::schema_id()
    );
    assert_eq!(
        Checked::<ExternalValue>::inline_schema(),
        ExternalValue::inline_schema()
    );
    assert_eq!(
        schemars::schema_for!(Checked<ExternalValue>),
        schemars::schema_for!(ExternalValue)
    );
}

struct BinaryUuid<'a>(&'a [u8]);
impl<'de> serde::Deserializer<'de> for BinaryUuid<'de> {
    type Error = serde::de::value::Error;
    fn deserialize_any<V: serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
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
fn nonhuman_decode_still_uses_owner_admission() {
    let uuid = uuid::Uuid::now_v7();
    assert_eq!(
        Checked::<ExternalValue>::deserialize(BinaryUuid(uuid.as_bytes()))
            .unwrap()
            .get()
            .0,
        uuid
    );
    assert!(
        Checked::<ExternalValue>::deserialize(BinaryUuid(uuid::Uuid::nil().as_bytes())).is_err()
    );
    assert!(Checked::<ExternalValue>::deserialize(BinaryUuid(&[0; 15])).is_err());
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExternalModel {
    #[serde(default = "default_limit")]
    limit: u16,
    end: u16,
}
fn default_limit() -> u16 {
    3
}
static MODEL_CHECKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
impl Check for ExternalModel {
    type Error = &'static str;
    fn check(&self) -> Result<(), Self::Error> {
        MODEL_CHECKS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.limit < self.end {
            Ok(())
        } else {
            Err("unordered model")
        }
    }
}
#[test]
fn fields_defaults_metadata_and_single_check_are_delegated() {
    MODEL_CHECKS.store(0, std::sync::atomic::Ordering::SeqCst);
    let value: Checked<ExternalModel> =
        serde_json::from_value(serde_json::json!({"end":4})).unwrap();
    assert_eq!(MODEL_CHECKS.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(value.limit, 3);
    assert!(
        serde_json::from_value::<Checked<ExternalModel>>(serde_json::json!({"end":3})).is_err()
    );
    assert!(
        serde_json::from_value::<Checked<ExternalModel>>(serde_json::json!({"end":4,"unknown":1}))
            .is_err()
    );
    assert_eq!(
        schemars::schema_for!(Checked<ExternalModel>),
        schemars::schema_for!(ExternalModel)
    );
}
