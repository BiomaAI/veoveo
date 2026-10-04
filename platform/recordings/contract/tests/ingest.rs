//! Pure ingest catalog, wire admission and owner declaration qualification.
use std::collections::BTreeMap;
use veoveo_gateway_contract::{
    CatalogFacts, CatalogRegistryBuilder, CatalogSection, GatewayAction, HttpUpstreamEndpoint,
    OAuthClientFacts,
};
use veoveo_recording_contract::*;
use veoveo_types::{ExtensionName, Identity, ResourceAddress};
fn resources() -> Vec<RecordingIngestResource> {
    let document: serde_json::Value =
        serde_json::from_str(include_str!("../../../../configs/gateway.smoke.json")).unwrap();
    serde_json::from_value(document["recording_ingest_resources"].clone()).unwrap()
}
fn facts(resources: &[RecordingIngestResource]) -> CatalogFacts {
    CatalogFacts {
        authorization_servers: resources
            .iter()
            .map(|resource| resource.authorization_server.clone())
            .collect(),
        policies: resources
            .iter()
            .map(|resource| resource.policy_version.clone())
            .collect(),
        tenants: resources
            .iter()
            .flat_map(|resource| {
                resource
                    .producers
                    .iter()
                    .map(|producer| producer.tenant.clone())
            })
            .collect(),
        data_labels: resources
            .iter()
            .flat_map(|resource| {
                resource
                    .producers
                    .iter()
                    .flat_map(|producer| producer.labels.iter().cloned())
            })
            .collect(),
        oauth_clients: resources
            .iter()
            .flat_map(|resource| {
                resource.producers.iter().map(|producer| OAuthClientFacts {
                    id: producer.oauth_client.clone(),
                    authorization_server: resource.authorization_server.clone(),
                    tenant: Some(producer.tenant.clone()),
                    allowed_resources: [resource.protected_resource.clone()].into(),
                    allowed_scopes: resource.required_scopes.clone(),
                    client_credentials: true,
                    private_key_jwt: true,
                })
            })
            .collect(),
    }
}
#[test]
fn plain_http_upstream_refuses_obsolete_mcp_transport() {
    let resources = resources();
    let mut value = serde_json::to_value(&resources[0].upstream).unwrap();
    assert!(value.get("transport").is_none());
    assert_eq!(
        serde_json::from_value::<HttpUpstreamEndpoint>(value.clone()).unwrap(),
        resources[0].upstream
    );
    value["transport"] = "streamable_http".into();
    assert!(serde_json::from_value::<HttpUpstreamEndpoint>(value).is_err());
    let schema = schemars::schema_for!(HttpUpstreamEndpoint);
    assert_eq!(schema.as_value()["additionalProperties"], false);
    assert!(schema.as_value()["properties"].get("transport").is_none());
}
#[test]
fn catalog_checks_limits_membership_and_registration_facts() {
    let original = resources();
    let current = facts(&original);
    RecordingCatalogSection(original.clone())
        .validate(&current)
        .unwrap();
    let catalog = RecordingCatalog::new(original.clone()).unwrap();
    let resource = catalog.single_resource().unwrap();
    let producer = &resource.producers[0];
    assert_eq!(catalog.producer(&producer.id).unwrap().0, resource);
    assert_eq!(catalog.resource(&resource.id), Some(resource));
    let mut unknown = current.clone();
    unknown.oauth_clients.clear();
    assert!(
        RecordingCatalogSection(original.clone())
            .validate(&unknown)
            .is_err()
    );
    for mutation in 0..7 {
        let mut changed = original.clone();
        match mutation {
            0 => changed[0].maximum_batch_bytes = 0,
            1 => changed[0].producers[0].quotas.maximum_concurrent_streams = 0,
            2 => changed[0].producers[0].retention.open_stream_days = 0,
            3 => changed[0].required_scopes.clear(),
            4 => changed[0].producers[0].tenant = "unknown".parse().unwrap(),
            5 => {
                changed[0].producers[0]
                    .labels
                    .insert("unknown".parse().unwrap());
            }
            _ => changed.push(changed[0].clone()),
        };
        assert!(
            RecordingCatalogSection(changed).validate(&current).is_err(),
            "mutation {mutation}"
        );
    }
}
#[test]
fn declared_sections_targets_and_audit_share_checked_owner_types() {
    let resources = resources();
    let current = facts(&resources);
    let section = RecordingCatalogSection(resources.clone());
    let values = BTreeMap::from([(
        RECORDING_INGEST_SECTION.to_owned(),
        serde_json::to_value(&section).unwrap(),
    )]);
    let empty = CatalogRegistryBuilder::new(Vec::<String>::new(), Vec::<ExtensionName>::new())
        .build()
        .unwrap();
    assert!(empty.admit_sections(&values, &current).is_err());
    let mut builder = CatalogRegistryBuilder::new(
        Vec::<String>::new(),
        ["gateway", "server", "resource"]
            .into_iter()
            .map(|kind| ExtensionName::parse(kind).unwrap()),
    );
    builder.register_kernel::<GatewayAction>().unwrap();
    register_catalog(&mut builder).unwrap();
    assert!(register_catalog(&mut builder).is_err());
    let registry = builder.build().unwrap();
    let admitted = registry.admit_sections(&values, &current).unwrap();
    let section_key = registry
        .section_key::<RecordingCatalogSection>(
            &ExtensionName::parse(RECORDING_INGEST_SECTION).unwrap(),
        )
        .unwrap();
    assert_eq!(admitted.get(&section_key).unwrap().unwrap(), section);
    let target = RecordingTarget::RecordingStream {
        producer: resources[0].producers[0].id.clone(),
        stream_id: "01983da0-0000-7000-8000-000000000001".parse().unwrap(),
    };
    let key = registry
        .target_key::<RecordingTarget>(&ExtensionName::parse(RECORDING_TARGET_GROUP).unwrap())
        .unwrap();
    registry.contribute_target(&key, &target).unwrap();
    let audit = target_audit_resource(&target).unwrap();
    assert_eq!(audit.server.as_str(), "recording-hub");
    let parsed = RecordingIngestUri::parse(&audit.uri).unwrap();
    assert_eq!(parsed.to_uri().unwrap(), audit.uri);
}

#[test]
fn ingest_ids_keep_v7_alias_spelling_and_unconstrained_string_schema() {
    let uuid = uuid::Uuid::now_v7();
    let alias = format!("urn:uuid:{}", uuid.hyphenated().to_string().to_uppercase());
    let id = RecordingIngestStreamId::parse(alias.clone()).unwrap();
    assert_eq!(id.as_str(), alias);
    assert_eq!(id.identity_text(), alias);
    assert_eq!(serde_json::to_value(&id).unwrap(), alias);
    assert_eq!(alias.parse::<RecordingIngestStreamId>().unwrap(), id);
    let v4 = "550e8400-e29b-41d4-a716-446655440000";
    assert!(RecordingIngestStreamId::parse(v4).is_err());
    assert!(v4.parse::<RecordingIngestStreamId>().is_err());
    assert!(serde_json::from_value::<RecordingIngestStreamId>(serde_json::json!(v4)).is_err());
    let schema = serde_json::to_value(schemars::schema_for!(RecordingIngestStreamId)).unwrap();
    assert_eq!(schema["type"], "string");
    assert!(schema.get("format").is_none());
    assert!(schema.get("pattern").is_none());
}
