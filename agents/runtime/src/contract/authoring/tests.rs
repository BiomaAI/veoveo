//! Pure owner acceptance for installation relationships and digest profiles.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use veoveo_gateway_contract::SecretPurpose;

fn template() -> RuntimeTemplate {
    serde_json::from_str(r#"{"id":"pilot","name":"Reviewed pilot","tenant":"tenant-a","workContexts":["operations"],"requiredDeployerScopes":["operator:use"],"profile":"operator","scopes":["operator:use"],"roles":["managed-pilot"],"membership":"contributor","models":["approved"],"tools":["media__describe_model"],"resourceSubscriptions":[],"parameters":{"vehicle":{"label":"Vehicle","shape":{"kind":"identifier","maxLength":40},"environmentVariable":"VEOVEO_PARAM_VEHICLE"}},"workload":{"namespace":"agents","configMap":"pilot-template","configDigest":"sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","image":"registry.test/kernel@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","databaseSecret":"agent-store","storageClass":"local-path","storageGib":2,"cpuMillis":500,"memoryMib":1024,"modelSecrets":[{"reference":"media_provider_api_key","secret":"agent-model","key":"api-key"}]}}"#).unwrap()
}
fn facts() -> InstallationFacts {
    InstallationFacts::new(
        [("operations".parse().unwrap(), "tenant-a".parse().unwrap())],
        [(
            "operator".parse().unwrap(),
            BTreeSet::from(["operator:use".parse().unwrap()]),
        )],
        [(
            "media_provider_api_key".parse().unwrap(),
            SecretPurpose::ProviderApiKey,
        )],
    )
    .unwrap()
}

#[test]
fn template_digest_binds_fixed_serialization_and_private_workload() {
    let mut template = template();
    assert_eq!(
        runtime_template_revision(&template).hex(),
        "c01242b34b31e2ae054b6d10c95e1fca3d7dd0f69762689c54651bae39d0f032"
    );
    template.workload.config_digest = veoveo_types::Sha256Digest::from_hex("c".repeat(64)).unwrap();
    assert_ne!(
        runtime_template_revision(&template).hex(),
        "c01242b34b31e2ae054b6d10c95e1fca3d7dd0f69762689c54651bae39d0f032"
    );
}

#[test]
fn template_validation_requires_installed_relationships_and_safe_parameters() {
    let original = template();
    original.validate(&facts()).unwrap();
    let empty = InstallationFacts::new([], [], []).unwrap();
    assert!(original.validate(&empty).is_err());
    let missing_profile = InstallationFacts::new(
        [("operations".parse().unwrap(), "tenant-a".parse().unwrap())],
        [],
        [(
            "media_provider_api_key".parse().unwrap(),
            SecretPurpose::ProviderApiKey,
        )],
    )
    .unwrap();
    assert!(original.validate(&missing_profile).is_err());
    let other_tenant = InstallationFacts::new(
        [("operations".parse().unwrap(), "tenant-b".parse().unwrap())],
        [(
            "operator".parse().unwrap(),
            BTreeSet::from(["operator:use".parse().unwrap()]),
        )],
        [(
            "media_provider_api_key".parse().unwrap(),
            SecretPurpose::ProviderApiKey,
        )],
    )
    .unwrap();
    assert!(original.validate(&other_tenant).is_err());
    let wrong_secret = InstallationFacts::new(
        [("operations".parse().unwrap(), "tenant-a".parse().unwrap())],
        [(
            "operator".parse().unwrap(),
            BTreeSet::from(["operator:use".parse().unwrap()]),
        )],
        [(
            "media_provider_api_key".parse().unwrap(),
            SecretPurpose::WebhookSecret,
        )],
    )
    .unwrap();
    assert!(original.validate(&wrong_secret).is_err());
    let mut unsafe_template = original.clone();
    unsafe_template
        .parameters
        .get_mut("vehicle")
        .unwrap()
        .environment_variable = "VEOVEO_AGENT_PRIVATE_KEY".into();
    assert!(unsafe_template.validate(&facts()).is_err());
    for value in ["${PRIVATE_KEY}", "../memory", "a;cmd", ""] {
        assert!(!original.accepts_parameters(&BTreeMap::from([(
            "vehicle".into(),
            TemplateParameter::Text(value.into())
        )])));
    }
    assert!(original.accepts_parameters(&BTreeMap::from([(
        "vehicle".into(),
        TemplateParameter::Text("uav-5".into())
    )])));
}

#[test]
fn caller_facts_keep_tenant_scope_and_context_requirements() {
    let template = template();
    let tenant = "tenant-a".parse().unwrap();
    let scopes = BTreeSet::from(["operator:use".parse().unwrap()]);
    let caller = CallerFacts {
        tenant: Some(&tenant),
        scopes: &scopes,
    };
    assert!(template.permits(caller, &"operations".parse().unwrap()));
    assert!(!template.permits(caller, &"elsewhere".parse().unwrap()));
    assert!(!template.permits(
        CallerFacts {
            tenant: None,
            scopes: &scopes
        },
        &"operations".parse().unwrap()
    ));
    assert!(!template.permits(
        CallerFacts {
            tenant: Some(&tenant),
            scopes: &BTreeSet::new()
        },
        &"operations".parse().unwrap()
    ));
}

#[test]
fn template_wire_refuses_retired_nested_names_without_closing_parameter_dictionaries() {
    let typed = template();
    typed.validate(&facts()).unwrap();
    let current = serde_json::to_value(typed).unwrap();
    for (parent, key, old) in [
        ("", "workContexts", "work_contexts"),
        ("", "requiredDeployerScopes", "required_deployer_scopes"),
        ("", "resourceSubscriptions", "resource_subscriptions"),
        (
            "/parameters/vehicle",
            "environmentVariable",
            "environment_variable",
        ),
        ("/parameters/vehicle/shape", "maxLength", "max_length"),
        ("/workload", "configMap", "config_map"),
        ("/workload", "configDigest", "config_digest"),
        ("/workload", "databaseSecret", "database_secret"),
        ("/workload", "storageClass", "storage_class"),
        ("/workload", "storageGib", "storage_gib"),
        ("/workload", "cpuMillis", "cpu_millis"),
        ("/workload", "memoryMib", "memory_mib"),
        ("/workload", "modelSecrets", "model_secrets"),
    ] {
        for keep in [false, true] {
            let mut bad = current.clone();
            let object = bad.pointer_mut(parent).unwrap().as_object_mut().unwrap();
            object.insert(old.into(), object[key].clone());
            if !keep {
                object.remove(key);
            }
            assert!(
                serde_json::from_value::<RuntimeTemplate>(bad.clone()).is_err(),
                "{parent}/{old} mixed={keep}"
            );
            assert!(
                serde_json::from_slice::<RuntimeTemplate>(&serde_json::to_vec(&bad).unwrap())
                    .is_err()
            );
        }
    }
}

#[test]
fn template_scopes_cover_installed_profile_without_mutating_grants() {
    let required: BTreeSet<veoveo_types::ScopeName> = [
        "operator:use",
        "manager:read",
        "manager:write",
        "time:read",
        "time:schedule",
    ]
    .into_iter()
    .map(|scope| scope.parse().unwrap())
    .collect();
    let installed = InstallationFacts::new(
        [("operations".parse().unwrap(), "tenant-a".parse().unwrap())],
        [("operator".parse().unwrap(), required.clone())],
        [(
            "media_provider_api_key".parse().unwrap(),
            SecretPurpose::ProviderApiKey,
        )],
    )
    .unwrap();
    let mut candidate = template();
    let original = serde_json::to_value(&candidate).unwrap();
    assert!(candidate.validate(&installed).is_err());
    assert_eq!(serde_json::to_value(&candidate).unwrap(), original);
    assert_eq!(
        installed.profile_required_scopes(&candidate.profile),
        Some(&required)
    );

    candidate.scopes = required.clone();
    candidate.validate(&installed).unwrap();
    candidate.scopes.insert("artifact:read".parse().unwrap());
    candidate.validate(&installed).unwrap();

    candidate.profile = "unknown".parse().unwrap();
    let unknown = serde_json::to_value(&candidate).unwrap();
    assert!(candidate.validate(&installed).is_err());
    assert_eq!(serde_json::to_value(&candidate).unwrap(), unknown);
}
