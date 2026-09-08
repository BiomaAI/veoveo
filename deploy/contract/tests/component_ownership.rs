use std::collections::BTreeSet;

use veoveo_deploy_contract::{KubernetesObjectKey, components::*};
use veoveo_extension_contract::SourceRevision;

#[path = "support/components.rs"]
mod support;
use support::*;

#[test]
fn stored_helm_inventory_keeps_retired_objects_inside_the_original_owner() {
    let platform = fixture("platform", ComponentRole::Platform);
    let extension = fixture("extension", ComponentRole::Extension);
    let catalog = [platform.clone(), extension.clone()];
    let target = &platform.units[0].target;
    let mut retired = platform.units[0].objects[0].identity.clone();
    retired.name = "retired-job".into();
    assert!(!platform.declaration.permitted_objects.contains(&retired));
    validate_helm_inventory(&catalog, &id("platform"), target, &[retired.clone()]).unwrap();
    let mut previous = platform.clone();
    previous.units[0].objects.push(RenderedObject {
        identity: retired.clone(),
        digest: digest('d'),
    });
    previous
        .declaration
        .permitted_objects
        .insert(retired.clone());
    let previous = relock(&previous);
    let plan = component_mutation_plan(
        &catalog,
        &BTreeSet::from([id("platform")]),
        &prepared(&platform),
        &observed(&previous),
    )
    .unwrap();
    assert_eq!(plan.mutations[0].retired_objects[0].identity, retired);
    assert!(
        validate_helm_inventory(
            &catalog,
            &id("platform"),
            target,
            &[extension.units[0].objects[0].identity.clone()]
        )
        .is_err()
    );
    retired.namespace = Some("unrelated".into());
    assert!(
        validate_helm_inventory(&catalog, &id("platform"), target, &[retired.clone()]).is_err()
    );
    retired.namespace = None;
    assert!(validate_helm_inventory(&catalog, &id("platform"), target, &[retired]).is_err());
    assert!(
        validate_helm_inventory(
            &catalog,
            &id("platform"),
            target,
            &[
                platform.units[0].objects[0].identity.clone(),
                platform.units[0].objects[0].identity.clone()
            ]
        )
        .is_err()
    );
}

#[test]
fn platform_only_image_update_has_no_extension_mutation() {
    let previous = fixture("platform", ComponentRole::Platform);
    let extension = fixture("extension", ComponentRole::Extension);
    let mut next = previous.clone();
    next.declaration.inputs = next
        .declaration
        .inputs
        .into_iter()
        .map(|mut input| {
            if let ComponentInput::Image { digest: value, .. } = &mut input {
                *value = digest('f');
            }
            input
        })
        .collect();
    next.units[0].inputs = next.declaration.inputs.clone();
    next.units[0].objects[0].digest = digest('f');
    let next = relock(&next);
    let plan = component_mutation_plan(
        &[next.clone(), extension.clone()],
        &BTreeSet::from([id("platform")]),
        &prepared(&next),
        &observed(&previous),
    )
    .unwrap();
    assert_eq!(plan.mutations.len(), 1);
    assert_eq!(plan.mutations[0].component, id("platform"));
    assert_eq!(
        plan.mutations[0].verb,
        ComponentMutationVerb::HelmUpgradeInstall
    );
    assert_eq!(
        plan.unselected_objects,
        extension.declaration.permitted_objects
    );
}

#[test]
fn extension_update_expands_platform_dependency_without_upgrading_it() {
    let platform = fixture("platform", ComponentRole::Platform);
    let previous = fixture("extension", ComponentRole::Extension);
    let mut extension = previous.clone();
    extension.declaration.dependencies.insert(id("platform"));
    extension.units[0].objects[0].digest = digest('f');
    let extension = relock(&extension);
    let renders = prepared(&platform)
        .into_iter()
        .chain(prepared(&extension))
        .collect::<Vec<_>>();
    let observations = observed(&platform)
        .into_iter()
        .chain(observed(&previous))
        .collect::<Vec<_>>();
    let plan = component_mutation_plan(
        &[extension, platform],
        &BTreeSet::from([id("extension")]),
        &renders,
        &observations,
    )
    .unwrap();
    assert_eq!(plan.requested, BTreeSet::from([id("extension")]));
    assert_eq!(plan.expanded, vec![id("platform"), id("extension")]);
    assert_eq!(plan.mutations[0].verb, ComponentMutationVerb::Unchanged);
    assert_eq!(
        plan.mutations[1].verb,
        ComponentMutationVerb::HelmUpgradeInstall
    );
}

#[test]
fn exact_selection_rejects_empty_unknown_and_unselected_renders() {
    let platform = fixture("platform", ComponentRole::Platform);
    let extension = fixture("extension", ComponentRole::Extension);
    let catalog = [platform.clone(), extension.clone()];
    assert_error(
        select_components(&catalog, &BTreeSet::new()),
        "exact component ID",
    );
    assert_error(
        select_components(&catalog, &BTreeSet::from([id("all")])),
        "unknown component all",
    );
    assert_error(
        component_mutation_plan(
            &catalog,
            &BTreeSet::from([id("platform")]),
            &prepared(&extension),
            &observed(&platform),
        ),
        "unselected component was rendered",
    );
}

#[test]
fn duplicate_component_release_and_reserved_object_ownership_fail_closed() {
    let platform = fixture("platform", ComponentRole::Platform);
    assert_error(
        validate_component_catalog(&[platform.clone(), platform.clone()]),
        "duplicate component",
    );
    let mut extension = fixture("extension", ComponentRole::Extension);
    extension.declaration.targets = platform.declaration.targets.clone();
    extension.units[0].target = platform.units[0].target.clone();
    assert_error(
        validate_component_catalog(&[platform.clone(), relock(&extension)]),
        "atomic target",
    );
    let mut extension = fixture("extension", ComponentRole::Extension);
    extension
        .declaration
        .permitted_objects
        .extend(platform.declaration.permitted_objects.clone());
    assert_error(
        validate_component_catalog(&[platform, extension]),
        "is owned by both",
    );
}

#[test]
fn served_api_versions_do_not_create_different_object_owners() {
    let object = KubernetesObjectKey {
        group: "example.invalid".into(),
        version: "v1".into(),
        kind: "Thing".into(),
        namespace: Some("veoveo".into()),
        name: "same-object".into(),
    };
    let mut later = object.clone();
    later.version = "v2".into();
    let mut platform = fixture("platform", ComponentRole::Platform);
    let mut extension = fixture("extension", ComponentRole::Extension);
    platform
        .declaration
        .permitted_objects
        .insert(ObjectIdentity::from(&object));
    extension
        .declaration
        .permitted_objects
        .insert(ObjectIdentity::from(&later));
    assert_error(
        validate_component_catalog(&[platform, extension]),
        "is owned by both",
    );
}

#[test]
fn missing_dependency_and_cycles_are_rejected_before_selection() {
    let mut platform = fixture("platform", ComponentRole::Platform);
    platform.declaration.dependencies.insert(id("extension"));
    assert_error(
        select_components(&[platform.clone()], &BTreeSet::from([id("platform")])),
        "missing dependency",
    );
    let mut extension = fixture("extension", ComponentRole::Extension);
    extension.declaration.dependencies.insert(id("platform"));
    assert_error(
        select_components(&[platform, extension], &BTreeSet::from([id("platform")])),
        "cycle",
    );
}

#[test]
fn immutable_render_source_inputs_and_complete_inventory_must_match_lock() {
    let platform = fixture("platform", ComponentRole::Platform);
    let catalog = [platform.clone()];
    let requested = BTreeSet::from([id("platform")]);
    let observations = observed(&platform);
    let mut renders = prepared(&platform);
    renders[0].source.revision = SourceRevision::new("b".repeat(40)).unwrap();
    assert_error(
        component_mutation_plan(&catalog, &requested, &renders, &observations),
        "source differs",
    );
    let mut renders = prepared(&platform);
    renders[0].inputs.pop_first();
    assert_error(
        component_mutation_plan(&catalog, &requested, &renders, &observations),
        "inputs differ",
    );
    let mut renders = prepared(&platform);
    renders[0].objects[0].digest = digest('f');
    assert_error(
        component_mutation_plan(&catalog, &requested, &renders, &observations),
        "rendering differs",
    );
    let mut renders = prepared(&platform);
    renders[0].objects.clear();
    assert_error(
        component_mutation_plan(&catalog, &requested, &renders, &observations),
        "no explicit objects",
    );
}

#[test]
fn complete_target_coverage_and_exact_tool_scope_are_required() {
    let platform = fixture("platform", ComponentRole::Platform);
    let catalog = [platform.clone()];
    let requested = BTreeSet::from([id("platform")]);
    assert_error(
        component_mutation_plan(&catalog, &requested, &[], &observed(&platform)),
        "missing a complete",
    );
    let mut renders = prepared(&platform);
    renders.push(renders[0].clone());
    assert_error(
        component_mutation_plan(&catalog, &requested, &renders, &observed(&platform)),
        "duplicate prepared",
    );
    let mut renders = prepared(&platform);
    renders[0].tool_scope = AtomicToolScope::Unscoped;
    assert_error(
        component_mutation_plan(&catalog, &requested, &renders, &observed(&platform)),
        "cannot target",
    );
}

#[test]
fn explicit_absence_observation_is_required_for_installation() {
    let platform = fixture("platform", ComponentRole::Platform);
    let catalog = [platform.clone()];
    let requested = BTreeSet::from([id("platform")]);
    let renders = prepared(&platform);
    assert_error(
        component_mutation_plan(&catalog, &requested, &renders, &[]),
        "missing a verified",
    );
    let mut observations = observed(&platform);
    observations[0].state = ObservedUnitState::Absent;
    let plan = component_mutation_plan(&catalog, &requested, &renders, &observations).unwrap();
    assert_eq!(
        plan.mutations[0].verb,
        ComponentMutationVerb::HelmUpgradeInstall
    );
}

#[test]
fn matching_stored_digest_does_not_hide_object_drift() {
    let platform = fixture("platform", ComponentRole::Platform);
    let mut observations = observed(&platform);
    let ObservedUnitState::Present { objects, .. } = &mut observations[0].state else {
        unreachable!()
    };
    objects[0].digest = digest('f');
    assert_error(
        component_mutation_plan(
            std::slice::from_ref(&platform),
            &BTreeSet::from([id("platform")]),
            &prepared(&platform),
            &observations,
        ),
        "inventory has drifted",
    );
}

#[test]
fn previous_helm_inventory_cannot_delete_another_owners_object() {
    let platform = fixture("platform", ComponentRole::Platform);
    let extension = fixture("extension", ComponentRole::Extension);
    let mut observations = observed(&platform);
    let ObservedUnitState::Present { objects, .. } = &mut observations[0].state else {
        unreachable!()
    };
    objects.push(extension.units[0].objects[0].clone());
    assert_error(
        component_mutation_plan(
            &[platform.clone(), extension],
            &BTreeSet::from([id("platform")]),
            &prepared(&platform),
            &observations,
        ),
        "outside component platform ownership",
    );
}

#[test]
fn namespaces_cluster_objects_secrets_and_lists_require_explicit_ownership() {
    let platform = fixture("platform", ComponentRole::Platform);
    let mut namespace = platform.clone();
    namespace.declaration.namespaces.clear();
    assert_error(
        lock_component(namespace.declaration.clone(), prepared(&namespace)),
        "undeclared namespace",
    );
    let mut renders = prepared(&platform);
    renders[0].objects[0].identity.namespace = None;
    assert_error(
        atomic_unit_digest(&platform.declaration, &renders[0]),
        "outside component",
    );
    for kind in ["Secret", "List"] {
        let mut candidate = platform.clone();
        let object = &mut candidate.units[0].objects[0].identity;
        object.group.clear();
        object.kind = kind.into();
        candidate.declaration.permitted_objects = BTreeSet::from([object.clone()]);
        assert!(
            lock_component(candidate.declaration.clone(), prepared(&candidate)).is_err(),
            "accepted {kind}"
        );
    }
}

#[test]
fn a_component_cannot_hide_mixed_ownership_inside_one_release() {
    let platform = fixture("platform", ComponentRole::Platform);
    let extension = fixture("extension", ComponentRole::Extension);
    let mut renders = prepared(&platform);
    renders[0]
        .objects
        .extend(extension.units[0].objects.clone());
    assert_error(
        component_mutation_plan(
            &[platform.clone(), extension],
            &BTreeSet::from([id("platform")]),
            &renders,
            &observed(&platform),
        ),
        "outside component platform ownership",
    );
}

#[test]
fn unit_digest_binds_inputs_and_is_independent_of_object_order() {
    let mut platform = fixture("platform", ComponentRole::Platform);
    let mut extra = platform.units[0].objects[0].clone();
    extra.identity.name = "another-deployment".into();
    platform
        .declaration
        .permitted_objects
        .insert(extra.identity.clone());
    platform.units[0].objects.push(extra);
    let platform = relock(&platform);
    let mut renders = prepared(&platform);
    renders[0].objects.reverse();
    assert_eq!(
        atomic_unit_digest(&platform.declaration, &renders[0]).unwrap(),
        platform.units[0].digest
    );
    renders[0].inputs.pop_first();
    assert_ne!(
        atomic_unit_digest(&platform.declaration, &renders[0]).unwrap(),
        platform.units[0].digest
    );
    let mut forged = platform;
    forged.units[0].digest = digest('f');
    assert_error(validate_component_catalog(&[forged]), "digest differs");
}

#[test]
fn raw_manifest_set_is_an_explicit_atomic_target() {
    let mut installation = fixture("installation", ComponentRole::Installation);
    let target = AtomicTarget::ManifestSet {
        name: "namespace-and-config".into(),
    };
    installation.declaration.targets = BTreeSet::from([target.clone()]);
    installation.units[0].target = target;
    let installation = relock(&installation);
    let mut observations = observed(&installation);
    observations[0].state = ObservedUnitState::Absent;
    let plan = component_mutation_plan(
        std::slice::from_ref(&installation),
        &BTreeSet::from([id("installation")]),
        &prepared(&installation),
        &observations,
    )
    .unwrap();
    assert_eq!(
        plan.mutations[0].verb,
        ComponentMutationVerb::ApplyExplicitObjects
    );
}

#[test]
fn extension_identity_is_mandatory_and_typed() {
    let mut extension = fixture("extension", ComponentRole::Extension);
    extension.declaration.extension_release = None;
    assert_error(
        lock_component(extension.declaration.clone(), prepared(&extension)),
        "extension release identity",
    );
    assert!(serde_json::from_str::<ComponentId>("\"ALL*\"").is_err());
    assert!(serde_json::from_str::<ComponentId>("\"\"").is_err());
    let schema = serde_json::to_value(schemars::schema_for!(ComponentId)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&serde_json::json!("platform")));
    assert!(!validator.is_valid(&serde_json::json!("ALL*")));
}

#[test]
fn source_identity_conflicts_are_rejected_without_exposing_credentials() {
    let platform = fixture("platform", ComponentRole::Platform);
    let mut extension = fixture("extension", ComponentRole::Extension);
    extension.declaration.inputs.insert(ComponentInput::File {
        source: ComponentSource {
            repository: "https://example.invalid/different-source.git".into(),
            ..platform.declaration.source.clone()
        },
        path: "public-values.yaml".into(),
        digest: digest('a'),
    });
    extension.units[0].inputs = extension.declaration.inputs.clone();
    assert_error(
        validate_component_catalog(&[platform.clone(), relock(&extension)]),
        "conflicting identities",
    );
    let mut credentialed = platform;
    credentialed.declaration.source.repository =
        "https://user:private-token@example.invalid/source.git".into();
    let error = format!(
        "{:#}",
        lock_component(credentialed.declaration.clone(), prepared(&credentialed)).unwrap_err()
    );
    assert!(error.contains("must not contain credentials"));
    assert!(!error.contains("private-token"));
}

#[test]
fn shared_inputs_retain_one_immutable_content_and_image_owner() {
    let platform = fixture("platform", ComponentRole::Platform);
    let mut extension = fixture("extension", ComponentRole::Extension);
    extension
        .declaration
        .inputs
        .extend(platform.declaration.inputs.clone());
    extension.units[0].inputs = extension.declaration.inputs.clone();
    let extension = relock(&extension);
    validate_component_catalog(&[platform.clone(), extension.clone()]).unwrap();

    let mut conflict = extension;
    conflict.declaration.inputs = conflict
        .declaration
        .inputs
        .into_iter()
        .map(|mut input| {
            if let ComponentInput::File {
                source,
                digest: value,
                ..
            } = &mut input
                && source.name == "platform"
            {
                *value = digest('f');
            }
            input
        })
        .collect();
    conflict.units[0].inputs = conflict.declaration.inputs.clone();
    assert_error(
        validate_component_catalog(&[platform.clone(), relock(&conflict)]),
        "conflicting locked contents",
    );

    let mut copied_owner = fixture("extension", ComponentRole::Extension);
    copied_owner.declaration.inputs = copied_owner
        .declaration
        .inputs
        .into_iter()
        .map(|mut input| {
            if let ComponentInput::Image { repository, .. } = &mut input {
                *repository = "registry.invalid/platform".into();
            }
            input
        })
        .collect();
    copied_owner.units[0].inputs = copied_owner.declaration.inputs.clone();
    assert_error(
        validate_component_catalog(&[platform, relock(&copied_owner)]),
        "multiple source owners",
    );
}

#[test]
fn release_split_cannot_transfer_an_installed_object_implicitly() {
    let previous = fixture("platform", ComponentRole::Platform);
    let mut next = previous.clone();
    let mut split = next.units[0].clone();
    split.target = AtomicTarget::HelmRelease {
        namespace: "veoveo".into(),
        name: "platform-split".into(),
    };
    next.declaration.targets.insert(split.target.clone());
    next.units[0].objects[0].identity.name = "platform-new".into();
    next.declaration
        .permitted_objects
        .insert(next.units[0].objects[0].identity.clone());
    next.units.push(split);
    let next = relock(&next);
    let mut observations = observed(&previous);
    observations.push(ObservedAtomicUnit {
        component: id("platform"),
        target: next.units[1].target.clone(),
        state: ObservedUnitState::Absent,
    });
    assert_error(
        component_mutation_plan(
            std::slice::from_ref(&next),
            &BTreeSet::from([id("platform")]),
            &prepared(&next),
            &observations,
        ),
        "cannot transfer between atomic targets",
    );
}

#[test]
fn helm_retirements_are_visible_and_raw_apply_cannot_pretend_to_remove_objects() {
    for raw in [false, true] {
        let mut previous = fixture("platform", ComponentRole::Platform);
        if raw {
            let target = AtomicTarget::ManifestSet {
                name: "platform-manifests".into(),
            };
            previous.declaration.targets = BTreeSet::from([target.clone()]);
            previous.units[0].target = target;
        }
        previous = relock(&previous);
        let mut next = previous.clone();
        next.units[0].objects[0].identity.name = "platform-new".into();
        next.declaration
            .permitted_objects
            .insert(next.units[0].objects[0].identity.clone());
        let next = relock(&next);
        let result = component_mutation_plan(
            std::slice::from_ref(&next),
            &BTreeSet::from([id("platform")]),
            &prepared(&next),
            &observed(&previous),
        );
        if raw {
            assert_error(result, "raw apply cannot remove retired objects");
        } else {
            assert_eq!(
                result.unwrap().mutations[0].retired_objects,
                previous.units[0].objects
            );
        }
    }
}
