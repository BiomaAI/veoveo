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
    collections::{BTreeMap, BTreeSet},
    io::Read,
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
/// Seven selected native artifacts currently occupy about 1.4 MiB with full graphs.
const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;

pub(crate) fn read_manifest(reader: impl Read) -> Result<ArtifactManifest> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_MANIFEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_MANIFEST_BYTES,
        "artifact manifest exceeds 4 MiB"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
std::thread_local! {
    static CONTENT_READS: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// This map belongs to one admission; no file identity survives another call.
fn admit_file(
    path: &Path,
    expected: &veoveo_types::Sha256Digest,
    admitted: &mut BTreeMap<PathBuf, veoveo_types::Sha256Digest>,
) -> Result<()> {
    if let Some(digest) = admitted.get(path) {
        ensure!(
            digest == expected,
            "conflicting artifact digests for one canonical path"
        );
        return Ok(());
    }
    let mut file = std::fs::File::open(path)?;
    #[cfg(test)]
    CONTENT_READS.with(|reads| reads.borrow_mut().push(path.to_owned()));
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let digest = veoveo_types::Sha256Digest::from_bytes(hash.finalize().into());
    ensure!(
        &digest == expected,
        "compiled artifact changed after preparation"
    );
    admitted.insert(path.to_owned(), digest);
    Ok(())
}

struct ArtifactStructure {
    target: PathBuf,
    files: BTreeMap<PathBuf, veoveo_types::Sha256Digest>,
}

fn declare_file(
    path: PathBuf,
    expected: &veoveo_types::Sha256Digest,
    files: &mut BTreeMap<PathBuf, veoveo_types::Sha256Digest>,
) -> Result<()> {
    if let Some(previous) = files.insert(path, expected.clone()) {
        ensure!(
            previous == *expected,
            "conflicting artifact digests for one canonical path"
        );
    }
    Ok(())
}

impl ArtifactManifest {
    fn admit_structure(&self, repository: &Path) -> Result<ArtifactStructure> {
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
        let mut files = BTreeMap::new();
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
            declare_file(executable, &entry.sha256, &mut files)?;
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
                ensure!(
                    path.starts_with(&target) && path.is_file(),
                    "runtime library escapes observed target root"
                );
                declare_file(path, &library.sha256, &mut files)?;
            }
        }
        Ok(ArtifactStructure { target, files })
    }

    pub fn admit(&self, repository: &Path) -> Result<()> {
        let structure = self.admit_structure(repository)?;
        let mut admitted = BTreeMap::new();
        for (path, expected) in structure.files {
            admit_file(&path, &expected, &mut admitted)
                .context("compiled executable or runtime library changed after preparation")?;
        }
        Ok(())
    }

    fn admit_entry(&self, entry: &ArtifactEntry) -> Result<()> {
        let mut admitted = BTreeMap::new();
        admit_file(
            &entry.executable.canonicalize()?,
            &entry.sha256,
            &mut admitted,
        )
        .context("compiled executable changed after preparation")?;
        for library in &entry.runtime_libraries {
            admit_file(
                &library.path.canonicalize()?,
                &library.sha256,
                &mut admitted,
            )
            .context("runtime library changed after compiler preparation")?;
        }
        Ok(())
    }

    /// Validate all declarations and freshly hash only this binary's runtime closure.
    pub fn admit_selected_binary(
        &self,
        repository: &Path,
        package: &str,
        target: &str,
    ) -> Result<&ArtifactEntry> {
        self.admit_structure(repository)?;
        let entry = self.selected_binary(package, target)?;
        self.admit_entry(entry)?;
        Ok(entry)
    }

    pub(crate) fn configure_program_runtime(
        &self,
        command: &mut std::process::Command,
        program: &Path,
    ) -> Result<()> {
        let structure = self.admit_structure(&self.repository)?;
        let cwd = command
            .get_current_dir()
            .map(Path::to_path_buf)
            .unwrap_or(std::env::current_dir()?);
        let cwd = if cwd.is_absolute() {
            cwd
        } else {
            std::env::current_dir()?.join(cwd)
        };
        let lexical = |path: &Path| {
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            };
            let mut normalized = PathBuf::new();
            for component in absolute.components() {
                match component {
                    std::path::Component::ParentDir => {
                        normalized.pop();
                    }
                    std::path::Component::CurDir => {}
                    component => normalized.push(component.as_os_str()),
                }
            }
            normalized
        };
        let lexical_target = lexical(&self.target_root);
        let is_target =
            |path: &Path| path.starts_with(&structure.target) || path.starts_with(&lexical_target);
        if !program.is_absolute() && program.components().count() == 1 {
            // Conservatively inspect every candidate, including inherited PATH when
            // Command clears its environment. Never emulate execvp's executable search.
            let mut paths = Vec::new();
            if let Some(value) = std::env::var_os("PATH") {
                paths.push(value);
            }
            for (key, value) in command.get_envs() {
                if key == "PATH"
                    && let Some(value) = value
                {
                    paths.push(value.to_owned());
                }
            }
            for paths in paths {
                for directory in std::env::split_paths(&paths) {
                    let candidate = lexical(&directory.join(program));
                    ensure!(
                        !is_target(&candidate)
                            && !cwd
                                .join(directory.join(program))
                                .canonicalize()
                                .is_ok_and(|path| path.starts_with(&structure.target)),
                        "bare command may select a Cargo child; use its admitted absolute executable path"
                    );
                }
            }
            return Ok(());
        }
        let candidate = lexical(program);
        let actual_path = if program.is_absolute() {
            program.to_path_buf()
        } else {
            cwd.join(program)
        };
        let canonical = actual_path.canonicalize()?;
        ensure!(
            !is_target(&candidate) || canonical.starts_with(&structure.target),
            "Cargo child path escapes the admitted target directory"
        );
        let mut matching = self.entries.iter().filter(|entry| {
            entry
                .executable
                .canonicalize()
                .is_ok_and(|path| path == canonical)
        });
        if let Some(entry) = matching.next() {
            ensure!(
                matching.next().is_none(),
                "ambiguous native executable path"
            );
            self.admit_entry(entry)?;
            configure_runtime(command, entry)
        } else {
            ensure!(
                !canonical.starts_with(structure.target),
                "Cargo child is absent from admitted artifact manifest"
            );
            Ok(())
        }
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
    fn selected_binary(&self, package: &str, target: &str) -> Result<&ArtifactEntry> {
        let mut matching = self.entries.iter().filter(|entry| {
            entry.selection.package.as_str() == package
                && entry.selection.target == target
                && entry.target_kind == NativeTargetKind::Binary
        });
        let entry = matching
            .next()
            .context("required observed native executable is absent")?;
        ensure!(
            matching.next().is_none(),
            "ambiguous native executable selection"
        );
        Ok(entry)
    }

    /// Admit all declarations, then freshly bind the selected binary and runtime together.
    pub fn prepare_binary_command(
        &self,
        repository: &Path,
        package: &str,
        target: &str,
    ) -> Result<std::process::Command> {
        let entry = self.admit_selected_binary(repository, package, target)?;
        let mut command = std::process::Command::new(&entry.executable);
        configure_runtime(&mut command, entry)?;
        Ok(command)
    }

    fn load_environment() -> Result<Self> {
        let path = std::env::var_os("VEOVEO_SMOKE_ARTIFACTS")
            .context("VEOVEO_SMOKE_ARTIFACTS is required")?;
        read_manifest(std::fs::File::open(path)?)
    }

    pub fn from_environment(repository: &Path) -> Result<Self> {
        let manifest = Self::load_environment()?;
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
        let explicit = command
            .get_envs()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.map(std::ffi::OsStr::to_owned));
        if matches!(explicit, Some(None)) {
            continue;
        }
        if let Some(value) = explicit.flatten().or_else(|| std::env::var_os(key)) {
            paths.extend(std::env::split_paths(&value));
        }
        command.env(key, std::env::join_paths(paths)?);
    }
    Ok(())
}

fn repository_root() -> Result<PathBuf> {
    let root = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;
    ensure!(root.status.success(), "cannot admit repository source root");
    Ok(PathBuf::from(std::str::from_utf8(&root.stdout)?.trim()))
}

pub fn executable(package: &str, target: &str) -> Result<PathBuf> {
    let root = repository_root()?;
    let manifest = ArtifactManifest::load_environment()?;
    Ok(manifest
        .admit_selected_binary(&root, package, target)?
        .executable
        .clone())
}

/// Prepare a compiler-observed binary with fresh selected-runtime content admission.
/// Provider and system programs retain their separately owned command paths.
pub fn binary_command(package: &str, target: &str) -> Result<std::process::Command> {
    let root = repository_root()?;
    ArtifactManifest::load_environment()?.prepare_binary_command(&root, package, target)
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
    fn manifest_reader_admits_current_shape_and_limits_consumption_before_parsing() {
        let entry = serde_json::json!({
            "targetKind":"bin",
            "selection":{"owner":"testing/fixtures/owner","package":"independent-owner",
                "target":"probe","features":[],"defaultFeatures":false,"profile":"dev"},
            "packageId":"path+file:///owner#independent-owner@0.1.0",
            "executable":"/target/probe","sha256":veoveo_types::Sha256Digest::from_bytes([0;32]),
            "runtimeLibraries":[],"compilerGraph":[],"effectiveFeatures":{},"targetFeatures":{}
        });
        let value = serde_json::json!({"format":"veoveo.ai/smoke-artifacts/v1",
            "repository":"/source","targetRoot":"/target","entries":vec![entry;7]});
        let mut bytes = serde_json::to_vec(&value).unwrap();
        // Match the real seven-artifact descriptor receipt size without copying executables.
        bytes.resize(1_440_812, b' ');
        assert_eq!(read_manifest(bytes.as_slice()).unwrap().entries.len(), 7);
        bytes.resize(MAX_MANIFEST_BYTES, b' ');
        assert_eq!(read_manifest(bytes.as_slice()).unwrap().entries.len(), 7);
        bytes.resize(MAX_MANIFEST_BYTES + 4096, b' ');
        let mut reader = std::io::Cursor::new(bytes);
        let error = read_manifest(&mut reader).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("artifact manifest exceeds 4 MiB")
        );
        assert_eq!(reader.position(), (MAX_MANIFEST_BYTES + 1) as u64);
        // Size admission does not replace the existing semantic artifact checks.
        let manifest = read_manifest(serde_json::to_vec(&value).unwrap().as_slice()).unwrap();
        assert!(manifest.admit(Path::new("/source")).is_err());
    }

    #[test]
    fn streaming_hash_deduplication_is_local_to_one_admission() {
        let target = tempfile::tempdir().unwrap();
        let path = target.path().join("large-fixture");
        std::fs::write(&path, vec![7; 3 * 64 * 1024 + 17]).unwrap();
        let expected = digest(&path);
        let mut admitted = BTreeMap::new();
        admit_file(&path, &expected, &mut admitted).unwrap();
        admit_file(&path, &expected, &mut admitted).unwrap();
        assert_eq!(admitted.len(), 1);
        assert!(
            admit_file(
                &path,
                &veoveo_types::Sha256Digest::from_bytes([0; 32]),
                &mut admitted
            )
            .is_err()
        );
        std::fs::write(&path, b"changed after prior admission").unwrap();
        assert!(admit_file(&path, &expected, &mut BTreeMap::new()).is_err());
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
    fn fixture() -> (tempfile::TempDir, tempfile::TempDir, ArtifactManifest) {
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
        let manifest = ArtifactManifest {
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
        (source, target, manifest)
    }

    #[test]
    fn hashed_external_target_and_native_library_are_bound_to_one_observed_graph() {
        let (source, target, mut manifest) = fixture();
        let executable = manifest.entries[0].executable.clone();
        let library = manifest.entries[0].runtime_libraries[0].path.clone();
        manifest.admit(source.path()).unwrap();
        let prepared = manifest
            .prepare_binary_command(source.path(), "independent-owner", "probe")
            .unwrap();
        assert_eq!(prepared.get_program(), executable.as_os_str());
        for key in ["LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH", "PATH"] {
            let runtime = prepared
                .get_envs()
                .find(|(name, _)| *name == key)
                .unwrap()
                .1
                .unwrap();
            assert_eq!(
                std::env::split_paths(runtime).next().unwrap(),
                target.path()
            );
        }
        assert!(
            manifest
                .prepare_binary_command(source.path(), "another-owner", "probe")
                .is_err()
        );
        assert!(
            manifest
                .prepare_binary_command(source.path(), "independent-owner", "absent")
                .is_err()
        );
        // Canonical aliases share hashing, while declarations still bind their digest.
        let mut shared = manifest.entries[0].runtime_libraries[0].clone();
        shared.path = target.path().join(".").join("libprobe.so");
        manifest.entries[0].runtime_libraries.push(shared);
        manifest
            .prepare_binary_command(source.path(), "independent-owner", "probe")
            .unwrap();
        manifest.entries[0].runtime_libraries[1].sha256 =
            veoveo_types::Sha256Digest::from_bytes([0; 32]);
        assert!(
            manifest
                .prepare_binary_command(source.path(), "independent-owner", "probe")
                .is_err()
        );
        manifest.entries[0].runtime_libraries.pop();
        let mut ambiguous = manifest.entries[0].clone();
        ambiguous.selection.owner = "testing/fixtures/another-owner".into();
        manifest.entries.push(ambiguous);
        assert!(
            manifest
                .prepare_binary_command(source.path(), "independent-owner", "probe")
                .is_err()
        );
        manifest.entries.pop();
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
        assert!(
            manifest
                .prepare_binary_command(source.path(), "independent-owner", "probe")
                .is_err()
        );
        std::fs::write(&library, b"observed fixture library").unwrap();
        manifest.admit(source.path()).unwrap();
        std::fs::write(&executable, b"replaced executable").unwrap();
        assert!(manifest.admit(source.path()).is_err());
        assert!(
            manifest
                .prepare_binary_command(source.path(), "independent-owner", "probe")
                .is_err()
        );
    }
    fn reads() -> Vec<PathBuf> {
        CONTENT_READS.with(|reads| std::mem::take(&mut *reads.borrow_mut()))
    }

    #[test]
    fn selected_runtime_admission_reads_only_its_closure_but_checks_all_declarations() {
        let (source, target, mut manifest) = fixture();
        let mut other = manifest.entries[0].clone();
        other.selection.target = "other".into();
        other.compiler_graph[0].target = "other".into();
        other.executable = target.path().join("other");
        std::fs::write(&other.executable, b"unselected content").unwrap();
        other.sha256 = digest(&other.executable);
        other.runtime_libraries.clear();
        manifest.entries.push(other);
        let duplicate_library = manifest.entries[0].runtime_libraries[0].clone();
        manifest.entries[0]
            .runtime_libraries
            .push(duplicate_library);
        reads();
        manifest.admit(source.path()).unwrap();
        assert_eq!(
            reads().len(),
            3,
            "full admission hashes each unique content path once"
        );
        manifest
            .prepare_binary_command(source.path(), "independent-owner", "probe")
            .unwrap();
        assert_eq!(
            reads().into_iter().collect::<BTreeSet<_>>(),
            BTreeSet::from([
                manifest.entries[0].executable.canonicalize().unwrap(),
                manifest.entries[0].runtime_libraries[0]
                    .path
                    .canonicalize()
                    .unwrap(),
            ])
        );
        std::fs::write(&manifest.entries[1].executable, b"replaced content!").unwrap();
        manifest
            .prepare_binary_command(source.path(), "independent-owner", "probe")
            .unwrap();
        assert!(
            manifest.admit(source.path()).is_err(),
            "full admission must read unselected content"
        );
        manifest.entries[1].compiler_graph[0].features.clear();
        reads();
        assert!(
            manifest
                .prepare_binary_command(source.path(), "independent-owner", "probe")
                .is_err()
        );
        assert!(
            reads().is_empty(),
            "all graph admission precedes selected reads"
        );
        manifest.entries[1].compiler_graph[0]
            .features
            .insert("smoke".into());
        let conflicting_path = manifest.entries[0].executable.clone();
        manifest.entries[1].runtime_libraries.push(RuntimeLibrary {
            path: conflicting_path,
            sha256: veoveo_types::Sha256Digest::from_bytes([0; 32]),
        });
        assert!(
            manifest
                .prepare_binary_command(source.path(), "independent-owner", "probe")
                .is_err(),
            "unselected conflicting canonical declarations must fail"
        );
        assert!(reads().is_empty());
        assert!(
            manifest
                .prepare_binary_command(target.path(), "independent-owner", "probe")
                .is_err()
        );
    }

    #[test]
    fn selected_runtime_rehashes_replacements_with_preserved_size_and_mtime() {
        let (source, _target, manifest) = fixture();
        for path in [
            &manifest.entries[0].executable,
            &manifest.entries[0].runtime_libraries[0].path,
        ] {
            manifest
                .prepare_binary_command(source.path(), "independent-owner", "probe")
                .unwrap();
            let original = std::fs::read(path).unwrap();
            let metadata = std::fs::metadata(path).unwrap();
            std::fs::write(path, vec![b'X'; original.len()]).unwrap();
            let file = std::fs::File::options().write(true).open(path).unwrap();
            file.set_times(std::fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
                .unwrap();
            assert_eq!(std::fs::metadata(path).unwrap().len(), metadata.len());
            assert_eq!(
                std::fs::metadata(path).unwrap().modified().unwrap(),
                metadata.modified().unwrap()
            );
            assert!(
                manifest
                    .prepare_binary_command(source.path(), "independent-owner", "probe")
                    .is_err()
            );
            std::fs::write(path, original).unwrap();
        }
    }

    #[test]
    fn command_path_admission_rejects_relative_unobserved_and_ambiguous_cargo_paths() {
        let (source, target, mut manifest) = fixture();
        let executable = manifest.entries[0].executable.clone();
        let mut command = std::process::Command::new(&executable);
        reads();
        manifest
            .configure_program_runtime(&mut command, Path::new("/bin/true"))
            .unwrap();
        assert!(
            reads().is_empty(),
            "system command must not hash Cargo content"
        );
        manifest
            .configure_program_runtime(&mut command, &executable)
            .unwrap();
        assert_eq!(reads().len(), 2);
        let unobserved = target.path().join("unobserved");
        std::fs::write(&unobserved, b"not admitted").unwrap();
        assert!(
            manifest
                .configure_program_runtime(&mut command, &unobserved)
                .is_err()
        );
        let current = std::env::current_dir().unwrap();
        // A relative traversal resolves to the same target; it cannot bypass admission.
        let relative = current
            .components()
            .filter(|part| matches!(part, std::path::Component::Normal(_)))
            .fold(PathBuf::new(), |path, _| path.join(".."))
            .join(unobserved.strip_prefix("/").unwrap());
        assert!(!relative.is_absolute());
        assert!(
            manifest
                .configure_program_runtime(&mut command, &relative)
                .is_err()
        );
        let relative_executable = Path::new(".").join(executable.file_name().unwrap());
        let mut relative_command = std::process::Command::new(&relative_executable);
        relative_command.current_dir(target.path());
        reads();
        manifest
            .configure_program_runtime(&mut relative_command, &relative_executable)
            .unwrap();
        assert_eq!(
            reads().len(),
            2,
            "relative observed executable uses command cwd"
        );
        let shadow = tempfile::tempdir().unwrap();
        let bare_name = Path::new(executable.file_name().unwrap());
        std::fs::write(shadow.path().join(bare_name), b"not executable").unwrap();
        let mut bare = std::process::Command::new(bare_name);
        bare.env(
            "PATH",
            std::env::join_paths([shadow.path(), target.path()]).unwrap(),
        );
        reads();
        assert!(
            manifest
                .configure_program_runtime(&mut bare, bare_name)
                .is_err(),
            "non-executable shadow must not hide a later Cargo PATH candidate"
        );
        assert!(reads().is_empty());
        let mut bare_relative = std::process::Command::new(bare_name);
        bare_relative.current_dir(target.path()).env("PATH", ".");
        assert!(
            manifest
                .configure_program_runtime(&mut bare_relative, bare_name)
                .is_err()
        );
        let outside = tempfile::NamedTempFile::new().unwrap();
        let escape = target.path().join("escaped");
        std::os::unix::fs::symlink(outside.path(), &escape).unwrap();
        assert!(
            manifest
                .configure_program_runtime(&mut command, &escape)
                .is_err(),
            "lexical Cargo path cannot escape into provider classification"
        );
        let mut environment = std::process::Command::new(&executable);
        environment
            .env("PATH", shadow.path())
            .env_remove("LD_LIBRARY_PATH");
        manifest
            .configure_program_runtime(&mut environment, &executable)
            .unwrap();
        assert_eq!(
            environment
                .get_envs()
                .find(|(key, _)| *key == "LD_LIBRARY_PATH")
                .unwrap()
                .1,
            None
        );
        let configured_path = environment
            .get_envs()
            .find(|(key, _)| *key == "PATH")
            .unwrap()
            .1
            .unwrap();
        assert!(std::env::split_paths(configured_path).any(|path| path == shadow.path()));
        let mut ambiguous = manifest.entries[0].clone();
        ambiguous.selection.owner = "testing/fixtures/other".into();
        manifest.entries.push(ambiguous);
        assert!(
            manifest
                .configure_program_runtime(&mut command, &executable)
                .is_err()
        );
        assert!(
            manifest
                .prepare_binary_command(source.path(), "independent-owner", "probe")
                .is_err()
        );
    }
}
