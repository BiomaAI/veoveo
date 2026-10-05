//! Lossless Task timestamps carried through clients with different datetime precision.
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{Error, Kind, SurrealValue, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskTimestampToken(DateTime<Utc>);

impl TaskTimestampToken {
    pub fn new(timestamp: DateTime<Utc>) -> Self {
        Self(timestamp)
    }

    pub fn timestamp(self) -> DateTime<Utc> {
        self.0
    }
}

impl SurrealValue for TaskTimestampToken {
    fn kind_of() -> Kind {
        Kind::String
    }

    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }

    fn into_value(self) -> Value {
        self.0
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
            .into_value()
    }

    fn from_value(value: Value) -> Result<Self, Error> {
        let Value::String(text) = value else {
            return Err(Error::internal(
                "Task timestamp token must be an RFC 3339 string".into(),
            ));
        };
        text.parse()
            .map(Self)
            .map_err(|_| Error::internal("invalid Task timestamp token timestamp".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_preserves_nanoseconds_and_rejects_nonstring_driver_values() {
        let timestamp = "2026-10-05T00:00:00.123456789Z".parse().unwrap();
        let token = TaskTimestampToken::new(timestamp);
        assert_eq!(
            TaskTimestampToken::from_value(token.into_value()).unwrap(),
            token
        );
        assert!(TaskTimestampToken::from_value(timestamp.into_value()).is_err());
        assert!(TaskTimestampToken::from_value("malformed".into_value()).is_err());
    }
}
