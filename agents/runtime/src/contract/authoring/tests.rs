//! Pure owner acceptance for installation relationships and digest profiles.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use veoveo_gateway_contract::SecretPurpose;

fn template() -> RuntimeTemplate {
    serde_json::from_str(r#"{"id":"pilot","name":"Reviewed pilot","tenant":"tenant-a","work_contexts":["operations"],"required_deployer_scopes":["operator:use"],"profile":"operator","scopes":["operator:use"],"roles":["managed-pilot"],"membership":"contributor","models":["approved"],"tools":["media__describe_model"],"resource_subscriptions":[],"parameters":{"vehicle":{"label":"Vehicle","shape":{"kind":"identifier","maxLength":40},"environment_variable":"VEOVEO_PARAM_VEHICLE"}},"workload":{"namespace":"agents","config_map":"pilot-template","config_digest":"sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","image":"registry.test/kernel@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","database_secret":"agent-store","storage_class":"local-path","storage_gib":2,"cpu_millis":500,"memory_mib":1024,"model_secrets":[{"reference":"media_provider_api_key","secret":"agent-model","key":"api-key"}]}}"#).unwrap()
}
fn facts() -> InstallationFacts {
    InstallationFacts::new(
        [("operations".parse().unwrap(), "tenant-a".parse().unwrap())],
        ["operator".parse().unwrap()],
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
        "ff907fcbee9917f9ceae3bda54ddbbb890a8e38205ff8ec6f7394319878df80d"
    );
    template.workload.config_digest = veoveo_types::Sha256Digest::from_hex("c".repeat(64)).unwrap();
    assert_ne!(
        runtime_template_revision(&template).hex(),
        "ff907fcbee9917f9ceae3bda54ddbbb890a8e38205ff8ec6f7394319878df80d"
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
        ["operator".parse().unwrap()],
        [(
            "media_provider_api_key".parse().unwrap(),
            SecretPurpose::ProviderApiKey,
        )],
    )
    .unwrap();
    assert!(original.validate(&other_tenant).is_err());
    let wrong_secret = InstallationFacts::new(
        [("operations".parse().unwrap(), "tenant-a".parse().unwrap())],
        ["operator".parse().unwrap()],
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
