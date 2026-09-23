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

#[path = "support/lock.rs"]
mod lock_support;

#[test]
fn fork_merge_preserves_separate_release_ownership_and_retained_images() {
    let workspace = TempDir::new().expect("temporary fork workspace");
    let platform = create_source_repository(
        workspace.path(),
        "fork-source",
        "platform-chart",
        "platform",
    );
    git(&platform, ["branch", "upstream"]);
    git(&platform, ["switch", "--quiet", "-c", "downstream"]);
    let platform_revision = git_output(&platform, ["rev-parse", "HEAD"]);
    let workload = platform.clone();
    let chart = platform.join("workload-chart");
    fs::create_dir(&chart).unwrap();
    fs::write(
        chart.join("Chart.yaml"),
        "apiVersion: v2\nname: workload-chart\nversion: 0.1.0\n",
    )
    .unwrap();
    fs::write(chart.join("source-values.yaml"), "sourceOwned: true\n").unwrap();
    git(&platform, ["add", "."]);
    git(&platform, ["commit", "--quiet", "-m", "fork workload"]);
    git(&platform, ["switch", "--quiet", "upstream"]);
    fs::write(platform.join("source.txt"), "upstream change\n").unwrap();
    git(&platform, ["add", "."]);
    git(
        &platform,
        ["commit", "--quiet", "-m", "upstream improvement"],
    );
    git(&platform, ["switch", "--quiet", "downstream"]);
    git(&platform, ["merge", "--quiet", "--no-edit", "upstream"]);
    assert!(chart.join("Chart.yaml").exists());
    assert_eq!(
        fs::read_to_string(platform.join("source.txt")).unwrap(),
        "upstream change\n"
    );
    let installation = workspace.path().join("installation");
    fs::create_dir(&installation).expect("create installation repository");
    git(&installation, ["init", "--quiet"]);
    configure_git(&installation);
    fs::write(
        installation.join("platform-values.yaml"),
        "global:\n  publicBaseUrl: https://veoveo.example.internal\n",
    )
    .expect("write installation-owned platform values");
    fs::write(
        installation.join("workload-values.yaml"),
        "replicaCount: 2\n",
    )
    .expect("write installation-owned workload values");

    let workload_revision = git_output(&workload, ["rev-parse", "HEAD"]);
    assert_ne!(
        platform_revision, workload_revision,
        "unchanged platform images retain their earlier fork revision"
    );

    let profile = serde_json::json!({
        "schemaVersion": "veoveo.io/deployment/v8",
        "name": "anonymous-installation",
        "registry": {
            "pushAddress": "registry.example.internal",
            "pullAddress": "registry.example.internal",
            "transport": "tls",
            "localConfig": null
        },
        "sources": [
            {
                "name": "platform",
                "role": "platform",
                "repository": {
                    "kind": "local",
                    "path": "../fork-source"
                },
                "revision": platform_revision,
                "imageGroups": [],
                "releases": [{
                    "name": "platform",
                    "chart": "platform-chart",
                    "sourceValues": ["platform-chart/source-values.yaml"],
                    "installationValues": ["platform-values.yaml"],
                    "valuesContract": "platform",
                    "timeoutSeconds": 600
                }]
            },
            {
                "name": "workload",
                "role": "workload",
                "repository": {
                    "kind": "local",
                    "path": "../fork-source"
                },
                "revision": workload_revision,
                "imageGroups": ["workload"],
                "releases": [{
                    "name": "workload",
                    "chart": "workload-chart",
                    "sourceValues": ["workload-chart/source-values.yaml"],
                    "installationValues": ["workload-values.yaml"],
                    "valuesContract": "veoveo-source",
                    "timeoutSeconds": 600
                }]
            }
        ],
        "components": [
            {"id":"installation", "owner":{"kind":"installation"}, "role":"installation",
             "dependencies":[], "namespaces":["veoveo"], "clusterObjects":[{"group":"", "kind":"Namespace", "namespace":null, "name":"veoveo"}],
             "releases":[], "installationInputs":["namespace"]},
            {"id":"platform", "owner":{"kind":"source", "name":"platform"}, "role":"platform",
             "dependencies":["installation"], "namespaces":["veoveo"], "clusterObjects":[],
             "releases":["platform"], "installationInputs":[]},
            {"id":"workload", "owner":{"kind":"source", "name":"workload"}, "role":"workload",
             "dependencies":["installation"], "namespaces":["veoveo"], "clusterObjects":[],
             "releases":["workload"], "installationInputs":[]}
        ],
        "kubernetes": {
            "context": "anonymous",
            "localCluster": null
        },
        "namespace": "veoveo",
        "resources": {
            "manifests": [],
            "configMaps": []
        },
        "platform": {
            "installationPreset": "foundation",
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
    let installation_inputs = loaded
        .installation_inputs()
        .expect("resolve installation inputs");
    assert!(installation_inputs.contains(&installation.join("platform-values.yaml")));
    assert!(installation_inputs.contains(&installation.join("workload-values.yaml")));
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
        source: "workload".to_owned(),
        target: "anonymous-workload".to_owned(),
        reference: format!(
            "registry.example.internal/workloads/anonymous:{}",
            workload_revision
        ),
    });
    loaded
        .validate_image_plan(&images)
        .expect("validate source-qualified plan");

    let resolved = loaded.resolved_platform().expect("resolve platform");
    let mut lock = DeploymentLock {
        schema_version: DEPLOYMENT_LOCK_SCHEMA.to_owned(),
        profile: loaded.definition.name.clone(),
        profile_revision: git_output(&installation, ["rev-parse", "HEAD"]),
        registry: loaded.definition.registry.locked(),
        sources: vec![
            LockedSource {
                name: "platform".to_owned(),
                role: DeploymentSourceRole::Platform,
                repository: "https://git.example.internal/fork".to_owned(),
                revision: platform_revision.clone(),
                images: required
                    .into_iter()
                    .map(|name| LockedImage {
                        repository: format!("registry.example.internal/platform/{name}"),
                        source_revision: veoveo_deploy_contract::SourceRevision::new(
                            &platform_revision,
                        )
                        .unwrap(),
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
                name: "workload".to_owned(),
                role: DeploymentSourceRole::Workload,
                repository: "https://git.example.internal/fork".to_owned(),
                revision: workload_revision.clone(),
                images: vec![LockedImage {
                    name: "anonymous-workload".to_owned(),
                    source_revision: veoveo_deploy_contract::SourceRevision::new(
                        &workload_revision,
                    )
                    .unwrap(),
                    repository: "registry.example.internal/workloads/anonymous".to_owned(),
                    digest: DIGEST_B.to_owned(),
                    publication_digest: DIGEST_A.to_owned(),
                }],
                charts: vec![LockedChart {
                    release: "workload".to_owned(),
                    coordinate: "source://workload/workload-chart".to_owned(),
                    digest: DIGEST_B.to_owned(),
                }],
            },
        ],
        platform: resolved,
        components: Vec::new(),
    };
    lock.components = lock_support::synthetic_catalog(&lock.sources, &lock.profile_revision);
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

    // Publication uses operator-selected local checkouts, never remote package sources.
    let mut invalid = profile.clone();
    invalid["sources"][1]["repository"] =
        serde_json::json!({"kind":"git", "url":"https://example.invalid/extension"});
    fs::write(&profile_path, serde_json::to_vec_pretty(&invalid).unwrap()).unwrap();
    assert!(LoadedProfile::load(&profile_path, &installation).is_err());
    let mut old_profile = profile.clone();
    old_profile["schemaVersion"] = serde_json::json!("veoveo.io/deployment/v7");
    old_profile["sources"][1]["role"] = serde_json::json!("extension");
    fs::write(
        &profile_path,
        serde_json::to_vec_pretty(&old_profile).unwrap(),
    )
    .unwrap();
    assert!(
        LoadedProfile::load(&profile_path, &installation)
            .unwrap_err()
            .to_string()
            .contains("regenerate this profile and its lock")
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
    fs::write(chart_root.join("source-values.yaml"), "sourceOwned: true\n")
        .expect("write source-owned values");
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
