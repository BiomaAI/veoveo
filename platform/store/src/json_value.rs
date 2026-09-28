//! Preserve JSON integer precision across the pinned SurrealDB driver boundary.
use serde_json::Value as JsonValue;
use surrealdb::types::{Error, Number, Object, SurrealValue, Value};

pub(crate) fn into_surreal(value: JsonValue) -> Value {
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

pub(crate) fn from_surreal(value: Value) -> Result<JsonValue, Error> {
    match value {
        Value::Object(fields) => fields
            .into_iter()
            .map(|(key, value)| from_surreal(value).map(|value| (key, value)))
            .collect::<Result<_, _>>()
            .map(JsonValue::Object),
        Value::Array(values) => values
            .into_iter()
            .map(from_surreal)
            .collect::<Result<_, _>>()
            .map(JsonValue::Array),
        Value::Number(Number::Decimal(number)) => number
            .to_string()
            .parse::<serde_json::Number>()
            .map(JsonValue::Number)
            .map_err(|_| Error::internal("stored decimal cannot be represented as JSON".into())),
        other => JsonValue::from_value(other),
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
            .map(|(key, value)| from_surreal(value).map(|value| (key, value)))
            .collect::<Result<_, _>>()?;
        Ok(Self::new(fields))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
