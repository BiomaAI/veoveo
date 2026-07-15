use std::{collections::BTreeSet, fs, path::PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageManifest {
    schema_version: u32,
    bundle_version: String,
    external_images: Vec<ExternalImage>,
    veoveo_images: Vec<VeoveoImage>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalImage {
    r#ref: String,
    source: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VeoveoImage {
    r#ref: String,
    dockerfile: String,
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("smoke crate lives under <root>/testing")
        .to_owned()
}

#[test]
fn offline_bundle_manifest_is_immutable_and_complete() {
    let root = repository_root();
    let manifest: ImageManifest = serde_json::from_slice(
        &fs::read(root.join("deploy/offline/images.lock.json")).expect("read image manifest"),
    )
    .expect("parse image manifest");
    assert_eq!(manifest.schema_version, 1);
    assert!(!manifest.bundle_version.trim().is_empty());

    let helm_values =
        fs::read_to_string(root.join("deploy/helm/veoveo/values.yaml")).expect("read Helm values");
    let mut references = BTreeSet::new();
    for image in &manifest.external_images {
        assert!(references.insert(&image.r#ref), "duplicate image reference");
        assert!(!image.r#ref.ends_with(":latest"));
        let digest = image
            .source
            .rsplit_once("@sha256:")
            .map(|(_, digest)| digest)
            .expect("external image source must carry a sha256 digest");
        assert_eq!(digest.len(), 64);
        assert!(
            digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        let (repository, tag) = image
            .r#ref
            .rsplit_once(':')
            .expect("external image ref must have an exact tag");
        assert!(
            helm_values.contains(&format!("repository: {repository}")),
            "Helm values do not use locked external repository {repository}"
        );
        assert!(
            helm_values.contains(&format!("tag: {tag}")),
            "Helm values do not use locked external tag {tag}"
        );
    }
    for image in &manifest.veoveo_images {
        assert!(references.insert(&image.r#ref), "duplicate image reference");
        assert!(!image.r#ref.ends_with(":latest"));
        assert!(
            root.join(&image.dockerfile).is_file(),
            "missing Dockerfile {}",
            image.dockerfile
        );
    }

    let loader = fs::read_to_string(root.join("deploy/offline/load-bundle.sh"))
        .expect("read offline loader");
    assert!(loader.contains("--install-dir"));
    assert!(loader.contains("cp -R \"$stage/payload/.\" \"$install_dir/\""));
    assert!(loader.contains("bundle-evidence"));

    let creator = fs::read_to_string(root.join("deploy/offline/create-bundle.sh"))
        .expect("read offline bundle creator");
    assert!(creator.contains("contract-schemas --output-dir \"$stage/payload/schemas\""));
    assert!(creator.contains("configs/deployments.json"));
    assert!(creator.contains("configs/gateway.local.json"));

    let offline_values = fs::read_to_string(root.join("deploy/offline/values.offline.yaml"))
        .expect("read offline Helm values");
    assert!(offline_values.contains("offline: true"));
    assert!(offline_values.contains("imagePullPolicy: Never"));
}
