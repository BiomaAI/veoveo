use super::*;
use veoveo_deploy_contract::{DeploymentLock, GatewayActivationSpec, components::*};

#[test]
fn retained_execution_uses_recorded_gateway_requirements_and_rollout_targets() {
    independent_sources(Some(verify_retained_execution));
}

fn verify_retained_execution(
    original: &LoadedProfile,
    base: &DeploymentLock,
    roots: &BTreeMap<String, PathBuf>,
) {
    let mut definition = original.definition.clone();
    definition.gateway_activation = Some(GatewayActivationSpec {
        config_map_name_prefix: "gateway".into(),
        control_plane_key: "gateway.json".into(),
        control_plane: "old-gateway.json".into(),
        public_files: BTreeMap::new(),
        confidential_secret: "old-credentials".into(),
        required_secret_keys: BTreeSet::from(["OLD_KEY".into()]),
    });
    definition
        .components
        .iter_mut()
        .find(|component| component.id.as_str() == "installation")
        .unwrap()
        .installation_inputs
        .insert(InstallationInput::GatewayActivation);
    definition.wait_for_deployments = vec!["platform".into()];
    let control = json!({"identity_providers":[], "authorization_servers":[], "servers":[], "profiles":[],
        "tenants":[], "work_contexts":[], "policies":[], "data_labels":[], "oidc_clients":[],
        "metadata":{"fixture":"old"}});
    fs::write(
        original.repository.join("old-gateway.json"),
        serde_json::to_vec_pretty(&control).unwrap(),
    )
    .unwrap();
    fs::write(
        &original.path,
        serde_json::to_vec_pretty(&definition).unwrap(),
    )
    .unwrap();
    let revision = commit(&original.repository, "old gateway execution configuration");
    let profile = LoadedProfile::load(&original.path, &original.repository).unwrap();
    let mut base = base.clone();
    base.profile_revision = revision.clone();
    base.components = compile_component_lock(&profile, &revision, &base.sources, roots).unwrap();

    let activation = definition.gateway_activation.as_mut().unwrap();
    activation.control_plane = "new-gateway.json".into();
    activation.confidential_secret = "new-credentials".into();
    activation.required_secret_keys = BTreeSet::from(["NEW_KEY".into()]);
    definition.wait_for_deployments = vec!["extension".into()];
    let mut control = control;
    control["metadata"]["fixture"] = json!("new");
    fs::remove_file(original.repository.join("old-gateway.json")).unwrap();
    fs::write(
        original.repository.join("new-gateway.json"),
        serde_json::to_vec_pretty(&control).unwrap(),
    )
    .unwrap();
    fs::write(
        &original.path,
        serde_json::to_vec_pretty(&definition).unwrap(),
    )
    .unwrap();
    commit(
        &original.repository,
        "new configuration keeps retained gateway inputs",
    );
    let profile = LoadedProfile::load(&original.path, &original.repository).unwrap();
    let extension = ComponentId::try_from("extension".to_owned()).unwrap();
    let updated = crate::update_components(
        &profile,
        &base,
        &BTreeSet::from([extension.clone()]),
        &crate::ComponentUpdates {
            refresh_configuration: true,
            ..Default::default()
        },
    )
    .unwrap();
    let all = updated
        .components
        .iter()
        .map(|component| component.declaration.id.clone())
        .collect();
    let snapshots = crate::sources::resolve_locked_sources(&profile, &updated, &all).unwrap();
    let exact_roots = snapshots
        .iter()
        .map(|(owner, source)| (owner.clone(), source.repository.clone()))
        .collect();
    let compiled = compile_locked_components(&profile, &updated, &exact_roots, &all).unwrap();
    let installation = compiled
        .iter()
        .find(|component| component.locked.declaration.id.as_str() == "installation")
        .unwrap();
    let activation = installation.execution.gateway_activation.as_ref().unwrap();
    assert_eq!(activation.confidential_secret, "old-credentials");
    assert_eq!(
        activation.required_secret_keys,
        BTreeSet::from(["OLD_KEY".into()])
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&activation.data["gateway.json"]).unwrap()["metadata"]
            ["fixture"],
        "old"
    );
    for component in &compiled {
        let id = component.locked.declaration.id.as_str();
        let expected = if id == "installation" {
            BTreeSet::new()
        } else {
            BTreeSet::from([id.to_owned()])
        };
        assert_eq!(
            component
                .execution
                .deployments
                .iter()
                .map(|identity| identity.name.clone())
                .collect::<BTreeSet<_>>(),
            expected
        );
        if id != "installation" {
            assert!(component.execution.gateway_activation.is_none());
        }
    }
    // The expanded selection retains its dependency and excludes platform waits.
    let selected = BTreeSet::from([
        extension,
        ComponentId::try_from("installation".to_owned()).unwrap(),
    ]);
    let selected = compile_locked_components(&profile, &updated, &exact_roots, &selected).unwrap();
    assert_eq!(selected.len(), 2);
    assert!(selected.iter().all(|component| {
        component
            .execution
            .deployments
            .iter()
            .all(|identity| identity.name != "platform")
    }));
}
