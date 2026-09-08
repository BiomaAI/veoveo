use veoveo_deploy_contract::{DeploymentLock, DeploymentProfile, components::*};
use veoveo_extension_contract::{ArtifactDigest, SourceRevision};

fn fixture() -> DeploymentLock {
    serde_json::from_str(include_str!("fixtures/deployment-lock.json")).unwrap()
}

fn relock(lock: &mut DeploymentLock, index: usize) {
    let component = &lock.components[index];
    let prepared = component
        .units
        .iter()
        .map(|unit| PreparedAtomicUnit {
            component: component.declaration.id.clone(),
            source: component.declaration.source.clone(),
            target: unit.target.clone(),
            inputs: unit.inputs.clone(),
            objects: unit.objects.clone(),
            tool_scope: AtomicToolScope::Exact,
        })
        .collect();
    let mut declaration = component.declaration.clone();
    declaration.inputs = component
        .units
        .iter()
        .flat_map(|unit| unit.inputs.iter().cloned())
        .collect();
    lock.components[index] = lock_component(declaration, prepared).unwrap();
}

fn profile(lock: &DeploymentLock) -> DeploymentProfile {
    let components = lock.components.iter().map(|component| {
        let declaration = &component.declaration;
        let installation = declaration.role == ComponentRole::Installation;
        serde_json::json!({
            "id": declaration.id, "role":declaration.role,
            "owner":if installation { serde_json::json!({"kind":"installation"}) }
                else { serde_json::json!({"kind":"source", "name":declaration.source.name}) },
            "dependencies":declaration.dependencies, "namespaces":declaration.namespaces,
            "clusterObjects":declaration.permitted_objects.iter().filter(|object| object.namespace.is_none()).collect::<Vec<_>>(),
            "releases":declaration.targets.iter().filter_map(|target| match target {
                AtomicTarget::HelmRelease { name, .. } => Some(name), _ => None,
            }).collect::<Vec<_>>(),
            "installationInputs":if installation { vec![InstallationInput::Namespace] } else { vec![] },
            "extensionRelease":declaration.extension_release
        })
    }).collect::<Vec<_>>();
    serde_json::from_value(serde_json::json!({
        "schemaVersion":veoveo_deploy_contract::PROFILE_SCHEMA, "name":lock.profile,
        "registry":{"pushAddress":"registry.example.invalid", "pullAddress":"registry.example.invalid", "transport":"tls"},
        "sources":lock.sources.iter().map(|source| serde_json::json!({
            "name":source.name, "role":source.role, "repository":{"kind":"git", "url":source.repository},
            "revision":source.revision, "imageGroups":[],
            "releases":source.charts.iter().map(|chart| serde_json::json!({
                "name":chart.release, "chart":"chart", "sourceValues":[], "installationValues":[],
                "valuesContract":if source.name == "platform" { "platform" } else { "extension" }, "timeoutSeconds":60,
            })).collect::<Vec<_>>()
        })).collect::<Vec<_>>(),
        "components":components, "kubernetes":{"context":"must-not-contact-a-cluster", "localCluster":null},
        "namespace":"veoveo", "resources":{"manifests":[], "configMaps":[]},
        "platform":{"installationPreset":"custom", "components":["platform-store"], "mcpServers":[], "artifactAudiences":[]},
        "gatewayRequirements":[], "waitForDeployments":[]
    })).unwrap()
}

#[test]
fn retained_image_provenance_is_bound_even_when_the_catalog_is_resealed() {
    let mut lock = fixture();
    lock.validate().unwrap();
    // A chart snapshot is not evidence that a retained image was built there.
    let revision = SourceRevision::new(&lock.sources[0].revision).unwrap();
    lock.components[1].units[0].inputs = lock.components[1].units[0]
        .inputs
        .iter()
        .cloned()
        .map(|mut input| {
            if let ComponentInput::Image { source, .. } = &mut input {
                source.revision = revision.clone();
            }
            input
        })
        .collect();
    relock(&mut lock, 1);
    validate_component_catalog(&lock.components).unwrap();
    assert!(
        lock.validate()
            .unwrap_err()
            .to_string()
            .contains("build provenance")
    );
}

#[test]
fn image_variants_bind_exact_build_revisions_instead_of_catalog_order() {
    for same_bytes in [false, true] {
        let mut lock = fixture();
        let mut newer = lock.sources[0].images[0].clone();
        newer.source_revision = SourceRevision::new("4".repeat(40)).unwrap();
        newer.publication_digest = format!("sha256:{}", "5".repeat(64));
        if !same_bytes {
            newer.digest = format!("sha256:{}", "6".repeat(64));
        }
        lock.sources[0].images.push(newer.clone());
        lock.components[1].units[0].inputs = lock.components[1].units[0]
            .inputs
            .iter()
            .cloned()
            .map(|mut input| {
                if let ComponentInput::Image {
                    source,
                    target,
                    digest,
                    ..
                } = &mut input
                    && target == &newer.name
                {
                    source.revision = newer.source_revision.clone();
                    *digest = ArtifactDigest::new(&newer.digest).unwrap();
                }
                input
            })
            .collect();
        relock(&mut lock, 1);
        lock.validate().unwrap();
        lock.sources[0].images.reverse();
        lock.validate().unwrap();

        lock.sources[0]
            .images
            .retain(|image| image.source_revision != newer.source_revision);
        assert!(
            lock.validate()
                .unwrap_err()
                .to_string()
                .contains("build provenance")
        );
    }
}

#[test]
fn retained_versions_do_not_relax_image_ownership_or_publication_identity() {
    let original = fixture();
    let mut newer = original.sources[0].images[0].clone();
    newer.source_revision = SourceRevision::new("4".repeat(40)).unwrap();

    let mut repeated = original.clone();
    repeated.sources[0]
        .images
        .push(original.sources[0].images[0].clone());
    assert!(
        repeated
            .validate()
            .unwrap_err()
            .to_string()
            .contains("repeats a build revision")
    );

    let mut moved = original.clone();
    let mut moved_image = newer.clone();
    moved_image.repository.push_str("-moved");
    moved.sources[0].images.push(moved_image);
    assert!(
        moved
            .validate()
            .unwrap_err()
            .to_string()
            .contains("changes repository")
    );

    let mut stolen = original.clone();
    stolen.sources[1].images.push(newer.clone());
    assert!(
        stolen
            .validate()
            .unwrap_err()
            .to_string()
            .contains("owned by both")
    );

    let mut renamed = original.clone();
    let mut renamed_image = newer.clone();
    renamed_image.name = "different-target".into();
    renamed.sources[0].images.push(renamed_image);
    assert!(
        renamed
            .validate()
            .unwrap_err()
            .to_string()
            .contains("owned by both")
    );

    let mut unqualified = original;
    newer.publication_digest = newer.digest.clone();
    unqualified.sources[0].images.push(newer);
    assert!(
        unqualified
            .validate()
            .unwrap_err()
            .to_string()
            .contains("attested publication")
    );
}

#[test]
fn resealed_chart_or_removed_artifact_cannot_escape_the_complete_lock() {
    let mut changed = fixture();
    changed.components[1].units[0].inputs = changed.components[1].units[0]
        .inputs
        .iter()
        .cloned()
        .map(|mut input| {
            if let ComponentInput::Chart { digest, .. } = &mut input {
                *digest = ArtifactDigest::new(format!("sha256:{}", "9".repeat(64))).unwrap();
            }
            input
        })
        .collect();
    relock(&mut changed, 1);
    validate_component_catalog(&changed.components).unwrap();
    assert!(
        changed
            .validate()
            .unwrap_err()
            .to_string()
            .contains("chart differs")
    );

    let mut removed = fixture();
    removed.sources[1].images.clear();
    assert!(
        removed
            .validate()
            .unwrap_err()
            .to_string()
            .contains("outside the artifact closure")
    );
}

#[test]
fn profile_permissions_and_dependency_changes_cannot_reuse_a_stale_catalog() {
    let lock = fixture();
    let original = profile(&lock);
    validate_profile_component_bindings(&original, &lock).unwrap();

    let mut changed = original.clone();
    changed.components[2]
        .dependencies
        .insert("platform".to_owned().try_into().unwrap());
    assert!(
        validate_profile_component_bindings(&changed, &lock)
            .unwrap_err()
            .to_string()
            .contains("metadata differs")
    );

    let mut changed = original.clone();
    changed.components[1]
        .cluster_objects
        .insert(ObjectIdentity {
            group: "rbac.authorization.k8s.io".into(),
            kind: "ClusterRole".into(),
            namespace: None,
            name: "new-permission".into(),
        });
    assert!(
        validate_profile_component_bindings(&changed, &lock)
            .unwrap_err()
            .to_string()
            .contains("permissions differ")
    );

    let mut changed = original;
    changed.namespace = "different".into();
    for component in &mut changed.components {
        component.namespaces.insert("different".into());
    }
    assert!(validate_profile_component_bindings(&changed, &lock).is_err());
}

#[test]
fn retained_catalog_cannot_reserve_unrendered_namespaced_objects() {
    let mut lock = fixture();
    let profile = profile(&lock);
    lock.components[2]
        .declaration
        .permitted_objects
        .insert(ObjectIdentity {
            group: "apps".into(),
            kind: "Deployment".into(),
            namespace: Some("veoveo".into()),
            name: "unrendered".into(),
        });
    relock(&mut lock, 2);
    lock.validate().unwrap();
    assert!(
        validate_profile_component_bindings(&profile, &lock)
            .unwrap_err()
            .to_string()
            .contains("permissions differ")
    );
}
