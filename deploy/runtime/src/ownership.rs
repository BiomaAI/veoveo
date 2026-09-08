//! Read-only ownership preflight, including Helm's historical deletion inventory.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use veoveo_deploy_contract::components::{
    AtomicTarget, LockedComponent, ObjectIdentity, validate_component_catalog,
    validate_helm_inventory,
};

use crate::{
    compile::{CompiledComponent, objects::ObjectScopes},
    configuration::append_yaml_bytes,
    discovery::validate_cluster_scopes,
    helm_state::{HelmReleaseMetadata, release_metadata, upgrade_revisions},
    process::output_checked,
};

#[cfg(test)]
mod live_tests;

#[derive(Debug, Default, Deserialize)]
struct Metadata {
    name: String,
    namespace: Option<String>,
    #[serde(default)]
    labels: BTreeMap<String, String>,
    #[serde(default)]
    annotations: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveObject {
    api_version: String,
    kind: String,
    metadata: Metadata,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LiveObjects {
    List { items: Vec<LiveObject> },
    Single(LiveObject),
}

pub(crate) fn validate_live_ownership(
    context: &str,
    catalog: &[LockedComponent],
    compiled: &[CompiledComponent],
) -> Result<()> {
    validate_component_catalog(catalog)?;
    let mut objects = compiled
        .iter()
        .flat_map(|component| &component.units)
        .flat_map(|unit| unit.objects.iter().cloned())
        .collect::<Vec<_>>();
    let mut expected = BTreeMap::<ObjectIdentity, AtomicTarget>::new();
    let mut releases = Vec::<HelmReleaseMetadata>::new();
    for component in compiled {
        for unit in &component.units {
            for object in &unit.prepared.objects {
                insert_owner(
                    &mut expected,
                    object.identity.clone(),
                    &unit.prepared.target,
                )?;
            }
            let AtomicTarget::HelmRelease { namespace, name } = &unit.prepared.target else {
                continue;
            };
            let Some(release) = release_metadata(context, namespace, name)? else {
                continue;
            };
            ensure!(
                matches!(
                    release.status.as_str(),
                    "deployed" | "failed" | "superseded" | "uninstalled"
                ),
                "Helm release {namespace}/{name} has an active or unsupported state {}; installation preflight cannot proceed",
                release.status
            );
            for revision in upgrade_revisions(context, &release)? {
                let mut historical = Vec::new();
                for section in ["manifest", "hooks"] {
                    let bytes = output_checked(
                        "helm",
                        [
                            "--kube-context",
                            context,
                            "get",
                            section,
                            name,
                            "--namespace",
                            namespace,
                            "--revision",
                            &revision.to_string(),
                        ],
                        None,
                    )?;
                    append_yaml_bytes(&bytes, "stored Helm manifest", &mut historical)?;
                }
                let mut scopes = ObjectScopes::default();
                scopes.declare_crds(&objects)?;
                scopes.declare_crds(&historical)?;
                let inventory = scopes.rendered(
                    &historical,
                    namespace,
                    &component.locked.declaration.permitted_objects,
                )?;
                let identities = inventory
                    .into_iter()
                    .map(|object| object.identity)
                    .collect::<Vec<_>>();
                validate_helm_inventory(
                    catalog,
                    &component.locked.declaration.id,
                    &unit.prepared.target,
                    &identities,
                )?;
                for identity in identities {
                    insert_owner(&mut expected, identity, &unit.prepared.target)?;
                }
                objects.extend(historical);
            }
            releases.push(release);
        }
    }
    let served = validate_cluster_scopes(context, catalog, &objects)?;
    let live = read_objects(context, &expected, &served)?;
    for (identity, metadata) in live {
        validate_manager(&expected[&identity], &metadata)
            .with_context(|| format!("checking existing object {identity:?}"))?;
    }
    // Bind every historical manifest to the observed revision and reject a
    // concurrent release transition during preflight. Execution fencing is separate.
    for before in releases {
        ensure!(
            release_metadata(context, &before.namespace, &before.name)?.as_ref() == Some(&before),
            "Helm release changed during installation preflight"
        );
    }
    Ok(())
}

fn insert_owner(
    owners: &mut BTreeMap<ObjectIdentity, AtomicTarget>,
    identity: ObjectIdentity,
    target: &AtomicTarget,
) -> Result<()> {
    if let Some(previous) = owners.insert(identity, target.clone()) {
        ensure!(
            previous == *target,
            "historical and desired objects belong to different atomic targets"
        );
    }
    Ok(())
}

fn read_objects(
    context: &str,
    expected: &BTreeMap<ObjectIdentity, AtomicTarget>,
    served: &BTreeSet<(String, String)>,
) -> Result<BTreeMap<ObjectIdentity, Metadata>> {
    let mut groups = BTreeMap::<(String, String, Option<String>), Vec<String>>::new();
    for identity in expected.keys() {
        if served.contains(&(identity.group.clone(), identity.kind.clone())) {
            groups
                .entry((
                    identity.group.clone(),
                    identity.kind.clone(),
                    identity.namespace.clone(),
                ))
                .or_default()
                .push(identity.name.clone());
        }
    }
    let mut result = BTreeMap::new();
    for ((group, kind, namespace), names) in groups {
        let resource = if group.is_empty() {
            kind
        } else {
            format!("{kind}.{group}")
        };
        let mut args = vec![
            "--context",
            context,
            "--request-timeout=10s",
            "get",
            &resource,
            "--ignore-not-found",
            "--output=json",
        ];
        if let Some(namespace) = &namespace {
            args.extend(["--namespace", namespace]);
        }
        args.extend(names.iter().map(String::as_str));
        let bytes = output_checked("kubectl", args, None)?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let objects =
            match serde_json::from_slice(&bytes).context("decoding live object metadata")? {
                LiveObjects::List { items } => items,
                LiveObjects::Single(object) => vec![object],
            };
        for object in objects {
            let identity = ObjectIdentity {
                group: object
                    .api_version
                    .split_once('/')
                    .map_or("", |(group, _)| group)
                    .into(),
                kind: object.kind,
                name: object.metadata.name.clone(),
                namespace: object.metadata.namespace.clone(),
            };
            ensure!(
                expected.contains_key(&identity),
                "Kubernetes returned an unrequested object identity"
            );
            ensure!(
                result.insert(identity, object.metadata).is_none(),
                "Kubernetes returned duplicate object identities"
            );
        }
    }
    Ok(result)
}

fn validate_manager(target: &AtomicTarget, metadata: &Metadata) -> Result<()> {
    // Recognized GitOps ownership markers take precedence over a matching Helm
    // release name. Absence of a marker is not proof of an imperative owner.
    ensure!(
        !metadata
            .labels
            .keys()
            .chain(metadata.annotations.keys())
            .any(|key| key.starts_with("kustomize.toolkit.fluxcd.io/")
                || key.starts_with("helm.toolkit.fluxcd.io/")
                || key == "argocd.argoproj.io/tracking-id"
                || key == "argocd.argoproj.io/instance"),
        "existing object belongs to a GitOps owner"
    );
    let release_name = metadata.annotations.get("meta.helm.sh/release-name");
    let release_namespace = metadata.annotations.get("meta.helm.sh/release-namespace");
    let manager = metadata.labels.get("app.kubernetes.io/managed-by");
    match target {
        AtomicTarget::HelmRelease { namespace, name } => ensure!(
            release_name == Some(name)
                && release_namespace == Some(namespace)
                && manager.map(String::as_str) == Some("Helm"),
            "existing object is not owned by the selected Helm release"
        ),
        AtomicTarget::ManifestSet { .. } => ensure!(
            release_name.is_none()
                && release_namespace.is_none()
                && manager.map(String::as_str) != Some("Helm"),
            "raw manifest operation overlaps an existing Helm owner"
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_helm_ownership_is_required_even_when_names_and_contents_match() {
        let target = AtomicTarget::HelmRelease {
            namespace: "veoveo".into(),
            name: "platform".into(),
        };
        let raw = AtomicTarget::ManifestSet {
            name: "public-resources".into(),
        };
        let mut metadata = Metadata::default();
        assert!(validate_manager(&target, &metadata).is_err());
        validate_manager(&raw, &metadata).unwrap();
        metadata
            .labels
            .insert("app.kubernetes.io/managed-by".into(), "Helm".into());
        metadata
            .annotations
            .insert("meta.helm.sh/release-name".into(), "platform".into());
        metadata
            .annotations
            .insert("meta.helm.sh/release-namespace".into(), "veoveo".into());
        validate_manager(&target, &metadata).unwrap();
        assert!(validate_manager(&raw, &metadata).is_err());
        for key in [
            "meta.helm.sh/release-name",
            "meta.helm.sh/release-namespace",
        ] {
            let previous = metadata
                .annotations
                .insert(key.into(), "foreign".into())
                .unwrap();
            assert!(validate_manager(&target, &metadata).is_err());
            metadata.annotations.insert(key.into(), previous);
        }
        for key in [
            "helm.toolkit.fluxcd.io/name",
            "kustomize.toolkit.fluxcd.io/namespace",
            "argocd.argoproj.io/tracking-id",
        ] {
            metadata.annotations.insert(key.into(), "owner".into());
            assert!(validate_manager(&target, &metadata).is_err());
            assert!(validate_manager(&raw, &metadata).is_err());
            metadata.annotations.remove(key);
        }
    }

    #[test]
    fn historical_objects_cannot_move_between_releases_or_into_raw_operations() {
        let identity = ObjectIdentity {
            group: "apps".into(),
            kind: "Deployment".into(),
            namespace: Some("veoveo".into()),
            name: "service".into(),
        };
        let target = AtomicTarget::HelmRelease {
            namespace: "veoveo".into(),
            name: "platform".into(),
        };
        let mut owners = BTreeMap::new();
        insert_owner(&mut owners, identity.clone(), &target).unwrap();
        insert_owner(&mut owners, identity.clone(), &target).unwrap();
        assert!(
            insert_owner(
                &mut owners,
                identity,
                &AtomicTarget::ManifestSet {
                    name: "public-resources".into()
                }
            )
            .is_err()
        );
    }
}
