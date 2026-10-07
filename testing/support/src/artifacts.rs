//! Compiler-observed artifact identities; no guessed target/debug executable paths.
use crate::descriptor::{CargoSelection, relative};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum ArtifactFormat {
    #[vocabulary(rename = "veoveo.ai/smoke-artifacts/v1")]
    V1,
}
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeLibrary {
    pub path: PathBuf,
    pub sha256: veoveo_types::Sha256Digest,
}
/// Kinds/features identify a differing context; equal native sets are not falsely distinguished.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum CompilerContext {
    Target,
    Host,
    NativeShared,
    BuildScript,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservedCompilerArtifact {
    pub context: CompilerContext,
    pub package_id: String,
    pub target: String,
    pub kinds: Vec<String>,
    pub features: BTreeSet<String>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, veoveo_types::Vocabulary)]
pub enum NativeTargetKind {
    #[vocabulary(rename = "bin")]
    Binary,
    #[vocabulary(rename = "test")]
    Test,
}
pub type EffectiveFeatures = std::collections::BTreeMap<String, BTreeSet<BTreeSet<String>>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactEntry {
    pub target_kind: NativeTargetKind,
    pub selection: CargoSelection,
    pub package_id: String,
    pub executable: PathBuf,
    pub sha256: veoveo_types::Sha256Digest,
    pub runtime_libraries: Vec<RuntimeLibrary>,
    pub compiler_graph: Vec<ObservedCompilerArtifact>,
    pub effective_features: EffectiveFeatures,
    pub target_features: EffectiveFeatures,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactManifest {
    pub format: ArtifactFormat,
    pub repository: PathBuf,
    pub target_root: PathBuf,
    pub entries: Vec<ArtifactEntry>,
}
impl ArtifactManifest {
    pub fn admit(&self, repository: &Path) -> Result<()> {
        ensure!(
            self.format == ArtifactFormat::V1,
            "unsupported smoke artifact revision"
        );
        let repository = repository.canonicalize()?;
        ensure!(
            self.repository.canonicalize()? == repository,
            "artifact source identity mismatch"
        );
        let target = self.target_root.canonicalize()?;
        let mut selected = BTreeSet::new();
        for entry in &self.entries {
            entry.selection.check()?;
            relative(&entry.selection.owner)?;
            ensure!(
                selected.insert((&entry.selection, entry.target_kind)),
                "duplicate compiler artifact selection"
            );
            let executable = entry.executable.canonicalize()?;
            ensure!(
                executable.starts_with(&target) && executable.is_file(),
                "executable escapes observed target root"
            );
            let digest = veoveo_types::Sha256Digest::from_bytes(
                Sha256::digest(std::fs::read(&executable)?).into(),
            );
            ensure!(
                digest == entry.sha256,
                "compiled executable changed after preparation"
            );
            ensure!(
                !entry.compiler_graph.is_empty(),
                "compiler dependency graph is absent"
            );
            let mut roots = entry.compiler_graph.iter().filter(|artifact| {
                artifact.package_id == entry.package_id
                    && artifact.target == entry.selection.target
                    && artifact
                        .kinds
                        .iter()
                        .any(|kind| kind == entry.target_kind.as_str())
            });
            let root = roots
                .next()
                .context("observed root package/target is absent")?;
            ensure!(roots.next().is_none(), "ambiguous observed root graph");
            ensure!(
                entry
                    .effective_features
                    .get(&entry.package_id)
                    .is_some_and(|sets| sets.contains(&root.features)),
                "requested features disagree with compiler root"
            );
            let mut observed = EffectiveFeatures::new();
            let mut target_observed = EffectiveFeatures::new();
            for artifact in &entry.compiler_graph {
                ensure!(
                    entry
                        .effective_features
                        .get(&artifact.package_id)
                        .is_some_and(|sets| sets.contains(&artifact.features)),
                    "unadmitted compiler feature/context"
                );
                let target_set = entry
                    .target_features
                    .get(&artifact.package_id)
                    .is_some_and(|sets| sets.contains(&artifact.features));
                match artifact.context {
                    CompilerContext::BuildScript => ensure!(
                        artifact.kinds.iter().any(|k| k == "custom-build"),
                        "build-script kind mismatch"
                    ),
                    CompilerContext::Host => ensure!(!target_set, "ambiguous host context"),
                    CompilerContext::Target | CompilerContext::NativeShared => {
                        ensure!(
                            target_set
                                && !artifact
                                    .kinds
                                    .iter()
                                    .any(|k| k == "custom-build" || k == "proc-macro"),
                            "target context disagrees with admitted projection"
                        );
                        target_observed
                            .entry(artifact.package_id.clone())
                            .or_default()
                            .insert(artifact.features.clone());
                    }
                }

                observed
                    .entry(artifact.package_id.clone())
                    .or_default()
                    .insert(artifact.features.clone());
            }
            ensure!(
                target_observed == entry.target_features,
                "target compiler projection differs from admission"
            );
            ensure!(
                observed == entry.effective_features,
                "compiler receipt differs from admitted effective closure"
            );
            for library in &entry.runtime_libraries {
                let path = library.path.canonicalize()?;
                let digest = veoveo_types::Sha256Digest::from_bytes(
                    Sha256::digest(std::fs::read(&path)?).into(),
                );
                ensure!(
                    digest == library.sha256,
                    "runtime library changed after compiler preparation"
                );
                ensure!(
                    path.starts_with(&target) && path.is_file(),
                    "runtime library escapes observed target root"
                );
            }
        }
        Ok(())
    }
    pub fn selected(
        &self,
        selection: &CargoSelection,
        kind: NativeTargetKind,
    ) -> Result<&ArtifactEntry> {
        self.entries
            .iter()
            .find(|entry| &entry.selection == selection && entry.target_kind == kind)
            .context("required compiler-observed artifact is absent")
    }
    pub fn from_environment(repository: &Path) -> Result<Self> {
        let path = std::env::var_os("VEOVEO_SMOKE_ARTIFACTS")
            .context("VEOVEO_SMOKE_ARTIFACTS is required")?;
        let bytes = std::fs::read(path)?;
        ensure!(
            bytes.len() <= 1024 * 1024,
            "artifact manifest exceeds 1 MiB"
        );
        let manifest: Self = serde_json::from_slice(&bytes)?;
        manifest.admit(repository)?;
        Ok(manifest)
    }
}
pub fn configure_runtime(command: &mut std::process::Command, entry: &ArtifactEntry) -> Result<()> {
    let mut directories = BTreeSet::new();
    for library in &entry.runtime_libraries {
        directories.insert(
            library
                .path
                .parent()
                .context("runtime library has no directory")?
                .to_owned(),
        );
    }
    for key in ["LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH", "PATH"] {
        let mut paths: Vec<_> = directories.iter().cloned().collect();
        if let Some(value) = std::env::var_os(key) {
            paths.extend(std::env::split_paths(&value));
        }
        command.env(key, std::env::join_paths(paths)?);
    }
    Ok(())
}

pub fn executable(package: &str, target: &str) -> anyhow::Result<PathBuf> {
    let root = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;
    anyhow::ensure!(root.status.success(), "cannot admit repository source root");
    let root = PathBuf::from(std::str::from_utf8(&root.stdout)?.trim());
    let manifest = ArtifactManifest::from_environment(&root)?;
    let mut matching = manifest.entries.iter().filter(|e| {
        e.selection.package.as_str() == package
            && e.selection.target == target
            && e.target_kind == NativeTargetKind::Binary
    });
    let entry = matching
        .next()
        .context("required observed native executable is absent")?;
    anyhow::ensure!(
        matching.next().is_none(),
        "ambiguous native executable selection"
    );
    Ok(entry.executable.clone())
}

/// An explicit native path must identify the same observed target; absent input uses its Cargo receipt.
pub fn requested_executable(
    requested: Option<PathBuf>,
    package: &str,
    target: &str,
) -> Result<PathBuf> {
    let observed = executable(package, target)?;
    if let Some(requested) = requested {
        ensure!(
            requested.canonicalize()? == observed.canonicalize()?,
            "requested executable disagrees with observed package/target"
        );
    }
    Ok(observed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::{BuildProfile, PackageName};
    fn digest(path: &Path) -> veoveo_types::Sha256Digest {
        veoveo_types::Sha256Digest::from_bytes(Sha256::digest(std::fs::read(path).unwrap()).into())
    }
    #[test]
    fn initial_artifact_protocol_rejects_old_keys_and_unknown_revision() {
        let value = serde_json::json!({"format":"veoveo.ai/smoke-artifacts/v1","repository":"/source","targetRoot":"/target","entries":[]});
        serde_json::from_value::<ArtifactManifest>(value.clone()).unwrap();
        let mut old = value.clone();
        old.as_object_mut().unwrap().remove("targetRoot");
        old["target_root"] = serde_json::json!("/target");
        assert!(serde_json::from_value::<ArtifactManifest>(old).is_err());
        let mut bad = value;
        bad["format"] = serde_json::json!("veoveo.ai/smoke-artifacts/v2");
        assert!(serde_json::from_value::<ArtifactManifest>(bad).is_err());
    }
    #[test]
    fn compiler_context_multiword_wire_spellings_use_the_public_vocabulary() {
        assert_eq!(
            serde_json::to_value(CompilerContext::NativeShared).unwrap(),
            "native_shared"
        );
        assert_eq!(
            serde_json::to_value(CompilerContext::BuildScript).unwrap(),
            "build_script"
        );
    }
    #[test]
    fn hashed_external_target_and_native_library_are_bound_to_one_observed_graph() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let executable = target.path().join("probe-0123456789abcdef");
        let library = target.path().join("libprobe.so");
        std::fs::write(&executable, b"observed fixture executable").unwrap();
        std::fs::write(&library, b"observed fixture library").unwrap();
        let selection = CargoSelection {
            owner: "testing/fixtures/owner".into(),
            package: PackageName::parse("independent-owner").unwrap(),
            target: "probe".into(),
            features: ["smoke".to_owned()].into(),
            default_features: false,
            profile: BuildProfile::Dev,
        };
        let mut manifest = ArtifactManifest {
            format: ArtifactFormat::V1,
            repository: source.path().to_owned(),
            target_root: target.path().to_owned(),
            entries: vec![ArtifactEntry {
                target_kind: NativeTargetKind::Binary,
                selection,
                package_id: "path+file:///owner#independent-owner@0.1.0".into(),
                executable: executable.clone(),
                sha256: digest(&executable),
                runtime_libraries: vec![RuntimeLibrary {
                    path: library.clone(),
                    sha256: digest(&library),
                }],
                effective_features: [(
                    "path+file:///owner#independent-owner@0.1.0".into(),
                    [BTreeSet::from(["smoke".into()])].into(),
                )]
                .into(),
                target_features: [(
                    "path+file:///owner#independent-owner@0.1.0".into(),
                    [BTreeSet::from(["smoke".into()])].into(),
                )]
                .into(),
                compiler_graph: vec![ObservedCompilerArtifact {
                    context: CompilerContext::Target,
                    package_id: "path+file:///owner#independent-owner@0.1.0".into(),
                    target: "probe".into(),
                    kinds: vec!["bin".into()],
                    features: ["smoke".to_owned()].into(),
                }],
            }],
        };
        manifest.admit(source.path()).unwrap();
        let mut test_entry = manifest.entries[0].clone();
        test_entry.target_kind = NativeTargetKind::Test;
        manifest.entries.push(test_entry);
        assert!(
            manifest.admit(source.path()).is_err(),
            "binary compiler evidence cannot admit a test target"
        );
        manifest.entries[1].compiler_graph[0].kinds = vec!["test".into()];
        manifest.admit(source.path()).unwrap();
        manifest.entries.pop();
        manifest.entries[0].compiler_graph[0].features.clear();
        assert!(manifest.admit(source.path()).is_err());
        manifest.entries[0].compiler_graph[0]
            .features
            .insert("smoke".into());
        std::fs::write(&library, b"replaced native library").unwrap();
        assert!(manifest.admit(source.path()).is_err());
        std::fs::write(&library, b"observed fixture library").unwrap();
        manifest.admit(source.path()).unwrap();
        std::fs::write(&executable, b"replaced executable").unwrap();
        assert!(manifest.admit(source.path()).is_err());
    }
}
