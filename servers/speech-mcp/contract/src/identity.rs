//! Separate identities for durable transcription and private browser dictation.

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid Speech identity for this operation")]
pub struct SpeechIdentityError;

#[veoveo_types::id(uuid(TranscriptionIdProfile), fresh)]
pub struct TranscriptionId(uuid::Uuid);

// Browser crypto.randomUUID uses v4; native callers generate v7.
#[veoveo_types::id(uuid(DictationSessionIdProfile), fresh)]
pub struct DictationSessionId(uuid::Uuid);

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

fn speech_id_schema(versions: &str) -> schemars::Schema {
    schemars::json_schema!({"type":"string","format":"uuid","maxLength":36,"pattern":format!("^[0-9a-f]{{8}}-[0-9a-f]{{4}}-{}[0-9a-f]{{3}}-[89ab][0-9a-f]{{3}}-[0-9a-f]{{12}}$",versions)})
}

use veoveo_types::{
    FreshId, IdGeneration, IdProfile, IdProfileSpec, IdSchema, UuidGrammar, UuidSpelling,
    UuidVariant,
};

#[doc(hidden)]
pub struct TranscriptionIdProfile;
impl IdProfile for TranscriptionIdProfile {
    type Error = SpeechIdentityError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        generation: IdGeneration {
            fresh: FreshId::UuidV7,
            stable_v5_namespace: None,
        },
        schema: IdSchema::Owner {
            schema: |_, _| speech_id_schema("7"),
            inline: false,
        },
        ..IdProfileSpec::uuid(
            UuidGrammar {
                versions: &[7],
                variant: UuidVariant::Rfc4122,
                spelling: UuidSpelling::CanonicalLowerHyphenated,
            },
            |_, _, _| SpeechIdentityError,
        )
    };
}
#[doc(hidden)]
pub struct DictationSessionIdProfile;
impl IdProfile for DictationSessionIdProfile {
    type Error = SpeechIdentityError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        generation: IdGeneration {
            fresh: FreshId::UuidV7,
            stable_v5_namespace: None,
        },
        schema: IdSchema::Owner {
            schema: |_, _| speech_id_schema("[47]"),
            inline: false,
        },
        ..IdProfileSpec::uuid(
            UuidGrammar {
                versions: &[4, 7],
                variant: UuidVariant::Rfc4122,
                spelling: UuidSpelling::CanonicalLowerHyphenated,
            },
            |_, _, _| SpeechIdentityError,
        )
    };
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
