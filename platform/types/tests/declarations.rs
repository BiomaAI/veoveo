#[path = "support/naming.rs"]
mod naming_baseline;
use uuid::Uuid;
use veoveo_types::*;

#[derive(Debug, thiserror::Error)]
#[error("invalid identity")]
struct Error;
fn error(_: &str, _: IdMetadata, _: IdFailure) -> Error {
    Error
}
struct Names;
impl IdProfile for Names {
    type Error = Error;
    const PROFILE: IdProfileSpec<Error> = IdProfileSpec::text(|value, _| {
        if !value.is_empty() && value.bytes().all(|b| b.is_ascii_lowercase()) {
            Ok(())
        } else {
            Err(Error)
        }
    });
}
struct MapIds;
impl IdProfile for MapIds {
    type Error = Error;
    const PROFILE: IdProfileSpec<Error> = IdProfileSpec {
        generation: IdGeneration {
            fresh: FreshId::UuidV7,
            stable_v5_namespace: Some(Uuid::from_bytes([1; 16])),
        },
        ..IdProfileSpec::uuid(
            UuidGrammar {
                versions: &[5, 7],
                variant: UuidVariant::Rfc4122,
                spelling: UuidSpelling::CanonicalLowerHyphenated,
            },
            error,
        )
    };
}
struct Requests;
impl IdProfile for Requests {
    type Error = Error;
    const PROFILE: IdProfileSpec<Error> = IdProfileSpec {
        generation: IdGeneration {
            fresh: FreshId::UuidV7,
            stable_v5_namespace: None,
        },
        wire: IdWire::UuidOutStringIn,
        schema: IdSchema::DerivedUuid,
        ..IdProfileSpec::uuid(
            UuidGrammar {
                versions: &[4, 7],
                variant: UuidVariant::Rfc4122,
                spelling: UuidSpelling::CanonicalLowerHyphenated,
            },
            error,
        )
    };
}
struct HexIds;
impl IdProfile for HexIds {
    type Error = Error;
    const PROFILE: IdProfileSpec<Error> = IdProfileSpec::hex(
        HexGrammar {
            length: 4,
            case: HexCase::Lower,
            nonzero: true,
        },
        error,
    );
}
#[veoveo_types::id(text(Names))]
#[schemars(rename = "OwnerName")]
struct Name(#[schemars(regex(pattern = "^[a-z]+$"))] String);
#[veoveo_types::id(text(Names), secret)]
struct Secret(String);
#[veoveo_types::id(prefixed(MapIds, "dataset-"), fresh, stable)]
struct DatasetId(String);
#[veoveo_types::id(uuid(Requests), fresh)]
struct RequestId(Uuid);
#[veoveo_types::id(hex(HexIds))]
struct Digest(String);

#[test]
fn admission_generation_and_schema_preserve_profile_choices() {
    let name = Name::parse("alpha").unwrap();
    let schema = serde_json::to_value(schemars::schema_for!(Name)).unwrap();
    assert_eq!(schema["title"], "OwnerName");
    assert_eq!(schema["pattern"], "^[a-z]+$");
    assert_eq!(serde_json::to_string(&name).unwrap(), "\"alpha\"");
    assert!(serde_json::from_str::<Name>("\"Upper\"").is_err());
    assert!(serde_json::from_str::<Secret>("\"Upper\"").is_err());
    assert_eq!(DatasetId::PREFIX, "dataset-");
    assert_eq!(DatasetId::new().as_uuid().get_version_num(), 7);
    let stable = DatasetId::from_stable_key(b"key");
    assert_eq!(stable.as_uuid().get_version_num(), 5);
    fn uuid<I: UuidIdentity>(value: &I) -> Uuid {
        value.as_uuid()
    }
    assert_eq!(uuid(&stable), stable.as_uuid());
    assert_eq!(stable, DatasetId::from_stable_key(b"key"));
    assert!(DatasetId::parse("dataset-not-a-uuid").is_err());
    assert!(DatasetId::parse(stable.to_string().to_uppercase()).is_err());
    let v4 = "550e8400-e29b-41d4-a716-446655440000";
    assert_eq!(RequestId::parse(v4).unwrap().as_uuid().get_version_num(), 4);
    assert_eq!(RequestId::new().as_uuid().get_version_num(), 7);
    assert!(RequestId::parse("550e8400-e29b-51d4-a716-446655440000").is_err());
    assert_eq!(Digest::parse("ab01").unwrap().as_str(), "ab01");
    assert!(Digest::parse("AB01").is_err());
    assert!(Digest::parse("0000").is_err());
}
#[test]
fn owned_admission_reuses_the_validated_allocation() {
    let value = String::from("alpha");
    let allocation = value.as_ptr();
    let name = Name::try_from(value).unwrap();
    assert_eq!(name.as_str().as_ptr(), allocation);
}
struct Uris;
impl ResourceProfile for Uris {
    type Error = Error;
    const PROFILE: ResourceProfileSpec<Self::Error> = ResourceProfileSpec {
        route_error: |_, _| Error,
    };
}
#[veoveo_types::resource_address(cached_checked(Uris), template = "sample://item/{name}")]
struct Uri {
    #[resource(cache)]
    uri: String,
    #[resource(accessor=owned)]
    name: Name,
}
#[test]
fn address_uses_the_independent_identity_admission_interface() {
    let uri = Uri::new(Name::parse("alpha").unwrap()).unwrap();
    assert_eq!(Uri::parse(uri.as_str()).unwrap(), uri);
    assert!(Uri::parse("sample://item/Upper").is_err());
}

static ADMISSIONS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn counted(value: &str) -> Result<(), Error> {
    ADMISSIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if value == "admitted" {
        Ok(())
    } else {
        Err(Error)
    }
}
#[veoveo_types::id(custom(string,error=Error,validate=counted,generate=||String::from("admitted")))]
struct Generated(String);
#[test]
fn generation_admits_its_owned_value_once() {
    ADMISSIONS.store(0, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(Generated::new().as_str(), "admitted");
    assert_eq!(ADMISSIONS.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[path = "declarations/address_forms.rs"]
mod address_forms;

#[test]
fn every_identity_schema_mode_keeps_admission_and_adds_local_classification() {
    use schemars::JsonSchema;
    fn actual<T: JsonSchema>() {
        let root = schemars::schema_for!(T);
        assert!(
            naming_profile(&root, veoveo_types::NamingSchemaContext::new(&root))
                .unwrap()
                .is_some()
        );
        let _ = naming_baseline::constraints(root.as_value().clone());
    }
    actual::<Name>();
    actual::<Secret>();
    actual::<DatasetId>();
    actual::<RequestId>();
    actual::<Digest>();
}
