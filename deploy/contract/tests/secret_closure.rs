use std::collections::BTreeSet;

use serde_json::json;
use veoveo_deploy_contract::{
    CustomSecretReferenceRegistry, CustomSecretReferenceSpec, SecretClosure, SecretClosureError,
    SecretClosureStatus, SecretObservation, SecretObservationStatus, SecretReferenceKind,
    collect_secret_requirements,
};

#[test]
fn rendered_workloads_produce_complete_sorted_secret_requirements() {
    let objects = vec![json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {"name": "worker", "namespace": "mission"},
        "spec": {"template": {"spec": {
            "imagePullSecrets": [{"name": "registry-pull"}],
            "initContainers": [{
                "name": "init",
                "envFrom": [{"secretRef": {"name": "bootstrap", "optional": true}}]
            }],
            "containers": [{
                "name": "worker",
                "env": [{"name": "TOKEN", "valueFrom": {"secretKeyRef": {
                    "name": "runtime", "key": "token"
                }}}]
            }],
            "volumes": [
                {"name": "identity", "secret": {
                    "secretName": "identity",
                    "optional": false,
                    "items": [{"key": "identity.pem", "path": "identity.pem"}]
                }},
                {"name": "projected", "projected": {"sources": [{"secret": {
                    "name": "bundle", "items": [{"key": "ca.pem", "path": "ca.pem"}]
                }}]}}
            ]
        }}}
    })];

    let requirements = collect_secret_requirements(
        &objects,
        "fallback",
        &CustomSecretReferenceRegistry::default(),
    )
    .expect("rendered requirements");

    assert_eq!(
        requirements
            .iter()
            .map(|requirement| (
                requirement.secret.name.as_str(),
                requirement.key.as_deref(),
                requirement.optional,
                requirement.kind,
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "bootstrap",
                None,
                true,
                SecretReferenceKind::EnvironmentFrom
            ),
            (
                "bundle",
                Some("ca.pem"),
                false,
                SecretReferenceKind::ProjectedVolume
            ),
            (
                "identity",
                Some("identity.pem"),
                false,
                SecretReferenceKind::SecretVolume
            ),
            ("registry-pull", None, false, SecretReferenceKind::ImagePull),
            (
                "runtime",
                Some("token"),
                false,
                SecretReferenceKind::EnvironmentKey
            ),
        ]
    );
    assert!(
        requirements
            .iter()
            .all(|item| item.secret.namespace == "mission")
    );
}

#[test]
fn ingress_gateway_and_registered_custom_references_are_typed() {
    let mut custom = CustomSecretReferenceRegistry::default();
    custom.register(CustomSecretReferenceSpec {
        group: "example.test".to_owned(),
        version: "v1".to_owned(),
        kind: "Pipeline".to_owned(),
        reference_path: "/spec/credentials/*".to_owned(),
        name_field: "secretName".to_owned(),
        key_field: Some("secretKey".to_owned()),
        namespace_field: None,
        optional_field: Some("optional".to_owned()),
    });
    let objects = vec![
        json!({
            "apiVersion": "networking.k8s.io/v1",
            "kind": "Ingress",
            "metadata": {"name": "edge", "namespace": "mission"},
            "spec": {"tls": [{"secretName": "ingress-tls"}]}
        }),
        json!({
            "apiVersion": "gateway.networking.k8s.io/v1",
            "kind": "Gateway",
            "metadata": {"name": "edge", "namespace": "mission"},
            "spec": {"listeners": [{"tls": {"certificateRefs": [{
                "group": "", "kind": "Secret", "name": "gateway-tls"
            }]}}]}
        }),
        json!({
            "apiVersion": "example.test/v1",
            "kind": "Pipeline",
            "metadata": {"name": "worker", "namespace": "mission"},
            "spec": {"credentials": [{
                "secretName": "provider", "secretKey": "token", "optional": false
            }]}
        }),
    ];

    let requirements = collect_secret_requirements(&objects, "fallback", &custom).unwrap();
    assert_eq!(
        requirements
            .iter()
            .map(|item| (item.secret.name.as_str(), item.key.as_deref(), item.kind,))
            .collect::<Vec<_>>(),
        vec![
            ("gateway-tls", None, SecretReferenceKind::GatewayTls),
            ("ingress-tls", None, SecretReferenceKind::IngressTls),
            (
                "provider",
                Some("token"),
                SecretReferenceKind::CustomResource
            ),
        ]
    );
}

#[test]
fn closure_distinguishes_missing_keys_and_protected_read_failures_without_values() {
    let requirements = collect_secret_requirements(
        &[json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": {"name": "worker", "namespace": "mission"},
            "spec": {"containers": [{"name": "worker", "env": [
                {"name": "A", "valueFrom": {"secretKeyRef": {"name": "runtime", "key": "present"}}},
                {"name": "B", "valueFrom": {"secretKeyRef": {"name": "runtime", "key": "missing"}}}
            ]}]}
        })],
        "fallback",
        &CustomSecretReferenceRegistry::default(),
    )
    .unwrap();
    let closure = SecretClosure::evaluate(
        "sha256:profile",
        "sha256:lock",
        requirements.clone(),
        vec![SecretObservation {
            secret: requirements[0].secret.clone(),
            status: SecretObservationStatus::Present {
                keys: BTreeSet::from(["present".to_owned()]),
            },
        }],
    )
    .unwrap();
    assert_eq!(closure.status, SecretClosureStatus::MissingKey);
    let serialized = serde_json::to_string(&closure).unwrap();
    assert!(!serialized.contains("secret-value-canary"));

    let forbidden = SecretClosure::evaluate(
        "sha256:profile",
        "sha256:lock",
        requirements,
        vec![SecretObservation {
            secret: closure.requirements[0].secret.clone(),
            status: SecretObservationStatus::Forbidden,
        }],
    )
    .unwrap();
    assert_eq!(forbidden.status, SecretClosureStatus::Forbidden);
}

#[test]
fn unknown_secret_bearing_custom_resources_and_rendered_secrets_fail_closed() {
    let custom = collect_secret_requirements(
        &[json!({
            "apiVersion": "example.test/v1",
            "kind": "Pipeline",
            "metadata": {"name": "unsafe", "namespace": "mission"},
            "spec": {"credentialSecretName": "private-provider"}
        })],
        "fallback",
        &CustomSecretReferenceRegistry::default(),
    )
    .unwrap_err();
    assert!(matches!(
        custom,
        SecretClosureError::UnverifiableObject { .. }
    ));

    let secret = collect_secret_requirements(
        &[json!({
            "apiVersion": "v1",
            "kind": "Secret",
            "metadata": {"name": "must-be-installation-owned", "namespace": "mission"},
            "stringData": {"token": "secret-value-canary"}
        })],
        "fallback",
        &CustomSecretReferenceRegistry::default(),
    )
    .unwrap_err();
    assert!(matches!(
        secret,
        SecretClosureError::RenderedSecretForbidden { .. }
    ));
    assert!(!secret.to_string().contains("secret-value-canary"));
}
