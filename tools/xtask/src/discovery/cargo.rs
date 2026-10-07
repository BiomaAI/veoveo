use serde::Deserialize;
use std::path::PathBuf;
#[derive(Debug, Deserialize)]
pub(crate) struct CargoMetadata {
    pub packages: Vec<CargoPackage>,
    #[serde(default)]
    pub target_directory: PathBuf,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CargoPackage {
    pub name: String,
    pub(crate) version: String,
    pub targets: Vec<CargoTarget>,
    #[serde(default)]
    pub features: std::collections::BTreeMap<String, Vec<String>>,
    pub(crate) id: String,
    pub(crate) manifest_path: PathBuf,
    pub(crate) source: Option<String>,
    pub(crate) metadata: Option<PackageMetadata>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CargoTarget {
    pub name: String,
    pub kind: Vec<String>,
    #[serde(default, rename = "required-features")]
    pub required_features: Vec<String>,
    pub(crate) src_path: PathBuf,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PackageMetadata {
    #[serde(default)]
    pub(crate) veoveo: VeoveoMetadata,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct VeoveoMetadata {
    #[serde(default)]
    pub(crate) image_build_inputs: Vec<PathBuf>,
    #[serde(default)]
    pub(crate) image_asset_inputs: Vec<PathBuf>,
}

pub(crate) fn inventory(
    repository: &std::path::Path,
    budget: &super::budget::Budget,
) -> anyhow::Result<CargoMetadata> {
    let mut command = std::process::Command::new("cargo");
    command
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--offline",
            "--all-features",
        ])
        .current_dir(repository);
    crate::process::remove_parent_cargo_package_environment(&mut command);
    decode_inventory(budget, command)
}

fn decode_inventory(
    budget: &super::budget::Budget,
    command: std::process::Command,
) -> anyhow::Result<CargoMetadata> {
    let output = budget.output(command)?;
    anyhow::ensure!(
        output.stdout.len() <= 64 * 1024 * 1024,
        "Cargo metadata exceeds64MiB"
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}
#[cfg(test)]
mod tests {
    use super::super::budget::Budget;
    use super::*;
    #[test]
    fn stuck_metadata_and_cancel_before_metadata_share_the_preparation_budget() {
        let temporary = tempfile::tempdir().unwrap();
        let budget = Budget::new(temporary.path(), 1, 1).unwrap();
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "sleep 4; printf '{\"packages\":[]}'"]);
        assert!(decode_inventory(&budget, command).is_err());
        let budget = Budget::new(temporary.path(), 5, 1).unwrap();
        budget.cancel_for_test();
        let sentinel = temporary.path().join("metadata-effect");
        let mut command = std::process::Command::new("touch");
        command.arg(&sentinel);
        assert!(decode_inventory(&budget, command).is_err());
        assert!(!sentinel.exists());
    }
}
