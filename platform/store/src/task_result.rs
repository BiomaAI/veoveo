//! Task results preserve JSON shape inside an explicit database envelope.
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use surrealdb::types::{Error, Kind, Object, SurrealValue, Value};

/// The required payload distinguishes a completed JSON null from an absent result.
/// Domain libraries own payload schemas; Store owns only this envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResultRecord {
    payload: JsonValue,
}

impl TaskResultRecord {
    pub fn new(payload: JsonValue) -> Self {
        Self { payload }
    }

    pub fn into_payload(self) -> JsonValue {
        self.payload
    }
}

impl SurrealValue for TaskResultRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }

    fn is_value(value: &Value) -> bool {
        matches!(value, Value::Object(object) if object.len() == 1
            && object.get("payload").is_some_and(|payload| *payload != Value::None))
    }

    fn into_value(self) -> Value {
        let mut object = Object::new();
        object.insert("payload", crate::json_value::into_surreal(self.payload));
        Value::Object(object)
    }

    fn from_value(value: Value) -> Result<Self, Error> {
        let invalid = || Error::internal("invalid stored Task result envelope".into());
        let Value::Object(mut object) = value else {
            return Err(invalid());
        };
        if object.len() != 1 {
            return Err(invalid());
        }
        match object.remove("payload") {
            Some(payload) if payload != Value::None => Ok(Self::new(
                crate::json_value::from_surreal_json_strict(payload)?,
            )),
            _ => Err(invalid()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn driver_and_json_admit_only_the_required_envelope() {
        for payload in [
            json!(null),
            json!({"value":42}),
            json!({"payload":null}),
            json!([]),
            json!(42),
        ] {
            let record = TaskResultRecord::new(payload.clone());
            let encoded = serde_json::to_value(&record).unwrap();
            assert_eq!(encoded, json!({"payload":payload}));
            assert_eq!(
                serde_json::from_value::<TaskResultRecord>(encoded)
                    .unwrap()
                    .into_payload(),
                payload
            );
            assert_eq!(
                TaskResultRecord::from_value(record.into_value())
                    .unwrap()
                    .into_payload(),
                payload
            );
        }
        for invalid in [
            json!({}),
            json!({"value":42}),
            json!({"payload":null,"extra":true}),
            json!(null),
        ] {
            assert!(serde_json::from_value::<TaskResultRecord>(invalid.clone()).is_err());
            let error = TaskResultRecord::from_value(invalid.into_value()).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("invalid stored Task result envelope")
            );
        }
    }
}
