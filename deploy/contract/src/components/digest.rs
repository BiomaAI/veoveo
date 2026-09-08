use std::collections::BTreeSet;

use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};
use veoveo_extension_contract::{ArtifactDigest, ExtensionId};

use super::{
    types::*,
    validation::{validate_declaration, validate_prepared},
};

/// Binds the exact immutable provenance and complete contents of a prepared unit.
/// This encoding is typed; it does not canonicalize arbitrary Kubernetes JSON.
pub fn atomic_unit_digest(
    component: &DeploymentComponent,
    unit: &PreparedAtomicUnit,
) -> Result<ArtifactDigest> {
    validate_declaration(component)?;
    validate_prepared(component, unit)?;
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Identity<'a> {
        format: &'static str,
        component: &'a ComponentId,
        role: ComponentRole,
        source: &'a ComponentSource,
        configuration: &'a InstallationSnapshot,
        extension_release: &'a Option<ComponentExtensionRelease>,
        target: &'a AtomicTarget,
        inputs: &'a BTreeSet<ComponentInput>,
        objects: Vec<&'a RenderedObject>,
    }
    hash(&Identity {
        format: "veoveo.io/atomic-deployment-unit/v2",
        component: &component.id,
        role: component.role,
        source: &component.source,
        configuration: &component.configuration,
        extension_release: &component.extension_release,
        target: &unit.target,
        inputs: &unit.inputs,
        objects: sorted_objects(unit),
    })
}

/// Identifies deployable contents independently of source revision provenance.
/// A new commit can reuse unchanged inputs without requiring a Helm revision.
/// Callers must still validate the exact provenance digest against the desired lock.
pub fn atomic_unit_content_digest(
    component: &DeploymentComponent,
    unit: &PreparedAtomicUnit,
) -> Result<ArtifactDigest> {
    validate_declaration(component)?;
    validate_prepared(component, unit)?;
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Identity<'a> {
        format: &'static str,
        component: &'a ComponentId,
        role: ComponentRole,
        source: ContentSource<'a>,
        configuration: (ContentSource<'a>, &'a str),
        extension: Option<&'a ExtensionId>,
        target: &'a AtomicTarget,
        inputs: BTreeSet<ContentInput<'a>>,
        objects: Vec<&'a RenderedObject>,
    }
    hash(&Identity {
        format: "veoveo.io/atomic-deployment-content/v2",
        component: &component.id,
        role: component.role,
        source: (&component.source).into(),
        configuration: (
            (&component.configuration.source).into(),
            &component.configuration.profile,
        ),
        extension: component.extension_release.as_ref().map(|r| &r.extension),
        target: &unit.target,
        inputs: unit.inputs.iter().map(ContentInput::from).collect(),
        objects: sorted_objects(unit),
    })
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct ContentSource<'a> {
    name: &'a str,
    repository: &'a str,
}

impl<'a> From<&'a ComponentSource> for ContentSource<'a> {
    fn from(source: &'a ComponentSource) -> Self {
        Self {
            name: &source.name,
            repository: &source.repository,
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum ContentInput<'a> {
    Image {
        source: ContentSource<'a>,
        target: &'a str,
        repository: &'a str,
        digest: &'a ArtifactDigest,
    },
    File {
        source: ContentSource<'a>,
        path: &'a str,
        digest: &'a ArtifactDigest,
    },
    Chart {
        source: ContentSource<'a>,
        coordinate: &'a str,
        digest: &'a ArtifactDigest,
    },
}

impl<'a> From<&'a ComponentInput> for ContentInput<'a> {
    fn from(input: &'a ComponentInput) -> Self {
        match input {
            ComponentInput::Image {
                source,
                target,
                repository,
                digest,
            } => Self::Image {
                source: source.into(),
                target,
                repository,
                digest,
            },
            ComponentInput::File {
                source,
                path,
                digest,
            } => Self::File {
                source: source.into(),
                path,
                digest,
            },
            ComponentInput::Chart {
                source,
                coordinate,
                digest,
            } => Self::Chart {
                source: source.into(),
                coordinate,
                digest,
            },
        }
    }
}

fn sorted_objects(unit: &PreparedAtomicUnit) -> Vec<&RenderedObject> {
    let mut objects = unit.objects.iter().collect::<Vec<_>>();
    objects.sort_by(|left, right| left.identity.cmp(&right.identity));
    objects
}

fn hash(value: &impl Serialize) -> Result<ArtifactDigest> {
    let hex = Sha256::digest(serde_json::to_vec(value)?)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(ArtifactDigest::new(format!("sha256:{hex}"))?)
}
