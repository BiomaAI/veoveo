//! Controlled Task failure and input-request envelopes at the native driver boundary.
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use surrealdb::types::{Error, Kind, SurrealValue, Value};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskFailureRecord {
    pub code: String,
    pub message: String,
    #[serde(
        default,
        deserialize_with = "present_details",
        skip_serializing_if = "Option::is_none"
    )]
    pub details: Option<JsonValue>,
}

fn present_details<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Option<JsonValue>, D::Error> {
    JsonValue::deserialize(decoder).map(Some)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, try_from = "InputRequestFields")]
pub struct TaskInputRequestRecord {
    method: String,
    params: crate::OpenObject,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputRequestFields {
    method: String,
    params: crate::OpenObject,
}
impl TryFrom<InputRequestFields> for TaskInputRequestRecord {
    type Error = String;
    fn try_from(value: InputRequestFields) -> Result<Self, Self::Error> {
        Self::new(value.method, value.params)
    }
}
impl TaskInputRequestRecord {
    pub fn new(method: String, params: crate::OpenObject) -> Result<Self, String> {
        if method.is_empty() || method.len() > 256 || method.chars().any(char::is_control) {
            return Err(
                "task input method is empty, too long, or contains a control character".into(),
            );
        }
        Ok(Self { method, params })
    }
    pub fn into_parts(self) -> (String, crate::OpenObject) {
        (self.method, self.params)
    }
}

impl SurrealValue for TaskFailureRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        crate::native_json_into_value(serde_json::to_value(self).expect("typed Task failure"))
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        let mut value = value;
        if let Value::Object(fields) = &mut value
            && fields.get("details") == Some(&Value::None)
        {
            fields.remove("details");
        }
        serde_json::from_value(crate::native_json_from_value_strict(value)?)
            .map_err(|error| Error::internal(format!("invalid stored Task failure: {error}")))
    }
}
impl SurrealValue for TaskInputRequestRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        crate::native_json_into_value(serde_json::to_value(self).expect("typed Task input request"))
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        serde_json::from_value(crate::native_json_from_value_strict(value)?)
            .map_err(|error| Error::internal(format!("invalid stored Task input request: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn failures_preserve_absent_null_and_opaque_details_with_closed_controls() {
        for value in [
            json!({"code":"owner.extension","message":"failed"}),
            json!({"code":"owner.extension","message":"failed","details":null}),
            json!({"code":"owner.extension","message":"failed","details":{"provider":[u64::MAX,null]}}),
        ] {
            let record: TaskFailureRecord = serde_json::from_value(value.clone()).unwrap();
            let restored = TaskFailureRecord::from_value(record.into_value()).unwrap();
            assert_eq!(serde_json::to_value(restored).unwrap(), value);
        }
        for value in [
            json!({"code":"x"}),
            json!({"code":42,"message":"x"}),
            json!({"code":"x","message":"x","unknown":true}),
        ] {
            assert!(TaskFailureRecord::from_value(crate::native_json_into_value(value)).is_err());
        }
    }
    #[test]
    fn native_none_is_removed_only_for_the_declared_optional_failure_field() {
        let mut failure =
            match crate::native_json_into_value(json!({"code":"x","message":"failed"})) {
                Value::Object(fields) => fields,
                _ => unreachable!(),
            };
        failure.insert("details", Value::None);
        assert!(
            TaskFailureRecord::from_value(Value::Object(failure.clone()))
                .unwrap()
                .details
                .is_none()
        );
        failure.insert("unknown", Value::None);
        assert!(TaskFailureRecord::from_value(Value::Object(failure)).is_err());
        for payload in [
            Value::None,
            Value::RecordId(crate::RecordId::new("task", "foreign")),
            chrono::Utc::now().into_value(),
        ] {
            let mut failure = match crate::native_json_into_value(
                json!({"code":"x","message":"failed","details":{"nested":[]}}),
            ) {
                Value::Object(fields) => fields,
                _ => unreachable!(),
            };
            let mut nested = surrealdb::types::Object::new();
            nested.insert("value", payload.clone());
            failure.insert("details", Value::Object(nested.clone()));
            assert!(TaskFailureRecord::from_value(Value::Object(failure)).is_err());
            let mut request =
                match crate::native_json_into_value(json!({"method":"input","params":{}})) {
                    Value::Object(fields) => fields,
                    _ => unreachable!(),
                };
            request.insert("params", Value::Object(nested));
            assert!(TaskInputRequestRecord::from_value(Value::Object(request.clone())).is_err());
            request.insert("params", crate::OpenObject::default().into_value());
            request.insert("unknown", Value::None);
            assert!(TaskInputRequestRecord::from_value(Value::Object(request)).is_err());
        }
    }

    #[test]
    fn retained_input_method_has_the_same_admission_as_construction() {
        for method in ["", "\u{0085}", "\n", &"é".repeat(129)] {
            assert!(
                TaskInputRequestRecord::new(method.into(), crate::OpenObject::default()).is_err()
            );
            assert!(
                TaskInputRequestRecord::from_value(crate::native_json_into_value(
                    json!({"method":method,"params":{}})
                ))
                .is_err()
            );
        }
        for value in [
            json!({"method":"input"}),
            json!({"method":"input","params":{},"unknown":true}),
        ] {
            assert!(
                TaskInputRequestRecord::from_value(crate::native_json_into_value(value)).is_err()
            );
        }
        let value = json!({"method":"owner/input","params":{"provider":[null,u64::MAX]}});
        let record =
            TaskInputRequestRecord::from_value(crate::native_json_into_value(value.clone()))
                .unwrap();
        assert_eq!(serde_json::to_value(record).unwrap(), value);
    }
}
