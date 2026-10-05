//! JSON contracts stored as native values; owner defaults are applied before CAS.
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value as Json;
use surrealdb::types::{Error, Kind, Number, SerdeWrapper, SurrealValue, Value};

pub(crate) fn encode(value: &impl Serialize) -> Json {
    serde_json::to_value(value).expect("typed Computer control object serializes as JSON")
}

pub(crate) fn native(value: Json) -> Value {
    SerdeWrapper(value).into_value()
}

fn json(value: Value) -> Result<Json, Error> {
    match value {
        Value::Null => Ok(Json::Null),
        Value::Bool(value) => Ok(value.into()),
        Value::String(value) => Ok(value.into()),
        Value::Number(Number::Int(value)) => Ok(value.into()),
        Value::Number(Number::Float(value)) => serde_json::Number::from_f64(value)
            .map(Json::Number)
            .ok_or_else(|| Error::internal("Computer controls require finite numbers".into())),
        Value::Number(Number::Decimal(value)) => value
            .normalize()
            .to_string()
            .parse::<serde_json::Number>()
            .map(Json::Number)
            .map_err(|_| Error::internal("Computer control decimal is outside JSON range".into())),
        Value::Array(values) => values
            .into_iter()
            .map(json)
            .collect::<Result<_, _>>()
            .map(Json::Array),
        Value::Object(fields) => fields
            .into_iter()
            .filter(|(_, value)| !matches!(value, Value::None))
            .map(|(key, value)| json(value).map(|value| (key, value)))
            .collect::<Result<_, _>>()
            .map(Json::Object),
        _ => Err(Error::internal(
            "Computer controls cannot contain native database values".into(),
        )),
    }
}

pub(crate) fn decode<T: DeserializeOwned>(value: Value) -> Result<T, Error> {
    serde_json::from_value(json(value)?).map_err(|error| {
        Error::internal(format!("invalid stored Computer control object: {error}"))
    })
}

fn principal_defaults(value: &mut Json) {
    let fields = value.as_object_mut().expect("typed Principal object");
    for name in [
        "groups",
        "group_roles",
        "roles",
        "scopes",
        "data_labels",
        "assurances",
    ] {
        fields
            .entry(name)
            .or_insert_with(|| Json::Array(Vec::new()));
    }
}

fn authority(value: &crate::AcceptedAuthority) -> Value {
    let mut value = encode(value);
    principal_defaults(&mut value["actor"]);
    principal_defaults(&mut value["request_context"]["principal"]);
    let policy = value["invocation"]["output_policy"]
        .as_object_mut()
        .expect("typed output policy object");
    policy.entry("classification").or_insert(Json::Null);
    for name in ["initial_grants", "data_labels"] {
        policy
            .entry(name)
            .or_insert_with(|| Json::Array(Vec::new()));
    }
    native(value)
}

impl SurrealValue for crate::AcceptedAuthority {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        authority(&self)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        decode(value)
    }
}

impl SurrealValue for crate::ExecutionDecision {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        native(encode(&self))
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        decode(value)
    }
}

impl SurrealValue for crate::commands::CommandDispatchDecision {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        native(encode(&self))
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        decode(value)
    }
}

impl SurrealValue for crate::files::FileDispatchDecision {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        native(encode(&self))
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        decode(value)
    }
}

impl SurrealValue for crate::secrets::CommandBinding {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        native(encode(&self))
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        decode(value)
    }
}

impl SurrealValue for crate::secrets::FileTransferBinding {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        native(encode(&self))
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        decode(value)
    }
}

/// Borrowed bindings keep sealed access non-cloneable and non-debuggable.
pub(crate) fn sealed_output_access(value: &crate::secrets::SealedOutputAccess) -> Value {
    native(encode(value))
}

pub(crate) fn sealed_file_access(value: &crate::secrets::SealedFileTransferAccess) -> Value {
    native(encode(value))
}

pub(crate) fn file_limits(value: &crate::api::FileTransferLimits) -> Value {
    native(encode(value))
}

pub(crate) fn file_result(value: &crate::api::FileTransferResult) -> Value {
    native(encode(value))
}

pub(crate) fn command_result(value: &crate::api::ExecutionResult) -> Value {
    native(encode(value))
}

#[derive(serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct CommandResultRecord(pub(crate) crate::api::ExecutionResult);

impl SurrealValue for CommandResultRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        command_result(&self.0)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        decode(value)
    }
}

#[derive(serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct FileLimitsRecord(pub(crate) crate::api::FileTransferLimits);

impl SurrealValue for FileLimitsRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        file_limits(&self.0)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        decode(value)
    }
}

#[derive(serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct FileResultRecord(pub(crate) crate::api::FileTransferResult);

impl SurrealValue for FileResultRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        file_result(&self.0)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        decode(value)
    }
}

impl SurrealValue for crate::secrets::SealedOutputAccess {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        sealed_output_access(&self)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        decode(value)
    }
}

impl SurrealValue for crate::secrets::SealedFileTransferAccess {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        sealed_file_access(&self)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        decode(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, PartialEq, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Controls {
        optional: Option<String>,
        count: u64,
    }

    #[test]
    fn file_limits_driver_preserves_known_fields_and_rejects_wrong_shapes() {
        let wire = serde_json::json!({"maximumSeconds":300, "maximumBytes":67108864, "onInterruption":"stop_computer"});
        let expected = native(wire.clone());
        let decoded = FileLimitsRecord::from_value(expected.clone()).unwrap();
        assert_eq!(file_limits(&decoded.0), expected);
        for malformed in [
            {
                let mut value = wire.clone();
                value["unknown"] = true.into();
                value
            },
            {
                let mut value = wire.clone();
                value["maximumBytes"] = "67108864".into();
                value
            },
            {
                let mut value = wire;
                value["onInterruption"] = "continue".into();
                value
            },
        ] {
            assert!(FileLimitsRecord::from_value(native(malformed)).is_err());
        }
        assert!(
            FileResultRecord::from_value(native(
                serde_json::json!({"result_uri":"computer://transfers/invalid"})
            ))
            .is_err()
        );
        assert!(
            CommandResultRecord::from_value(native(
                serde_json::json!({"result_uri":"computer://executions/invalid"})
            ))
            .is_err()
        );
    }

    fn sealed_wire() -> Json {
        serde_json::json!({
            "version": 1,
            "key_id": "019b0000-0000-7000-8000-000000000001",
            "nonce": "fixture-nonce",
            "ciphertext": "opaque-fixture-ciphertext",
            "fingerprint": "fixture-fingerprint"
        })
    }

    #[test]
    fn sealed_access_driver_round_trip_preserves_every_cas_field() {
        let wire = sealed_wire();
        let output: crate::secrets::SealedOutputAccess =
            serde_json::from_value(wire.clone()).unwrap();
        let file: crate::secrets::SealedFileTransferAccess =
            serde_json::from_value(wire.clone()).unwrap();
        let expected = native(wire.clone());
        assert_eq!(sealed_output_access(&output), expected);
        assert_eq!(sealed_file_access(&file), expected);
        let decoded = crate::secrets::SealedOutputAccess::from_value(expected.clone()).unwrap();
        assert_eq!(sealed_output_access(&decoded), expected);
        let decoded =
            crate::secrets::SealedFileTransferAccess::from_value(expected.clone()).unwrap();
        assert_eq!(sealed_file_access(&decoded), expected);
        for field in ["version", "key_id", "nonce", "ciphertext", "fingerprint"] {
            let mut changed = wire.clone();
            changed[field] = match field {
                "version" => 2.into(),
                "key_id" => "019b0000-0000-7000-8000-000000000002".into(),
                _ => "changed".into(),
            };
            let decoded = crate::secrets::SealedOutputAccess::from_value(native(changed)).unwrap();
            assert_ne!(
                sealed_output_access(&decoded),
                expected,
                "CAS includes {field}"
            );
        }
    }

    #[test]
    fn sealed_access_driver_rejects_unknown_missing_and_native_fields() {
        for malformed in [
            {
                let mut wire = sealed_wire();
                wire["unexpected"] = true.into();
                wire
            },
            {
                let mut wire = sealed_wire();
                wire.as_object_mut().unwrap().remove("nonce");
                wire
            },
            {
                let mut wire = sealed_wire();
                wire["version"] = Json::Null;
                wire
            },
        ] {
            assert!(
                crate::secrets::SealedOutputAccess::from_value(native(malformed.clone())).is_err()
            );
            assert!(
                crate::secrets::SealedFileTransferAccess::from_value(native(malformed)).is_err()
            );
        }
        let mut native_fields = match native(sealed_wire()) {
            Value::Object(fields) => fields,
            _ => unreachable!(),
        };
        native_fields.insert(
            "ciphertext",
            Value::RecordId(surrealdb::types::RecordId::new("secret", "fixture")),
        );
        assert!(
            crate::secrets::SealedOutputAccess::from_value(Value::Object(native_fields)).is_err()
        );
    }

    #[test]
    fn null_absent_and_unsigned_controls_preserve_json_semantics() {
        for optional in [Value::Null, Value::None] {
            let value = Value::Object(
                [
                    ("optional".into(), optional),
                    ("count".into(), SerdeWrapper(u64::MAX).into_value()),
                ]
                .into_iter()
                .collect(),
            );
            assert_eq!(
                decode::<Controls>(value).unwrap(),
                Controls {
                    optional: None,
                    count: u64::MAX
                }
            );
        }
    }

    #[test]
    fn native_id_and_absent_array_members_are_rejected() {
        assert!(
            json(Value::RecordId(surrealdb::types::RecordId::new(
                "principal",
                "fixture"
            )))
            .is_err()
        );
        assert!(json(Value::Array(vec![Value::None].into())).is_err());
        assert!(
            json(Value::Object(
                [(
                    "nested".into(),
                    Value::RecordId(surrealdb::types::RecordId::new("principal", "fixture"))
                )]
                .into_iter()
                .collect()
            ))
            .is_err()
        );
    }
}
