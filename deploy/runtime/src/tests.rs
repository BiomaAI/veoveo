use std::{
    cell::Cell,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use veoveo_deploy_contract::{
    DeploymentSource, DeploymentSourceRole, LoadedProfile, LockedImage, LockedSource, ReleaseSpec,
    ReleaseValuesContract, SecretClosure, SecretClosureStatus, SecretObjectKey,
    SecretObservationStatus, SourceRepository,
};

use crate::{
    charts::{
        lock_source_charts, ordered_release_values, release_image_digests, validate_locked_charts,
    },
    configuration::{
        after_secret_closure, decode_secret_observation, gateway_mount_key,
        prepare_gateway_activation, validate_gateway_public_file,
    },
    images::locked_image_digests_for_registry,
    sources::normalize_origin,
};

const DIGEST_A: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const DIGEST_B: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[test]
fn publisher_and_installer_share_chart_inventory_and_content_validation() {
    let repository = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(repository.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "--quiet"]);
    git(&["config", "user.name", "Deployment Test"]);
    git(&["config", "user.email", "deployment@example.invalid"]);
    let chart = repository.path().join("chart");
    std::fs::create_dir(&chart).unwrap();
    std::fs::write(
        chart.join("Chart.yaml"),
        "apiVersion: v2\nname: fixture\nversion: 1.0.0\n",
    )
    .unwrap();
    std::fs::write(chart.join("values.yaml"), "replicas: 1\n").unwrap();
    git(&["add", "chart"]);
    git(&[
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "-m",
        "initial chart",
    ]);
    let mut source = DeploymentSource {
        name: "platform".into(),
        role: DeploymentSourceRole::Platform,
        repository: SourceRepository::Local {
            path: repository.path().into(),
        },
        revision: git(&["rev-parse", "HEAD"]),
        image_groups: Vec::new(),
        releases: vec![ReleaseSpec {
            name: "platform".into(),
            chart: PathBuf::from("chart"),
            source_values: vec![PathBuf::from("chart/values.yaml")],
            installation_values: Vec::new(),
            values_contract: ReleaseValuesContract::Platform,
            timeout_seconds: 60,
        }],
    };
    let locked = LockedSource {
        name: source.name.clone(),
        role: source.role,
        repository: "https://example.invalid/platform".into(),
        revision: source.revision.clone(),
        images: Vec::new(),
        charts: lock_source_charts(&source, repository.path()).unwrap(),
    };
    validate_locked_charts(&source, &locked, repository.path()).unwrap();
    std::fs::write(repository.path().join("README.md"), "unrelated change\n").unwrap();
    assert_eq!(
        locked.charts,
        lock_source_charts(&source, repository.path()).unwrap()
    );
    std::fs::write(chart.join("values.yaml"), "replicas: 2\n").unwrap();
    assert!(
        lock_source_charts(&source, repository.path())
            .unwrap_err()
            .to_string()
            .contains("differs from its immutable revision")
    );
    git(&["add", "chart"]);
    git(&[
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "-m",
        "changed chart",
    ]);
    let error = validate_locked_charts(&source, &locked, repository.path()).unwrap_err();
    assert!(error.to_string().contains("chart digest"));
    assert_ne!(
        locked.charts,
        lock_source_charts(&source, repository.path()).unwrap()
    );
    source.releases.push(source.releases[0].clone());
    assert!(
        lock_source_charts(&source, repository.path())
            .unwrap_err()
            .to_string()
            .contains("duplicate Helm release")
    );
}

#[test]
fn deployment_image_closure_spans_sources() {
    let sources = [
        LockedSource {
            name: "platform".to_owned(),
            role: DeploymentSourceRole::Platform,
            repository: "https://example.invalid/platform".to_owned(),
            revision: "a".repeat(40),
            images: vec![LockedImage {
                name: "agent-kernel".to_owned(),
                repository: "registry.example/veoveo/agent-kernel".to_owned(),
                source_revision: veoveo_extension_contract::SourceRevision::new("a".repeat(40))
                    .unwrap(),
                digest: DIGEST_A.to_owned(),
                publication_digest: DIGEST_B.to_owned(),
            }],
            charts: vec![],
        },
        LockedSource {
            name: "extension".to_owned(),
            role: DeploymentSourceRole::Extension,
            repository: "https://example.invalid/extension".to_owned(),
            revision: "b".repeat(40),
            images: vec![LockedImage {
                name: "runtime".to_owned(),
                repository: "registry.example/extension/runtime".to_owned(),
                source_revision: veoveo_extension_contract::SourceRevision::new("b".repeat(40))
                    .unwrap(),
                digest: DIGEST_B.to_owned(),
                publication_digest: DIGEST_A.to_owned(),
            }],
            charts: vec![],
        },
    ];

    assert_eq!(
        locked_image_digests_for_registry("registry.example", &sources).unwrap(),
        [
            ("extension/runtime".to_owned(), DIGEST_B.to_owned()),
            ("veoveo/agent-kernel".to_owned(), DIGEST_A.to_owned()),
        ]
        .into_iter()
        .collect()
    );
}

#[test]
fn external_values_receive_platform_images_without_polluting_platform_values() {
    let source = [("veoveo/gateway".to_owned(), DIGEST_A.to_owned())]
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let deployment = [
        ("extension/runtime".to_owned(), DIGEST_B.to_owned()),
        ("veoveo/gateway".to_owned(), DIGEST_A.to_owned()),
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>();

    assert_eq!(
        release_image_digests(ReleaseValuesContract::Platform, &source, &deployment),
        &source
    );
    assert_eq!(
        release_image_digests(ReleaseValuesContract::VeoveoSource, &source, &deployment),
        &source
    );
    assert_eq!(
        release_image_digests(ReleaseValuesContract::Extension, &source, &deployment),
        &deployment
    );
}

#[test]
fn normalizes_scp_git_origins_for_lock_comparison() {
    assert_eq!(
        normalize_origin("git@github.com:BiomaAI/veoveo.git").expect("normalize origin"),
        "ssh://git@github.com/BiomaAI/veoveo"
    );
}

#[test]
fn installation_values_override_source_values_in_helm_order() {
    let release = ReleaseSpec {
        name: "example".to_owned(),
        chart: PathBuf::from("chart"),
        source_values: vec![PathBuf::from("defaults.yaml")],
        installation_values: vec![
            PathBuf::from("environment.yaml"),
            PathBuf::from("site.yaml"),
        ],
        values_contract: ReleaseValuesContract::Platform,
        timeout_seconds: 60,
    };

    assert_eq!(
        ordered_release_values(Path::new("/source"), Path::new("/installation"), &release),
        [
            PathBuf::from("/source/defaults.yaml"),
            PathBuf::from("/installation/environment.yaml"),
            PathBuf::from("/installation/site.yaml"),
        ]
    );
}

#[test]
fn checked_in_profile_preflights_revisioned_gateway_activation() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let profile_path = repository.join("showcase/sumo/deploy/deployment.json");
    let profile = LoadedProfile::load(&profile_path, &repository).unwrap();
    let activation = prepare_gateway_activation(&profile).unwrap().unwrap();

    assert!(activation.config_map_name.starts_with("veoveo-gateway-"));
    assert_eq!(activation.revision.len(), 64);
    assert!(
        activation
            .revision
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    );
    assert_eq!(
        activation.data.keys().cloned().collect::<Vec<_>>(),
        ["gateway.json", "jwks.json"]
    );
    assert!(
        activation
            .required_secret_keys
            .contains("oidc-client-secret")
    );
}

#[test]
fn gateway_public_file_validation_rejects_invalid_trust_material() {
    let invalid_jwks = validate_gateway_public_file("jwks.json", "not-json", true, false)
        .expect_err("invalid JWKS must fail");
    assert!(invalid_jwks.to_string().contains("decoding gateway JWKS"));

    let empty_jwks = validate_gateway_public_file("jwks.json", r#"{"keys":[]}"#, true, false)
        .expect_err("empty JWKS must fail");
    assert!(empty_jwks.to_string().contains("has no keys"));

    let invalid_ca = validate_gateway_public_file("ca.pem", "not-pem", false, true)
        .expect_err("invalid CA must fail");
    assert!(invalid_ca.to_string().contains("gateway CA bundle"));
}

#[test]
fn gateway_public_files_must_be_direct_mount_keys() {
    assert_eq!(
        gateway_mount_key("/etc/veoveo/gateway/ca.pem").unwrap(),
        "ca.pem"
    );
    assert!(gateway_mount_key("/tmp/ca.pem").is_err());
    assert!(gateway_mount_key("/etc/veoveo/gateway/trust/ca.pem").is_err());
}

#[test]
fn failed_secret_closure_never_enters_the_mutation_phase() {
    let called = Cell::new(false);
    let closure = SecretClosure {
        profile_digest: DIGEST_A.to_owned(),
        lock_digest: DIGEST_B.to_owned(),
        requirements: Vec::new(),
        presence: Vec::new(),
        status: SecretClosureStatus::MissingSecret,
    };

    let error = after_secret_closure(closure, |_| {
        called.set(true);
        Ok(())
    })
    .expect_err("failed closure must stop before mutation");
    assert!(!called.get());
    assert!(error.to_string().contains("missing_secret"));
}

#[test]
fn secret_read_decoding_retains_only_presence_and_key_names() {
    let secret = SecretObjectKey {
        namespace: "mission".to_owned(),
        name: "runtime".to_owned(),
    };
    let observed = decode_secret_observation(
        secret.clone(),
        true,
        br#"{"apiVersion":"v1","kind":"Secret","metadata":{"name":"runtime","namespace":"mission"},"data":{"token":"secret-value-canary"}}"#,
        &[],
    );
    assert!(matches!(
        observed.status,
        SecretObservationStatus::Present { ref keys } if keys.contains("token")
    ));
    assert!(
        !serde_json::to_string(&observed)
            .unwrap()
            .contains("secret-value-canary")
    );

    let forbidden = decode_secret_observation(
        secret.clone(),
        false,
        &[],
        b"Error from server (Forbidden): secret-value-canary",
    );
    assert_eq!(forbidden.status, SecretObservationStatus::Forbidden);
    assert!(
        !serde_json::to_string(&forbidden)
            .unwrap()
            .contains("secret-value-canary")
    );

    let missing = decode_secret_observation(secret, true, b"\n", &[]);
    assert_eq!(missing.status, SecretObservationStatus::Missing);

    let malformed = decode_secret_observation(
        SecretObjectKey {
            namespace: "mission".to_owned(),
            name: "runtime".to_owned(),
        },
        true,
        b"not-json",
        &[],
    );
    assert_eq!(malformed.status, SecretObservationStatus::Malformed);

    let timeout = decode_secret_observation(
        SecretObjectKey {
            namespace: "mission".to_owned(),
            name: "runtime".to_owned(),
        },
        false,
        &[],
        b"request deadline exceeded: secret-value-canary",
    );
    assert_eq!(timeout.status, SecretObservationStatus::Timeout);

    let transport = decode_secret_observation(
        SecretObjectKey {
            namespace: "mission".to_owned(),
            name: "runtime".to_owned(),
        },
        false,
        &[],
        b"connection refused: secret-value-canary",
    );
    assert_eq!(transport.status, SecretObservationStatus::Transport);
}
