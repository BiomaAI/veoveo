//! Resolve real Cargo selections before effects; a candidate union must preserve each closure.
use super::{
    budget::Budget,
    cargo::{CargoMetadata, CargoPackage},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};
use veoveo_testing_support::{artifacts::EffectiveFeatures, descriptor::CargoSelection};
#[derive(Clone, Debug)]
pub(crate) struct BuildGroup {
    pub roots: Vec<(CargoSelection, bool)>,
    pub effective: EffectiveFeatures,
    pub target_features: EffectiveFeatures,
}
pub(crate) fn root_arguments(
    metadata: &CargoMetadata,
    roots: &[(CargoSelection, bool)],
) -> Vec<String> {
    let mut args = vec!["--no-default-features".into()];
    let mut packages = BTreeSet::new();
    let mut features = BTreeSet::new();
    for (root, _) in roots {
        if packages.insert(root.package.as_str()) {
            args.extend(["--package".into(), root.package.to_string()]);
        }
        for feature in &root.features {
            features.insert(format!("{}/{feature}", root.package));
        }
        if root.default_features
            && metadata
                .packages
                .iter()
                .any(|p| p.name == root.package.as_str() && p.features.contains_key("default"))
        {
            features.insert(format!("{}/default", root.package));
        }
    }
    if !features.is_empty() {
        args.extend([
            "--features".into(),
            features.into_iter().collect::<Vec<_>>().join(","),
        ]);
    }
    args
}
fn package_label(package: &CargoPackage) -> Result<String> {
    let macro_suffix = if package
        .targets
        .iter()
        .any(|t| t.kind.iter().any(|k| k == "proc-macro"))
    {
        " (proc-macro)"
    } else {
        ""
    };
    let suffix = if package.source.is_none() {
        format!(
            " ({})",
            package
                .manifest_path
                .parent()
                .context("manifest parent missing")?
                .display()
        )
    } else if let Some(source) = package.source.as_ref().and_then(|s| s.strip_prefix("git+")) {
        // Keep the full metadata identity; Cargo may display a revision prefix.
        let (url, commit) = source
            .rsplit_once('#')
            .context("git source lacks revision")?;
        ensure!(
            !commit.is_empty() && commit.bytes().all(|c| c.is_ascii_hexdigit()),
            "invalid metadata git revision"
        );
        format!(" ({url}#{commit})")
    } else {
        String::new()
    };
    Ok(format!(
        "{} v{}{}{}",
        package.name, package.version, macro_suffix, suffix
    ))
}
pub(crate) fn decode(metadata: &CargoMetadata, bytes: &[u8]) -> Result<EffectiveFeatures> {
    let mut identities = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut candidates: BTreeMap<String, Vec<(&CargoPackage, String)>> = BTreeMap::new();
    for package in &metadata.packages {
        let identity = package_label(package)?;
        ensure!(
            identities.insert(identity.clone()) && ids.insert(&package.id),
            "ambiguous Cargo source identity"
        );
        candidates
            .entry(format!("{} v{}", package.name, package.version))
            .or_default()
            .push((package, identity));
    }
    let mut result = EffectiveFeatures::new();
    for line in std::str::from_utf8(bytes)?
        .lines()
        .filter(|l| !l.is_empty())
    {
        // Cargo appends this marker when the same dependency subtree was already shown.
        let line = line.strip_suffix(" (*)").unwrap_or(line);
        let (label, features) = line.rsplit_once("|veoveo-features|").with_context(|| {
            format!(
                "malformed Cargo feature receipt line: {:?}",
                line.chars().take(256).collect::<String>()
            )
        })?;
        let name_version = label.split_once(" (").map_or(label, |(head, _)| head);
        let mut matching =
            candidates
                .get(name_version)
                .into_iter()
                .flatten()
                .filter(|(package, identity)| {
                    let Some(source) = package.source.as_ref().and_then(|s| s.strip_prefix("git+"))
                    else {
                        return identity == label;
                    };
                    // package_label has already admitted the full metadata revision.
                    let (_, revision) = source.rsplit_once('#').unwrap();
                    let prefix = identity.strip_suffix(&format!("{revision})")).unwrap();
                    let Some(displayed) =
                        label.strip_prefix(prefix).and_then(|s| s.strip_suffix(')'))
                    else {
                        return false;
                    };
                    !displayed.is_empty()
                        && displayed.bytes().all(|c| c.is_ascii_hexdigit())
                        && revision.starts_with(displayed)
                });
        let (package, _) = matching
            .next()
            .context("Cargo tree package identity disagrees with metadata")?;
        ensure!(matching.next().is_none(), "ambiguous Cargo source identity");
        let id = &package.id;
        let features: BTreeSet<String> = features
            .split(',')
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(|feature| {
                ensure!(
                    !feature.contains(['(', ')', '*']) && !feature.chars().any(char::is_whitespace),
                    "malformed Cargo feature receipt suffix"
                );
                Ok(feature.to_owned())
            })
            .collect::<Result<_>>()?;
        result.entry(id.clone()).or_default().insert(features);
    }
    ensure!(!result.is_empty(), "empty effective Cargo closure");
    Ok(result)
}
fn resolve(
    repository: &Path,
    metadata: &CargoMetadata,
    budget: &Budget,
    roots: &[(CargoSelection, bool)],
    target_projection: bool,
) -> Result<EffectiveFeatures> {
    ensure!(
        !roots.is_empty() && roots.iter().all(|(_, test)| *test == roots[0].1),
        "incompatible Cargo target mode"
    );
    let mut command = Command::new("cargo");
    command.current_dir(repository).args([
        "tree",
        "--offline",
        "--locked",
        "--prefix",
        "none",
        "--format",
        "{p}|veoveo-features|{f}",
        "--edges",
        match (roots[0].1, target_projection) {
            (true, true) => "normal,dev,no-proc-macro",
            (false, true) => "normal,no-proc-macro",
            (true, false) => "normal,build,dev",
            (false, false) => "normal,build",
        },
    ]);
    // Cargo accepts an absent default feature as an empty selection only when it is omitted.
    let mut admitted = roots.to_vec();
    for (root, _) in &mut admitted {
        if !metadata
            .packages
            .iter()
            .find(|p| p.name == root.package.as_str())
            .context("selected root missing")?
            .features
            .contains_key("default")
        {
            root.default_features = false;
        }
    }
    command.args(root_arguments(metadata, &admitted));
    crate::process::remove_parent_cargo_package_environment(&mut command);
    let effective = decode(metadata, &budget.output(command)?.stdout)?;
    for (root, _) in roots {
        let package = metadata
            .packages
            .iter()
            .find(|p| p.name == root.package.as_str())
            .context("root missing")?;
        let sets = effective
            .get(&package.id)
            .context("Cargo tree omitted selected root")?;
        ensure!(
            sets.len() == 1,
            "ambiguous root host/target feature identity"
        );
        let target = package
            .targets
            .iter()
            .find(|t| {
                t.name == root.target
                    && t.kind
                        .iter()
                        .any(|kind| kind == if roots[0].1 { "test" } else { "bin" })
            })
            .context("target missing")?;
        ensure!(
            target
                .required_features
                .iter()
                .all(|f| sets.iter().next().unwrap().contains(f)),
            "effective target required features are missing"
        );
    }
    Ok(effective)
}
fn merge(a: &EffectiveFeatures, b: &EffectiveFeatures) -> EffectiveFeatures {
    let mut union = a.clone();
    for (id, sets) in b {
        union.entry(id.clone()).or_default().extend(sets.clone());
    }
    union
}
/// The dispatcher uses this exact union predicate; tests do not implement a second graph checker.
pub(crate) fn compatible(
    a: &EffectiveFeatures,
    b: &EffectiveFeatures,
    candidate: &EffectiveFeatures,
) -> bool {
    &merge(a, b) == candidate
}
pub(crate) fn plan(
    repository: &Path,
    metadata: &CargoMetadata,
    budget: &Budget,
    roots: Vec<(CargoSelection, bool)>,
) -> Result<Vec<BuildGroup>> {
    let mut identities = BTreeMap::new();
    for (root, test) in &roots {
        let key = (root.package.clone(), root.target.clone(), *test);
        if let Some(previous) = identities.insert(key, root.clone()) {
            ensure!(
                &previous == root,
                "conflicting executable identity before preparation"
            );
        }
    }
    let mut groups: Vec<BuildGroup> = Vec::new();
    for (root, test) in roots {
        if groups
            .iter()
            .any(|g| g.roots.contains(&(root.clone(), test)))
        {
            continue;
        }
        let single = resolve(repository, metadata, budget, &[(root.clone(), test)], false)?;
        let target_single = resolve(repository, metadata, budget, &[(root.clone(), test)], true)?;
        ensure!(
            target_single.values().all(|sets| sets.len() == 1),
            "ambiguous native target feature context; declare and qualify an explicit execution profile"
        );
        let mut placed = false;
        for group in &mut groups {
            if group.roots[0].1 != test {
                continue;
            }
            let mut proposed = group.roots.clone();
            proposed.push((root.clone(), test));
            let candidate = resolve(repository, metadata, budget, &proposed, false)?;
            let target_candidate = resolve(repository, metadata, budget, &proposed, true)?;
            if compatible(&group.effective, &single, &candidate)
                && compatible(&group.target_features, &target_single, &target_candidate)
            {
                group.roots = proposed;
                group.effective = candidate;
                group.target_features = target_candidate;
                placed = true;
                break;
            }
        }
        if !placed {
            groups.push(BuildGroup {
                roots: vec![(root, test)],
                effective: single,
                target_features: target_single,
            });
        }
    }
    Ok(groups)
}
#[cfg(test)]
mod tests {
    use super::*;
    const GIT_URL: &str =
        "https://github.com/rozgo/rust-sdk?rev=917e7914c93975fc1eddbff8792f1e2de933bc17";
    const GIT_REVISION: &str = "917e7914c93975fc1eddbff8792f1e2de933bc17";

    fn git_package(name: &str, procedural_macro: bool, revision: &str) -> CargoPackage {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "version": "3.5.0",
            "id": format!("{name}@{revision}"),
            "source": format!("git+{GIT_URL}#{revision}"),
            "manifest_path": format!("/fixture/{name}/Cargo.toml"),
            "targets": [{
                "name": name,
                "kind": [if procedural_macro { "proc-macro" } else { "lib" }],
                "src_path": format!("/fixture/{name}/src/lib.rs")
            }]
        }))
        .unwrap()
    }

    fn metadata(packages: Vec<CargoPackage>) -> CargoMetadata {
        CargoMetadata {
            packages,
            target_directory: Default::default(),
        }
    }

    #[test]
    fn git_tree_labels_bind_abbreviated_and_full_revisions_for_library_and_macro() {
        let metadata = metadata(vec![
            git_package("rmcp", false, GIT_REVISION),
            git_package("rmcp-macros", true, GIT_REVISION),
        ]);
        for revision in [&GIT_REVISION[..7], &GIT_REVISION[..8], GIT_REVISION] {
            let receipt = format!(
                "rmcp v3.5.0 ({GIT_URL}#{revision})|veoveo-features|server\n\
                 rmcp-macros v3.5.0 (proc-macro) ({GIT_URL}#{revision})|veoveo-features|default\n"
            );
            let admitted = decode(&metadata, receipt.as_bytes()).unwrap();
            assert_eq!(admitted.len(), 2);
            assert_eq!(
                admitted[&metadata.packages[0].id],
                BTreeSet::from([BTreeSet::from(["server".to_owned()])])
            );
            assert_eq!(
                admitted[&metadata.packages[1].id],
                BTreeSet::from([BTreeSet::from(["default".to_owned()])])
            );
        }
    }
    #[test]
    fn repeat_markers_admit_only_the_exact_terminal_cargo_suffix() {
        let metadata = metadata(vec![git_package("rmcp", false, GIT_REVISION)]);
        let label = format!("rmcp v3.5.0 ({GIT_URL}#917e7914)");
        for features in ["", "server"] {
            let plain = format!("{label}|veoveo-features|{features}");
            let repeated = format!("{plain} (*)");
            assert_eq!(
                decode(&metadata, plain.as_bytes()).unwrap(),
                decode(&metadata, repeated.as_bytes()).unwrap()
            );
        }
        for receipt in [
            format!("{label}|veoveo-features|server (*) (*)"),
            format!("{label}|veoveo-features|server (*) trailing"),
            format!("{label}|veoveo-features|server (* )"),
            format!("{label}|veoveo-features|server (**)"),
            format!("{label}|veoveo-features|server(*)"),
            format!("{label}|veoveo-features|server (*"),
            format!("{label} (*)|veoveo-features|server"),
            format!("{label} (*)"),
        ] {
            assert!(decode(&metadata, receipt.as_bytes()).is_err(), "{receipt}");
        }
    }

    #[test]
    fn git_tree_labels_refuse_foreign_sources_and_invalid_revisions() {
        let metadata = metadata(vec![git_package("rmcp", false, GIT_REVISION)]);
        for label in [
            format!("rmcp v3.5.0 ({GIT_URL}#deadbeef)"),
            format!("rmcp v3.5.0 ({GIT_URL}#{GIT_REVISION}0)"),
            format!("rmcp v3.5.0 ({GIT_URL}#)"),
            format!("rmcp v3.5.0 ({GIT_URL}#917e791z)"),
            format!("rmcp v3.5.0 (https://foreign.invalid/rust-sdk?rev={GIT_REVISION}#917e7914)"),
            "rmcp v3.5.0 (https://github.com/rozgo/rust-sdk?branch=main#917e7914)".into(),
            format!("rmcp v3.5.0 (proc-macro) ({GIT_URL}#917e7914)"),
            format!("rmcp v3.5.1 ({GIT_URL}#917e7914)"),
            format!("other v3.5.0 ({GIT_URL}#917e7914)"),
        ] {
            assert!(
                decode(
                    &metadata,
                    format!("{label}|veoveo-features|server").as_bytes()
                )
                .is_err(),
                "{label}"
            );
        }
    }

    #[test]
    fn git_tree_labels_refuse_ambiguous_prefixes_and_duplicate_metadata() {
        let other = "917e7914c93975fc1eddbff8792f1e2de933bc18";
        let metadata = metadata(vec![
            git_package("rmcp", false, GIT_REVISION),
            git_package("rmcp", false, other),
        ]);
        assert!(
            decode(
                &metadata,
                format!("rmcp v3.5.0 ({GIT_URL}#917e7914)|veoveo-features|server").as_bytes()
            )
            .is_err()
        );
        assert!(
            decode(
                &metadata,
                format!("rmcp v3.5.0 ({GIT_URL}#{GIT_REVISION})|veoveo-features|server").as_bytes()
            )
            .is_ok()
        );
        let mut duplicate = git_package("rmcp", false, GIT_REVISION);
        duplicate.id = "different-id".into();
        let duplicate_metadata =
            super::tests::metadata(vec![git_package("rmcp", false, GIT_REVISION), duplicate]);
        assert!(
            decode(
                &duplicate_metadata,
                format!("rmcp v3.5.0 ({GIT_URL}#917e7914)|veoveo-features|server").as_bytes()
            )
            .is_err()
        );
        let mut duplicate_id = git_package("rmcp", false, other);
        duplicate_id.id = format!("rmcp@{GIT_REVISION}");
        let duplicate_metadata =
            super::tests::metadata(vec![git_package("rmcp", false, GIT_REVISION), duplicate_id]);
        assert!(
            decode(
                &duplicate_metadata,
                format!("rmcp v3.5.0 ({GIT_URL}#{GIT_REVISION})|veoveo-features|server").as_bytes()
            )
            .is_err()
        );
    }

    #[test]
    fn local_tree_labels_require_the_exact_source_path() {
        let mut package = git_package("local", false, GIT_REVISION);
        package.source = None;
        let metadata = metadata(vec![package]);
        assert!(
            decode(
                &metadata,
                b"local v3.5.0 (/fixture/local)|veoveo-features|default"
            )
            .is_ok()
        );
        assert!(
            decode(
                &metadata,
                b"local v3.5.0 (/foreign/local)|veoveo-features|default"
            )
            .is_err()
        );
    }

    #[test]
    fn dispatcher_union_admission_refuses_indirect_and_two_root_contamination() {
        let leaf: EffectiveFeatures = [(
            "bridge".into(),
            [BTreeSet::from(["declaration".into()])].into(),
        )]
        .into();
        let other: EffectiveFeatures = [("owner".into(), [BTreeSet::new()].into())].into();
        let mut contaminated = merge(&leaf, &other);
        contaminated.insert(
            "bridge".into(),
            [BTreeSet::from(["declaration".into(), "runtime".into()])].into(),
        );
        assert!(!compatible(&leaf, &other, &contaminated));
        assert!(compatible(&leaf, &other, &merge(&leaf, &other)));
        let mut indirect = merge(&leaf, &other);
        indirect.insert("runtime".into(), [BTreeSet::new()].into());
        assert!(!compatible(&leaf, &other, &indirect));
    }
}

#[cfg(test)]
mod native_graph_tests {
    use super::*;
    use veoveo_testing_support::descriptor::{BuildProfile, PackageName};
    fn package(root: &Path, name: &str, extra: &str, bin: bool) {
        let owner = root.join(name);
        std::fs::create_dir_all(owner.join("src")).unwrap();
        std::fs::write(
            owner.join("Cargo.toml"),
            format!("[package]\nname='{name}'\nversion='1.0.0'\nedition='2024'\n{extra}\n"),
        )
        .unwrap();
        std::fs::write(
            owner.join(if bin { "src/main.rs" } else { "src/lib.rs" }),
            if bin { "fn main() {}" } else { "" },
        )
        .unwrap();
    }
    fn selection(name: &str) -> CargoSelection {
        CargoSelection {
            owner: name.into(),
            package: PackageName::parse(name).unwrap(),
            target: name.into(),
            features: BTreeSet::new(),
            default_features: true,
            profile: BuildProfile::Dev,
        }
    }
    fn assert_deduplicated_tree_matches_expanded(
        root: &Path,
        metadata: &CargoMetadata,
        budget: &Budget,
        roots: &[(CargoSelection, bool)],
    ) {
        for target_projection in [false, true] {
            let edges = match (roots[0].1, target_projection) {
                (true, true) => "normal,dev,no-proc-macro",
                (false, true) => "normal,no-proc-macro",
                (true, false) => "normal,build,dev",
                (false, false) => "normal,build",
            };
            let tree = |expanded: bool| {
                let mut command = Command::new("cargo");
                command.current_dir(root).args([
                    "tree",
                    "--offline",
                    "--locked",
                    "--prefix",
                    "none",
                    "--format",
                    "{p}|veoveo-features|{f}",
                    "--edges",
                    edges,
                ]);
                if expanded {
                    command.arg("--no-dedupe");
                }
                command.args(root_arguments(metadata, roots));
                crate::process::remove_parent_cargo_package_environment(&mut command);
                budget.output(command).unwrap().stdout
            };
            let normal = tree(false);
            let expanded = tree(true);
            assert!(std::str::from_utf8(&normal).unwrap().contains(" (*)"));
            assert_eq!(
                decode(metadata, &normal).unwrap(),
                decode(metadata, &expanded).unwrap()
            );
        }
    }
    #[test]
    fn wired_plan_groups_real_default_transitive_roots_and_separates_isolated_runtime_union() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nresolver='3'\nmembers=['leaf','friend','other','bridge','runtime','descendant']\n",
        )
        .unwrap();
        package(
            root,
            "leaf",
            "[features]\ndefault=['operation']\noperation=['nested']\nnested=[]\n[[bin]]\nname='leaf'\npath='src/main.rs'\nrequired-features=['nested']\n[dependencies]\nbridge={path='../bridge',default-features=false,features=['declaration']}",
            true,
        );
        package(
            root,
            "friend",
            "[dependencies]\nbridge={path='../bridge',default-features=false,features=['declaration']}",
            true,
        );
        package(
            root,
            "other",
            "[dependencies]\nbridge={path='../bridge',default-features=false,features=['runtime']}",
            true,
        );
        package(
            root,
            "bridge",
            "[features]\ndefault=[]\ndeclaration=[]\nruntime=['dep:runtime']\n[dependencies]\ndescendant={path='../descendant'}\nruntime={path='../runtime',optional=true}",
            false,
        );
        package(root, "runtime", "", false);
        package(root, "descendant", "", false);
        let budget = Budget::new(
            &std::env::var_os("CARGO_TARGET_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target")
                }),
            60,
            2,
        )
        .unwrap();
        let mut lock = Command::new("cargo");
        lock.args(["generate-lockfile", "--offline"])
            .current_dir(root);
        budget.output(lock).unwrap();
        let metadata = super::super::cargo::inventory(root, &budget).unwrap();
        let roots = vec![
            (selection("leaf"), false),
            (selection("friend"), false),
            (selection("other"), false),
        ];
        let groups = plan(root, &metadata, &budget, roots).unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].roots.len(), 2);
        let mut conflicting = selection("leaf");
        conflicting.default_features = false;
        assert!(
            plan(
                root,
                &metadata,
                &budget,
                vec![(selection("leaf"), false), (conflicting, false)]
            )
            .is_err()
        );
        assert_deduplicated_tree_matches_expanded(
            root,
            &metadata,
            &budget,
            &[(selection("leaf"), false), (selection("friend"), false)],
        );
        assert!(!root.join("preparation-effect").exists());
        budget.completed();
    }
    #[test]
    fn wired_plan_preserves_shared_package_host_build_and_test_dev_features() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nresolver='3'\nmembers=['probe','shared','derive_probe','descendant']\n",
        )
        .unwrap();
        package(
            root,
            "shared",
            "[features]\ndefault=[]\ntarget=[]\nhost=[]\ndev=[]\n[dependencies]\ndescendant={path='../descendant'}",
            false,
        );
        package(root, "descendant", "", false);
        package(
            root,
            "derive_probe",
            "[lib]\nproc-macro=true\n[dependencies]\nshared={path='../shared',default-features=false,features=['host']}",
            false,
        );
        package(
            root,
            "probe",
            "[dependencies]\nshared={path='../shared',default-features=false,features=['target']}\nderive_probe={path='../derive_probe'}\n[build-dependencies]\nshared={path='../shared',default-features=false,features=['host']}\n[dev-dependencies]\nshared={path='../shared',default-features=false,features=['dev']}\n[[test]]\nname='probe'\npath='tests/probe.rs'",
            true,
        );
        std::fs::create_dir(root.join("probe/tests")).unwrap();
        std::fs::write(root.join("probe/tests/probe.rs"), "#[test] fn owned() {}\n").unwrap();
        std::fs::write(root.join("probe/build.rs"), "fn main() {}\n").unwrap();
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target")
            });
        let budget = Budget::new(&target, 60, 2).unwrap();
        let mut lock = Command::new("cargo");
        lock.args(["generate-lockfile", "--offline"])
            .current_dir(root);
        budget.output(lock).unwrap();
        let metadata = super::super::cargo::inventory(root, &budget).unwrap();
        let groups = plan(root, &metadata, &budget, vec![(selection("probe"), true)]).unwrap();
        let id = &metadata
            .packages
            .iter()
            .find(|p| p.name == "shared")
            .unwrap()
            .id;
        assert_eq!(
            groups[0].target_features[id],
            BTreeSet::from([BTreeSet::from(["dev".into(), "target".into()])])
        );
        assert!(groups[0].effective[id].contains(&BTreeSet::from(["host".into()])));
        assert_deduplicated_tree_matches_expanded(
            root,
            &metadata,
            &budget,
            &[(selection("probe"), true)],
        );
        budget.completed();
    }
}
