//! Synthetic schema inputs only. These objects and artifact digests are not
//! publication receipts, compiled chart output, or installation evidence.

use std::collections::BTreeSet;

use veoveo_deploy_contract::{DeploymentSourceRole, LockedSource, components::*};
use veoveo_extension_contract::{ArtifactDigest, ExtensionId, ReleaseVersion, SourceRevision};

pub fn synthetic_catalog(
    sources: &[LockedSource],
    installation_revision: &str,
) -> Vec<LockedComponent> {
    let installation = ComponentSource {
        name: INSTALLATION_SOURCE_NAME.into(),
        repository: "https://example.invalid/installation.git".into(),
        revision: SourceRevision::new(installation_revision).unwrap(),
    };
    let namespace = ObjectIdentity {
        group: String::new(),
        kind: "Namespace".into(),
        namespace: None,
        name: "veoveo".into(),
    };
    let target = AtomicTarget::ManifestSet {
        name: "installation-namespace".into(),
    };
    let mut catalog = vec![synthetic_component(
        installation.clone(),
        &installation,
        ComponentRole::Installation,
        vec![(target, BTreeSet::new(), namespace)],
    )];
    for source in sources {
        let owner = ComponentSource {
            name: source.name.clone(),
            repository: source.repository.clone(),
            revision: SourceRevision::new(&source.revision).unwrap(),
        };
        let images = source
            .images
            .iter()
            .map(|image| ComponentInput::Image {
                source: ComponentSource {
                    revision: image.source_revision.clone(),
                    ..owner.clone()
                },
                target: image.name.clone(),
                repository: image.repository.clone(),
                digest: ArtifactDigest::new(&image.digest).unwrap(),
            })
            .collect::<BTreeSet<_>>();
        let units = source
            .charts
            .iter()
            .map(|chart| {
                let mut inputs = images.clone();
                inputs.insert(ComponentInput::Chart {
                    source: owner.clone(),
                    coordinate: chart.coordinate.clone(),
                    digest: ArtifactDigest::new(&chart.digest).unwrap(),
                });
                (
                    AtomicTarget::HelmRelease {
                        namespace: "veoveo".into(),
                        name: chart.release.clone(),
                    },
                    inputs,
                    ObjectIdentity {
                        group: "apps".into(),
                        kind: "Deployment".into(),
                        namespace: Some("veoveo".into()),
                        name: chart.release.clone(),
                    },
                )
            })
            .collect();
        let role = match source.role {
            DeploymentSourceRole::Platform => ComponentRole::Platform,
            DeploymentSourceRole::Workload => ComponentRole::Workload,
            DeploymentSourceRole::Extension => ComponentRole::Extension,
        };
        catalog.push(synthetic_component(owner, &installation, role, units));
    }
    catalog
}

fn synthetic_component(
    source: ComponentSource,
    installation: &ComponentSource,
    role: ComponentRole,
    units: Vec<(AtomicTarget, BTreeSet<ComponentInput>, ObjectIdentity)>,
) -> LockedComponent {
    let id: ComponentId = source.name.clone().try_into().unwrap();
    let declaration = DeploymentComponent {
        id: id.clone(),
        role,
        source: source.clone(),
        configuration: InstallationSnapshot {
            source: installation.clone(),
            profile: "deployment.json".into(),
        },
        namespaces: BTreeSet::from(["veoveo".into()]),
        targets: units.iter().map(|(target, _, _)| target.clone()).collect(),
        permitted_objects: units.iter().map(|(_, _, object)| object.clone()).collect(),
        inputs: units
            .iter()
            .flat_map(|(_, inputs, _)| inputs.iter().cloned())
            .collect(),
        dependencies: if role == ComponentRole::Installation {
            BTreeSet::new()
        } else {
            BTreeSet::from([INSTALLATION_SOURCE_NAME.to_owned().try_into().unwrap()])
        },
        extension_release: (role == ComponentRole::Extension).then(|| ComponentExtensionRelease {
            extension: ExtensionId::new("fixture.example").unwrap(),
            version: ReleaseVersion::new("1.0.0").unwrap(),
            manifest_digest: ArtifactDigest::new(format!("sha256:{}", "e".repeat(64))).unwrap(),
        }),
    };
    let prepared = units
        .into_iter()
        .map(|(target, inputs, identity)| PreparedAtomicUnit {
            component: id.clone(),
            source: source.clone(),
            configuration: declaration.configuration.clone(),
            target,
            inputs,
            objects: vec![RenderedObject {
                identity,
                digest: ArtifactDigest::new(format!("sha256:{}", "f".repeat(64))).unwrap(),
            }],
            tool_scope: AtomicToolScope::Exact,
        })
        .collect();
    lock_component(declaration, prepared).unwrap()
}
