//! Read-only destination checks for the complete locked ownership boundary.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use veoveo_deploy_contract::components::{LockedComponent, ObjectIdentity};

use crate::process::output_checked;

#[derive(Deserialize)]
struct ApiGroups {
    groups: Vec<ApiGroup>,
}

#[derive(Deserialize)]
struct ApiGroup {
    name: String,
    versions: Vec<GroupVersion>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupVersion {
    group_version: String,
}

#[derive(Deserialize)]
struct ApiResources {
    resources: Vec<ApiResource>,
}

#[derive(Deserialize)]
struct ApiResource {
    name: String,
    kind: String,
    namespaced: bool,
}

#[derive(Default)]
struct Scopes {
    // Scope cannot change across versions of a group/kind.
    kinds: BTreeMap<(String, String), bool>,
    versions: BTreeSet<(String, String)>,
}

impl Scopes {
    fn add(&mut self, group: &str, version: &str, kind: &str, namespaced: bool) -> Result<()> {
        let key = (group.to_owned(), kind.to_owned());
        if let Some(previous) = self.kinds.get(&key) {
            ensure!(
                *previous == namespaced,
                "Kubernetes resource {group}/{kind} changes scope across declarations"
            );
        }
        self.kinds.insert(key, namespaced);
        self.versions.insert((version.to_owned(), kind.to_owned()));
        Ok(())
    }

    fn resources(&mut self, version: &str, resources: ApiResources) -> Result<()> {
        let group = version.split_once('/').map_or("", |(group, _)| group);
        for resource in resources.resources {
            if !resource.name.contains('/') {
                self.add(group, version, &resource.kind, resource.namespaced)?;
            }
        }
        Ok(())
    }

    fn crds(&mut self, objects: &[Value]) -> Result<()> {
        for object in objects {
            if object["apiVersion"] != "apiextensions.k8s.io/v1"
                || object["kind"] != "CustomResourceDefinition"
            {
                continue;
            }
            let group = string(object, "/spec/group")?;
            let kind = string(object, "/spec/names/kind")?;
            let namespaced = match string(object, "/spec/scope")? {
                "Namespaced" => true,
                "Cluster" => false,
                _ => anyhow::bail!("prepared CRD has an invalid resource scope"),
            };
            for version in object
                .pointer("/spec/versions")
                .and_then(Value::as_array)
                .context("prepared CRD omits versions")?
            {
                if version["served"] == true {
                    let version = format!("{group}/{}", string(version, "/name")?);
                    self.add(group, &version, kind, namespaced)?;
                }
            }
        }
        Ok(())
    }

    fn identity(&self, object: &ObjectIdentity) -> Result<()> {
        let scope = self
            .kinds
            .get(&(object.group.clone(), object.kind.clone()))
            .with_context(|| {
                format!(
                    "destination API and prepared CRDs do not declare {}/{}",
                    object.group, object.kind
                )
            })?;
        ensure!(
            *scope == object.namespace.is_some(),
            "locked object {:?} disagrees with destination resource scope",
            object
        );
        Ok(())
    }
}

/// Verifies every locked owner, including unselected inventories and reserved
/// object permissions. CRDs can add a new kind but cannot override a served kind's
/// scope. The entire check finishes before the caller may issue an API write.
pub(crate) fn validate_cluster_scopes(
    context: &str,
    catalog: &[LockedComponent],
    objects: &[Value],
) -> Result<BTreeSet<(String, String)>> {
    let identities = catalog
        .iter()
        .flat_map(|component| {
            component.declaration.permitted_objects.iter().chain(
                component
                    .units
                    .iter()
                    .flat_map(|unit| unit.objects.iter().map(|object| &object.identity)),
            )
        })
        .collect::<BTreeSet<_>>();
    let needed = identities
        .iter()
        .map(|object| object.group.as_str())
        .chain(objects.iter().filter_map(|object| {
            object["apiVersion"]
                .as_str()
                .map(|version| version.split_once('/').map_or("", |(group, _)| group))
        }))
        .chain(objects.iter().filter_map(|object| {
            (object["apiVersion"] == "apiextensions.k8s.io/v1"
                && object["kind"] == "CustomResourceDefinition")
                .then(|| object.pointer("/spec/group").and_then(Value::as_str))
                .flatten()
        }))
        .collect::<BTreeSet<_>>();
    let mut scopes = Scopes::default();
    if needed.contains("") {
        scopes.resources("v1", read_discovery(context, "/api/v1")?)?;
    }
    let groups: ApiGroups = read_discovery(context, "/apis")?;
    for group in groups
        .groups
        .into_iter()
        .filter(|group| needed.contains(group.name.as_str()))
    {
        for version in group.versions {
            ensure!(
                version
                    .group_version
                    .starts_with(&format!("{}/", group.name))
                    && version
                        .group_version
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric()
                            || matches!(byte, b'.' | b'-' | b'/')),
                "destination returned an invalid API discovery version"
            );
            scopes.resources(
                &version.group_version,
                read_discovery(context, &format!("/apis/{}", version.group_version))?,
            )?;
        }
    }
    // Merge only after loading authoritative scopes, so a prepared declaration
    // cannot hide a mismatch by short-circuiting live discovery.
    let served = scopes.kinds.keys().cloned().collect();
    scopes.crds(objects)?;
    for identity in identities {
        scopes.identity(identity)?;
    }
    for object in objects {
        let version = string(object, "/apiVersion")?;
        let kind = string(object, "/kind")?;
        ensure!(
            scopes
                .versions
                .contains(&(version.to_owned(), kind.to_owned())),
            "rendered object uses unserved Kubernetes API {version} {kind}"
        );
    }
    Ok(served)
}

fn read_discovery<T: serde::de::DeserializeOwned>(context: &str, path: &str) -> Result<T> {
    let bytes = output_checked(
        "kubectl",
        [
            "--context",
            context,
            "--request-timeout=10s",
            "get",
            "--raw",
            path,
        ],
        None,
    )?;
    serde_json::from_slice(&bytes).context("decoding Kubernetes API discovery")
}

fn string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("rendered object omits {pointer}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn crd(scope: &str) -> Value {
        json!({"apiVersion":"apiextensions.k8s.io/v1", "kind":"CustomResourceDefinition",
            "spec":{"group":"example.invalid", "names":{"kind":"Scene"}, "scope":scope,
                "versions":[{"name":"v1", "served":true}, {"name":"v2", "served":false}]}})
    }

    #[test]
    fn prepared_crd_cannot_override_an_existing_api_scope() {
        let mut scopes = Scopes::default();
        scopes
            .resources(
                "example.invalid/v1",
                serde_json::from_value(json!({"resources":[
                    {"name":"scenes", "kind":"Scene", "namespaced":true}
                ]}))
                .unwrap(),
            )
            .unwrap();
        assert!(
            scopes
                .crds(&[crd("Cluster")])
                .unwrap_err()
                .to_string()
                .contains("changes scope")
        );
    }

    #[test]
    fn new_crds_allow_only_their_declared_scope_and_served_versions() {
        let mut scopes = Scopes::default();
        scopes.crds(&[crd("Namespaced")]).unwrap();
        assert!(
            scopes
                .versions
                .contains(&("example.invalid/v1".into(), "Scene".into()))
        );
        assert!(
            !scopes
                .versions
                .contains(&("example.invalid/v2".into(), "Scene".into()))
        );
        let mut object = ObjectIdentity {
            group: "example.invalid".into(),
            kind: "Scene".into(),
            namespace: Some("veoveo".into()),
            name: "sample".into(),
        };
        scopes.identity(&object).unwrap();
        object.namespace = None;
        assert!(scopes.identity(&object).is_err());
    }

    #[test]
    fn subresources_do_not_redefine_the_parent_kind() {
        let mut scopes = Scopes::default();
        scopes
            .resources(
                "apps/v1",
                serde_json::from_value(json!({"resources":[
                    {"name":"deployments", "kind":"Deployment", "namespaced":true},
                    {"name":"deployments/scale", "kind":"Scale", "namespaced":true}
                ]}))
                .unwrap(),
            )
            .unwrap();
        assert!(
            scopes
                .kinds
                .contains_key(&("apps".into(), "Deployment".into()))
        );
        assert!(!scopes.kinds.contains_key(&("apps".into(), "Scale".into())));
        assert!(scopes.add("apps", "apps/v2", "Deployment", false).is_err());
    }
}
