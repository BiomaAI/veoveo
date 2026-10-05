//! Preserve JSON integer precision across the pinned SurrealDB driver boundary.
use serde_json::Value as JsonValue;
use surrealdb::types::{Error, Number, Object, SurrealValue, Value};

pub fn into_surreal(value: JsonValue) -> Value {
    match value {
        // The SDK's JSON conversion uses f64 for unsigned values above i64::MAX.
        // SurrealDB decimals represent every u64 exactly and still support SQL reads.
        JsonValue::Number(ref number) if number.is_u64() && number.as_i64().is_none() => {
            Value::Number(Number::Decimal(
                number.as_u64().expect("unsigned number").into(),
            ))
        }
        JsonValue::Object(fields) => {
            let mut object = Object::new();
            for (key, value) in fields {
                object.insert(key, into_surreal(value));
            }
            Value::Object(object)
        }
        JsonValue::Array(values) => Value::Array(values.into_iter().map(into_surreal).collect()),
        other => other.into_value(),
    }
}

/// Decode JSON without coercing native database identities or timestamps.
pub fn from_surreal_json(value: Value) -> Result<JsonValue, Error> {
    decode_json(value, true)
}

/// Decode an opaque JSON payload after the owner has removed admitted optional fields.
/// Every native NONE, including object members, is rejected.
pub fn from_surreal_json_strict(value: Value) -> Result<JsonValue, Error> {
    decode_json(value, false)
}

fn decode_json(value: Value, omit_absent_fields: bool) -> Result<JsonValue, Error> {
    match value {
        Value::Null => Ok(JsonValue::Null),
        Value::Bool(value) => Ok(JsonValue::Bool(value)),
        Value::String(value) => Ok(JsonValue::String(value)),
        Value::Number(Number::Int(value)) => Ok(value.into()),
        Value::Number(Number::Float(value)) => serde_json::Number::from_f64(value)
            .map(JsonValue::Number)
            .ok_or_else(|| Error::internal("Task JSON requires finite numbers".into())),
        Value::Number(Number::Decimal(value)) => value
            .to_string()
            .parse::<serde_json::Number>()
            .map(JsonValue::Number)
            .map_err(|_| Error::internal("Task decimal cannot be represented as JSON".into())),
        Value::Array(values) => values
            .into_iter()
            .map(|value| decode_json(value, omit_absent_fields))
            .collect::<Result<_, _>>()
            .map(JsonValue::Array),
        Value::Object(fields) => fields
            .into_iter()
            .filter(|(_, value)| !omit_absent_fields || *value != Value::None)
            .map(|(key, value)| decode_json(value, omit_absent_fields).map(|value| (key, value)))
            .collect::<Result<_, _>>()
            .map(JsonValue::Object),
        _ => Err(Error::internal(
            "Task JSON cannot contain native database values".into(),
        )),
    }
}

impl SurrealValue for crate::OpenObject {
    fn kind_of() -> surrealdb::types::Kind {
        surrealdb::types::Kind::Object
    }

    fn into_value(self) -> Value {
        into_surreal(JsonValue::Object(self.into_map().into_iter().collect()))
    }

    fn from_value(value: Value) -> Result<Self, Error> {
        let Value::Object(fields) = value else {
            return Err(Error::internal("expected a stored JSON object".into()));
        };
        let fields = fields
            .into_iter()
            .filter(|(_, value)| *value != Value::None)
            .map(|(key, value)| from_surreal_json(value).map(|value| (key, value)))
            .collect::<Result<_, _>>()?;
        Ok(Self::new(fields))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn opaque_json_rejects_nested_native_values_and_preserves_absence() {
        for native in [
            Value::RecordId(surrealdb::types::RecordId::new("task", "foreign")),
            chrono::Utc::now().into_value(),
        ] {
            let mut inner = Object::new();
            inner.insert("value", native);
            let mut outer = Object::new();
            outer.insert("provider", Value::Array(vec![Value::Object(inner)].into()));
            assert!(crate::OpenObject::from_value(Value::Object(outer)).is_err());
        }
        assert!(from_surreal_json(Value::Array(vec![Value::None].into())).is_err());
        let mut opaque = Object::new();
        opaque.insert("unknown", Value::None);
        assert!(from_surreal_json_strict(Value::Object(opaque)).is_err());
        let mut object = Object::new();
        object.insert("absent", Value::None);
        object.insert("null", Value::Null);
        assert_eq!(
            from_surreal_json(Value::Object(object)).unwrap(),
            json!({"null":null})
        );
    }

    #[test]
    fn json_object_and_task_payload_keep_nested_unsigned_integers_exact() {
        let payload = json!({"bounds":[i64::MIN, i64::MAX, (i64::MAX as u64) + 1, u64::MAX],
            "nested":{"value":u64::MAX}, "fraction":2.5, "null":null});
        let object: crate::OpenObject = serde_json::from_value(payload.clone()).unwrap();
        let restored = crate::OpenObject::from_value(object.into_value()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), payload);
        let result = crate::TaskResultRecord::new(payload.clone());
        assert_eq!(
            crate::TaskResultRecord::from_value(result.into_value())
                .unwrap()
                .into_payload(),
            payload
        );
    }
}
