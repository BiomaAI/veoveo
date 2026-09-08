use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::json;
use veoveo_deploy_contract::{LoadedProfile, LockedImage, LockedSource};

use super::{compile_component_lock, compile_components, compile_locked_components};
use crate::charts::lock_source_charts;

mod publication;

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn initialize(root: &Path) {
    fs::create_dir_all(root).unwrap();
    git(root, &["init", "--quiet"]);
    git(root, &["config", "user.name", "Component Compiler Test"]);
    git(root, &["config", "user.email", "compiler@example.invalid"]);
    git(
        root,
        &[
            "config",
            "remote.origin.url",
            &format!("file://{}", root.display()),
        ],
    );
}

fn commit(root: &Path, message: &str) -> String {
    git(root, &["add", "."]);
    git(
        root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            message,
        ],
    );
    git(root, &["rev-parse", "HEAD"])
}

fn source(root: &Path, name: &str) -> String {
    initialize(root);
    fs::create_dir_all(root.join("chart/templates")).unwrap();
    fs::write(
        root.join("chart/Chart.yaml"),
        format!("apiVersion: v2\nname: {name}\nversion: 1.0.0\n"),
    )
    .unwrap();
    let (registry, digests) = if name == "platform" {
        (
            ".Values.global.veoveoRegistry",
            ".Values.global.imageDigests",
        )
    } else {
        (".Values.veoveo.registry", ".Values.veoveo.imageDigests")
    };
    let target = image_target(name);
    fs::write(
        root.join("chart/templates/workload.yaml"),
        format!(
            r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{{{ .Release.Name }}}}
  annotations:
    test.example/selectedImages: "{{{{ len {digests} }}}}"
    test.example/setting: "{{{{ .Values.fixtureSetting | default "initial" }}}}"
spec:
  selector:
    matchLabels:
      app: {name}
  template:
    metadata:
      labels:
        app: {name}
    spec:
      containers:
        - name: service
          image: "{{{{ {registry} }}}}/{target}@{{{{ index {digests} "{target}" }}}}"
"#
        ),
    )
    .unwrap();
    commit(root, name)
}

fn image_target(source: &str) -> &str {
    if source == "platform" {
        "artifact-service"
    } else {
        source
    }
}

fn publish_images(
    profile: &LoadedProfile,
    base: &veoveo_deploy_contract::DeploymentLock,
    requested: &BTreeSet<veoveo_deploy_contract::components::ComponentId>,
    images: &BTreeMap<String, Vec<LockedImage>>,
) -> anyhow::Result<veoveo_deploy_contract::DeploymentLock> {
    crate::update_components(
        profile,
        base,
        requested,
        &crate::ComponentUpdates {
            images: images.clone(),
            ..Default::default()
        },
    )
}

fn verify_image_publication(
    profile: &LoadedProfile,
    base: &veoveo_deploy_contract::DeploymentLock,
    roots: &BTreeMap<String, PathBuf>,
) {
    use veoveo_deploy_contract::components::{ComponentId, ComponentInput};
    use veoveo_extension_contract::SourceRevision;
    let mut lock = base.clone();
    for (owner, other) in [("platform", "extension"), ("extension", "platform")] {
        let source = lock
            .sources
            .iter()
            .find(|source| source.name == owner)
            .unwrap();
        let mut image = source.images[0].clone();
        fs::write(
            roots[owner].join("implementation.txt"),
            "new image implementation",
        )
        .unwrap();
        let revision = commit(&roots[owner], "new image build input");
        image.source_revision = SourceRevision::new(revision).unwrap();
        image.digest = format!("sha256:{}", "d".repeat(64));
        image.publication_digest = format!("sha256:{}", "e".repeat(64));
        let requested = BTreeSet::from([ComponentId::try_from(owner.to_owned()).unwrap()]);
        let updates = BTreeMap::from([(owner.to_owned(), vec![image.clone()])]);
        let hidden = roots[other].with_extension("unavailable");
        fs::rename(&roots[other], &hidden).unwrap();
        let updated = publish_images(profile, &lock, &requested, &updates).unwrap();
        fs::rename(&hidden, &roots[other]).unwrap();
        for previous in &lock.components {
            let current = updated
                .components
                .iter()
                .find(|component| component.declaration.id == previous.declaration.id)
                .unwrap();
            assert_fixed_inputs(&current.declaration, &previous.declaration);
            if previous.declaration.id.as_str() == owner {
                assert_ne!(
                    current.units[0].content_digest,
                    previous.units[0].content_digest
                );
                assert!(current.units[0].inputs.iter().any(|input| matches!(input,
                    ComponentInput::Image { source, digest, .. } if source.revision == image.source_revision && digest.as_str() == image.digest)));
            } else {
                assert_eq!(
                    current, previous,
                    "unrequested owners including dependencies must be retained verbatim"
                );
            }
        }
        assert_eq!(updated.profile_revision, lock.profile_revision);
        assert_eq!(updated.platform, lock.platform);
        let all = updated
            .components
            .iter()
            .map(|component| component.declaration.id.clone())
            .collect();
        let snapshots = crate::sources::resolve_locked_sources(profile, &updated, &all).unwrap();
        let exact_roots = snapshots
            .iter()
            .map(|(identity, source)| (identity.clone(), source.repository.clone()))
            .collect();
        let prepared = compile_locked_components(profile, &updated, &exact_roots, &all).unwrap();
        assert_eq!(
            prepared
                .into_iter()
                .map(|component| component.locked)
                .collect::<Vec<_>>(),
            updated.components
        );
        // Reusing the exact qualification is deterministic; no duplicate catalog version.
        assert_eq!(
            publish_images(profile, &updated, &requested, &updates).unwrap(),
            updated
        );
        // A component cannot import an image outside its recorded consumer closure.
        let unrelated = BTreeSet::from([ComponentId::try_from(other.to_owned()).unwrap()]);
        assert!(
            publish_images(profile, &updated, &unrelated, &updates)
                .unwrap_err()
                .to_string()
                .contains("not consumed")
        );
        let mut conflicting = updates.clone();
        conflicting.get_mut(owner).unwrap()[0].publication_digest =
            format!("sha256:{}", "f".repeat(64));
        assert!(
            publish_images(profile, &updated, &requested, &conflicting)
                .unwrap_err()
                .to_string()
                .contains("already qualified")
        );
        let mut duplicate = updates.clone();
        duplicate.get_mut(owner).unwrap().push(image.clone());
        assert!(
            publish_images(profile, &updated, &requested, &duplicate)
                .unwrap_err()
                .to_string()
                .contains("duplicate image")
        );
        let mut foreign = updates.clone();
        foreign.get_mut(owner).unwrap()[0]
            .repository
            .push_str("-different");
        assert!(
            publish_images(profile, &updated, &requested, &foreign)
                .unwrap_err()
                .to_string()
                .contains("repository ownership")
        );
        lock = updated;
    }
}

fn assert_fixed_inputs(
    current: &veoveo_deploy_contract::components::DeploymentComponent,
    previous: &veoveo_deploy_contract::components::DeploymentComponent,
) {
    use veoveo_deploy_contract::components::ComponentInput;
    let mut current = current.clone();
    let mut previous = previous.clone();
    current
        .inputs
        .retain(|input| !matches!(input, ComponentInput::Image { .. }));
    previous
        .inputs
        .retain(|input| !matches!(input, ComponentInput::Image { .. }));
    assert_eq!(
        current, previous,
        "every non-image declaration field must stay fixed"
    );
}

#[test]
fn selected_compilation_reuses_unselected_inventory_without_its_checkout_or_render() {
    independent_sources(None);
}

#[test]
fn image_publication_updates_independent_sources_in_both_directions() {
    independent_sources(Some(verify_image_publication));
}

type PublicationCheck =
    fn(&LoadedProfile, &veoveo_deploy_contract::DeploymentLock, &BTreeMap<String, PathBuf>);

fn independent_sources(publication: Option<PublicationCheck>) {
    // Image digests here are synthetic compiler inputs. This test establishes
    // render selection and provenance, not image publication or live zero writes.
    let workspace = tempfile::tempdir().unwrap();
    let platform = workspace.path().join("platform");
    let extension = workspace.path().join("extension");
    let installation = workspace.path().join("installation");
    let platform_revision = source(&platform, "platform");
    let extension_revision = source(&extension, "extension");
    assert_ne!(platform_revision, extension_revision);
    initialize(&installation);
    let namespace = json!({"group":"", "kind":"Namespace", "namespace":null, "name":"veoveo"});
    let profile_value = json!({
        "schemaVersion":"veoveo.io/deployment/v7", "name":"compiler-fixture",
        "registry":{"pushAddress":"registry.example.invalid", "pullAddress":"registry.example.invalid", "transport":"tls"},
        "sources":[
            {"name":"platform", "role":"platform", "repository":{"kind":"local", "path":"../platform"},
                "revision":"HEAD", "imageGroups":[], "releases":[{"name":"platform", "chart":"chart", "sourceValues":[], "installationValues":[], "valuesContract":"platform", "timeoutSeconds":60}]},
            {"name":"extension", "role":"extension", "repository":{"kind":"local", "path":"../extension"},
                "revision":"HEAD", "imageGroups":["extension"], "releases":[{"name":"extension", "chart":"chart", "sourceValues":[], "installationValues":[], "valuesContract":"extension", "timeoutSeconds":60}]}
        ],
        "components":[
            {"id":"installation", "owner":{"kind":"installation"}, "role":"installation", "dependencies":[], "namespaces":["veoveo"], "clusterObjects":[namespace], "releases":[], "installationInputs":["namespace"], "extensionRelease":null},
            {"id":"platform", "owner":{"kind":"source", "name":"platform"}, "role":"platform", "dependencies":["installation"], "namespaces":["veoveo"], "clusterObjects":[], "releases":["platform"], "installationInputs":[], "extensionRelease":null},
            {"id":"extension", "owner":{"kind":"source", "name":"extension"}, "role":"extension", "dependencies":["installation"], "namespaces":["veoveo"], "clusterObjects":[], "releases":["extension"], "installationInputs":[],
                "extensionRelease":{"extension":"compiler.example", "version":"1.0.0", "manifestDigest":format!("sha256:{}", "c".repeat(64))}}
        ],
        "kubernetes":{"context":"must-not-contact-a-cluster", "localCluster":null},
        "namespace":"veoveo", "resources":{"manifests":[], "configMaps":[]},
        "platform":{"installationPreset":"custom", "components":["platform-store", "object-store", "artifact-service"], "mcpServers":[], "artifactAudiences":[], "externalWorkloads":[]},
        "gatewayRequirements":[], "waitForDeployments":[]
    });
    let path = installation.join("deployment.json");
    fs::write(&path, serde_json::to_vec_pretty(&profile_value).unwrap()).unwrap();
    let profile_revision = commit(&installation, "installation");
    let profile = LoadedProfile::load(&path, &installation).unwrap();
    let roots = BTreeMap::from([
        ("platform".into(), platform.clone()),
        ("extension".into(), extension.clone()),
    ]);
    let mut sources = profile
        .definition
        .sources
        .iter()
        .map(|source| LockedSource {
            name: source.name.clone(),
            role: source.role,
            repository: format!("file://{}", roots[&source.name].display()),
            revision: git(&roots[&source.name], &["rev-parse", "HEAD"]),
            images: vec![LockedImage {
                name: image_target(&source.name).into(),
                repository: format!("registry.example.invalid/{}", image_target(&source.name)),
                source_revision: veoveo_extension_contract::SourceRevision::new(git(
                    &roots[&source.name],
                    &["rev-parse", "HEAD"],
                ))
                .unwrap(),
                digest: format!("sha256:{}", "a".repeat(64)),
                publication_digest: format!("sha256:{}", "b".repeat(64)),
            }],
            charts: lock_source_charts(source, &roots[&source.name]).unwrap(),
        })
        .collect::<Vec<_>>();
    let initial = compile_component_lock(&profile, &profile_revision, &sources, &roots).unwrap();
    let old_platform = initial
        .iter()
        .find(|component| component.declaration.id.as_str() == "platform")
        .unwrap();
    let fresh = veoveo_deploy_contract::DeploymentLock {
        schema_version: veoveo_deploy_contract::DEPLOYMENT_LOCK_SCHEMA.into(),
        profile: profile.definition.name.clone(),
        profile_revision: profile_revision.clone(),
        registry: profile.definition.registry.locked(),
        sources: sources.clone(),
        components: initial.clone(),
        platform: profile.resolved_platform().unwrap(),
    };
    if let Some(verify) = publication {
        verify(&profile, &fresh, &roots);
        return;
    }
    let exact_roots = initial
        .iter()
        .filter_map(|component| {
            let owner = &component.declaration.source;
            roots
                .get(&owner.name)
                .map(|root| (owner.clone(), root.clone()))
        })
        .collect();
    let all = initial
        .iter()
        .map(|component| component.declaration.id.clone())
        .collect();
    let prepared = compile_locked_components(&profile, &fresh, &exact_roots, &all).unwrap();
    assert_eq!(
        prepared
            .iter()
            .map(|component| component.locked.clone())
            .collect::<Vec<_>>(),
        initial
    );
    let extension_render = prepared
        .iter()
        .find(|component| component.locked.declaration.id.as_str() == "extension")
        .unwrap();
    assert_eq!(
        extension_render.units[0].objects[0]["metadata"]["annotations"]["test.example/selectedImages"],
        "1"
    );
    let requested = BTreeSet::from(["platform".to_owned().try_into().unwrap()]);
    let selected = veoveo_deploy_contract::components::select_components(&initial, &requested)
        .unwrap()
        .into_iter()
        .collect();

    fs::write(
        platform.join("source-change.txt"),
        "new platform implementation\n",
    )
    .unwrap();
    sources[0].revision = commit(&platform, "platform implementation update");
    // An unselected chart would fail if rendered, and its checkout is omitted.
    fs::write(
        extension.join("chart/templates/workload.yaml"),
        "{{ fail \"unselected chart was evaluated\" }}",
    )
    .unwrap();
    let selected_roots = BTreeMap::<String, PathBuf>::from([("platform".into(), platform)]);
    let retained = compile_components(
        &profile,
        &profile_revision,
        &sources,
        &selected_roots,
        &selected,
    )
    .unwrap();
    let retained_platform = retained
        .iter()
        .find(|component| component.locked.declaration.id.as_str() == "platform")
        .unwrap();
    assert_ne!(
        old_platform.units[0].digest, retained_platform.locked.units[0].digest,
        "chart provenance advances with its source snapshot"
    );
    assert_eq!(
        old_platform.units[0].content_digest, retained_platform.locked.units[0].content_digest,
        "identical deployable contents keep their content identity"
    );
    let retained_image = retained_platform.locked.units[0]
        .inputs
        .iter()
        .find_map(|input| match input {
            veoveo_deploy_contract::components::ComponentInput::Image { source, .. } => {
                Some(source)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(retained_image.revision.as_str(), platform_revision);
    assert_ne!(retained_image.revision.as_str(), sources[0].revision);

    sources[0].images[0].source_revision =
        veoveo_extension_contract::SourceRevision::new(&sources[0].revision).unwrap();
    sources[0].images[0].digest = format!("sha256:{}", "d".repeat(64));
    let compiled = compile_components(
        &profile,
        &profile_revision,
        &sources,
        &selected_roots,
        &selected,
    )
    .unwrap();
    assert_eq!(compiled.len(), 2);
    assert!(
        compiled
            .iter()
            .all(|component| component.locked.declaration.id.as_str() != "extension")
    );
    let new_platform = compiled
        .iter()
        .find(|component| component.locked.declaration.id.as_str() == "platform")
        .unwrap();
    assert_ne!(
        old_platform.units[0].content_digest,
        new_platform.locked.units[0].content_digest
    );
    let mut merged = initial.clone();
    for component in compiled {
        let previous = merged
            .iter_mut()
            .find(|previous| previous.declaration.id == component.locked.declaration.id)
            .unwrap();
        *previous = component.locked;
    }
    veoveo_deploy_contract::components::validate_component_catalog(&merged).unwrap();
    assert_eq!(
        merged
            .iter()
            .find(|component| component.declaration.id.as_str() == "extension"),
        initial
            .iter()
            .find(|component| component.declaration.id.as_str() == "extension")
    );
    let lock = veoveo_deploy_contract::DeploymentLock {
        schema_version: veoveo_deploy_contract::DEPLOYMENT_LOCK_SCHEMA.into(),
        profile: profile.definition.name.clone(),
        profile_revision,
        registry: profile.definition.registry.locked(),
        sources,
        components: merged,
        platform: profile.resolved_platform().unwrap(),
    };
    lock.validate().unwrap();
    veoveo_deploy_contract::components::validate_profile_component_bindings(
        &profile.definition,
        &lock,
    )
    .unwrap();
    crate::images::validate_locked_images(&profile, &lock).unwrap();

    // Selected files come from the locked Git snapshot even when the mutable
    // checkout is incomplete. No part of the unselected repository remains.
    fs::remove_dir_all(selected_roots["platform"].join("chart")).unwrap();
    fs::remove_dir_all(&extension).unwrap();
    let loaded = LoadedProfile::load(&path, &installation).unwrap();
    crate::sources::validate_locked_profile(&loaded, &lock).unwrap();
    let resolved = crate::sources::resolve_locked_sources(&loaded, &lock, &selected).unwrap();
    assert_eq!(resolved.len(), 1);
    let platform_snapshot = resolved.values().next().unwrap();
    assert_eq!(platform_snapshot.definition.name, "platform");
    assert_ne!(platform_snapshot.repository, selected_roots["platform"]);
    let immutable_roots = resolved
        .iter()
        .map(|(identity, source)| (identity.clone(), source.repository.clone()))
        .collect();
    let prepared = compile_locked_components(&loaded, &lock, &immutable_roots, &selected).unwrap();
    for component in prepared {
        assert_eq!(
            Some(&component.locked),
            lock.components
                .iter()
                .find(|locked| locked.declaration.id == component.locked.declaration.id)
        );
    }
    let extension_selection = BTreeSet::from([
        "installation".to_owned().try_into().unwrap(),
        "extension".to_owned().try_into().unwrap(),
    ]);
    assert!(crate::sources::resolve_locked_sources(&loaded, &lock, &extension_selection).is_err());
}

#[test]
fn components_from_one_source_retain_distinct_chart_revisions() {
    retained_component_inputs(InputChange::Chart);
}

#[test]
fn component_image_update_retains_another_consumers_previous_digest() {
    retained_component_inputs(InputChange::Image);
}

#[test]
fn identical_image_bytes_retain_each_consumers_exact_build_provenance() {
    retained_component_inputs(InputChange::ImageProvenance);
}

#[test]
fn retained_configuration_restores_values_removed_from_the_current_installation() {
    retained_component_inputs(InputChange::Configuration);
}

#[derive(Clone, Copy)]
enum InputChange {
    Chart,
    Image,
    ImageProvenance,
    Configuration,
}

fn retained_component_inputs(change: InputChange) {
    use veoveo_deploy_contract::{DeploymentLock, components::ComponentId};
    use veoveo_extension_contract::SourceRevision;

    // Synthetic image identities isolate real Git/Helm preparation from publication.
    let workspace = tempfile::tempdir().unwrap();
    let platform = workspace.path().join("platform");
    let old_revision = source(&platform, "platform");
    let installation = workspace.path().join("installation");
    initialize(&installation);
    let configuration_change = matches!(change, InputChange::Configuration);
    if configuration_change {
        fs::write(
            installation.join("values-old.yaml"),
            "fixtureSetting: old\n",
        )
        .unwrap();
    }
    let releases = ["current", "retained"].map(|name| {
        json!({
            "name":name, "chart":"chart", "sourceValues":[], "installationValues":if configuration_change {vec!["values-old.yaml"]} else {vec![]},
            "valuesContract":"platform", "timeoutSeconds":60
        })
    });
    let mut components = vec![json!({
        "id":"installation", "owner":{"kind":"installation"}, "role":"installation",
        "dependencies":[], "namespaces":["veoveo"],
        "clusterObjects":[{"group":"", "kind":"Namespace", "namespace":null, "name":"veoveo"}],
        "releases":[], "installationInputs":["namespace"], "extensionRelease":null
    })];
    for name in ["current", "retained"] {
        components.push(json!({
            "id":name, "owner":{"kind":"source", "name":"platform"}, "role":"platform",
            "dependencies":["installation"], "namespaces":["veoveo"], "clusterObjects":[],
            "releases":[name], "installationInputs":[], "extensionRelease":null
        }));
    }
    let path = installation.join("deployment.json");
    fs::write(&path, serde_json::to_vec_pretty(&json!({
        "schemaVersion":"veoveo.io/deployment/v7", "name":"revision-fixture",
        "registry":{"pushAddress":"registry.example.invalid", "pullAddress":"registry.example.invalid", "transport":"tls"},
        "sources":[{"name":"platform", "role":"platform", "repository":{"kind":"local", "path":"../platform"},
            "revision":"HEAD", "imageGroups":[], "releases":releases}],
        "components":components,
        "kubernetes":{"context":"must-not-contact-a-cluster", "localCluster":null},
        "namespace":"veoveo", "resources":{"manifests":[], "configMaps":[]},
        "platform":{"installationPreset":"custom", "components":["platform-store", "object-store", "artifact-service"],
            "mcpServers":[], "artifactAudiences":[], "externalWorkloads":[]},
        "gatewayRequirements":[], "waitForDeployments":[]
    })).unwrap()).unwrap();
    let mut profile_revision = commit(&installation, "two components from one repository");
    let mut profile = LoadedProfile::load(&path, &installation).unwrap();
    let definition = profile.definition.sources[0].clone();
    let mut sources = vec![LockedSource {
        name: "platform".into(),
        role: definition.role,
        repository: format!("file://{}", platform.display()),
        revision: old_revision.clone(),
        images: vec![LockedImage {
            name: "artifact-service".into(),
            repository: "registry.example.invalid/artifact-service".into(),
            source_revision: SourceRevision::new(&old_revision).unwrap(),
            digest: format!("sha256:{}", "a".repeat(64)),
            publication_digest: format!("sha256:{}", "b".repeat(64)),
        }],
        charts: lock_source_charts(&definition, &platform).unwrap(),
    }];
    let roots = BTreeMap::from([("platform".into(), platform.clone())]);
    let initial = compile_component_lock(&profile, &profile_revision, &sources, &roots).unwrap();
    let publication_base = DeploymentLock {
        schema_version: veoveo_deploy_contract::DEPLOYMENT_LOCK_SCHEMA.into(),
        profile: profile.definition.name.clone(),
        profile_revision: profile_revision.clone(),
        registry: profile.definition.registry.locked(),
        sources: sources.clone(),
        components: initial.clone(),
        platform: profile.resolved_platform().unwrap(),
    };
    if configuration_change {
        fs::remove_file(installation.join("values-old.yaml")).unwrap();
        fs::write(
            installation.join("values-current.yaml"),
            "fixtureSetting: new\n",
        )
        .unwrap();
        for release in &mut profile.definition.sources[0].releases {
            release.installation_values = vec!["values-current.yaml".into()];
        }
        fs::write(
            &path,
            serde_json::to_vec_pretty(&profile.definition).unwrap(),
        )
        .unwrap();
        profile_revision = commit(
            &installation,
            "replace the current installation values file",
        );
        profile = LoadedProfile::load(&path, &installation).unwrap();
    }
    let previous_image = sources[0].images[0].clone();
    if matches!(change, InputChange::Chart) {
        let template = platform.join("chart/templates/workload.yaml");
        let content = fs::read_to_string(&template).unwrap().replace(
            "test.example/selectedImages:",
            "test.example/revision: updated\n    test.example/selectedImages:",
        );
        fs::write(template, content).unwrap();
    } else {
        fs::write(
            platform.join("image-source.txt"),
            "new image build snapshot",
        )
        .unwrap();
    }
    let new_revision = commit(&platform, "advance only the current component input");
    sources[0].revision = new_revision.clone();
    if matches!(change, InputChange::Image | InputChange::ImageProvenance) {
        let image = &mut sources[0].images[0];
        image.source_revision = SourceRevision::new(&new_revision).unwrap();
        image.publication_digest = format!("sha256:{}", "d".repeat(64));
        if matches!(change, InputChange::Image) {
            image.digest = format!("sha256:{}", "c".repeat(64));
        }
    }
    let updated_chart = lock_source_charts(&definition, &platform)
        .unwrap()
        .into_iter()
        .find(|chart| chart.release == "current")
        .unwrap();
    *sources[0]
        .charts
        .iter_mut()
        .find(|chart| chart.release == "current")
        .unwrap() = updated_chart;
    let selection = BTreeSet::from([
        ComponentId::try_from("current".to_owned()).unwrap(),
        ComponentId::try_from("installation".to_owned()).unwrap(),
    ]);
    let updated =
        compile_components(&profile, &profile_revision, &sources, &roots, &selection).unwrap();
    if matches!(change, InputChange::Image | InputChange::ImageProvenance) {
        sources[0].images.push(previous_image);
    }
    let mut catalog = initial.clone();
    for component in updated {
        let previous = catalog
            .iter_mut()
            .find(|locked| locked.declaration.id == component.locked.declaration.id)
            .unwrap();
        *previous = component.locked;
    }
    assert_eq!(
        catalog
            .iter()
            .find(|component| component.declaration.id.as_str() == "retained"),
        initial
            .iter()
            .find(|component| component.declaration.id.as_str() == "retained")
    );
    let lock = DeploymentLock {
        schema_version: veoveo_deploy_contract::DEPLOYMENT_LOCK_SCHEMA.into(),
        profile: profile.definition.name.clone(),
        profile_revision,
        registry: profile.definition.registry.locked(),
        sources,
        components: catalog,
        platform: profile.resolved_platform().unwrap(),
    };
    lock.validate().unwrap();
    crate::images::validate_locked_images(&profile, &lock).unwrap();
    crate::sources::validate_locked_profile(&profile, &lock).unwrap();
    if matches!(change, InputChange::Chart | InputChange::Configuration) {
        let requested = BTreeSet::from([ComponentId::try_from("current".to_owned()).unwrap()]);
        let mut updates = crate::ComponentUpdates {
            refresh_configuration: configuration_change,
            ..Default::default()
        };
        if matches!(change, InputChange::Chart) {
            updates.source_revisions.insert(
                "platform".into(),
                SourceRevision::new(&new_revision).unwrap(),
            );
        }
        let composed =
            crate::update_components(&profile, &publication_base, &requested, &updates).unwrap();
        for previous in &publication_base.components {
            let current = composed
                .components
                .iter()
                .find(|component| component.declaration.id == previous.declaration.id)
                .unwrap();
            if previous.declaration.id.as_str() == "current" {
                let expected = lock
                    .components
                    .iter()
                    .find(|component| component.declaration.id == previous.declaration.id)
                    .unwrap();
                assert_eq!(
                    current.units[0].content_digest,
                    expected.units[0].content_digest
                );
            } else {
                assert_eq!(
                    current, previous,
                    "shared-source publication retains every unrequested component"
                );
            }
        }
        assert_eq!(
            composed.sources[0].images,
            publication_base.sources[0].images
        );
        let retained_chart = |source: &LockedSource| {
            source
                .charts
                .iter()
                .find(|chart| chart.release == "retained")
                .unwrap()
                .clone()
        };
        assert_eq!(
            retained_chart(&composed.sources[0]),
            retained_chart(&publication_base.sources[0])
        );
    }
    let all = lock
        .components
        .iter()
        .map(|component| component.declaration.id.clone())
        .collect();
    let snapshots = crate::sources::resolve_locked_sources(&profile, &lock, &all).unwrap();
    assert_eq!(snapshots.len(), 2);
    assert!(snapshots.keys().all(|source| source.name == "platform"));
    assert_eq!(
        snapshots
            .keys()
            .map(|source| source.revision.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([old_revision.as_str(), new_revision.as_str()])
    );
    let mut immutable_roots = snapshots
        .iter()
        .map(|(identity, snapshot)| (identity.clone(), snapshot.repository.clone()))
        .collect::<BTreeMap<_, _>>();
    let incomplete = BTreeSet::from([ComponentId::try_from("current".to_owned()).unwrap()]);
    let resolution_error = crate::sources::resolve_locked_sources(&profile, &lock, &incomplete)
        .err()
        .unwrap();
    assert!(resolution_error.to_string().contains("expanded dependency"));
    let preparation_error =
        compile_locked_components(&profile, &lock, &immutable_roots, &incomplete)
            .err()
            .unwrap();
    assert!(
        preparation_error
            .to_string()
            .contains("expanded dependency")
    );
    let prepared = compile_locked_components(&profile, &lock, &immutable_roots, &all).unwrap();
    assert_eq!(prepared.len(), lock.components.len());
    if configuration_change {
        for (id, expected) in [("current", "new"), ("retained", "old")] {
            let component = prepared
                .iter()
                .find(|component| component.locked.declaration.id.as_str() == id)
                .unwrap();
            assert_eq!(
                component.units[0].objects[0]["metadata"]["annotations"]["test.example/setting"],
                expected
            );
        }
    }
    for component in prepared {
        assert_eq!(
            Some(&component.locked),
            lock.components
                .iter()
                .find(|locked| locked.declaration.id == component.locked.declaration.id)
        );
    }
    if matches!(change, InputChange::Image | InputChange::ImageProvenance) {
        let image = lock.sources[0]
            .images
            .iter()
            .find(|image| image.source_revision.as_str() == new_revision)
            .unwrap()
            .clone();
        let requested = BTreeSet::from([ComponentId::try_from("retained".to_owned()).unwrap()]);
        let promoted = publish_images(
            &profile,
            &lock,
            &requested,
            &BTreeMap::from([("platform".into(), vec![image])]),
        )
        .unwrap();
        for previous in &lock.components {
            let current = promoted
                .components
                .iter()
                .find(|component| component.declaration.id == previous.declaration.id)
                .unwrap();
            if previous.declaration.id.as_str() == "retained" {
                assert_fixed_inputs(&current.declaration, &previous.declaration);
                assert_ne!(current.units[0].digest, previous.units[0].digest);
                if matches!(change, InputChange::ImageProvenance) {
                    assert_eq!(
                        current.units[0].content_digest,
                        previous.units[0].content_digest
                    );
                }
            } else {
                assert_eq!(current, previous);
            }
        }
    }
    // A name-only map would silently replace one of these checkout roots.
    let old_identity = immutable_roots
        .keys()
        .find(|identity| identity.revision.as_str() == old_revision)
        .unwrap()
        .clone();
    let new_root = immutable_roots
        .iter()
        .find(|(identity, _)| identity.revision.as_str() == new_revision)
        .unwrap()
        .1
        .clone();
    immutable_roots.insert(old_identity, new_root);
    assert!(compile_locked_components(&profile, &lock, &immutable_roots, &all).is_err());
}
