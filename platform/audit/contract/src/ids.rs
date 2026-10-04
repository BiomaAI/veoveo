use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid audit identity: {0}")]
pub struct AuditIdentityError(&'static str);

#[veoveo_types::id(uuid(AuditRecordIdProfile), fresh)]
pub struct AuditRecordId(Uuid);
#[veoveo_types::id(uuid(AuditRecordIdProfile), fresh)]
pub struct AuditRequestId(Uuid);
#[veoveo_types::id(uuid(AuditRecordIdProfile), fresh)]
pub struct AuditEpisodeId(Uuid);
#[veoveo_types::id(hex(AuditTraceIdProfile))]
pub struct AuditTraceId(String);

#[veoveo_types::id(hex(AuditSpanIdProfile))]
pub struct AuditSpanId(String);

fn uuid_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type":"string","pattern":"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"})
}
fn hex_schema(length: usize) -> schemars::Schema {
    schemars::json_schema!({"type":"string","pattern":format!("^[0-9a-f]{{{}}}$",length),"not":{"const":"0".repeat(length)}})
}

use veoveo_types::{HexCase, HexGrammar, IdProfile, IdProfileSpec, UuidGrammar};

#[doc(hidden)]
pub struct AuditRecordIdProfile;
impl IdProfile for AuditRecordIdProfile {
    type Error = AuditIdentityError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::generated_uuid(UuidGrammar::canonical(&[7]), |_, metadata, _| {
            AuditIdentityError(metadata.error_context)
        })
        .owner_schema(|generator, _| uuid_schema(generator), false);
}

#[doc(hidden)]
pub struct AuditTraceIdProfile;
impl IdProfile for AuditTraceIdProfile {
    type Error = AuditIdentityError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::hex(
        HexGrammar {
            length: 32,
            case: HexCase::Lower,
            nonzero: true,
        },
        |_, metadata, _| AuditIdentityError(metadata.error_context),
    )
    .owner_schema(|_, _| hex_schema(32), false);
}
#[doc(hidden)]
pub struct AuditSpanIdProfile;
impl IdProfile for AuditSpanIdProfile {
    type Error = AuditIdentityError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::hex(
        HexGrammar {
            length: 16,
            case: HexCase::Lower,
            nonzero: true,
        },
        |_, metadata, _| AuditIdentityError(metadata.error_context),
    )
    .owner_schema(|_, _| hex_schema(16), false);
}

#[cfg(test)]
mod identity_profiles {
    use super::*;
    use schemars::JsonSchema;
    use veoveo_types::Identity;
    fn check<
        I: Identity<Error = AuditIdentityError>
            + JsonSchema
            + serde::Serialize
            + serde::de::DeserializeOwned,
    >(
        name: &str,
    ) {
        let raw = "01983da0-0000-7000-8000-000000000001";
        assert_eq!(I::parse_identity(raw).unwrap().identity_text(), raw);
        assert_eq!(I::schema_id(), name);
        assert_eq!(I::schema_name(), name);
        assert!(!I::inline_schema());
        assert_eq!(
            serde_json::to_value(I::json_schema(&mut schemars::SchemaGenerator::default()))
                .unwrap(),
            serde_json::json!({"type":"string","pattern":"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"})
        );
        for value in [
            raw.to_uppercase(),
            raw.replace('-', ""),
            format!("urn:uuid:{raw}"),
            "550e8400-e29b-41d4-a716-446655440000".to_owned(),
        ] {
            assert!(I::parse_identity(&value).is_err());
            assert!(serde_json::from_value::<I>(serde_json::json!(value)).is_err());
        }
    }
    #[test]
    fn uuid_owners_keep_frozen_schema_and_canonical_profile() {
        check::<AuditRecordId>("AuditRecordId");
        check::<AuditRequestId>("AuditRequestId");
        check::<AuditEpisodeId>("AuditEpisodeId");
    }
    #[test]
    fn audit_hex_profiles_preserve_lengths_lowercase_and_nonzero() {
        let trace = "1".repeat(32);
        let span = "a".repeat(16);
        assert_eq!(AuditTraceId::parse(&trace).unwrap().identity_text(), trace);
        assert_eq!(AuditSpanId::parse(&span).unwrap().identity_text(), span);
        assert!(AuditTraceId::parse("0".repeat(32)).is_err());
        assert!(AuditSpanId::parse("A".repeat(16)).is_err());
        assert!(AuditSpanId::parse(&trace).is_err());
        assert_eq!(
            serde_json::to_value(AuditSpanId::json_schema(
                &mut schemars::SchemaGenerator::default()
            ))
            .unwrap(),
            serde_json::json!({"type":"string","pattern":"^[0-9a-f]{16}$","not":{"const":"0".repeat(16)}})
        );
    }
}
