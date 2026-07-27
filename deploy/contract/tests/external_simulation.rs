use std::{collections::BTreeSet, path::Path};

use veoveo_deploy_contract::{LoadedProfile, PlannedImage};

#[test]
fn external_simulation_profile_selects_only_core_renderer_gpu_images() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = repository.join("testing/fixtures/external-simulation-installation/deployment.json");
    let loaded = LoadedProfile::load(&path, &repository).expect("load external simulation profile");
    let required = loaded
        .required_platform_images()
        .expect("resolve platform image closure");
    assert_eq!(
        required,
        BTreeSet::from([
            "artifact-mcp".to_owned(),
            "artifact-service".to_owned(),
            "frames-mcp".to_owned(),
            "mcp-gateway".to_owned(),
            "simulation-runtime".to_owned(),
            "simulation-view-isaac".to_owned(),
            "simulation-view-mcp".to_owned(),
            "simulation-view-pose".to_owned(),
        ])
    );

    let mut images = required
        .iter()
        .map(|target| PlannedImage {
            source: "veoveo".to_owned(),
            target: target.clone(),
            reference: format!("registry.example.internal/veoveo/{target}:fixture"),
        })
        .collect::<Vec<_>>();
    images.push(PlannedImage {
        source: "anonymous-simulation".to_owned(),
        target: "anonymous-simulation-mcp".to_owned(),
        reference: "registry.example.internal/extensions/anonymous-simulation-mcp:fixture"
            .to_owned(),
    });
    loaded
        .validate_image_plan(&images)
        .expect("validate source-qualified platform and extension plan");

    let platform = loaded.resolved_platform().expect("resolve platform");
    assert_eq!(
        platform.artifact_audiences,
        BTreeSet::from([
            "anonymous-simulation".to_owned(),
            "simulation-view".to_owned(),
        ])
    );
    let scheduling = platform
        .gpu_scheduling
        .expect("Simulation View requires explicit GPU scheduling");
    assert_eq!(scheduling.allocatable_devices, 1);
    assert_eq!(scheduling.workloads.len(), 1);
    assert_eq!(scheduling.workloads[0].workload, "simulation-view-renderer");
    assert!(
        platform
            .external_workloads
            .contains("anonymous-simulation-mcp")
    );
}
