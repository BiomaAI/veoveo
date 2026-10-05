//! Closed Task control envelopes with an opaque domain input.
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeSet;
use surrealdb::types::{Error, Kind, Number, Object, SurrealValue, Value};
use veoveo_types::{
    DataLabelId, GatewayProfileId, InvocationAuthority, PrincipalId, TenantId, TokenIssuer,
    TokenSubject,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskOwnerRecord {
    pub principal_key: PrincipalId,
    pub principal_kind: crate::PrincipalKind,
    pub issuer: TokenIssuer,
    pub subject: TokenSubject,
    pub profile: GatewayProfileId,
    pub tenant_key: Option<TenantId>,
    pub data_labels: BTreeSet<DataLabelId>,
    pub authority: InvocationAuthority,
}

impl SurrealValue for TaskOwnerRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        let mut value = serde_json::to_value(&self).expect("typed Task owner");
        let policy = value["authority"]["output_policy"]
            .as_object_mut()
            .expect("typed output policy");
        policy.insert(
            "initial_grants".into(),
            serde_json::to_value(&self.authority.output_policy.initial_grants)
                .expect("typed grants"),
        );
        policy.insert(
            "classification".into(),
            serde_json::to_value(&self.authority.output_policy.classification)
                .expect("typed classification"),
        );
        policy.insert(
            "data_labels".into(),
            serde_json::to_value(&self.authority.output_policy.data_labels).expect("typed labels"),
        );
        crate::json_value::into_surreal(value)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        serde_json::from_value(crate::json_value::from_surreal_json(value)?)
            .map_err(|error| Error::internal(format!("invalid stored Task owner: {error}")))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRequestRecord {
    pub input: JsonValue,
    pub status_message: Option<String>,
    pub ttl_ms: Option<u64>,
    pub poll_interval_ms: Option<u64>,
}
impl SurrealValue for TaskRequestRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        let mut object = Object::new();
        object.insert("input", crate::json_value::into_surreal(self.input));
        object.insert(
            "status_message",
            self.status_message
                .map(SurrealValue::into_value)
                .unwrap_or(Value::Null),
        );
        object.insert(
            "ttl_ms",
            self.ttl_ms
                .map(|value| crate::json_value::into_surreal(value.into()))
                .unwrap_or(Value::Null),
        );
        object.insert(
            "poll_interval_ms",
            self.poll_interval_ms
                .map(|value| crate::json_value::into_surreal(value.into()))
                .unwrap_or(Value::Null),
        );
        Value::Object(object)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        let invalid = || Error::internal("invalid stored Task request envelope".into());
        let Value::Object(mut object) = value else {
            return Err(invalid());
        };
        let input = object
            .remove("input")
            .filter(|value| *value != Value::None)
            .ok_or_else(invalid)?;
        let status_message = decode_optional(object.remove("status_message"))?;
        let ttl_ms = decode_optional(object.remove("ttl_ms"))?;
        let poll_interval_ms = decode_optional(object.remove("poll_interval_ms"))?;
        if !object.is_empty() {
            return Err(invalid());
        }
        Ok(Self {
            input: crate::json_value::from_surreal_json_strict(input)?,
            status_message,
            ttl_ms,
            poll_interval_ms,
        })
    }
}

fn decode_optional<T: serde::de::DeserializeOwned>(
    value: Option<Value>,
) -> Result<Option<T>, Error> {
    let value = value.map(|value| match value {
        Value::Number(Number::Decimal(number)) => {
            Value::Number(Number::Decimal(number.normalize()))
        }
        other => other,
    });
    match value {
        None | Some(Value::None | Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(crate::json_value::from_surreal_json(value)?)
            .map(Some)
            .map_err(|error| {
                Error::internal(format!("invalid stored Task request option: {error}"))
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn request_driver_closes_controls_and_preserves_every_json_input() {
        for input in [
            json!(null),
            json!(false),
            json!(42),
            json!(u64::MAX),
            json!("provider"),
            json!([null,{"nested":[true,u64::MAX]}]),
            json!({"provider":{"undeclared":[1,2]}}),
        ] {
            let record = TaskRequestRecord {
                input: input.clone(),
                status_message: None,
                ttl_ms: Some(u64::MAX),
                poll_interval_ms: Some(1),
            };
            let decoded = TaskRequestRecord::from_value(record.into_value()).unwrap();
            assert_eq!(decoded.input, input);
            assert_eq!(decoded.ttl_ms, Some(u64::MAX));
        }
        for value in [
            json!({}),
            json!({"input":null,"owner":{}}),
            json!({"input":{},"undeclared":true}),
            json!({"input":{},"ttl_ms":-1}),
            json!({"input":{},"poll_interval_ms":1.5}),
        ] {
            assert!(TaskRequestRecord::from_value(crate::json_value::into_surreal(value)).is_err());
        }
    }
    #[test]
    fn task_json_rejects_nested_native_values_and_nonfinite_numbers() {
        for invalid in [
            Value::RecordId(surrealdb::types::RecordId::new("provider", "native")),
            chrono::Utc::now().into_value(),
            Value::Number(Number::Float(f64::NAN)),
            Value::Number(Number::Float(f64::INFINITY)),
        ] {
            let mut value = TaskRequestRecord {
                input: json!(null),
                status_message: None,
                ttl_ms: None,
                poll_interval_ms: None,
            }
            .into_value();
            let Value::Object(ref mut controls) = value else {
                unreachable!()
            };
            let mut nested = Object::new();
            nested.insert("provider", Value::Array(vec![invalid].into()));
            controls.insert("input", Value::Object(nested));
            assert!(TaskRequestRecord::from_value(value).is_err());
        }
    }
    #[test]
    fn request_metadata_accepts_scaled_integral_decimals_without_rounding() {
        for (text, expected) in [("1.0", 1), ("18446744073709551615.0", u64::MAX)] {
            let mut value = TaskRequestRecord {
                input: json!(null),
                status_message: None,
                ttl_ms: None,
                poll_interval_ms: None,
            }
            .into_value();
            let Value::Object(ref mut object) = value else {
                unreachable!()
            };
            object.insert(
                "poll_interval_ms",
                Value::Number(Number::Decimal(text.parse().unwrap())),
            );
            assert_eq!(
                TaskRequestRecord::from_value(value)
                    .unwrap()
                    .poll_interval_ms,
                Some(expected)
            );
        }
    }
    #[test]
    fn owner_driver_requires_clearance_and_closes_nested_authority() {
        let valid = json!({"principal_key":"pilot","principal_kind":"user","issuer":"https://issuer.invalid","subject":"pilot","profile":"operator","tenant_key":null,"data_labels":["clearance"],"authority":{"work_context":"mission","tenant":"installation","membership":"contributor","policy_revision":"native","output_policy":{"owner":{"kind":"principal","id":"pilot"},"data_labels":["output"]},"provenance":{"mode":"automated"}}});
        let owner =
            TaskOwnerRecord::from_value(crate::json_value::into_surreal(valid.clone())).unwrap();
        let decoded = TaskOwnerRecord::from_value(owner.into_value()).unwrap();
        assert_eq!(
            decoded.data_labels,
            BTreeSet::from(["clearance".parse().unwrap()])
        );
        assert_ne!(
            decoded
                .authority
                .output_policy
                .data_labels
                .iter()
                .map(ToString::to_string)
                .collect::<BTreeSet<_>>(),
            decoded
                .data_labels
                .iter()
                .map(ToString::to_string)
                .collect::<BTreeSet<_>>()
        );
        for path in [
            "",
            "/authority",
            "/authority/output_policy",
            "/authority/provenance",
        ] {
            let mut invalid = valid.clone();
            invalid
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("undeclared".into(), json!(true));
            assert!(TaskOwnerRecord::from_value(crate::json_value::into_surreal(invalid)).is_err());
        }
        for (field, value) in [
            ("principal_key", ""),
            ("profile", "bad profile"),
            ("issuer", ""),
            ("subject", ""),
            ("tenant_key", ""),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = json!(value);
            assert!(
                TaskOwnerRecord::from_value(crate::json_value::into_surreal(invalid)).is_err(),
                "invalid {field}"
            );
        }
        for field in [
            "principal_key",
            "principal_kind",
            "issuer",
            "subject",
            "profile",
            "data_labels",
            "authority",
        ] {
            let mut invalid = valid.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(
                TaskOwnerRecord::from_value(crate::json_value::into_surreal(invalid)).is_err(),
                "missing {field}"
            );
        }
    }
}
