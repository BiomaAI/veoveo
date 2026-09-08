//! Non-secret evidence of one completed atomic installation.
use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_extension_contract::ArtifactDigest;

use super::*;

pub const INSTALLED_UNIT_SCHEMA: &str = "veoveo.io/installed-deployment-unit/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledUnitReceipt {
    pub schema_version: String,
    pub cluster_uid: String,
    pub component: DeploymentComponent,
    pub unit: LockedAtomicUnit,
    pub helm: Option<InstalledHelmRevision>,
    pub objects: Vec<InstalledObjectObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledHelmRevision {
    pub revision: u64,
    pub manifest_digest: ArtifactDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledObjectObservation {
    pub identity: ObjectIdentity,
    pub state: InstalledObjectState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum InstalledObjectState {
    Present {
        uid: String,
        digest: ArtifactDigest,
        /// A successful Job with declared TTL may be absent on a later visit.
        completed_ttl_job: bool,
    },
    /// Helm recorded successful execution and a hook-succeeded deletion policy.
    CompletedHook,
}

impl InstalledUnitReceipt {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == INSTALLED_UNIT_SCHEMA,
            "unsupported installed-unit receipt schema"
        );
        ensure!(
            !self.cluster_uid.is_empty() && !self.cluster_uid.chars().any(char::is_whitespace),
            "installed-unit receipt has no cluster identity"
        );
        ensure!(
            matches!(
                (&self.unit.target, &self.helm),
                (
                    AtomicTarget::HelmRelease { .. },
                    Some(InstalledHelmRevision { revision: 1.., .. })
                ) | (AtomicTarget::ManifestSet { .. }, None)
            ),
            "installed-unit receipt has inconsistent Helm revision evidence"
        );
        let prepared = PreparedAtomicUnit {
            component: self.component.id.clone(),
            source: self.component.source.clone(),
            configuration: self.component.configuration.clone(),
            target: self.unit.target.clone(),
            inputs: self.unit.inputs.clone(),
            objects: self.unit.objects.clone(),
            tool_scope: AtomicToolScope::Exact,
        };
        ensure!(
            atomic_unit_digest(&self.component, &prepared)? == self.unit.digest
                && atomic_unit_content_digest(&self.component, &prepared)?
                    == self.unit.content_digest,
            "installed-unit receipt has invalid provenance or content identity"
        );
        let mut observed = BTreeSet::new();
        for object in &self.objects {
            ensure!(
                observed.insert(&object.identity),
                "installed-unit receipt repeats an object"
            );
            match &object.state {
                InstalledObjectState::Present {
                    uid,
                    completed_ttl_job,
                    ..
                } => {
                    ensure!(
                        !uid.is_empty() && !uid.chars().any(char::is_whitespace),
                        "installed object has no UID"
                    );
                    ensure!(
                        !completed_ttl_job
                            || object.identity.group == "batch" && object.identity.kind == "Job",
                        "only a Job can have completed TTL evidence"
                    );
                }
                InstalledObjectState::CompletedHook => ensure!(
                    self.helm.is_some(),
                    "completed hook requires Helm revision evidence"
                ),
            }
        }
        ensure!(
            observed
                == self
                    .unit
                    .objects
                    .iter()
                    .map(|object| &object.identity)
                    .collect(),
            "installed-unit receipt omits or adds object observations"
        );
        Ok(())
    }
}
