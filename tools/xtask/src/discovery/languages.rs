//! Admit source, package and locked environment identity before any language preparation.
use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};
use veoveo_testing_support::descriptor::{
    CargoSelection, HarnessTarget, Preparation, Scenario, contained,
};
fn owned_bytes(root: &Path, file: &str) -> Result<Vec<u8>> {
    let path = contained(root, Path::new(file))?;
    let metadata = std::fs::metadata(&path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= 64 * 1024 * 1024,
        "owned manifest/lock must be a regular file of at most 64 MiB"
    );
    let bytes = std::fs::read(path)?;
    ensure!(
        bytes.len() <= 64 * 1024 * 1024,
        "owned manifest/lock exceeds 64 MiB"
    );
    Ok(bytes)
}
/// Every native root and prerequisite owns a bounded, regular manifest.
pub(crate) fn native_manifest(repository: &Path, selection: &CargoSelection) -> Result<PathBuf> {
    selection.check()?;
    let owner = contained(repository, &selection.owner)?;
    let manifest: toml::Value =
        toml::from_str(std::str::from_utf8(&owned_bytes(&owner, "Cargo.toml")?)?)?;
    ensure!(
        manifest
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(toml::Value::as_str)
            == Some(selection.package.as_str()),
        "native package identity disagrees with selected owner"
    );
    Ok(owner.join("Cargo.toml").canonicalize()?)
}
/// Metadata may supply an absolute source path; containment and regular-file admission still apply.
pub(crate) fn native_source(owner: &Path, source: &Path) -> Result<()> {
    let source = source.canonicalize()?;
    ensure!(
        source.starts_with(owner) && std::fs::metadata(&source)?.is_file(),
        "native target source must be a contained regular file"
    );
    Ok(())
}
fn text<'a>(value: &'a toml::Value, field: &str) -> Result<&'a str> {
    let text = value
        .get(field)
        .and_then(toml::Value::as_str)
        .context("missing Python project identity")?;
    ensure!(!text.trim().is_empty(), "blank Python project identity");
    Ok(text)
}
fn normalized(name: &str) -> String {
    name.to_ascii_lowercase()
        .split(['-', '_', '.'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
/// UV owns one lock per explicitly declared workspace. Exact contained members only.
fn python_lock(repository: &Path, owner: &Path) -> Result<(PathBuf, String)> {
    let repository = repository.canonicalize()?;
    let root = contained(&repository, owner)?;
    ensure!(
        root == repository.join(owner),
        "Python owner must not traverse a symlink"
    );
    for ancestor in root.ancestors().take_while(|p| p.starts_with(&repository)) {
        let manifest = ancestor.join("pyproject.toml");
        if !manifest.exists() {
            continue;
        }
        ensure!(
            !std::fs::symlink_metadata(&manifest)?
                .file_type()
                .is_symlink(),
            "UV manifest must not be a symlink"
        );
        let project: toml::Value = toml::from_str(std::str::from_utf8(&owned_bytes(
            ancestor,
            "pyproject.toml",
        )?)?)?;
        let Some(workspace) = project
            .get("tool")
            .and_then(|v| v.get("uv"))
            .and_then(|v| v.get("workspace"))
        else {
            continue;
        };
        ensure!(
            workspace.get("exclude").is_none(),
            "UV workspace exclusions are unsupported; declare exact contained members"
        );
        let members = workspace
            .get("members")
            .and_then(toml::Value::as_array)
            .context("UV workspace requires exact member paths")?;
        let mut seen = std::collections::BTreeSet::new();
        let mut selected = None;
        for member in members {
            let member = member.as_str().context("UV member path must be text")?;
            let path = Path::new(member);
            ensure!(
                !member.is_empty()
                    && !member.contains(['*', '?', '[', ']', '{', '}'])
                    && path
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_))),
                "UV workspace supports exact contained member paths only"
            );
            ensure!(
                seen.insert(path.to_owned()),
                "ambiguous duplicate UV workspace member"
            );
            let directory = contained(ancestor, path)?;
            ensure!(
                directory == ancestor.join(path) && directory.is_dir(),
                "UV member must be a contained directory without symlinks"
            );
            ensure!(
                !std::fs::symlink_metadata(directory.join("pyproject.toml"))?
                    .file_type()
                    .is_symlink(),
                "UV member manifest must not be a symlink"
            );
            let _: toml::Value = toml::from_str(std::str::from_utf8(&owned_bytes(
                &directory,
                "pyproject.toml",
            )?)?)?;
            if directory == root {
                selected = Some(member.to_owned());
            }
        }
        let source = selected
            .context("Python owner is not an exact member of its nearest declared UV workspace")?;
        ensure!(
            std::fs::symlink_metadata(root.join("uv.lock")).is_err(),
            "UV member-local lock shadows its authoritative workspace lock; remove the stale member lock"
        );
        ensure!(
            !std::fs::symlink_metadata(ancestor.join("uv.lock"))?
                .file_type()
                .is_symlink(),
            "UV workspace lock must not be a symlink"
        );
        return Ok((ancestor.to_owned(), source));
    }
    ensure!(
        !std::fs::symlink_metadata(root.join("uv.lock"))?
            .file_type()
            .is_symlink(),
        "UV project lock must not be a symlink"
    );
    Ok((root, ".".into()))
}
/// Build preparation from admitted member identity; this function performs no installation.
pub(crate) fn python_sync_command(
    repository: &Path,
    owner: &Path,
    extras: &std::collections::BTreeSet<String>,
    groups: &std::collections::BTreeSet<String>,
) -> Result<std::process::Command> {
    let project = python(repository, owner, extras, groups)?;
    let (root, source) = python_lock(repository, owner)?;
    let mut command = std::process::Command::new("uv");
    command.args(["sync", "--locked"]).current_dir(root);
    if source != "." {
        command.arg("--package").arg(text(
            project
                .get("project")
                .context("missing Python project identity")?,
            "name",
        )?);
    }
    for extra in extras {
        command.args(["--extra", extra]);
    }
    for group in groups {
        command.args(["--group", group]);
    }
    Ok(command)
}
pub(crate) fn python(
    repository: &Path,
    owner: &Path,
    extras: &std::collections::BTreeSet<String>,
    groups: &std::collections::BTreeSet<String>,
) -> Result<toml::Value> {
    let root = contained(repository, owner)?;
    let project: toml::Value =
        toml::from_str(std::str::from_utf8(&owned_bytes(&root, "pyproject.toml")?)?)?;
    let identity = project
        .get("project")
        .context("Python project table missing")?;
    let name = normalized(text(identity, "name")?);
    let version = text(identity, "version")?;
    let (lock_root, source) = python_lock(repository, owner)?;
    let lock: toml::Value =
        toml::from_str(std::str::from_utf8(&owned_bytes(&lock_root, "uv.lock")?)?)?;
    let mut candidates = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .context("uv package identities absent")?
        .iter()
        .filter(|p| {
            p.get("name")
                .and_then(toml::Value::as_str)
                .is_some_and(|n| normalized(n) == name)
                && p.get("version").and_then(toml::Value::as_str) == Some(version)
                && p.get("source").is_some_and(|s| {
                    s.get("editable")
                        .or_else(|| s.get("virtual"))
                        .and_then(toml::Value::as_str)
                        == Some(source.as_str())
                })
        });
    let selected = candidates
        .next()
        .context("uv lock does not bind owning project")?;
    ensure!(candidates.next().is_none(), "ambiguous uv owner identity");
    for extra in extras {
        ensure!(
            identity
                .get("optional-dependencies")
                .and_then(|v| v.get(extra))
                .is_some()
                && selected
                    .get("optional-dependencies")
                    .and_then(|v| v.get(extra))
                    .is_some(),
            "uv extra absent from owner or lock"
        );
    }
    for group in groups {
        ensure!(
            project
                .get("dependency-groups")
                .and_then(|v| v.get(group))
                .is_some()
                && selected
                    .get("dev-dependencies")
                    .and_then(|v| v.get(group))
                    .is_some(),
            "uv group absent from owner or lock"
        );
    }
    Ok(project)
}
pub(crate) fn node(repository: &Path, owner: &Path) -> Result<()> {
    let root = contained(repository, owner)?;
    let project: serde_json::Value = serde_json::from_slice(&owned_bytes(&root, "package.json")?)?;
    let lock: serde_json::Value =
        serde_json::from_slice(&owned_bytes(&root, "package-lock.json")?)?;
    let pinned = &lock["packages"][""];
    ensure!(
        project["name"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty())
            && project["version"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty())
            && project["name"] == pinned["name"]
            && project["version"] == pinned["version"],
        "npm lock identity disagrees with owning package"
    );
    ensure!(
        matches!(lock["lockfileVersion"].as_u64(), Some(2 | 3)),
        "unsupported npm lock format"
    );
    for field in ["dependencies", "devDependencies", "optionalDependencies"] {
        let dependencies =
            |value: &serde_json::Value| -> Result<std::collections::BTreeMap<String, String>> {
                if value.is_null() {
                    return Ok(std::collections::BTreeMap::new());
                }
                value
                    .as_object()
                    .context("npm dependencies must be an object")?
                    .iter()
                    .map(|(name, selection)| {
                        ensure!(!name.trim().is_empty(), "blank npm dependency identity");
                        let selection = selection
                            .as_str()
                            .context("npm dependency selection must be text")?;
                        ensure!(
                            !selection.trim().is_empty(),
                            "blank npm dependency selection"
                        );
                        Ok((name.clone(), selection.to_owned()))
                    })
                    .collect()
            };
        ensure!(
            dependencies(&project[field])? == dependencies(&pinned[field])?,
            "npm dependency selection disagrees with lock"
        );
    }
    Ok(())
}
pub(crate) fn admit(
    repository: &Path,
    descriptor_owner: &Path,
    scenario: &Scenario,
) -> Result<PathBuf> {
    let owner = scenario
        .execution_owner
        .as_ref()
        .map(|declaration| declaration.owner.as_path())
        .unwrap_or(descriptor_owner);
    let root = contained(repository, owner)?;
    match &scenario.target {
        HarnessTarget::CargoBinary { selection } | HarnessTarget::CargoTest { selection, .. } => {
            let selected = contained(repository, &selection.owner)?;
            native_manifest(repository, selection)?;
            ensure!(
                selected.starts_with(&root),
                "native harness escapes selected component; explicit executionOwner required"
            );
        }
        HarnessTarget::PythonModule { module } | HarnessTarget::Pytest { case: module, .. } => {
            let preparations: Vec<_> = scenario
                .prerequisites
                .iter()
                .filter_map(|p| match p {
                    Preparation::UvSync {
                        owner: prepared,
                        extras,
                        groups,
                    } if prepared.as_path() == owner => Some((extras, groups)),
                    _ => None,
                })
                .collect();
            ensure!(
                preparations.len() == 1,
                "Python harness requires one owning locked preparation"
            );
            let project = python(repository, owner, preparations[0].0, preparations[0].1)?;
            if let HarnessTarget::PythonModule { .. } = &scenario.target {
                let packages = project
                    .get("tool")
                    .and_then(|v| v.get("hatch"))
                    .and_then(|v| v.get("build"))
                    .and_then(|v| v.get("targets"))
                    .and_then(|v| v.get("wheel"))
                    .and_then(|v| v.get("packages"))
                    .and_then(toml::Value::as_array)
                    .context("Python module needs declared Hatch package roots")?;
                let segments: Vec<_> = module.split('.').collect();
                let mut matches = 0;
                for package in packages {
                    let package =
                        Path::new(package.as_str().context("invalid Python package root")?);
                    let directory = contained(&root, package)?;
                    if directory.file_name().and_then(|n| n.to_str()) != segments.first().copied() {
                        continue;
                    }
                    let mut relative = PathBuf::new();
                    for part in &segments[1..] {
                        relative.push(part);
                    }
                    let candidate = directory.join(relative);
                    let file = if candidate.is_dir() {
                        candidate.join("__main__.py")
                    } else {
                        candidate.with_extension("py")
                    };
                    ensure!(
                        file.canonicalize()?.starts_with(&directory),
                        "Python module escapes declared package"
                    );
                    ensure!(file.is_file(), "Python module source is absent");
                    matches += 1;
                }
                ensure!(
                    matches == 1,
                    "Python module is not uniquely owned by declared package"
                );
            } else if let HarnessTarget::Pytest { file, .. } = &scenario.target {
                ensure!(
                    contained(&root, file)?.is_file()
                        && file.extension().is_some_and(|s| s == "py"),
                    "pytest source is not an owned Python file"
                );
            }
        }
        HarnessTarget::NodeModule { file } | HarnessTarget::NodeTest { file, .. } => {
            let preparations = scenario
                .prerequisites
                .iter()
                .filter(|p| matches!(p,Preparation::NpmCi {owner: prepared} if prepared.as_path()==owner))
                .count();
            ensure!(
                preparations == 1,
                "Node harness requires one owning locked preparation"
            );
            node(repository, owner)?;
            let file = contained(&root, file)?;
            ensure!(
                file.is_file()
                    && file
                        .extension()
                        .is_some_and(|s| matches!(s.to_str(), Some("js" | "mjs" | "cjs"))),
                "Node target is not an owned supported module"
            );
        }
    }
    // Explicit cross-owner prerequisites are allowed, but each selection admits its own identity.
    for preparation in &scenario.prerequisites {
        match preparation {
            Preparation::UvSync {
                owner,
                extras,
                groups,
            } => {
                python(repository, owner, extras, groups)?;
            }
            Preparation::NpmCi { owner } => node(repository, owner)?,
            Preparation::CargoBinary { selection } | Preparation::CargoTest { selection } => {
                native_manifest(repository, selection)?;
            }
        }
    }
    Ok(owner.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    fn python_owner(root: &Path) {
        std::fs::create_dir_all(root.join("python/src/independent")).unwrap();
        std::fs::write(
            root.join("python/src/independent/run.py"),
            "print('owned')\n",
        )
        .unwrap();
        std::fs::write(root.join("python/pyproject.toml"), "[project]\nname='independent-owner'\nversion='1.0.0'\n[project.optional-dependencies]\nchecks=[]\n[dependency-groups]\nverify=[]\n[tool.hatch.build.targets.wheel]\npackages=['src/independent']\n").unwrap();
        std::fs::write(root.join("python/uv.lock"), "version=1\n[[package]]\nname='independent-owner'\nversion='1.0.0'\nsource={editable='.'}\n[package.optional-dependencies]\nchecks=[]\n[package.dev-dependencies]\nverify=[]\n").unwrap();
    }
    fn scenario(target: serde_json::Value, preparation: serde_json::Value) -> Scenario {
        serde_json::from_value(serde_json::json!({"id":"independent-owner","description":"Source admission before effects","target":target,"arguments":[],"prerequisites":[preparation],"requirements":{"network":false,"credentials":false,"clusterMutation":false,"billedEffects":false,"nvidia":false,"headedGraphics":false,"explanation":"Local source only"},"deadlineSeconds":30,"cleanupSeconds":2})).unwrap()
    }
    #[test]
    fn declared_uv_workspace_member_binds_root_lock_and_exact_preparation() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        python_owner(root);
        std::fs::remove_file(root.join("python/uv.lock")).unwrap();
        let manifest = "[tool.uv.workspace]\nmembers=['python']\n";
        let lock = "version=1\n[[package]]\nname='independent-owner'\nversion='1.0.0'\nsource={editable='python'}\n[package.optional-dependencies]\nchecks=[]\n[package.dev-dependencies]\nverify=[]\n";
        std::fs::write(root.join("pyproject.toml"), manifest).unwrap();
        std::fs::write(root.join("uv.lock"), lock).unwrap();
        let empty = BTreeSet::new();
        let command = python_sync_command(
            root,
            Path::new("python"),
            &BTreeSet::from(["checks".into()]),
            &BTreeSet::from(["verify".into()]),
        )
        .unwrap();
        assert_eq!(command.get_current_dir(), Some(root));
        assert_eq!(
            command
                .get_args()
                .map(|v| v.to_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "sync",
                "--locked",
                "--package",
                "independent-owner",
                "--extra",
                "checks",
                "--group",
                "verify"
            ]
        );
        for wrong in [
            "[tool.uv.workspace]\nmembers=['python','python']\n",
            "[tool.uv.workspace]\nmembers=['py*']\n",
            "[tool.uv.workspace]\nmembers=['../python']\n",
            "[tool.uv.workspace]\nmembers=['python']\nexclude=['python']\n",
            "[tool.uv.workspace]\nmembers=[]\n",
        ] {
            std::fs::write(root.join("pyproject.toml"), wrong).unwrap();
            assert!(python(root, Path::new("python"), &empty, &empty).is_err());
        }
        std::fs::write(root.join("pyproject.toml"), manifest).unwrap();
        std::fs::write(
            root.join("uv.lock"),
            lock.replace("editable='python'", "editable='foreign'"),
        )
        .unwrap();
        assert!(python(root, Path::new("python"), &empty, &empty).is_err());
        std::fs::write(root.join("uv.lock"), lock).unwrap();
        std::fs::write(root.join("python/uv.lock"), lock).unwrap();
        assert!(python(root, Path::new("python"), &empty, &empty).is_err());
        std::fs::remove_file(root.join("python/uv.lock")).unwrap();
        std::fs::write(root.join("uv.lock"), format!("{lock}{lock}")).unwrap();
        assert!(python(root, Path::new("python"), &empty, &empty).is_err());
        std::fs::write(root.join("uv.lock"), lock).unwrap();
        #[cfg(unix)]
        {
            std::fs::rename(root.join("python"), root.join("foreign")).unwrap();
            std::os::unix::fs::symlink(root.join("foreign"), root.join("python")).unwrap();
            assert!(python(root, Path::new("python"), &empty, &empty).is_err());
            std::fs::remove_file(root.join("python")).unwrap();
            std::fs::rename(root.join("foreign"), root.join("python")).unwrap();
            std::fs::rename(root.join("uv.lock"), root.join("foreign.lock")).unwrap();
            std::os::unix::fs::symlink(root.join("foreign.lock"), root.join("uv.lock")).unwrap();
            assert!(python(root, Path::new("python"), &empty, &empty).is_err());
            std::fs::remove_file(root.join("uv.lock")).unwrap();
            std::fs::rename(root.join("foreign.lock"), root.join("uv.lock")).unwrap();
        }
        std::fs::remove_file(root.join("uv.lock")).unwrap();
        assert!(python(root, Path::new("python"), &empty, &empty).is_err());
        assert!(!root.join("preparation-effect").exists());
    }
    #[test]
    fn actual_sdk_uv_member_uses_its_authoritative_workspace_lock() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let empty = BTreeSet::new();
        let command = python_sync_command(&root, Path::new("sdk/python"), &empty, &empty).unwrap();
        assert_eq!(command.get_current_dir(), Some(root.join("sdk").as_path()));
        assert_eq!(
            command
                .get_args()
                .map(|v| v.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["sync", "--locked", "--package", "veoveo-mcp"]
        );
    }
    #[test]
    fn independent_python_module_lock_extras_and_groups_bind_before_effects() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        python_owner(root);
        let mut case = scenario(
            serde_json::json!({"kind":"python_module","module":"independent.run"}),
            serde_json::json!({"kind":"uv_sync","owner":"python","extras":["checks"],"groups":["verify"]}),
        );
        admit(root, Path::new("python"), &case).unwrap();
        case.target = HarnessTarget::PythonModule {
            module: "unrelated.run".into(),
        };
        assert!(admit(root, Path::new("python"), &case).is_err());
        assert!(
            python(
                root,
                Path::new("python"),
                &BTreeSet::from(["absent".into()]),
                &BTreeSet::new()
            )
            .is_err()
        );
        assert!(
            python(
                root,
                Path::new("python"),
                &BTreeSet::new(),
                &BTreeSet::from(["absent".into()])
            )
            .is_err()
        );
        std::fs::write(
            root.join("python/uv.lock"),
            "version=1\n[[package]]\nname='foreign'\nversion='1.0.0'\nsource={editable='.'}\n",
        )
        .unwrap();
        assert!(
            python(
                root,
                Path::new("python"),
                &BTreeSet::new(),
                &BTreeSet::new()
            )
            .is_err()
        );
        std::fs::remove_file(root.join("python/uv.lock")).unwrap();
        assert!(
            python(
                root,
                Path::new("python"),
                &BTreeSet::new(),
                &BTreeSet::new()
            )
            .is_err()
        );
        assert!(!root.join("preparation-effect").exists());
    }
    #[test]
    #[cfg(unix)]
    fn owned_manifest_admission_rejects_foreign_symlinks_and_nonregular_files() {
        use std::os::unix::fs::symlink;
        let owner = tempfile::tempdir().unwrap();
        let foreign = tempfile::tempdir().unwrap();
        std::fs::write(foreign.path().join("package.json"), "{}").unwrap();
        symlink(
            foreign.path().join("package.json"),
            owner.path().join("package.json"),
        )
        .unwrap();
        assert!(owned_bytes(owner.path(), "package.json").is_err());
        std::fs::create_dir(owner.path().join("uv.lock")).unwrap();
        assert!(owned_bytes(owner.path(), "uv.lock").is_err());
        assert!(!owner.path().join("preparation-effect").exists());
    }
    #[test]
    fn independent_node_manifest_lock_target_and_delegation_are_admitted() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        std::fs::create_dir(root.join("node")).unwrap();
        std::fs::write(root.join("node/check.mjs"), "console.log('owned');\n").unwrap();
        std::fs::write(
            root.join("node/package.json"),
            r#"{"name":"independent-node","version":"1.0.0","type":"module"}"#,
        )
        .unwrap();
        std::fs::write(root.join("node/package-lock.json"),r#"{"lockfileVersion":3,"packages":{"":{"name":"independent-node","version":"1.0.0"}}}"#).unwrap();
        let mut case = scenario(
            serde_json::json!({"kind":"node_module","file":"check.mjs"}),
            serde_json::json!({"kind":"npm_ci","owner":"node"}),
        );
        admit(root, Path::new("node"), &case).unwrap();
        case.prerequisites.clear();
        assert!(admit(root, Path::new("node"), &case).is_err());
        case.prerequisites.push(Preparation::NpmCi {
            owner: "node".into(),
        });
        case.target = HarnessTarget::NodeModule {
            file: "../foreign.mjs".into(),
        };
        assert!(admit(root, Path::new("node"), &case).is_err());
        case.target = HarnessTarget::NodeModule {
            file: "check.mjs".into(),
        };
        std::fs::create_dir(root.join("composition")).unwrap();
        case.execution_owner=Some(serde_json::from_value(serde_json::json!({"owner":"node","reason":"Composition delegates its maintained Node harness."})).unwrap());
        admit(root, Path::new("composition"), &case).unwrap();
        assert!(
            serde_json::from_value::<veoveo_testing_support::descriptor::ExecutionOwner>(
                serde_json::json!({"owner":"node","reason":" "})
            )
            .is_err()
        );
        std::fs::write(
            root.join("node/package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"name":"foreign","version":"1.0.0"}}}"#,
        )
        .unwrap();
        assert!(admit(root, Path::new("composition"), &case).is_err());
        std::fs::write(root.join("node/package.json"), "{}").unwrap();
        assert!(node(root, Path::new("node")).is_err());
    }
}
