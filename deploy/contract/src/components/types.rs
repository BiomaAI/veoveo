use std::{collections::BTreeSet, fmt};

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_extension_contract::{ArtifactDigest, ExtensionId, ReleaseVersion, SourceRevision};

use crate::{KubernetesObjectKey, validate_name};

/// Globally unique, installation-owned deployment component name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(!try_from, !into)]
pub struct ComponentId(
    #[schemars(regex(pattern = "^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$"))] String,
);

impl ComponentId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ComponentId {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self> {
        validate_name("component", &value)?;
        Ok(Self(value))
    }
}

impl From<ComponentId> for String {
    fn from(value: ComponentId) -> Self {
        value.0
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComponentRole {
    Platform,
    Workload,
    Extension,
    Installation,
}

/// Immutable owner of the chart and manifest inputs used by a component.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentSource {
    pub name: String,
    pub repository: String,
    pub revision: SourceRevision,
}

/// Kubernetes object identity across served API versions.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectIdentity {
    pub group: String,
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
}

impl From<&KubernetesObjectKey> for ObjectIdentity {
    fn from(value: &KubernetesObjectKey) -> Self {
        Self {
            group: value.group.clone(),
            kind: value.kind.clone(),
            namespace: value.namespace.clone(),
            name: value.name.clone(),
        }
    }
}

/// One indivisible target understood by an installation mutation tool.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum AtomicTarget {
    HelmRelease {
        namespace: String,
        name: String,
    },
    /// A named, explicit set of rendered manifests, never a directory-wide apply.
    ManifestSet {
        name: String,
    },
}

/// Non-secret evidence identifying every image, values file and chart input.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ComponentInput {
    Image {
        source: ComponentSource,
        target: String,
        repository: String,
        digest: ArtifactDigest,
    },
    File {
        source: ComponentSource,
        path: String,
        digest: ArtifactDigest,
    },
    Chart {
        source: ComponentSource,
        coordinate: String,
        digest: ArtifactDigest,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentExtensionRelease {
    pub extension: ExtensionId,
    pub version: ReleaseVersion,
    pub manifest_digest: ArtifactDigest,
}

/// Ownership declaration resolved from a deployment profile into its immutable lock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeploymentComponent {
    pub id: ComponentId,
    pub role: ComponentRole,
    pub source: ComponentSource,
    pub namespaces: BTreeSet<String>,
    pub targets: BTreeSet<AtomicTarget>,
    pub permitted_objects: BTreeSet<ObjectIdentity>,
    pub inputs: BTreeSet<ComponentInput>,
    pub dependencies: BTreeSet<ComponentId>,
    pub extension_release: Option<ComponentExtensionRelease>,
}

/// Object inventories are retained for unselected owners without rendering them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockedComponent {
    pub declaration: DeploymentComponent,
    pub units: Vec<LockedAtomicUnit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockedAtomicUnit {
    pub target: AtomicTarget,
    pub inputs: BTreeSet<ComponentInput>,
    /// Identity of the complete locked inputs and rendered objects for this unit.
    pub digest: ArtifactDigest,
    pub objects: Vec<RenderedObject>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderedObject {
    pub identity: ObjectIdentity,
    pub digest: ArtifactDigest,
}

/// Complete rendering of one selected atomic target, collected before any write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedAtomicUnit {
    pub component: ComponentId,
    pub source: ComponentSource,
    pub target: AtomicTarget,
    pub inputs: BTreeSet<ComponentInput>,
    pub objects: Vec<RenderedObject>,
    pub tool_scope: AtomicToolScope,
}

/// Whether the renderer's execution tool can mutate only this atomic unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomicToolScope {
    Exact,
    Unscoped,
}

/// Executor-verified current unit state, including objects a Helm upgrade could delete.
/// The digest must describe the installed inputs, not a copy of the desired lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedAtomicUnit {
    pub component: ComponentId,
    pub target: AtomicTarget,
    pub state: ObservedUnitState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedUnitState {
    /// Both the release and every desired object are absent. A missing Helm release
    /// alone cannot authorize adopting objects managed by another release or tool.
    Absent,
    Present {
        digest: ArtifactDigest,
        objects: Vec<RenderedObject>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentMutationVerb {
    HelmUpgradeInstall,
    ApplyExplicitObjects,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentMutation {
    pub component: ComponentId,
    pub source: ComponentSource,
    pub target: AtomicTarget,
    pub digest: ArtifactDigest,
    pub objects: Vec<RenderedObject>,
    /// Previous Helm objects omitted from the new render, which Helm may remove.
    pub retired_objects: Vec<RenderedObject>,
    pub verb: ComponentMutationVerb,
}

/// A preflight result. The executor still has to prove what it actually wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentMutationPlan {
    pub schema_version: String,
    pub requested: BTreeSet<ComponentId>,
    pub expanded: Vec<ComponentId>,
    pub mutations: Vec<ComponentMutation>,
    pub unselected_objects: BTreeSet<ObjectIdentity>,
}
