use std::{collections::BTreeSet, fs, path::Path, process::Command};

use sha2::{Digest, Sha256};
use tempfile::TempDir;
use veoveo_deploy_contract::components::*;
use veoveo_extension_contract::{ArtifactDigest, ReleaseVersion, SourceRevision};

#[path = "support/components.rs"]
mod support;
use support::*;

/// Independent histories exercise the planner's immutable input boundary. These
/// fixtures do not publish images, execute Helm, or attest to Kubernetes writes.
struct Repository(TempDir);

impl Repository {
    fn new(name: &str) -> Self {
        let repository = Self(TempDir::new().unwrap());
        repository.git(&["init", "--quiet"]);
        repository.git(&["config", "user.name", "Veoveo contract test"]);
        repository.git(&["config", "user.email", "contract@example.invalid"]);
        fs::write(repository.path().join("owner"), name).unwrap();
        fs::write(repository.path().join("values.yaml"), "replicas: 1\n").unwrap();
        fs::write(repository.path().join("chart"), "fixture chart contents\n").unwrap();
        repository.commit("initial inputs");
        repository
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(self.path())
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {:?}", output.stderr);
        String::from_utf8(output.stdout).unwrap()
    }

    fn commit(&self, message: &str) -> SourceRevision {
        self.git(&["add", "."]);
        self.git(&[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            message,
        ]);
        SourceRevision::new(self.git(&["rev-parse", "HEAD"]).trim()).unwrap()
    }

    fn source(&self, name: &str) -> ComponentSource {
        ComponentSource {
            name: name.into(),
            repository: self.path().to_str().unwrap().into(),
            revision: SourceRevision::new(self.git(&["rev-parse", "HEAD"]).trim()).unwrap(),
        }
    }

    fn input_digest(&self, source: &ComponentSource, path: &str) -> ArtifactDigest {
        let bytes = self.git(&["show", &format!("{}:{path}", source.revision)]);
        let hex = Sha256::digest(bytes.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        ArtifactDigest::new(format!("sha256:{hex}")).unwrap()
    }

    fn locked(&self, name: &str, role: ComponentRole) -> LockedComponent {
        let component = fixture(name, role);
        self.revise(&component)
    }

    /// Update chart and values provenance to HEAD, retaining the published image's
    /// original revision. Read committed bytes even when the worktree has changed.
    fn revise(&self, previous: &LockedComponent) -> LockedComponent {
        let mut next = previous.clone();
        let source = self.source(next.declaration.id.as_str());
        let first = next.declaration.source.repository != source.repository;
        next.declaration.source = source.clone();
        next.declaration.inputs = next
            .declaration
            .inputs
            .into_iter()
            .map(|input| match input {
                ComponentInput::File { path, .. } => ComponentInput::File {
                    digest: self.input_digest(&source, &path),
                    source: source.clone(),
                    path,
                },
                ComponentInput::Chart { coordinate, .. } => ComponentInput::Chart {
                    digest: self.input_digest(&source, "chart"),
                    source: source.clone(),
                    coordinate,
                },
                ComponentInput::Image {
                    target,
                    repository,
                    digest,
                    source: original,
                } => ComponentInput::Image {
                    source: if first { source.clone() } else { original },
                    target,
                    repository,
                    digest,
                },
            })
            .collect();
        next.units[0].inputs = next.declaration.inputs.clone();
        relock(&next)
    }
}

#[test]
fn independent_git_revisions_reuse_unchanged_artifacts_and_dependencies() {
    let platform_repository = Repository::new("platform");
    let extension_repository = Repository::new("extension");
    let previous_platform = platform_repository.locked("platform", ComponentRole::Platform);
    let previous_extension = extension_repository.locked("extension", ComponentRole::Extension);
    assert_ne!(
        previous_platform.declaration.source.revision,
        previous_extension.declaration.source.revision
    );

    fs::write(
        platform_repository.path().join("README.md"),
        "New documentation\n",
    )
    .unwrap();
    platform_repository.commit("document platform");
    // Mutable working-tree bytes must not enter the lock for the committed revision.
    fs::write(
        platform_repository.path().join("values.yaml"),
        "replicas: 99\n",
    )
    .unwrap();
    let next_platform = platform_repository.revise(&previous_platform);
    assert_ne!(
        next_platform.units[0].digest,
        previous_platform.units[0].digest
    );
    assert_eq!(
        next_platform.units[0].content_digest,
        previous_platform.units[0].content_digest
    );
    let prior_image = previous_platform
        .declaration
        .inputs
        .iter()
        .find(|i| matches!(i, ComponentInput::Image { .. }))
        .unwrap()
        .clone();
    assert!(next_platform.declaration.inputs.contains(&prior_image));

    let plan = component_mutation_plan(
        &[next_platform.clone(), previous_extension.clone()],
        &BTreeSet::from([id("platform")]),
        &prepared(&next_platform),
        &observed(&previous_platform),
    )
    .unwrap();
    assert_eq!(plan.mutations.len(), 1);
    assert_eq!(plan.mutations[0].verb, ComponentMutationVerb::Unchanged);
    assert_eq!(
        plan.unselected_objects,
        previous_extension.declaration.permitted_objects
    );

    fs::write(
        extension_repository.path().join("values.yaml"),
        "replicas: 2\n",
    )
    .unwrap();
    extension_repository.commit("scale extension");
    let mut next_extension = extension_repository.revise(&previous_extension);
    next_extension
        .declaration
        .dependencies
        .insert(id("platform"));
    next_extension.declaration.inputs.insert(prior_image);
    next_extension.units[0].inputs = next_extension.declaration.inputs.clone();
    next_extension.units[0].objects[0].digest = digest('f');
    let next_extension = relock(&next_extension);
    let plan = component_mutation_plan(
        &[next_platform.clone(), next_extension.clone()],
        &BTreeSet::from([id("extension")]),
        &prepared(&next_platform)
            .into_iter()
            .chain(prepared(&next_extension))
            .collect::<Vec<_>>(),
        &observed(&previous_platform)
            .into_iter()
            .chain(observed(&previous_extension))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(plan.expanded, vec![id("platform"), id("extension")]);
    assert_eq!(plan.mutations[0].verb, ComponentMutationVerb::Unchanged);
    assert_eq!(
        plan.mutations[1].verb,
        ComponentMutationVerb::HelmUpgradeInstall
    );

    let mut wrong_checkout = prepared(&next_platform);
    wrong_checkout[0].source = previous_platform.declaration.source.clone();
    assert_error(
        component_mutation_plan(
            &[next_platform],
            &BTreeSet::from([id("platform")]),
            &wrong_checkout,
            &observed(&previous_platform),
        ),
        "source differs",
    );
}

#[test]
fn different_immutable_revisions_may_contain_different_versions_of_a_shared_input() {
    let previous = fixture("platform", ComponentRole::Platform);
    let mut next = previous.clone();
    next.declaration.source.revision = SourceRevision::new("b".repeat(40)).unwrap();
    next.declaration.inputs = next
        .declaration
        .inputs
        .into_iter()
        .map(|mut input| {
            if let ComponentInput::Image {
                source,
                digest: contents,
                ..
            } = &mut input
            {
                *source = next.declaration.source.clone();
                *contents = digest('f');
            }
            input
        })
        .collect();
    next.units[0].inputs = next.declaration.inputs.clone();
    let next = relock(&next);
    let mut extension = fixture("extension", ComponentRole::Extension);
    extension
        .declaration
        .inputs
        .extend(previous.declaration.inputs);
    extension.units[0].inputs = extension.declaration.inputs.clone();
    validate_component_catalog(&[next, relock(&extension)]).unwrap();
}

#[test]
fn extension_release_provenance_alone_does_not_force_an_upgrade() {
    let previous = fixture("extension", ComponentRole::Extension);
    let mut next = previous.clone();
    let release = next.declaration.extension_release.as_mut().unwrap();
    release.version = ReleaseVersion::new("1.0.1").unwrap();
    release.manifest_digest = digest('f');
    let next = relock(&next);
    assert_ne!(next.units[0].digest, previous.units[0].digest);
    let plan = component_mutation_plan(
        std::slice::from_ref(&next),
        &BTreeSet::from([id("extension")]),
        &prepared(&next),
        &observed(&previous),
    )
    .unwrap();
    assert_eq!(plan.mutations[0].verb, ComponentMutationVerb::Unchanged);
}

#[test]
fn forged_content_identity_and_inconsistent_observations_are_rejected() {
    let platform = fixture("platform", ComponentRole::Platform);
    let mut forged = platform.clone();
    forged.units[0].content_digest = digest('f');
    assert_error(
        validate_component_catalog(&[forged]),
        "content digest differs",
    );
    let mut observations = observed(&platform);
    let ObservedUnitState::Present { content_digest, .. } = &mut observations[0].state else {
        unreachable!()
    };
    *content_digest = digest('f');
    assert_error(
        component_mutation_plan(
            std::slice::from_ref(&platform),
            &BTreeSet::from([id("platform")]),
            &prepared(&platform),
            &observations,
        ),
        "installed provenance matches but content digest differs",
    );
}

#[test]
fn every_changed_input_forces_an_upgrade_even_with_identical_rendered_objects() {
    let previous = fixture("platform", ComponentRole::Platform);
    for changed in &previous.declaration.inputs {
        let mut next = previous.clone();
        next.declaration.inputs.remove(changed);
        let mut replacement = changed.clone();
        match &mut replacement {
            ComponentInput::Image {
                digest: contents, ..
            }
            | ComponentInput::File {
                digest: contents, ..
            }
            | ComponentInput::Chart {
                digest: contents, ..
            } => *contents = digest('f'),
        }
        next.declaration.inputs.insert(replacement);
        next.units[0].inputs = next.declaration.inputs.clone();
        let next = relock(&next);
        let plan = component_mutation_plan(
            std::slice::from_ref(&next),
            &BTreeSet::from([id("platform")]),
            &prepared(&next),
            &observed(&previous),
        )
        .unwrap();
        assert_eq!(
            plan.mutations[0].verb,
            ComponentMutationVerb::HelmUpgradeInstall,
            "{changed:?}"
        );
    }
}
