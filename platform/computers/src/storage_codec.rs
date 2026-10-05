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
