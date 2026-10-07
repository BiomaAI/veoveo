//! Tracked and nonignored descriptors are the only scenario inventory authority.
use super::cargo::CargoMetadata;
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, path::Path};
use veoveo_testing_support::descriptor::{
    CargoSelection, ScenarioDescriptor, ScenarioId, contained,
};
pub(crate) struct DiscoveredScenario {
    pub descriptor: ScenarioDescriptor,
    pub index: usize,
}
pub(crate) fn discover(
    repository: &Path,
    budget: &super::budget::Budget,
) -> Result<BTreeMap<ScenarioId, DiscoveredScenario>> {
    let mut inventory = std::process::Command::new("git");
    inventory
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(repository);
    let bytes = budget.output(inventory)?.stdout;
    let mut scenarios = BTreeMap::new();
    for name in bytes.split(|b| *b == 0).filter(|n| !n.is_empty()) {
        let name = std::str::from_utf8(name)?;
        let path = Path::new(name);
        if !name.ends_with("/smoke/scenarios.json") {
            continue;
        }
        let file = contained(repository, path)
            .with_context(|| format!("admitting smoke descriptor {name}"))?;
        let bytes = std::fs::read(&file)?;
        ensure!(bytes.len() <= 1024 * 1024, "smoke descriptor exceeds 1 MiB");
        let descriptor: ScenarioDescriptor = serde_json::from_slice(&bytes)?;
        let owner = path
            .parent()
            .and_then(Path::parent)
            .context("descriptor owner is absent")?;
        ensure!(
            descriptor.owner == owner,
            "descriptor owner disagrees with containing component"
        );
        contained(repository, owner)?;
        for (index, scenario) in descriptor.scenarios.iter().enumerate() {
            super::languages::admit(repository, owner, scenario)
                .with_context(|| format!("admitting smoke scenario {} in {name}", scenario.id))?;
            ensure!(
                scenarios
                    .insert(
                        scenario.id.clone(),
                        DiscoveredScenario {
                            descriptor: descriptor.clone(),
                            index
                        }
                    )
                    .is_none(),
                "duplicate global smoke scenario identity"
            );
        }
    }
    Ok(scenarios)
}
pub(crate) fn admit_cargo(
    repository: &Path,
    metadata: &CargoMetadata,
    selection: &CargoSelection,
    kind: &str,
) -> Result<()> {
    let manifest = super::languages::native_manifest(repository, selection)?;
    let owner = contained(repository, &selection.owner)?;
    let package = metadata
        .packages
        .iter()
        .find(|p| p.name == selection.package.as_str())
        .context("declared Cargo package is absent")?;
    ensure!(
        package.manifest_path.canonicalize()? == manifest,
        "Cargo package escapes declared owner"
    );
    let target = package
        .targets
        .iter()
        .find(|t| t.name == selection.target && t.kind.iter().any(|k| k == kind))
        .context("declared native target kind/name is absent")?;
    super::languages::native_source(&owner, &target.src_path)?;
    for f in &selection.features {
        ensure!(
            package.features.contains_key(f),
            "requested Cargo feature is absent"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_testing_support::descriptor::{BuildProfile, PackageName};
    #[test]
    fn independent_owner_admits_metadata_without_core_registration_and_rejects_target_drift() {
        let root = tempfile::tempdir().unwrap();
        let owner = root.path().join("independent");
        std::fs::create_dir(&owner).unwrap();
        std::fs::write(
            owner.join("Cargo.toml"),
            "[package]\nname='outside-owner'\nversion='0.1.0'\n",
        )
        .unwrap();
        std::fs::write(owner.join("case.rs"), "fn main() {}\n").unwrap();
        let bytes = serde_json::json!({"target_directory": root.path().join("external-target"), "packages": [{
            "name": "outside-owner", "version":"0.1.0", "id":"path+file:///outside#outside-owner@0.1.0",
            "manifest_path": owner.join("Cargo.toml"), "source":null,
            "features":{"smoke":[]}, "targets":[{"name":"outside-probe", "kind":["bin"], "required-features":["smoke"], "src_path":owner.join("case.rs")}]
        }]});
        let mut metadata: CargoMetadata = serde_json::from_value(bytes).unwrap();
        let mut selection = CargoSelection {
            owner: "independent".into(),
            package: PackageName::parse("outside-owner").unwrap(),
            target: "outside-probe".into(),
            features: ["smoke".into()].into(),
            default_features: false,
            profile: BuildProfile::Dev,
        };
        admit_cargo(root.path(), &metadata, &selection, "bin").unwrap();
        assert!(admit_cargo(root.path(), &metadata, &selection, "test").is_err());
        selection.features.clear();
        admit_cargo(root.path(), &metadata, &selection, "bin").unwrap();
        selection.features.insert("smoke".into());
        selection.features.insert("invented".into());
        assert!(admit_cargo(root.path(), &metadata, &selection, "bin").is_err());
        selection.features.remove("invented");
        std::fs::write(root.path().join("foreign.rs"), "fn main() {}\n").unwrap();
        metadata.packages[0].targets[0].src_path = root.path().join("foreign.rs");
        assert!(admit_cargo(root.path(), &metadata, &selection, "bin").is_err());
        metadata.packages[0].targets[0].src_path = owner.clone();
        assert!(admit_cargo(root.path(), &metadata, &selection, "bin").is_err());
        metadata.packages[0].targets[0].src_path = owner.join("case.rs");
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='foreign'\nversion='0.1.0'\n",
        )
        .unwrap();
        metadata.packages[0].manifest_path = root.path().join("Cargo.toml");
        assert!(admit_cargo(root.path(), &metadata, &selection, "bin").is_err());
    }
}
