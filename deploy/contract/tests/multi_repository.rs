use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use tempfile::TempDir;
use veoveo_deploy_contract::{
    DEPLOYMENT_LOCK_SCHEMA, DeploymentLock, DeploymentSourceRole, LoadedProfile, LockedChart,
    LockedImage, LockedSource, PlannedImage,
};

const DIGEST_A: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const DIGEST_B: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[test]
fn independent_git_sources_produce_one_validated_installation_lock() {
    let workspace = TempDir::new().expect("temporary multi-repository workspace");
    let platform = create_source_repository(
        workspace.path(),
        "platform-source",
        "platform-chart",
        "platform",
    );
    let extension = create_source_repository(
        workspace.path(),
        "extension-source",
        "extension-chart",
        "extension",
    );
    let installation = workspace.path().join("installation");
    fs::create_dir(&installation).expect("create installation repository");
    git(&installation, ["init", "--quiet"]);
    configure_git(&installation);

    let platform_revision = git_output(&platform, ["rev-parse", "HEAD"]);
    let extension_revision = git_output(&extension, ["rev-parse", "HEAD"]);
    assert_ne!(
        platform_revision, extension_revision,
        "fixture sources must have independent revisions"
    );

    let profile = serde_json::json!({
        "schemaVersion": "veoveo.io/deployment/v2",
        "name": "anonymous-installation",
        "registry": {
            "address": "registry.example.internal",
            "localConfig": null
        },
        "sources": [
            {
                "name": "platform",
                "role": "platform",
                "repository": {
                    "kind": "local",
                    "path": "../platform-source"
                },
                "revision": platform_revision,
                "imageGroups": ["platform"],
                "releases": [{
                    "name": "platform",
                    "chart": "platform-chart",
                    "values": [],
                    "valuesContract": "platform",
                    "createNamespace": true,
                    "timeoutSeconds": 600
                }]
            },
            {
                "name": "extension",
                "role": "extension",
                "repository": {
                    "kind": "local",
                    "path": "../extension-source"
                },
                "revision": extension_revision,
                "imageGroups": ["extension"],
                "releases": [{
                    "name": "extension",
                    "chart": "extension-chart",
                    "values": [],
                    "valuesContract": "extension",
                    "createNamespace": false,
                    "timeoutSeconds": 600
                }]
            }
        ],
        "kubernetes": {
            "context": "anonymous",
            "localCluster": null
        },
        "namespace": "veoveo",
        "resources": {
            "manifests": [],
            "configMaps": [],
            "secrets": []
        },
        "platform": {
            "installationPreset": "extension-foundation",
            "components": [],
            "mcpServers": [],
            "artifactAudiences": ["anonymous"]
        },
        "gatewayRequirements": [],
        "waitForDeployments": []
    });
    let profile_path = installation.join("deployment.json");
    fs::write(
        &profile_path,
        serde_json::to_vec_pretty(&profile).expect("serialize profile"),
    )
    .expect("write profile");
    git(&installation, ["add", "."]);
    git(&installation, ["commit", "--quiet", "-m", "installation"]);

    let loaded = LoadedProfile::load(&profile_path, &installation).expect("load installation");
    let required = loaded
        .required_platform_images()
        .expect("resolve platform closure");
    let mut images = required
        .iter()
        .map(|target| PlannedImage {
            source: "platform".to_owned(),
            target: target.clone(),
            reference: format!(
                "registry.example.internal/platform/{target}:{}",
                platform_revision
            ),
        })
        .collect::<Vec<_>>();
    images.push(PlannedImage {
        source: "extension".to_owned(),
        target: "anonymous-extension".to_owned(),
        reference: format!(
            "registry.example.internal/extensions/anonymous:{}",
            extension_revision
        ),
    });
    loaded
        .validate_image_plan(&images)
        .expect("validate source-qualified plan");

    let resolved = loaded.resolved_platform().expect("resolve platform");
    let lock = DeploymentLock {
        schema_version: DEPLOYMENT_LOCK_SCHEMA.to_owned(),
        profile: loaded.definition.name.clone(),
        registry: loaded.definition.registry.address.clone(),
        sources: vec![
            LockedSource {
                name: "platform".to_owned(),
                role: DeploymentSourceRole::Platform,
                repository: "https://git.example.internal/platform".to_owned(),
                revision: platform_revision,
                images: required
                    .into_iter()
                    .map(|name| LockedImage {
                        repository: format!("registry.example.internal/platform/{name}"),
                        name,
                        digest: DIGEST_A.to_owned(),
                        publication_digest: DIGEST_B.to_owned(),
                    })
                    .collect(),
                charts: vec![LockedChart {
                    release: "platform".to_owned(),
                    coordinate: "source://platform/platform-chart".to_owned(),
                    digest: DIGEST_A.to_owned(),
                }],
            },
            LockedSource {
                name: "extension".to_owned(),
                role: DeploymentSourceRole::Extension,
                repository: "https://git.example.internal/extension".to_owned(),
                revision: extension_revision,
                images: vec![LockedImage {
                    name: "anonymous-extension".to_owned(),
                    repository: "registry.example.internal/extensions/anonymous".to_owned(),
                    digest: DIGEST_B.to_owned(),
                    publication_digest: DIGEST_A.to_owned(),
                }],
                charts: vec![LockedChart {
                    release: "extension".to_owned(),
                    coordinate: "source://extension/extension-chart".to_owned(),
                    digest: DIGEST_B.to_owned(),
                }],
            },
        ],
        platform: resolved,
    };
    lock.validate()
        .expect("validate combined two-source deployment lock");
    assert_eq!(
        lock.sources
            .iter()
            .map(|source| source.revision.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );

    let mut colliding_lock = lock.clone();
    colliding_lock.sources[1].images[0].repository =
        colliding_lock.sources[0].images[0].repository.clone();
    assert!(
        colliding_lock
            .validate()
            .expect_err("cross-source repository collision must fail")
            .to_string()
            .contains("owned by both")
    );

    let mut colliding_release = lock;
    colliding_release.sources[1].charts[0].release = "platform".to_owned();
    assert!(
        colliding_release
            .validate()
            .expect_err("cross-source Helm release collision must fail")
            .to_string()
            .contains("owned by both")
    );
}

fn create_source_repository(workspace: &Path, name: &str, chart: &str, marker: &str) -> PathBuf {
    let repository = workspace.join(name);
    fs::create_dir(&repository).expect("create source repository");
    git(&repository, ["init", "--quiet"]);
    configure_git(&repository);
    let chart_root = repository.join(chart);
    fs::create_dir(&chart_root).expect("create chart root");
    fs::write(
        chart_root.join("Chart.yaml"),
        format!("apiVersion: v2\nname: {chart}\nversion: 0.1.0\n"),
    )
    .expect("write chart");
    fs::write(repository.join("source.txt"), format!("{marker}\n")).expect("write source marker");
    git(&repository, ["add", "."]);
    git(&repository, ["commit", "--quiet", "-m", marker]);
    repository
}

fn configure_git(repository: &Path) {
    git(repository, ["config", "user.email", "test@example.com"]);
    git(repository, ["config", "user.name", "Veoveo Test"]);
    git(repository, ["config", "commit.gpgsign", "false"]);
}

fn git<const N: usize>(repository: &Path, arguments: [&str; N]) {
    let status = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .status()
        .expect("run git");
    assert!(status.success(), "git command failed");
}

fn git_output<const N: usize>(repository: &Path, arguments: [&str; N]) -> String {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .expect("run git");
    assert!(output.status.success(), "git command failed");
    String::from_utf8(output.stdout)
        .expect("git output is UTF-8")
        .trim()
        .to_owned()
}
