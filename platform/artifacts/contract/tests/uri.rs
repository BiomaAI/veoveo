use serde_json::{Value, json};
use veoveo_artifact_contract::{ArtifactAddress, ArtifactId, ArtifactMetadata, ArtifactUri};
use veoveo_types::{ResourceAddress, ResourceScheme};

const ID: &str = "0197f78e-f2f0-7a6e-8a5d-f41c691e4471";

#[test]
fn typed_builders_cover_neutral_independent_and_standard_schemes() {
    let id = ArtifactId::parse(ID).unwrap();
    let neutral = id.plane_uri();
    assert_eq!(neutral.as_str(), format!("artifact://{ID}"));
    assert_eq!(neutral.address(), &ArtifactAddress::Plane(id));
    for name in [
        "independent+fixture.v2",
        "artifact",
        "map",
        "https",
        "http",
        "file",
        "ftp",
        "ws",
        "wss",
    ] {
        let scheme = ResourceScheme::parse(name).unwrap();
        let uri = ArtifactUri::presented(&scheme, id);
        assert_eq!(uri.as_str(), format!("{name}://artifact/{ID}"));
        assert_eq!(uri.artifact_id(), id);
        assert_eq!(
            uri.address(),
            &ArtifactAddress::Presented {
                scheme,
                artifact_id: id
            }
        );
        let wire = uri.to_uri().unwrap();
        assert_eq!(<ArtifactUri as ResourceAddress>::parse(&wire).unwrap(), uri);
        assert_eq!(
            serde_json::from_value::<ArtifactUri>(json!(uri)).unwrap(),
            uri
        );
    }
    assert_eq!(ArtifactUri::parse(neutral.as_str()).unwrap(), neutral);
}

#[test]
fn accepted_uuid_aliases_preserve_uri_spelling() {
    let id = ArtifactId::parse(ID).unwrap();
    for spelling in [ID.to_owned(), ID.to_ascii_uppercase(), ID.replace('-', "")] {
        for wire in [
            format!("artifact://{spelling}"),
            format!("fixture://artifact/{spelling}"),
        ] {
            let uri: ArtifactUri = wire.parse().unwrap();
            assert_eq!(uri.artifact_id(), id);
            assert_eq!(uri.to_string(), wire);
            assert_eq!(serde_json::to_value(uri).unwrap(), json!(wire));
        }
    }
    let wire = format!("fixture://artifact/urn:uuid:{ID}");
    let uri: ArtifactUri = wire.parse().unwrap();
    assert_eq!(uri.artifact_id(), id);
    assert_eq!(uri.as_str(), wire);
    // Resource spelling stays distinct; occurrence identity is explicitly available.
    assert_ne!(
        id.plane_uri(),
        format!("artifact://{}", ID.to_ascii_uppercase())
            .parse()
            .unwrap()
    );
}

#[test]
fn artifact_routes_reject_unqualified_addresses_without_echoing_input() {
    let invalid = [
        String::new(),
        ID.to_owned(),
        format!("artifact://{ID}/"),
        format!("artifact://urn:uuid:{ID}"),
        format!("artifact://{{{ID}}}"),
        format!("fixture://artifact/{{{ID}}}"),
        format!("fixture://artifact/{ID}/extra"),
        format!("fixture://artifact/artifact/{ID}"),
        format!("fixture://artifact//{ID}"),
        format!("fixture://wrong/{ID}"),
        format!("fixture://{ID}"),
        format!("fixture://artifact/{ID}?"),
        format!("artifact://{ID}?secret=sentinel"),
        format!("fixture://artifact/{ID}?x=1&x=2"),
        format!("fixture://artifact/{ID}#sentinel"),
        format!("fixture://sentinel@artifact/{ID}"),
        format!("fixture://artifact:80/{ID}"),
        format!("Fixture://artifact/{ID}"),
        format!("fixture://artifact/%30{}", &ID[1..]),
        format!("fixture://artifact/{ID}%2Fextra"),
        format!("fixture://artifact/{ID}\n"),
        "artifact://0197f78e-f2f0-4a6e-8a5d-f41c691e4471".to_owned(),
        "artifact://0197f78e-f2f0-7a6e-ca5d-f41c691e4471".to_owned(),
        "fixture://artifact/not-an-id".to_owned(),
    ];
    for wire in invalid {
        let error = ArtifactUri::parse(&wire).expect_err(&wire);
        assert!(!format!("{error:?}: {error}").contains("sentinel"));
        assert!(serde_json::from_value::<ArtifactUri>(json!(wire)).is_err());
    }
    for value in [Value::Null, json!(7), json!({"uri": ID})] {
        assert!(serde_json::from_value::<ArtifactUri>(value).is_err());
    }
}

#[test]
fn occurrence_admission_requires_version_seven_and_the_rfc_variant() {
    for nibble in "0123456789abcdef".chars() {
        let wire = format!("0197f78e-f2f0-7a6e-{nibble}a5d-f41c691e4471");
        assert_eq!(
            ArtifactId::try_from(uuid::Uuid::parse_str(&wire).unwrap()).is_ok(),
            matches!(nibble, '8' | '9' | 'a' | 'b')
        );
        let expected = matches!(nibble, '8' | '9' | 'a' | 'b');
        assert_eq!(ArtifactId::parse(&wire).is_ok(), expected, "{wire}");
        assert_eq!(
            serde_json::from_value::<ArtifactId>(json!(wire)).is_ok(),
            expected
        );
        assert_eq!(
            ArtifactUri::parse(&format!("artifact://{wire}")).is_ok(),
            expected
        );
    }
    for version in "012345689abcdef".chars() {
        assert!(
            ArtifactId::parse(format!("0197f78e-f2f0-{version}a6e-8a5d-f41c691e4471")).is_err()
        );
    }
    let generated = ArtifactId::new();
    assert_eq!(ArtifactId::parse(generated.to_string()).unwrap(), generated);
}

#[test]
fn metadata_identity_is_derived_and_conflicting_wire_identity_is_rejected() {
    let wire = json!({
        "artifactId": ID,
        "artifactUri": format!("artifact://{ID}"),
        "byteLen": 1,
        "createdAt": "2026-09-27T00:00:00Z"
    });
    let metadata: ArtifactMetadata = serde_json::from_value(wire.clone()).unwrap();
    let scheme = ResourceScheme::parse("independent-domain").unwrap();
    let presented = metadata.presented_under_scheme(&scheme);
    assert_eq!(presented.artifact_id(), ArtifactId::parse(ID).unwrap());
    let encoded = serde_json::to_value(&presented).unwrap();
    assert_eq!(encoded["artifactId"], ID);
    assert_eq!(
        encoded["artifactUri"],
        format!("independent-domain://artifact/{ID}")
    );
    let schema = serde_json::to_value(schemars::schema_for!(ArtifactMetadata)).unwrap();
    assert!(
        jsonschema::validator_for(&schema)
            .unwrap()
            .is_valid(&encoded)
    );
    assert_eq!(
        serde_json::from_value::<ArtifactMetadata>(encoded).unwrap(),
        presented
    );
    let mut mismatched = wire.clone();
    mismatched["artifactId"] = json!("0197f78e-f2f0-7a6e-8a5d-f41c691e4472");
    assert_eq!(
        serde_json::from_value::<ArtifactMetadata>(mismatched)
            .unwrap_err()
            .to_string(),
        "artifact metadata id and URI must identify the same occurrence"
    );
    for field in ["artifactId", "artifactUri"] {
        let mut missing = wire.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<ArtifactMetadata>(missing).is_err());
    }
    let mut alias = wire;
    alias["artifactUri"] = json!(format!("artifact://{}", ID.to_ascii_uppercase()));
    let value: ArtifactMetadata = serde_json::from_value(alias.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(value).unwrap()["artifactUri"],
        alias["artifactUri"]
    );
}
