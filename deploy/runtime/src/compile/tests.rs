use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::json;
use veoveo_deploy_contract::{DeploymentProfile, LoadedProfile, LockedImage, LockedSource};

use super::{compile_component_lock, compile_components};
use crate::charts::lock_source_charts;

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
    fs::write(
        root.join("chart/templates/workload.yaml"),
        format!(
            r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {name}
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
          image: "{{{{ {registry} }}}}/{name}@{{{{ index {digests} "{name}" }}}}"
"#
        ),
    )
    .unwrap();
    commit(root, name)
}

#[test]
fn selected_compilation_reuses_unselected_inventory_without_its_checkout_or_render() {
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
        "platform":{"installationPreset":"custom", "components":["platform-store"], "mcpServers":[], "artifactAudiences":[], "externalWorkloads":[]},
        "gatewayRequirements":[], "waitForDeployments":[]
    });
    let path = installation.join("deployment.json");
    fs::write(&path, serde_json::to_vec_pretty(&profile_value).unwrap()).unwrap();
    let profile_revision = commit(&installation, "installation");
    let definition: DeploymentProfile = serde_json::from_value(profile_value).unwrap();
    let profile = LoadedProfile {
        definition,
        path,
        directory: installation.clone(),
        repository: installation,
    };
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
                name: source.name.clone(),
                repository: format!("registry.example.invalid/{}", source.name),
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
}
