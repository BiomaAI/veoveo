//! Separate identities for durable transcription and private browser dictation.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpeechIdentityError;
impl fmt::Display for SpeechIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Speech identity for this operation")
    }
}
impl std::error::Error for SpeechIdentityError {}

macro_rules! identity {
    ($name:ident, $versions:literal, $valid:expr) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(uuid::Uuid);
        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7())
            }
            pub fn parse(value: impl AsRef<str>) -> Result<Self, SpeechIdentityError> {
                let value = value.as_ref();
                let uuid = uuid::Uuid::parse_str(value).map_err(|_| SpeechIdentityError)?;
                if uuid.get_variant() != uuid::Variant::RFC4122
                    || !$valid(uuid.get_version_num())
                    || uuid.to_string() != value
                {
                    return Err(SpeechIdentityError);
                }
                Ok(Self(uuid))
            }
        }
        impl Default for $name {
            fn default() -> Self { Self::new() }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.0.fmt(f) }
        }
        impl FromStr for $name {
            type Err = SpeechIdentityError;
            fn from_str(value: &str) -> Result<Self, Self::Err> { Self::parse(value) }
        }
        impl TryFrom<String> for $name {
            type Error = SpeechIdentityError;
            fn try_from(value: String) -> Result<Self, Self::Error> { Self::parse(value) }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self { value.to_string() }
        }
        impl JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> { stringify!($name).into() }
            fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({ "type": "string", "format": "uuid", "maxLength": 36,
                    "pattern": concat!("^[0-9a-f]{8}-[0-9a-f]{4}-", $versions, "[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$") })
            }
        }
    };
}

identity!(TranscriptionId, "7", |version| version == 7);
// Browser crypto.randomUUID uses v4; native callers generate v7.
identity!(DictationSessionId, "[47]", |version| matches!(
    version,
    4 | 7
));

impl TryFrom<veoveo_types::TaskId> for TranscriptionId {
    type Error = SpeechIdentityError;
    fn try_from(value: veoveo_types::TaskId) -> Result<Self, Self::Error> {
        let value = value.as_uuid();
        if value.get_version_num() != 7 || value.get_variant() != uuid::Variant::RFC4122 {
            return Err(SpeechIdentityError);
        }
        Ok(Self(value))
    }
}

impl TranscriptionId {
    pub fn task_id(self) -> veoveo_types::TaskId {
        veoveo_types::TaskId::from_uuid(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_identity_profiles_are_distinct_and_redact_invalid_input() {
        let browser = "01983da0-0000-4000-8000-000000000001";
        assert!(DictationSessionId::parse(browser).is_ok());
        assert!(TranscriptionId::parse(browser).is_err());
        let task = TranscriptionId::new();
        assert_eq!(task.task_id().to_string(), task.to_string());
        assert_eq!(TranscriptionId::try_from(task.task_id()).unwrap(), task);
        for invalid in [
            "01983da0-0000-4000-8000-000000000001",
            "01983da0-0000-7000-0000-000000000001",
        ] {
            assert!(
                TranscriptionId::try_from(veoveo_types::TaskId::from_uuid(
                    invalid.parse().unwrap()
                ))
                .is_err()
            );
        }
        assert_eq!(
            serde_json::from_str::<TranscriptionId>(&serde_json::to_string(&task).unwrap())
                .unwrap(),
            task
        );
        for value in [
            "secret",
            "00000000-0000-0000-0000-000000000000",
            "01983da0-0000-7000-0000-000000000001",
            "01983da0-0000-5000-8000-000000000001",
            "01983DA0-0000-7000-8000-000000000001",
        ] {
            assert!(
                !DictationSessionId::parse(value)
                    .unwrap_err()
                    .to_string()
                    .contains(value)
            );
            assert!(TranscriptionId::parse(value).is_err());
        }
    }
}
