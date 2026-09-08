//! Immutable manifest and hook execution evidence returned by Helm status.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use veoveo_deploy_contract::components::{
    DeploymentComponent, InstalledHelmRevision, ObjectIdentity, RenderedObject,
};

use super::release_metadata;
use crate::{
    compile::objects::{ObjectScopes, bytes_digest},
    configuration::append_yaml_bytes,
    process::output_checked,
};

#[derive(Deserialize)]
pub(crate) struct ReleaseSnapshot {
    name: String,
    namespace: String,
    version: u64,
    info: ReleaseInfo,
    manifest: String,
    #[serde(default)]
    hooks: Vec<Hook>,
}

#[derive(Deserialize)]
struct ReleaseInfo {
    status: String,
}

#[derive(Deserialize)]
struct Hook {
    manifest: String,
    #[serde(default)]
    delete_policies: Vec<String>,
    last_run: Option<HookRun>,
}

#[derive(Deserialize)]
struct HookRun {
    phase: Option<String>,
}

pub(crate) struct HelmSnapshotEvidence {
    pub(crate) revision: InstalledHelmRevision,
    pub(crate) objects: Vec<RenderedObject>,
    pub(crate) completed_hooks: BTreeSet<ObjectIdentity>,
}

impl ReleaseSnapshot {
    pub(crate) fn read(context: &str, namespace: &str, name: &str) -> Result<Option<Self>> {
        let Some(metadata) = release_metadata(context, namespace, name)? else {
            return Ok(None);
        };
        let revision = metadata.revision.value()?;
        let bytes = output_checked(
            "helm",
            [
                "--kube-context",
                context,
                "status",
                name,
                "--namespace",
                namespace,
                "--revision",
                &revision.to_string(),
                "--output=json",
            ],
            None,
        )?;
        let snapshot: Self =
            serde_json::from_slice(&bytes).context("decoding installed Helm release")?;
        ensure!(
            snapshot.name == name
                && snapshot.namespace == namespace
                && snapshot.version == revision
                && snapshot.info.status == metadata.status,
            "Helm snapshot differs from observed release metadata"
        );
        Ok(Some(snapshot))
    }

    pub(crate) fn evidence(&self, owner: &DeploymentComponent) -> Result<HelmSnapshotEvidence> {
        ensure!(
            self.info.status == "deployed",
            "only a deployed Helm revision can be reused"
        );
        let mut objects = Vec::new();
        append_yaml_bytes(
            self.manifest.as_bytes(),
            "installed Helm manifest",
            &mut objects,
        )?;
        let mut hook_objects = Vec::new();
        for hook in &self.hooks {
            let mut rendered = Vec::new();
            append_yaml_bytes(
                hook.manifest.as_bytes(),
                "installed Helm hook",
                &mut rendered,
            )?;
            ensure!(
                rendered.len() == 1,
                "stored Helm hook must contain one complete object"
            );
            objects.extend(rendered.iter().cloned());
            hook_objects.push(rendered);
        }
        let mut scopes = ObjectScopes::default();
        scopes.declare_crds(&objects)?;
        let mut inventory = scopes.rendered(&objects, &self.namespace, &owner.permitted_objects)?;
        inventory.sort_by(|left, right| left.identity.cmp(&right.identity));
        ensure!(
            inventory
                .iter()
                .map(|object| &object.identity)
                .collect::<BTreeSet<_>>()
                .len()
                == inventory.len(),
            "installed Helm manifest repeats an object"
        );
        let mut completed = BTreeSet::new();
        for (hook, objects) in self.hooks.iter().zip(hook_objects) {
            if hook
                .delete_policies
                .iter()
                .any(|policy| policy == "hook-succeeded")
                && hook.last_run.as_ref().and_then(|run| run.phase.as_deref()) == Some("Succeeded")
            {
                completed.insert(
                    scopes.rendered(&objects, &self.namespace, &owner.permitted_objects)?[0]
                        .identity
                        .clone(),
                );
            }
        }
        Ok(HelmSnapshotEvidence {
            revision: InstalledHelmRevision {
                revision: self.version,
                manifest_digest: bytes_digest(&serde_json::to_vec(&inventory)?)?,
            },
            objects: inventory,
            completed_hooks: completed,
        })
    }

    pub(crate) fn deployed(&self) -> bool {
        self.info.status == "deployed"
    }
}

pub(crate) fn same_inventory(left: &[RenderedObject], right: &[RenderedObject]) -> bool {
    let as_map = |objects: &[RenderedObject]| {
        objects
            .iter()
            .map(|object| (object.identity.clone(), object.digest.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    as_map(left) == as_map(right)
}
