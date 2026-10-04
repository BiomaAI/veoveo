//! Ask Cargo for the compiler family's actual unified production feature graph.
use super::*;

fn arguments(packages: &[String]) -> Vec<String> {
    let mut args = [
        "tree",
        "--locked",
        "--target",
        "x86_64-unknown-linux-gnu",
        "--edges",
        "normal,build",
        "--prefix",
        "none",
        "--format",
        "{p}",
    ]
    .map(str::to_owned)
    .to_vec();
    for package in packages {
        args.extend(["--package".into(), package.clone()]);
    }
    // Match the existing rust-workspace Dockerfile's package-qualified feature.
    if packages
        .iter()
        .any(|package| package == "veoveo-recording-mcp")
    {
        args.extend(["--features".into(), "veoveo-recording-mcp/redap".into()]);
    }
    args
}

pub(super) fn closure(
    repository: &Path,
    metadata: &CargoMetadata,
    packages: &[String],
) -> Result<BTreeSet<String>> {
    ensure!(
        !packages.is_empty(),
        "compiler family has no Cargo packages"
    );
    let output = process::output("cargo", arguments(packages), Some(repository))
        .context("resolving compiler-family Cargo production features")?;
    selected(
        metadata,
        packages,
        std::str::from_utf8(&output.stdout).context("Cargo tree is not UTF-8")?,
    )
}

fn display(package: &CargoPackage) -> Result<String> {
    let directory = package
        .manifest_path
        .parent()
        .context("Cargo manifest has no parent")?;
    let proc_macro = if package
        .targets
        .iter()
        .any(|target| target.kind.iter().any(|kind| kind == "proc-macro"))
    {
        " (proc-macro)"
    } else {
        ""
    };
    Ok(format!(
        "{} v{}{} ({})",
        package.name,
        package.version,
        proc_macro,
        directory.display()
    ))
}

fn selected(metadata: &CargoMetadata, roots: &[String], tree: &str) -> Result<BTreeSet<String>> {
    let local = metadata
        .packages
        .iter()
        .filter(|package| package.source.is_none())
        .map(|package| Ok((display(package)?, &package.id)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut selected = BTreeSet::new();
    for line in tree.lines() {
        let line = line.trim().strip_suffix(" (*)").unwrap_or(line.trim());
        if let Some(id) = local.get(line) {
            selected.insert((*id).clone());
        } else if let Some((_, directory)) = line.rsplit_once(" (") {
            let directory = directory.strip_suffix(')').unwrap_or(directory);
            ensure!(
                !Path::new(directory).is_absolute(),
                "Cargo tree local package is absent from metadata: {line}"
            );
        }
    }
    for root in roots {
        ensure!(
            metadata
                .packages
                .iter()
                .any(|package| package.source.is_none()
                    && &package.name == root
                    && selected.contains(&package.id)),
            "Cargo tree omitted selected local package {root}"
        );
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_display_preserves_versions_spaces_proc_macros_and_refuses_unknown_local_nodes() {
        let root = Path::new("/source with spaces");
        let mut metadata = super::super::tests::fixture_metadata(root);
        metadata.packages[0].targets[0].kind = vec!["proc-macro".into()];
        let tree = format!(
            "{} (*)\nshared v2.0.0 (/other/shared)\n",
            display(&metadata.packages[0]).unwrap()
        );
        assert!(selected(&metadata, &["bff".into()], &tree).is_err());
        let tree = format!(
            "{} (*)\nshared v2.0.0\n",
            display(&metadata.packages[0]).unwrap()
        );
        assert_eq!(
            selected(&metadata, &["bff".into()], &tree).unwrap(),
            BTreeSet::from(["bff".into()])
        );
    }

    fn package(root: &Path, name: &str, extra: &str, proc_macro: bool) {
        let directory = root.join(name);
        fs::create_dir_all(directory.join("src")).unwrap();
        fs::write(
            directory.join("Cargo.toml"),
            format!(
                "[package]\nname='{name}'\nversion='0.1.0'\nedition='2024'\n{}\n{extra}",
                if proc_macro {
                    "[lib]\nproc-macro=true"
                } else {
                    ""
                }
            ),
        )
        .unwrap();
        fs::write(directory.join("src/lib.rs"), "").unwrap();
    }

    #[test]
    fn real_cargo_selection_unifies_family_features_without_disabled_optional_back_edges() {
        let directory = tempfile::Builder::new()
            .prefix("veoveo feature graph ")
            .tempdir()
            .unwrap();
        let root = directory.path();
        fs::write(root.join("Cargo.toml"), "[workspace]\nresolver='3'\nmembers=['app','composition','bridge','owner','kernel','build-helper','default-helper','proc-helper','veoveo-recording-mcp','redap-helper']").unwrap();
        package(
            root,
            "app",
            "[features]\ndefault=['dep:default-helper']\n[dependencies]\ndefault-helper={path='../default-helper',optional=true}\nbridge={path='../bridge',default-features=false,features=['leaf']}\nproc-helper={path='../proc-helper'}\n[build-dependencies]\nbuild-helper={path='../build-helper'}",
            false,
        );
        package(
            root,
            "composition",
            "[dependencies]\nbridge={path='../bridge',default-features=false,features=['gateway']}",
            false,
        );
        package(
            root,
            "bridge",
            "[features]\ndefault=[]\nleaf=['dep:owner']\ngateway=['owner?/gateway']\n[dependencies]\nowner={path='../owner',optional=true,default-features=false}",
            false,
        );
        package(
            root,
            "owner",
            "[features]\ndefault=[]\ngateway=['dep:kernel']\n[dependencies]\nkernel={path='../kernel',optional=true}",
            false,
        );
        package(
            root,
            "veoveo-recording-mcp",
            "[features]\ndefault=[]\nredap=['dep:redap-helper']\n[dependencies]\nredap-helper={path='../redap-helper',optional=true}",
            false,
        );
        for name in ["kernel", "build-helper", "default-helper", "redap-helper"] {
            package(root, name, "", false);
        }
        package(root, "proc-helper", "", true);
        process::output("cargo", ["generate-lockfile", "--offline"], Some(root)).unwrap();
        let metadata = super::super::metadata(root).unwrap();
        let names = |selected: BTreeSet<String>| source_packages(&metadata, &selected).unwrap();
        let app = names(closure(root, &metadata, &["app".into()]).unwrap());
        for name in [
            "app",
            "bridge",
            "owner",
            "build-helper",
            "default-helper",
            "proc-helper",
        ] {
            assert!(
                app.iter().any(|candidate| candidate == name),
                "missing {name}"
            );
        }
        assert!(!app.iter().any(|name| name == "kernel"));
        assert!(
            !names(closure(root, &metadata, &["composition".into()]).unwrap())
                .iter()
                .any(|name| name == "kernel")
        );
        assert!(
            names(closure(root, &metadata, &["app".into(), "composition".into()]).unwrap())
                .iter()
                .any(|name| name == "kernel")
        );
        assert!(
            names(closure(root, &metadata, &["veoveo-recording-mcp".into()]).unwrap())
                .iter()
                .any(|name| name == "redap-helper")
        );
    }
}
