//! Media-owned cancellation request receipt at the opaque Tasks journal boundary.
use super::ProviderCancellationOutcome;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{Error, Kind, SurrealValue, Value};
use veoveo_platform_store::OpenObject;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CancellationReceiptRecord {
    pub recorded_at: DateTime<Utc>,
    pub result: ProviderCancellationOutcome,
}
impl SurrealValue for CancellationReceiptRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        veoveo_platform_store::native_json_into_value(
            serde_json::to_value(self).expect("typed provider cancellation receipt"),
        )
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        serde_json::from_value(veoveo_platform_store::native_json_from_value_strict(value)?)
            .map_err(|_| Error::internal("invalid provider cancellation receipt".into()))
    }
}
impl CancellationReceiptRecord {
    pub fn into_payload(self) -> OpenObject {
        OpenObject::from_value(self.into_value())
            .expect("typed cancellation receipt is a JSON object")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_cancellation_receipts_preserve_json_timestamp_and_all_outcomes() {
        let recorded_at = DateTime::parse_from_rfc3339("2026-01-01T01:02:03.123456789Z")
            .unwrap()
            .with_timezone(&Utc);
        for result in [
            ProviderCancellationOutcome::Requested,
            ProviderCancellationOutcome::Accepted {
                deleted_count: u64::MAX,
            },
            ProviderCancellationOutcome::NotDeleted { deleted_count: 0 },
            ProviderCancellationOutcome::Failed {
                error: "request unavailable".into(),
            },
        ] {
            let expected = serde_json::json!({"recorded_at": recorded_at, "result": result});
            let native = CancellationReceiptRecord {
                recorded_at,
                result,
            }
            .into_value();
            assert_eq!(
                veoveo_platform_store::native_json_from_value_strict(native.clone()).unwrap(),
                expected
            );
            let decoded = CancellationReceiptRecord::from_value(native.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), expected);
            for invalid in [
                Value::None,
                surrealdb::types::RecordId::new("task", "foreign").into_value(),
                recorded_at.into_value(),
            ] {
                let Value::Object(mut object) = native.clone() else {
                    panic!("receipt is a native object");
                };
                object.insert("recorded_at", invalid);
                assert!(CancellationReceiptRecord::from_value(Value::Object(object)).is_err());
            }
        }
    }
}
