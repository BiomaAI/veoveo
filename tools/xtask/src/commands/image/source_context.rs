//! Cargo-derived source boundaries for every Rust image compiler family.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::process;

#[derive(Debug, Deserialize)]
pub(super) struct CargoMetadata {
    pub packages: Vec<CargoPackage>,
    resolve: Resolve,
}

#[derive(Debug, Deserialize)]
pub(super) struct CargoPackage {
    pub name: String,
    pub targets: Vec<CargoTarget>,
    id: String,
    manifest_path: PathBuf,
    source: Option<String>,
    metadata: Option<PackageMetadata>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CargoTarget {
    pub name: String,
    pub kind: Vec<String>,
    src_path: PathBuf,
}

#[derive(Debug, Default, Deserialize)]
struct PackageMetadata {
    #[serde(default)]
    veoveo: VeoveoMetadata,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct VeoveoMetadata {
    #[serde(default)]
    image_build_inputs: Vec<PathBuf>,
    #[serde(default)]
    image_asset_inputs: Vec<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Debug, Deserialize)]
struct Node {
    id: String,
    deps: Vec<Dependency>,
}

#[derive(Debug, Deserialize)]
struct Dependency {
    pkg: String,
    dep_kinds: Vec<DependencyKind>,
}

#[derive(Debug, Deserialize)]
struct DependencyKind {
    kind: Option<Kind>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Build,
    Dev,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InputIdentity {
    pub digest: String,
    pub files: usize,
    pub source_packages: Vec<String>,
}

pub(super) struct SourceContext {
    pub identity: InputIdentity,
    directory: TempDir,
}

pub(super) struct PreparedSources {
    pub compilation: SourceContext,
    pub assets: Option<SourceContext>,
}

impl SourceContext {
    pub fn path(&self) -> &Path {
        self.directory.path()
    }
}

pub(super) fn metadata(repository: &Path) -> Result<CargoMetadata> {
    // All features make this a conservative production-input closure even for
    // the package-qualified Recording redap feature enabled by the image builder.
    let output = process::output(
        "cargo",
        [
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--all-features",
            "--filter-platform",
            "x86_64-unknown-linux-gnu",
        ],
        Some(repository),
    )?;
    serde_json::from_slice(&output.stdout).context("decoding locked Cargo source graph")
}

pub(super) fn prepare(
    repository: &Path,
    metadata: &CargoMetadata,
    packages: &[String],
) -> Result<PreparedSources> {
    let files = tracked_files(repository)?;
    let asset_files = asset_files(repository, metadata, packages, &files)?;
    let selected = input_files(repository, metadata, packages, &files)?;
    let context = materialize(repository, selected, source_packages(metadata, packages)?)?;
    process::output(
        "cargo",
        [
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--locked",
            "--offline",
        ],
        Some(context.path()),
    )
    .context("validating the generated Cargo workspace before image compilation")?;
    let assets = (!asset_files.is_empty())
        .then(|| {
            materialize(
                repository,
                asset_files,
                source_packages(metadata, packages)?,
            )
        })
        .transpose()?;
    Ok(PreparedSources {
        compilation: context,
        assets,
    })
}

pub(super) fn tracked_files(repository: &Path) -> Result<BTreeSet<PathBuf>> {
    let tracked = process::output(
        "git",
        [
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        Some(repository),
    )?;
    let candidates = tracked
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            std::str::from_utf8(path)
                .map(PathBuf::from)
                .context("Cargo input path is not UTF-8")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let mut files = BTreeSet::new();
    for path in candidates {
        match fs::symlink_metadata(repository.join(&path)) {
            Ok(_) => {
                files.insert(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("inspecting Cargo input {}", path.display()));
            }
        }
    }
    Ok(files)
}

fn closure(metadata: &CargoMetadata, packages: &[String]) -> Result<BTreeSet<String>> {
    let by_name = metadata
        .packages
        .iter()
        .filter(|package| package.source.is_none())
        .map(|package| (package.name.as_str(), &package.id))
        .collect::<BTreeMap<_, _>>();
    let mut selected = packages
        .iter()
        .map(|name| {
            by_name
                .get(name.as_str())
                .map(|id| (*id).clone())
                .with_context(|| format!("unknown local Cargo package {name}"))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    loop {
        let previous = selected.len();
        let dependencies = metadata
            .resolve
            .nodes
            .iter()
            .filter(|node| selected.contains(&node.id))
            .flat_map(|node| {
                node.deps
                    .iter()
                    .filter(|dependency| {
                        dependency
                            .dep_kinds
                            .iter()
                            .any(|kind| kind.kind != Some(Kind::Dev))
                    })
                    .map(|dependency| dependency.pkg.clone())
            })
            .collect::<Vec<_>>();
        selected.extend(dependencies);
        if selected.len() == previous {
            return Ok(selected);
        }
    }
}

fn source_packages(metadata: &CargoMetadata, packages: &[String]) -> Result<Vec<String>> {
    let selected = closure(metadata, packages)?;
    Ok(metadata
        .packages
        .iter()
        .filter(|package| package.source.is_none() && selected.contains(&package.id))
        .map(|package| package.name.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn relative(repository: &Path, path: &Path) -> Result<PathBuf> {
    let path = path.strip_prefix(repository).with_context(|| {
        format!(
            "Cargo source {} is outside repository {}",
            path.display(),
            repository.display()
        )
    })?;
    ensure!(
        path.components()
            .all(|component| matches!(component, Component::Normal(_))),
        "invalid Cargo source path {}",
        path.display()
    );
    Ok(path.to_owned())
}

fn input_files(
    repository: &Path,
    metadata: &CargoMetadata,
    packages: &[String],
    available: &BTreeSet<PathBuf>,
) -> Result<BTreeSet<PathBuf>> {
    let selected = closure(metadata, packages)?;
    let mut required = BTreeSet::from([
        PathBuf::from("Cargo.toml"),
        PathBuf::from("Cargo.lock"),
        PathBuf::from("rust-toolchain.toml"),
        PathBuf::from(".dockerignore"),
        PathBuf::from("tools/image-build/rust-workspace.Dockerfile"),
        PathBuf::from("tools/image-build/source-freshness.rs"),
    ]);
    let mut roots = vec![PathBuf::from(".cargo")];
    for package in metadata
        .packages
        .iter()
        .filter(|package| package.source.is_none())
    {
        let manifest = relative(repository, &package.manifest_path)?;
        required.insert(manifest.clone());
        // Cargo parses every workspace member even when compiling one package.
        // Retain its real declared/autodiscovered target entrypoints as well as
        // its manifest. Only production dependencies receive complete sources.
        for target in &package.targets {
            required.insert(relative(repository, &target.src_path)?);
        }
        if selected.contains(&package.id) {
            roots.push(
                manifest
                    .parent()
                    .context("Cargo manifest has no parent")?
                    .to_owned(),
            );
            for input in package
                .metadata
                .iter()
                .flat_map(|metadata| &metadata.veoveo.image_build_inputs)
            {
                ensure!(
                    input
                        .components()
                        .all(|component| matches!(component, Component::Normal(_))),
                    "image-build-inputs must contain repository-relative paths: {}",
                    input.display()
                );
                ensure!(
                    available.iter().any(|path| path.starts_with(input)),
                    "declared Cargo image input {} is missing",
                    input.display()
                );
                roots.push(input.clone());
            }
        }
    }
    for path in &required {
        ensure!(
            available.contains(path),
            "required Cargo metadata input {} is absent or ignored",
            path.display()
        );
    }
    required.extend(
        available
            .iter()
            .filter(|path| roots.iter().any(|root| path.starts_with(root)))
            .cloned(),
    );
    for path in asset_files(repository, metadata, packages, available)? {
        ensure!(
            !metadata
                .packages
                .iter()
                .flat_map(|package| package.metadata.iter())
                .flat_map(|metadata| &metadata.veoveo.image_build_inputs)
                .any(|input| path.starts_with(input)),
            "image asset is also an explicit compiler input: {}",
            path.display()
        );
        ensure!(
            !metadata.packages.iter().any(|package| {
                package.manifest_path == repository.join(&path)
                    || package
                        .targets
                        .iter()
                        .any(|target| target.src_path == repository.join(&path))
            }) && path.extension().is_none_or(|extension| extension != "rs"),
            "image-asset-inputs cannot remove Cargo metadata or Rust source: {}",
            path.display()
        );
        required.remove(&path);
    }
    Ok(required)
}

fn asset_files(
    repository: &Path,
    metadata: &CargoMetadata,
    packages: &[String],
    available: &BTreeSet<PathBuf>,
) -> Result<BTreeSet<PathBuf>> {
    let selected = closure(metadata, packages)?;
    let mut assets = BTreeSet::new();
    for package in metadata
        .packages
        .iter()
        .filter(|package| package.source.is_none() && selected.contains(&package.id))
    {
        let manifest = relative(repository, &package.manifest_path)?;
        let directory = manifest.parent().context("Cargo manifest has no parent")?;
        for input in package
            .metadata
            .iter()
            .flat_map(|metadata| &metadata.veoveo.image_asset_inputs)
        {
            ensure!(
                input
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
                    && input.starts_with(directory)
                    && input != directory,
                "image-asset-inputs must name repository-relative paths inside the declaring package: {}",
                input.display()
            );
            let matching = available
                .iter()
                .filter(|path| path.starts_with(input))
                .cloned()
                .collect::<Vec<_>>();
            ensure!(
                !matching.is_empty(),
                "declared image asset {} is absent or ignored",
                input.display()
            );
            assets.extend(matching);
        }
    }
    Ok(assets)
}

pub(super) fn materialize(
    repository: &Path,
    files: BTreeSet<PathBuf>,
    source_packages: Vec<String>,
) -> Result<SourceContext> {
    let directory = tempfile::Builder::new()
        .prefix("veoveo-rust-context-")
        .tempdir()?;
    let mut digest = Sha256::new();
    digest.update(b"veoveo.io/rust-source-context/v1\0");
    for relative in &files {
        let source = repository.join(relative);
        let metadata = fs::symlink_metadata(&source)
            .with_context(|| format!("reading Cargo input {}", source.display()))?;
        ensure!(
            metadata.is_file() || metadata.is_symlink(),
            "Cargo input {} is not a file",
            source.display()
        );
        let target = directory.path().join(relative);
        fs::create_dir_all(target.parent().context("Cargo input has no parent")?)?;
        let bytes = if metadata.is_symlink() {
            let resolved = fs::canonicalize(&source)
                .with_context(|| format!("resolving Cargo input link {}", source.display()))?;
            let referent = self::relative(repository, &resolved)?;
            ensure!(
                files.contains(&referent),
                "Cargo input symlink {} points outside the declared input closure; declare {} in image-build-inputs",
                relative.display(),
                referent.display()
            );
            let link = fs::read_link(&source)?;
            ensure!(
                !link.is_absolute(),
                "Cargo input symlink {} must be relative",
                relative.display()
            );
            #[cfg(unix)]
            std::os::unix::fs::symlink(&link, &target)?;
            #[cfg(not(unix))]
            anyhow::bail!("Cargo source contexts require a Unix build host");
            link.as_os_str().as_encoded_bytes().to_vec()
        } else {
            let bytes = fs::read(&source)?;
            fs::write(&target, &bytes)?;
            fs::set_permissions(&target, metadata.permissions())?;
            // Retain checkout metadata here. The compiler action synchronizes
            // timestamps against its executed-input mirror under the cache lock.
            fs::File::open(&target)?
                .set_times(fs::FileTimes::new().set_modified(metadata.modified()?))?;
            bytes
        };
        let path = relative.as_os_str().as_encoded_bytes();
        digest.update((path.len() as u64).to_le_bytes());
        digest.update(path);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            digest.update(metadata.permissions().mode().to_le_bytes());
        }
        digest.update([u8::from(metadata.is_symlink())]);
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(&bytes);
    }
    Ok(SourceContext {
        identity: InputIdentity {
            digest: format!("sha256:{}", hex::encode(digest.finalize())),
            files: files.len(),
            source_packages,
        },
        directory,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(root: &Path) -> CargoMetadata {
        let packages = ["bff", "shared", "unrelated", "test-helper"].map(|name| serde_json::json!({
            "id":name, "name":name, "manifest_path":root.join(name).join("Cargo.toml"), "source":null, "metadata":{},
            "targets":[{"name":name,"kind":["lib"],"src_path":root.join(name).join("src/lib.rs")}]
        }));
        serde_json::from_value(serde_json::json!({"packages":packages,"resolve":{"nodes":[
            {"id":"bff","deps":[{"pkg":"shared","dep_kinds":[{"kind":null}]},{"pkg":"test-helper","dep_kinds":[{"kind":"dev"}]}]},
            {"id":"shared","deps":[]},{"id":"unrelated","deps":[]},{"id":"test-helper","deps":[]}
        ]}})).unwrap()
    }

    #[test]
    fn web_edits_reuse_rust_inputs_but_shared_embedded_assets_invalidate_them() {
        let root = tempfile::tempdir().unwrap();
        let metadata = graph(root.path());
        let mut files = BTreeSet::new();
        for path in [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            ".dockerignore",
            "tools/image-build/rust-workspace.Dockerfile",
            "tools/image-build/source-freshness.rs",
            "web/app.tsx",
        ] {
            files.insert(PathBuf::from(path));
        }
        for package in ["bff", "shared", "unrelated", "test-helper"] {
            for path in [
                "Cargo.toml",
                "src/lib.rs",
                "src/implementation.rs",
                "assets/view.html",
            ] {
                files.insert(PathBuf::from(format!("{package}/{path}")));
            }
        }
        for path in &files {
            fs::create_dir_all(root.path().join(path).parent().unwrap()).unwrap();
            fs::write(root.path().join(path), b"initial").unwrap();
        }
        let inputs = input_files(root.path(), &metadata, &["bff".to_owned()], &files).unwrap();
        assert!(inputs.contains(Path::new("unrelated/Cargo.toml")));
        assert!(inputs.contains(Path::new("unrelated/src/lib.rs")));
        assert!(!inputs.contains(Path::new("unrelated/src/implementation.rs")));
        assert!(!inputs.contains(Path::new("test-helper/assets/view.html")));
        assert!(inputs.contains(Path::new("shared/assets/view.html")));
        let before = materialize(root.path(), inputs.clone(), vec![]).unwrap();
        fs::write(root.path().join("web/app.tsx"), b"changed web").unwrap();
        let web = materialize(root.path(), inputs.clone(), vec![]).unwrap();
        assert_eq!(before.identity.digest, web.identity.digest);
        fs::write(
            root.path().join("shared/assets/view.html"),
            b"changed embedded HTML",
        )
        .unwrap();
        let shared = materialize(root.path(), inputs, vec![]).unwrap();
        assert_ne!(before.identity.digest, shared.identity.digest);
    }

    #[test]
    fn build_dependencies_and_declared_external_inputs_enter_the_closure() {
        let root = Path::new("/source");
        let mut metadata = graph(root);
        metadata.resolve.nodes[1].deps.push(Dependency {
            pkg: "unrelated".to_owned(),
            dep_kinds: vec![DependencyKind {
                kind: Some(Kind::Build),
            }],
        });
        metadata.packages[1].metadata = Some(PackageMetadata {
            veoveo: VeoveoMetadata {
                image_build_inputs: vec![PathBuf::from("configs/shared.json")],
                ..Default::default()
            },
        });
        assert_eq!(
            source_packages(&metadata, &["bff".to_owned()]).unwrap(),
            ["bff", "shared", "unrelated"]
        );
        let mut files = BTreeSet::from(
            [
                "Cargo.toml",
                "Cargo.lock",
                "rust-toolchain.toml",
                ".dockerignore",
                "tools/image-build/rust-workspace.Dockerfile",
                "tools/image-build/source-freshness.rs",
            ]
            .map(PathBuf::from),
        );
        for package in &metadata.packages {
            files.insert(relative(root, &package.manifest_path).unwrap());
            files.extend(
                package
                    .targets
                    .iter()
                    .map(|target| relative(root, &target.src_path).unwrap()),
            );
        }
        assert!(
            input_files(root, &metadata, &["bff".to_owned()], &files)
                .unwrap_err()
                .to_string()
                .contains("configs/shared.json is missing")
        );
        files.insert(PathBuf::from("configs/shared.json"));
        assert!(
            input_files(root, &metadata, &["bff".to_owned()], &files)
                .unwrap()
                .contains(Path::new("configs/shared.json"))
        );
    }

    #[test]
    fn packaged_app_edits_change_only_the_asset_context() {
        let root = tempfile::tempdir().unwrap();
        let mut metadata = graph(root.path());
        metadata.packages[0].metadata = Some(PackageMetadata {
            veoveo: VeoveoMetadata {
                image_asset_inputs: vec![PathBuf::from("bff/assets")],
                ..Default::default()
            },
        });
        let mut files = BTreeSet::from(
            [
                "Cargo.toml",
                "Cargo.lock",
                "rust-toolchain.toml",
                ".dockerignore",
                "tools/image-build/rust-workspace.Dockerfile",
                "tools/image-build/source-freshness.rs",
                "bff/assets/view.html",
            ]
            .map(PathBuf::from),
        );
        for package in &metadata.packages {
            files.insert(relative(root.path(), &package.manifest_path).unwrap());
            files.extend(
                package
                    .targets
                    .iter()
                    .map(|target| relative(root.path(), &target.src_path).unwrap()),
            );
        }
        for path in &files {
            fs::create_dir_all(root.path().join(path).parent().unwrap()).unwrap();
            fs::write(root.path().join(path), "original").unwrap();
        }
        let packages = ["bff".to_owned()];
        let inputs = input_files(root.path(), &metadata, &packages, &files).unwrap();
        let assets = asset_files(root.path(), &metadata, &packages, &files).unwrap();
        let compiler_before = materialize(root.path(), inputs.clone(), vec![]).unwrap();
        let assets_before = materialize(root.path(), assets.clone(), vec![]).unwrap();
        assert!(!compiler_before.path().join("bff/assets/view.html").exists());
        fs::write(root.path().join("bff/assets/view.html"), "edited App").unwrap();
        assert_eq!(
            compiler_before.identity.digest,
            materialize(root.path(), inputs, vec![])
                .unwrap()
                .identity
                .digest
        );
        assert_ne!(
            assets_before.identity.digest,
            materialize(root.path(), assets, vec![])
                .unwrap()
                .identity
                .digest
        );
        for forbidden in [
            "bff/Cargo.toml",
            "bff/src/lib.rs",
            "shared/assets",
            "bff/../shared",
        ] {
            metadata.packages[0]
                .metadata
                .as_mut()
                .unwrap()
                .veoveo
                .image_asset_inputs = vec![PathBuf::from(forbidden)];
            assert!(
                input_files(root.path(), &metadata, &packages, &files).is_err(),
                "accepted {forbidden}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_source_links_that_escape_the_input_boundary() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("private.txt"), "outside the input closure").unwrap();
        std::os::unix::fs::symlink("private.txt", root.path().join("input.txt")).unwrap();
        let error = materialize(
            root.path(),
            BTreeSet::from([PathBuf::from("input.txt")]),
            vec![],
        )
        .err()
        .unwrap();
        assert!(
            error
                .to_string()
                .contains("outside the declared input closure")
        );
    }
    #[test]
    fn gpu_control_consumers_do_not_enable_unneeded_analytics() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        // Metadata's all-feature graph is intentionally conservative for file
        // discovery. Cargo tree resolves the requested production feature set.
        let output = std::process::Command::new("cargo")
            .args([
                "tree",
                "--locked",
                "--package",
                "veoveo-stream-mcp",
                "--package",
                "veoveo-reason-mcp",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--edges",
                "normal,build",
                "--prefix",
                "none",
                "--format",
                "{p}",
            ])
            .current_dir(repository)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let tree = String::from_utf8(output.stdout).unwrap();
        let packages = tree
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .collect::<BTreeSet<_>>();
        for expected in [
            "veoveo-stream-mcp",
            "veoveo-reason-mcp",
            "veoveo-task-runtime",
        ] {
            assert!(
                packages.contains(expected),
                "missing expected consumer {expected}"
            );
        }
        for unneeded in ["duckdb", "libduckdb-sys"] {
            assert!(
                !packages.contains(unneeded),
                "GPU control consumers enabled {unneeded}"
            );
        }
    }

    #[test]
    fn real_recording_consumers_exclude_service_implementations_and_keep_native_inputs() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let metadata = metadata(&repository).unwrap();
        let files = tracked_files(&repository).unwrap();
        for package in ["veoveo-stream-mcp", "veoveo-reason-mcp"] {
            let inputs =
                input_files(&repository, &metadata, &[package.to_owned()], &files).unwrap();
            assert!(inputs.contains(Path::new("platform/recordings/reader/src/read.rs")));
            assert!(!inputs.contains(Path::new("platform/recordings/hub/src/ingest.rs")));
            assert!(!inputs.contains(Path::new("servers/recording-mcp/src/service.rs")));
            assert!(!inputs.contains(Path::new("platform/recordings/forwarder/src/client.rs")));
            // Cargo still receives all real workspace manifests and target entrypoints.
            assert!(inputs.contains(Path::new("servers/recording-mcp/Cargo.toml")));
            assert!(inputs.contains(Path::new("servers/recording-mcp/src/lib.rs")));
            let source_packages = source_packages(&metadata, &[package.to_owned()]).unwrap();
            assert!(!source_packages.iter().any(|name| matches!(
                name.as_str(),
                "veoveo-recording-mcp" | "veoveo-recording-hub" | "veoveo-recording-forwarder"
            )));
            match package {
                "veoveo-stream-mcp" => {
                    assert!(
                        inputs.contains(Path::new("servers/stream-mcp/gst-runner/CMakeLists.txt"))
                    );
                    assert!(!inputs.contains(Path::new("servers/stream-mcp/assets/live.html")));
                    assert!(
                        asset_files(&repository, &metadata, &[package.to_owned()], &files)
                            .unwrap()
                            .contains(Path::new("servers/stream-mcp/assets/live.html"))
                    );
                }
                "veoveo-reason-mcp" => {
                    let assets =
                        asset_files(&repository, &metadata, &[package.to_owned()], &files).unwrap();
                    for runner in [
                        "servers/reason-mcp/runner/pyproject.toml",
                        "servers/reason-mcp/runner/uv.lock",
                        "servers/reason-mcp/runner/src/reason_runner/gpu_model.py",
                    ] {
                        assert!(!inputs.contains(Path::new(runner)));
                        assert!(assets.contains(Path::new(runner)));
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../image-build/source-freshness.rs"]
mod source_freshness;
