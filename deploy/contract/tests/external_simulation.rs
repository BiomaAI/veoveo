use std::{collections::BTreeSet, path::Path};

use veoveo_deploy_contract::{LoadedProfile, PlannedImage};

#[test]
fn external_simulation_profile_keeps_live_view_inside_the_extension() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = repository.join("testing/fixtures/external-simulation-installation/deployment.json");
    let loaded = LoadedProfile::load(&path, &repository).expect("load external simulation profile");
    let required = loaded
        .required_platform_images()
        .expect("resolve platform image closure");
    assert_eq!(
        required,
        BTreeSet::from(["mcp-gateway".to_owned(), "simulation-runtime".to_owned()])
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
    assert!(platform.artifact_audiences.is_empty());
    assert!(platform.gpu_scheduling.is_none());
    assert_eq!(
        platform.external_workloads,
        BTreeSet::from(["anonymous-simulation-mcp".to_owned()])
    );
}
