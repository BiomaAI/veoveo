use std::collections::BTreeSet;

use veoveo_deploy_contract::components::*;
use veoveo_extension_contract::{ArtifactDigest, ExtensionId, ReleaseVersion, SourceRevision};

pub fn id(name: &str) -> ComponentId {
    name.to_owned().try_into().unwrap()
}

pub fn digest(byte: char) -> ArtifactDigest {
    ArtifactDigest::new(format!("sha256:{}", byte.to_string().repeat(64))).unwrap()
}

pub fn fixture(name: &str, role: ComponentRole) -> LockedComponent {
    let source = ComponentSource {
        name: name.into(),
        repository: format!("https://example.invalid/{name}.git"),
        revision: SourceRevision::new("a".repeat(40)).unwrap(),
    };
    let target = AtomicTarget::HelmRelease {
        namespace: "veoveo".into(),
        name: name.into(),
    };
    let object = RenderedObject {
        identity: ObjectIdentity {
            group: "apps".into(),
            kind: "Deployment".into(),
            namespace: Some("veoveo".into()),
            name: name.into(),
        },
        digest: digest('a'),
    };
    let inputs = BTreeSet::from([
        ComponentInput::Image {
            source: source.clone(),
            target: name.into(),
            repository: format!("registry.invalid/{name}"),
            digest: digest('b'),
        },
        ComponentInput::File {
            source: source.clone(),
            path: "values.yaml".into(),
            digest: digest('c'),
        },
        ComponentInput::Chart {
            source: source.clone(),
            coordinate: format!("source://{name}/chart"),
            digest: digest('d'),
        },
    ]);
    let declaration = DeploymentComponent {
        id: id(name),
        role,
        source: source.clone(),
        configuration: InstallationSnapshot {
            source: ComponentSource {
                name: INSTALLATION_SOURCE_NAME.into(),
                repository: "https://example.invalid/installation.git".into(),
                revision: SourceRevision::new("a".repeat(40)).unwrap(),
            },
            profile: "deployment.json".into(),
        },
        namespaces: BTreeSet::from(["veoveo".into()]),
        targets: BTreeSet::from([target.clone()]),
        permitted_objects: BTreeSet::from([object.identity.clone()]),
        inputs: inputs.clone(),
        dependencies: BTreeSet::new(),
        extension_release: (role == ComponentRole::Extension).then(|| ComponentExtensionRelease {
            extension: ExtensionId::new(name).unwrap(),
            version: ReleaseVersion::new("1.0.0").unwrap(),
            manifest_digest: digest('e'),
        }),
    };
    lock_component(
        declaration.clone(),
        vec![PreparedAtomicUnit {
            component: declaration.id,
            source,
            configuration: declaration.configuration.clone(),
            target,
            inputs,
            objects: vec![object],
            tool_scope: AtomicToolScope::Exact,
        }],
    )
    .unwrap()
}

pub fn prepared(component: &LockedComponent) -> Vec<PreparedAtomicUnit> {
    component
        .units
        .iter()
        .map(|unit| PreparedAtomicUnit {
            component: component.declaration.id.clone(),
            source: component.declaration.source.clone(),
            configuration: component.declaration.configuration.clone(),
            target: unit.target.clone(),
            inputs: unit.inputs.clone(),
            objects: unit.objects.clone(),
            tool_scope: AtomicToolScope::Exact,
        })
        .collect()
}

pub fn observed(component: &LockedComponent) -> Vec<ObservedAtomicUnit> {
    component
        .units
        .iter()
        .map(|unit| ObservedAtomicUnit {
            component: component.declaration.id.clone(),
            target: unit.target.clone(),
            state: ObservedUnitState::Present {
                digest: unit.digest.clone(),
                content_digest: unit.content_digest.clone(),
                objects: unit.objects.clone(),
            },
        })
        .collect()
}

pub fn relock(component: &LockedComponent) -> LockedComponent {
    lock_component(component.declaration.clone(), prepared(component)).unwrap()
}

pub fn assert_error<T: std::fmt::Debug>(result: anyhow::Result<T>, expected: &str) {
    let error = format!("{:#}", result.unwrap_err());
    assert!(
        error.contains(expected),
        "expected {expected:?}; got {error:?}"
    );
}
