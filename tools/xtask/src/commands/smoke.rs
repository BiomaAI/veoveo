//! Dispatch discovered owner targets. No server/scenario vocabulary is compiled here.
use crate::{
    context::RepositoryContext,
    discovery::{cargo, features, smoke as discovery},
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};
use veoveo_testing_support::{
    artifacts::{
        ArtifactEntry, ArtifactFormat, ArtifactManifest, CompilerContext, NativeTargetKind,
        ObservedCompilerArtifact, RuntimeLibrary, configure_runtime,
    },
    descriptor::{CargoSelection, HarnessTarget, Preparation, ScenarioId},
};

pub(crate) fn run(context: &RepositoryContext, arguments: &[OsString]) -> Result<()> {
    let repository = context.root();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| repository.join("target"));
    // Source inventory/help never execute an owner or unbounded Cargo operation.
    let mut budget = crate::discovery::budget::Budget::new(&target, 30, 5)?;
    let scenarios = discovery::discover(repository, &budget)?;
    let Some(name) = arguments.first().and_then(|s| s.to_str()) else {
        let result = list(&scenarios);
        budget.completed();
        return result;
    };
    if matches!(name, "--help" | "-h" | "list") {
        let result = list(&scenarios);
        budget.completed();
        return result;
    }
    let id = ScenarioId::parse(name)?;
    let selected = scenarios
        .get(&id)
        .context("unknown discovered smoke scenario")?;
    let scenario = &selected.descriptor.scenarios[selected.index];
    let help = arguments[1..].iter().any(|a| a == "--help" || a == "-h");
    if help {
        println!("{}: {}", scenario.id, scenario.description);
        println!("target: {}", serde_json::to_string(&scenario.target)?);
        println!(
            "requirements: {}",
            serde_json::to_string(&scenario.requirements)?
        );
        println!(
            "execution budget: {}s; local cleanup budget: {}s",
            scenario.deadline_seconds, scenario.cleanup_seconds
        );
        budget.completed();
        return Ok(());
    }
    budget.select(scenario.deadline_seconds, scenario.cleanup_seconds)?;
    if matches!(
        scenario.target,
        HarnessTarget::CargoTest { .. }
            | HarnessTarget::Pytest { .. }
            | HarnessTarget::NodeTest { .. }
    ) {
        veoveo_testing_support::framework::admit_exact_arguments(&scenario.arguments)?;
        let user = arguments[1..]
            .iter()
            .map(|a| {
                a.to_str()
                    .context("non-UTF8 framework argv")
                    .map(str::to_owned)
            })
            .collect::<Result<Vec<_>>>()?;
        veoveo_testing_support::framework::admit_exact_arguments(&user)?;
    }
    let metadata = cargo::inventory(repository, &budget)?;
    let mut roots = Vec::new();
    for prerequisite in &scenario.prerequisites {
        match prerequisite {
            Preparation::CargoBinary { selection } => roots.push((selection.clone(), false)),
            Preparation::CargoTest { selection } => roots.push((selection.clone(), true)),
            _ => {}
        }
    }
    match &scenario.target {
        HarnessTarget::CargoBinary { selection } => roots.push((selection.clone(), false)),
        HarnessTarget::CargoTest { selection, .. } => roots.push((selection.clone(), true)),
        _ => {}
    }
    for (selection, test) in &roots {
        discovery::admit_cargo(
            repository,
            &metadata,
            selection,
            if *test { "test" } else { "bin" },
        )?;
    }
    // Resolve and admit every selection/candidate union before any build/install prerequisite.
    let groups = features::plan(repository, &metadata, &budget, roots)?;
    let mut targets = BTreeMap::new();
    let mut manifest = ArtifactManifest {
        format: ArtifactFormat::V1,
        repository: repository.canonicalize()?,
        target_root: metadata.target_directory.clone(),
        entries: Vec::new(),
    };
    for group in &groups {
        prepare_group(
            &budget,
            repository,
            &metadata,
            group,
            &mut targets,
            &mut manifest,
        )?;
    }
    manifest.target_root = metadata.target_directory.canonicalize()?;
    manifest.admit(repository)?;
    if !help {
        for prerequisite in &scenario.prerequisites {
            match prerequisite {
                Preparation::CargoBinary { .. } | Preparation::CargoTest { .. } => {}
                Preparation::UvSync {
                    owner,
                    extras,
                    groups,
                } => {
                    let command = crate::discovery::languages::python_sync_command(
                        repository, owner, extras, groups,
                    )?;
                    budget.output(command)?;
                }
                Preparation::NpmCi { owner } => {
                    let mut command = Command::new("npm");
                    command.arg("ci").current_dir(repository.join(owner));
                    budget.output(command)?;
                }
            }
        }
    }
    let directory = budget.path();
    let framework_report = directory.join("framework.json");
    let framework_stdout = directory.join("framework.stdout");
    let mut framework_selection = None;
    let mut command = match &scenario.target {
        HarnessTarget::CargoBinary { selection } | HarnessTarget::CargoTest { selection, .. } => {
            let test = matches!(&scenario.target, HarnessTarget::CargoTest { .. });
            let entry = manifest.selected(
                selection,
                if test {
                    NativeTargetKind::Test
                } else {
                    NativeTargetKind::Binary
                },
            )?;
            let mut command = Command::new(&entry.executable);
            configure_runtime(&mut command, entry)?;
            if let HarnessTarget::CargoTest { case, ignored, .. } = &scenario.target {
                exact_rust_case(&budget, entry, case)?;
                command.args([case, "--exact", "--show-output", "--format", "pretty"]);
                command.stdout(std::fs::File::create(&framework_stdout)?);
                framework_selection = Some((
                    veoveo_testing_support::framework::Framework::Libtest,
                    case.clone(),
                ));
                if *ignored {
                    command.arg("--ignored");
                }
            }
            command
        }
        HarnessTarget::PythonModule { module } => {
            let mut c = Command::new("uv");
            c.args(["run", "--locked", "--no-sync", "python", "-m", module]);
            c
        }
        HarnessTarget::Pytest { file, case } => {
            let node_id = format!("{}::{case}", file.display());
            framework_selection = Some((
                veoveo_testing_support::framework::Framework::Pytest,
                node_id.clone(),
            ));
            let mut c = Command::new("uv");
            c.args(["run", "--locked", "--no-sync", "python"])
                .arg(repository.join("testing/support/framework/run_pytest.py"))
                .arg(node_id)
                .arg(&framework_report);
            c
        }
        HarnessTarget::NodeModule { file } => {
            let mut c = Command::new("node");
            c.arg(file);
            c
        }
        HarnessTarget::NodeTest { file, case } => {
            framework_selection = Some((
                veoveo_testing_support::framework::Framework::Node,
                case.clone(),
            ));
            let mut c = Command::new("node");
            c.arg(repository.join("testing/support/framework/node.mjs"))
                .arg(file)
                .arg(case)
                .arg(&framework_report);
            c
        }
    };
    manifest.target_root = metadata.target_directory.canonicalize()?;
    manifest.admit(repository)?;
    let file = directory.join("artifacts.json");
    std::fs::write(&file, serde_json::to_vec(&manifest)?)?;
    let groups = directory.join("groups");

    command
        .current_dir(match &scenario.target {
            HarnessTarget::CargoBinary { .. } | HarnessTarget::CargoTest { .. } => {
                repository.to_owned()
            }
            _ => repository.join(
                scenario
                    .execution_owner
                    .as_ref()
                    .map(|declaration| declaration.owner.as_path())
                    .unwrap_or(selected.descriptor.owner.as_path()),
            ),
        })
        .env("VEOVEO_SMOKE_ARTIFACTS", &file)
        .env("VEOVEO_SMOKE_LOCAL_GROUPS", &groups);
    crate::process::remove_parent_cargo_package_environment(&mut command);
    command.args(&scenario.arguments).args(&arguments[1..]);
    if help {
        ensure!(budget.finish(command)?.success(), "owner help failed");
        budget.completed();
        return Ok(());
    }
    let status = budget.finish(command);
    let completion = (|| -> Result<()> {
        let status = status?;
        if let Some((framework, case)) = framework_selection {
            let outcome = match framework {
                veoveo_testing_support::framework::Framework::Libtest => {
                    veoveo_testing_support::framework::libtest_summary(
                        &std::fs::read(&framework_stdout)?,
                        &case,
                    )?
                }
                _ => veoveo_testing_support::framework::FrameworkOutcome::read(
                    &framework_report,
                    framework,
                    &case,
                )?,
            };
            if framework == veoveo_testing_support::framework::Framework::Libtest {
                std::fs::write(&framework_report, serde_json::to_vec(&outcome)?)?;
            }
        }
        ensure!(
            status.success(),
            "selected owner harness failed; consult its own result"
        );
        Ok(())
    })();
    if let Err(error) = completion {
        let retained = directory.to_owned();
        budget.retain();
        return Err(error.context(format!(
            "owner cleanup/results retained at {}",
            retained.display()
        )));
    }
    budget.completed();
    Ok(())
}
fn list(scenarios: &BTreeMap<ScenarioId, discovery::DiscoveredScenario>) -> Result<()> {
    for (id, owner) in scenarios {
        let s = &owner.descriptor.scenarios[owner.index];
        println!(
            "{id}: {} ({})",
            s.description,
            owner.descriptor.owner.display()
        );
    }
    Ok(())
}
fn prepare_group(
    budget: &crate::discovery::budget::Budget,
    repository: &Path,
    metadata: &cargo::CargoMetadata,
    group: &features::BuildGroup,
    selected: &mut BTreeMap<(String, String, NativeTargetKind), CargoSelection>,
    manifest: &mut ArtifactManifest,
) -> Result<()> {
    let test = group.roots[0].1;
    let mut args = vec![
        if test { "test" } else { "build" }.to_string(),
        "--locked".into(),
        "--offline".into(),
        "--message-format=json-render-diagnostics".into(),
    ];
    args.extend(features::root_arguments(metadata, &group.roots));
    if test {
        args.push("--no-run".into());
    }
    let mut names = BTreeSet::new();
    for (selection, _) in &group.roots {
        if names.insert(&selection.target) {
            args.extend([
                if test { "--test" } else { "--bin" }.into(),
                selection.target.clone(),
            ]);
        }
    }
    let mut command = Command::new("cargo");
    command.args(args).current_dir(repository);
    crate::process::remove_parent_cargo_package_environment(&mut command);
    let output = budget.output(command)?;
    for (selection, test) in &group.roots {
        let test = *test;
        let kind = if test {
            NativeTargetKind::Test
        } else {
            NativeTargetKind::Binary
        };
        let identity = (
            selection.package.to_string(),
            selection.target.clone(),
            kind,
        );
        let package = metadata
            .packages
            .iter()
            .find(|p| p.name == selection.package.as_str())
            .context("selected Cargo package disappeared")?;
        let mut compiler_graph = Vec::new();
        let mut linked_directories = BTreeSet::new();
        let mut executable = None;
        let mut libraries = BTreeSet::new();
        for line in output
            .stdout
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
        {
            let value: Value = serde_json::from_slice(line)?;
            if value["reason"] == "build-script-executed" {
                for path in value["linked_paths"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    let path = path.strip_prefix("native=").unwrap_or(path);
                    let path = PathBuf::from(path);
                    if path.is_dir()
                        && path
                            .canonicalize()?
                            .starts_with(metadata.target_directory.canonicalize()?)
                    {
                        linked_directories.insert(path);
                    }
                }
            }
            if value["reason"] == "compiler-artifact" {
                let feature_set: BTreeSet<String> =
                    serde_json::from_value(value["features"].clone())?;
                let package_id = value["package_id"]
                    .as_str()
                    .context("Cargo package id absent")?;
                let kinds: Vec<String> = serde_json::from_value(value["target"]["kind"].clone())?;
                let context = if kinds.iter().any(|k| k == "custom-build") {
                    CompilerContext::BuildScript
                } else if kinds.iter().any(|k| k == "proc-macro")
                    || !group
                        .target_features
                        .get(package_id)
                        .is_some_and(|sets| sets.contains(&feature_set))
                {
                    CompilerContext::Host
                } else if kinds.iter().any(|k| k == "bin" || k == "test") {
                    CompilerContext::Target
                } else {
                    CompilerContext::NativeShared
                };
                compiler_graph.push(ObservedCompilerArtifact {
                    context,
                    package_id: value["package_id"]
                        .as_str()
                        .context("Cargo artifact package identity missing")?
                        .to_owned(),
                    target: value["target"]["name"]
                        .as_str()
                        .context("Cargo target identity missing")?
                        .to_owned(),
                    kinds: serde_json::from_value(value["target"]["kind"].clone())?,
                    features: serde_json::from_value(value["features"].clone())?,
                });
                if value["package_id"] == package.id
                    && value["target"]["name"] == selection.target
                    && value["target"]["kind"]
                        .as_array()
                        .is_some_and(|k| k.iter().any(|k| k == if test { "test" } else { "bin" }))
                {
                    if let Some(path) = value["executable"].as_str() {
                        ensure!(executable.is_none(), "ambiguous observed executable");
                        executable = Some(PathBuf::from(path));
                    }
                }
                if let Some(files) = value["filenames"].as_array() {
                    for path in files.iter().filter_map(Value::as_str) {
                        if path.ends_with(".so")
                            || path.ends_with(".dylib")
                            || path.ends_with(".dll")
                        {
                            libraries.insert(PathBuf::from(path));
                        }
                    }
                }
            }
        }
        for directory in linked_directories {
            for file in std::fs::read_dir(directory)? {
                let path = file?.path();
                if path.is_file()
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|name| {
                            name.ends_with(".so")
                                || name.contains(".so.")
                                || name.ends_with(".dylib")
                                || name.ends_with(".dll")
                        })
                {
                    libraries.insert(path);
                }
            }
        }
        let runtime_libraries = libraries
            .into_iter()
            .map(|path| {
                let sha256 = veoveo_types::Sha256Digest::from_bytes(
                    Sha256::digest(std::fs::read(&path)?).into(),
                );
                Ok(RuntimeLibrary { path, sha256 })
            })
            .collect::<Result<Vec<_>>>()?;
        let executable = executable.context("Cargo emitted no selected executable")?;
        let sha256 = veoveo_types::Sha256Digest::from_bytes(
            Sha256::digest(std::fs::read(&executable)?).into(),
        );
        manifest.entries.push(ArtifactEntry {
            target_kind: kind,
            selection: selection.clone(),
            package_id: package.id.clone(),
            executable,
            sha256,
            runtime_libraries,
            compiler_graph,
            effective_features: group.effective.clone(),
            target_features: group.target_features.clone(),
        });
        selected.insert(identity, selection.clone());
    }
    Ok(())
}
fn exact_rust_case(
    budget: &crate::discovery::budget::Budget,
    entry: &ArtifactEntry,
    case: &str,
) -> Result<()> {
    let mut command = Command::new(&entry.executable);
    configure_runtime(&mut command, entry)?;
    command.args(["--list", "--format", "terse"]);
    let output = budget.output(command)?;
    let listing = std::str::from_utf8(&output.stdout)?;
    ensure!(
        listing
            .lines()
            .filter(|line| *line == format!("{case}: test"))
            .count()
            == 1,
        "exact Rust test is missing or duplicated"
    );
    Ok(())
}

#[cfg(test)]
#[path = "smoke/tests.rs"]
mod tests;
