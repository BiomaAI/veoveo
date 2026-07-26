use jsonschema::Validator;
use serde_json::json;
use veoveo_extension_contract::{
    ArtifactCoordinate, ArtifactDescriptor, ArtifactDigest, ArtifactKind, ArtifactName,
    CompatibilityManifest, CompatibilityManifestSchema, CompatibilityReleaseId,
    ContractCompatibility, ContractKind, ExtensionContractError, ExtensionId,
    ExtensionReleaseManifest, ExtensionReleaseSchema, ExtensionSource, ReleaseVersion,
    SdkCompatibility, SdkLanguage, SourceRevision, VersionRequirement,
    compatibility_manifest_schema, extension_release_schema,
};

fn digest(byte: char) -> ArtifactDigest {
    ArtifactDigest::new(format!("sha256:{}", byte.to_string().repeat(64))).expect("valid digest")
}

fn artifact(name: &str, kind: ArtifactKind, byte: char) -> ArtifactDescriptor {
    ArtifactDescriptor {
        name: ArtifactName::new(name).expect("artifact name"),
        kind,
        version: ReleaseVersion::new("1.0.0").expect("version"),
        coordinate: ArtifactCoordinate::new(format!("oci://registry.example/{name}:1.0.0"))
            .expect("coordinate"),
        digest: digest(byte),
        platform: None,
        media_type: None,
    }
}

fn manifest() -> CompatibilityManifest {
    CompatibilityManifest {
        schema_version: CompatibilityManifestSchema::V1,
        release: CompatibilityReleaseId::new("1.0.0").expect("release"),
        platform_version: ReleaseVersion::new("1.0.0").expect("platform version"),
        contracts: vec![ContractCompatibility {
            kind: ContractKind::McpServer,
            version: "2".to_owned(),
        }],
        sdks: vec![SdkCompatibility {
            language: SdkLanguage::Python,
            runtime: VersionRequirement::new(">=3.13,<3.14").expect("runtime"),
            artifact: artifact("veoveo-mcp", ArtifactKind::PythonWheel, 'a'),
        }],
        conformance: artifact(
            "veoveo-mcp-conformance",
            ArtifactKind::ConformanceImage,
            'b',
        ),
        gateway_composer: artifact("veoveo-gateway-composer", ArtifactKind::OciImage, 'd'),
        helm_library: artifact("veoveo-extension", ArtifactKind::HelmChart, 'c'),
        simulation_runtimes: vec![],
    }
}

#[test]
fn compatibility_manifest_round_trips_through_generated_schema() {
    let manifest = manifest();
    manifest.validate().expect("manifest validation");
    let value = serde_json::to_value(&manifest).expect("serialize manifest");
    let schema = serde_json::to_value(compatibility_manifest_schema()).expect("serialize schema");
    let validator = Validator::new(&schema).expect("compile schema");
    assert!(validator.is_valid(&value));
}

#[test]
fn compatibility_manifest_rejects_duplicate_contract_kinds() {
    let mut manifest = manifest();
    manifest.contracts.push(manifest.contracts[0].clone());
    assert!(matches!(
        manifest.validate(),
        Err(ExtensionContractError::Duplicate {
            kind: "contract kind",
            ..
        })
    ));
}

#[test]
fn controlled_identifiers_reject_mutable_or_ambiguous_values() {
    assert!(ArtifactDigest::new("sha256:abc").is_err());
    assert!(ArtifactCoordinate::new("oci://registry.example/image:latest").is_err());
    assert!(ReleaseVersion::new("release-one").is_err());
}

#[test]
fn generated_schema_rejects_unknown_fields() {
    let schema = serde_json::to_value(compatibility_manifest_schema()).expect("serialize schema");
    let validator = Validator::new(&schema).expect("compile schema");
    let mut value = serde_json::to_value(manifest()).expect("serialize manifest");
    value
        .as_object_mut()
        .expect("manifest object")
        .insert("unknown".to_owned(), json!(true));
    assert!(!validator.is_valid(&value));
}

#[test]
fn generated_schema_carries_identifier_constraints() {
    let schema = serde_json::to_value(compatibility_manifest_schema()).expect("serialize schema");
    let validator = Validator::new(&schema).expect("compile schema");
    let mut value = serde_json::to_value(manifest()).expect("serialize manifest");
    value["platformVersion"] = json!("release-one");
    value["conformance"]["digest"] = json!("sha256:abc");
    value["helmLibrary"]["coordinate"] = json!("oci://registry.example/chart:latest");
    assert!(!validator.is_valid(&value));
}

#[test]
fn extension_release_round_trips_through_generated_schema() {
    let manifest = ExtensionReleaseManifest {
        schema_version: ExtensionReleaseSchema::V1,
        extension: ExtensionId::new("example-extension").expect("extension id"),
        version: ReleaseVersion::new("1.2.3").expect("version"),
        source: ExtensionSource {
            name: ArtifactName::new("example-source").expect("source name"),
            revision: SourceRevision::new("d".repeat(40)).expect("source revision"),
        },
        compatibility_release: CompatibilityReleaseId::new("1.0.0").expect("compatibility release"),
        artifacts: vec![artifact("example-image", ArtifactKind::OciImage, 'd')],
        helm_chart: artifact("example-chart", ArtifactKind::HelmChart, 'e'),
        gateway_fragment: artifact("example-fragment", ArtifactKind::GatewayServerFragment, 'f'),
        conformance_results: vec![artifact(
            "example-conformance",
            ArtifactKind::ConformanceResult,
            '1',
        )],
        simulation_overlay: None,
    };
    manifest.validate().expect("release validation");
    let value = serde_json::to_value(&manifest).expect("serialize release");
    let schema = serde_json::to_value(extension_release_schema()).expect("serialize schema");
    let validator = Validator::new(&schema).expect("compile schema");
    assert!(validator.is_valid(&value));
}
