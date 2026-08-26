use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{
    BakeDefinition, BakeTarget, PACKAGE_LABEL, PlanFormat, bake_print_all, rust_labels_present,
};
use crate::{context::RepositoryContext, process};

const AFFECTED_SCHEMA: &str = "veoveo.io/image-affected-plan/v1";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AffectedPlan {
    schema_version: &'static str,
    baseline_revision: String,
    current_revision: String,
    includes_working_tree: bool,
    changed_paths: Vec<String>,
    image_targets: Vec<String>,
    reasons: BTreeMap<String, Vec<String>>,
    helm_changed: bool,
    sdk_changed: bool,
    generated_contracts_changed: bool,
    lock_inputs_changed: bool,
    broadened: bool,
    broadening_reasons: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<MetadataPackage>,
    workspace_members: Vec<String>,
    resolve: Option<MetadataResolve>,
}

#[derive(Debug, Deserialize)]
struct MetadataPackage {
    id: String,
    name: String,
    manifest_path: PathBuf,
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MetadataResolve {
    nodes: Vec<MetadataNode>,
}

#[derive(Debug, Deserialize)]
struct MetadataNode {
    id: String,
    deps: Vec<MetadataDependency>,
}

#[derive(Debug, Deserialize)]
struct MetadataDependency {
    pkg: String,
}

pub(super) fn command(
    repository: &RepositoryContext,
    since: &str,
    format: PlanFormat,
) -> Result<()> {
    let plan = resolve(repository, since)?;
    match format {
        PlanFormat::Json => {
            serde_json::to_writer_pretty(std::io::stdout(), &plan)?;
            println!();
        }
        PlanFormat::Human => print_human(&plan),
    }
    Ok(())
}

fn resolve(repository: &RepositoryContext, since: &str) -> Result<AffectedPlan> {
    let baseline_revision = git_text(repository.root(), ["rev-parse", "--verify", since])?
        .trim()
        .to_owned();
    let current_revision = git_text(repository.root(), ["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let changed_paths = changed_paths(repository.root(), &baseline_revision)?;
    let definition = bake_print_all(repository, repository.root(), &BTreeMap::new())?;
    let metadata = cargo_metadata(repository.root())?;
    let package_changes = changed_workspace_packages(repository.root(), &metadata, &changed_paths)?;
    let dependent_packages = dependent_package_closure(&metadata, &package_changes);
    let mut reasons = BTreeMap::<String, BTreeSet<String>>::new();
    let mut broadening_reasons = BTreeSet::new();
    let mut initially_affected = BTreeSet::new();

    let global_rust_change = changed_paths.iter().any(|path| {
        matches!(
            path.as_str(),
            "Cargo.lock" | "Cargo.toml" | "rust-toolchain.toml" | "rust-toolchain"
        ) || path.starts_with(".cargo/")
            || path == "tools/image-build/rust-workspace.Dockerfile"
    });
    let bake_catalog_changed = changed_paths.iter().any(|path| path == "docker-bake.hcl");

    let package_ids_by_name = metadata
        .packages
        .iter()
        .filter(|package| package.source.is_none())
        .map(|package| (package.name.as_str(), package.id.as_str()))
        .collect::<BTreeMap<_, _>>();

    for (name, target) in &definition.target {
        if bake_catalog_changed {
            select(
                &mut initially_affected,
                &mut reasons,
                name,
                "docker-bake.hcl changed the target graph",
            );
        }
        if rust_labels_present(name, target)? {
            if global_rust_change {
                select(
                    &mut initially_affected,
                    &mut reasons,
                    name,
                    "workspace toolchain or lock input changed",
                );
            } else if let Some(package_id) =
                package_ids_by_name.get(target.labels[PACKAGE_LABEL].as_str())
                && dependent_packages.contains(*package_id)
            {
                select(
                    &mut initially_affected,
                    &mut reasons,
                    name,
                    format!(
                        "Cargo package {} or one of its workspace dependencies changed",
                        target.labels[PACKAGE_LABEL]
                    ),
                );
            }
        }

        let dockerfile = dockerfile_path(repository.root(), target);
        if let Some(dockerfile) = &dockerfile {
            let relative = relative_text(repository.root(), dockerfile)?;
            if changed_paths.contains(&relative) {
                select(
                    &mut initially_affected,
                    &mut reasons,
                    name,
                    format!("Dockerfile {relative} changed"),
                );
            }
            match docker_copy_inputs(target, dockerfile) {
                Ok(inputs) => {
                    for changed in &changed_paths {
                        if let Some(input) = inputs
                            .iter()
                            .find(|input| path_matches_input(changed, input))
                        {
                            select(
                                &mut initially_affected,
                                &mut reasons,
                                name,
                                format!("{changed} enters the image through COPY/ADD {input}"),
                            );
                        }
                    }
                }
                Err(error) => {
                    if changed_paths
                        .iter()
                        .any(|path| path_may_affect_context(path, target))
                    {
                        select(
                            &mut initially_affected,
                            &mut reasons,
                            name,
                            "the target context changed and its Dockerfile inputs could not be resolved",
                        );
                        broadening_reasons.insert(format!("{name}: {error:#}"));
                    }
                }
            }
        }

        for context in target.contexts.values() {
            if context.starts_with("target:") || external_context(context) {
                continue;
            }
            let normalized = normalize_relative(context);
            for changed in &changed_paths {
                if path_matches_input(changed, &normalized) {
                    select(
                        &mut initially_affected,
                        &mut reasons,
                        name,
                        format!("{changed} enters named context {normalized}"),
                    );
                }
            }
        }
    }

    apply_contract_consumers(&changed_paths, &mut initially_affected, &mut reasons);

    let affected_graph = consumer_closure(&definition, initially_affected);
    for target in &affected_graph {
        for (consumer, definition) in &definition.target {
            if definition
                .contexts
                .values()
                .any(|context| context == &format!("target:{target}"))
                && affected_graph.contains(consumer)
            {
                reasons
                    .entry(consumer.clone())
                    .or_default()
                    .insert(format!("depends on affected Bake target {target}"));
            }
        }
    }

    let image_targets = affected_graph
        .into_iter()
        .filter(|name| {
            definition
                .target
                .get(name)
                .is_some_and(|target| !target.tags.is_empty())
        })
        .collect::<Vec<_>>();
    let reasons = image_targets
        .iter()
        .map(|name| {
            (
                name.clone(),
                reasons
                    .remove(name)
                    .unwrap_or_else(|| {
                        BTreeSet::from(["selected by affected dependency closure".to_owned()])
                    })
                    .into_iter()
                    .collect(),
            )
        })
        .collect();

    let helm_changed = changed_paths.iter().any(|path| is_helm_path(path));
    let sdk_changed = changed_paths.iter().any(|path| path.starts_with("sdk/"));
    let generated_contracts_changed = changed_paths.iter().any(|path| {
        path.starts_with("mcp/contract/")
            || path.starts_with("deploy/contract/")
            || path.starts_with("extensions/contract/")
    });
    let lock_inputs_changed = changed_paths.iter().any(|path| is_lock_input(path));
    Ok(AffectedPlan {
        schema_version: AFFECTED_SCHEMA,
        baseline_revision,
        current_revision,
        includes_working_tree: true,
        changed_paths: changed_paths.into_iter().collect(),
        image_targets,
        reasons,
        helm_changed,
        sdk_changed,
        generated_contracts_changed,
        lock_inputs_changed,
        broadened: !broadening_reasons.is_empty(),
        broadening_reasons: broadening_reasons.into_iter().collect(),
    })
}

fn changed_paths(repository: &Path, baseline: &str) -> Result<BTreeSet<String>> {
    let output = process::output(
        "git",
        [
            "diff",
            "--name-only",
            "--diff-filter=ACDMRTUXB",
            baseline,
            "--",
        ],
        Some(repository),
    )?;
    let mut paths = String::from_utf8(output.stdout)
        .context("Git changed paths are not UTF-8")?
        .lines()
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let untracked = process::output(
        "git",
        ["ls-files", "--others", "--exclude-standard"],
        Some(repository),
    )?;
    paths.extend(
        String::from_utf8(untracked.stdout)
            .context("Git untracked paths are not UTF-8")?
            .lines()
            .filter(|path| !path.is_empty())
            .map(str::to_owned),
    );
    Ok(paths)
}

fn cargo_metadata(repository: &Path) -> Result<Metadata> {
    let output = process::output(
        "cargo",
        ["metadata", "--format-version", "1", "--locked"],
        Some(repository),
    )?;
    serde_json::from_slice(&output.stdout).context("decoding affected-target Cargo metadata")
}

fn changed_workspace_packages(
    repository: &Path,
    metadata: &Metadata,
    changed_paths: &BTreeSet<String>,
) -> Result<BTreeSet<String>> {
    let members = metadata.workspace_members.iter().collect::<BTreeSet<_>>();
    let mut changed = BTreeSet::new();
    for package in &metadata.packages {
        if !members.contains(&package.id) {
            continue;
        }
        let root = package
            .manifest_path
            .parent()
            .context("Cargo package manifest has no parent")?;
        let relative = root
            .strip_prefix(repository)
            .with_context(|| format!("Cargo package {} is outside the repository", package.name))?;
        let relative = normalize_path(relative);
        if changed_paths
            .iter()
            .any(|path| package_runtime_path_changed(path, &relative))
        {
            changed.insert(package.id.clone());
        }
    }
    Ok(changed)
}

fn package_runtime_path_changed(changed: &str, package_root: &str) -> bool {
    if !path_matches_input(changed, package_root) {
        return false;
    }
    let relative = changed
        .strip_prefix(package_root)
        .and_then(|path| path.strip_prefix('/'))
        .unwrap_or(changed);
    relative != "tests" && !relative.starts_with("tests/")
}

fn dependent_package_closure(metadata: &Metadata, changed: &BTreeSet<String>) -> BTreeSet<String> {
    let workspace = metadata
        .workspace_members
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut closure = changed.clone();
    let Some(resolve) = &metadata.resolve else {
        return workspace;
    };
    loop {
        let before = closure.len();
        for node in &resolve.nodes {
            if workspace.contains(&node.id)
                && node
                    .deps
                    .iter()
                    .any(|dependency| closure.contains(&dependency.pkg))
            {
                closure.insert(node.id.clone());
            }
        }
        if closure.len() == before {
            return closure;
        }
    }
}

fn consumer_closure(
    definition: &BakeDefinition,
    initially_affected: BTreeSet<String>,
) -> BTreeSet<String> {
    let mut affected = initially_affected;
    loop {
        let before = affected.len();
        for (name, target) in &definition.target {
            if target.contexts.values().any(|context| {
                context
                    .strip_prefix("target:")
                    .is_some_and(|dependency| affected.contains(dependency))
            }) {
                affected.insert(name.clone());
            }
        }
        if affected.len() == before {
            return affected;
        }
    }
}

fn dockerfile_path(repository: &Path, target: &BakeTarget) -> Option<PathBuf> {
    if target.dockerfile.is_empty() || external_context(&target.context) {
        return None;
    }
    let direct = repository.join(&target.dockerfile);
    if direct.is_file() {
        return Some(direct);
    }
    let context = if target.context.is_empty() {
        Path::new(".")
    } else {
        Path::new(&target.context)
    };
    let contextual = repository.join(context).join(&target.dockerfile);
    contextual.is_file().then_some(contextual)
}

fn docker_copy_inputs(target: &BakeTarget, dockerfile: &Path) -> Result<BTreeSet<String>> {
    let source = fs::read_to_string(dockerfile)
        .with_context(|| format!("reading {}", dockerfile.display()))?;
    let context = if target.context.is_empty() {
        Path::new(".")
    } else {
        Path::new(&target.context)
    };
    let mut logical = String::new();
    let mut inputs = BTreeSet::new();
    for physical in source.lines() {
        let trimmed = physical.trim();
        logical.push_str(trimmed.trim_end_matches('\\'));
        if trimmed.ends_with('\\') {
            logical.push(' ');
            continue;
        }
        let line = logical.trim();
        if line.starts_with("COPY ") || line.starts_with("ADD ") {
            let rest = line.split_once(' ').map_or("", |(_, rest)| rest).trim();
            if !rest.starts_with('[')
                && !rest
                    .split_whitespace()
                    .any(|part| part.starts_with("--from="))
            {
                let operands = rest
                    .split_whitespace()
                    .filter(|part| !part.starts_with("--"))
                    .collect::<Vec<_>>();
                for operand in operands.iter().take(operands.len().saturating_sub(1)) {
                    if !external_context(operand) && !operand.contains('$') {
                        inputs.insert(normalize_path(&context.join(operand)));
                    }
                }
            }
        }
        logical.clear();
    }
    Ok(inputs)
}

fn select(
    targets: &mut BTreeSet<String>,
    reasons: &mut BTreeMap<String, BTreeSet<String>>,
    target: &str,
    reason: impl Into<String>,
) {
    targets.insert(target.to_owned());
    reasons
        .entry(target.to_owned())
        .or_default()
        .insert(reason.into());
}

fn apply_contract_consumers(
    changed_paths: &BTreeSet<String>,
    targets: &mut BTreeSet<String>,
    reasons: &mut BTreeMap<String, BTreeSet<String>>,
) {
    let apps_contract_changed = changed_paths
        .iter()
        .any(|path| path_matches_input(path, "mcp/apps-extension"));
    let app_host_changed = changed_paths
        .iter()
        .any(|path| path_matches_input(path, "apps/console/web"));
    if apps_contract_changed || app_host_changed {
        select(
            targets,
            reasons,
            "console-bff",
            "MCP App presentation or host contract changed",
        );
    }
}

fn path_may_affect_context(path: &str, target: &BakeTarget) -> bool {
    let context = normalize_relative(if target.context.is_empty() {
        "."
    } else {
        &target.context
    });
    context == "." || path_matches_input(path, &context)
}

fn path_matches_input(changed: &str, input: &str) -> bool {
    input == "."
        || changed == input
        || changed
            .strip_prefix(input)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn normalize_relative(path: &str) -> String {
    normalize_path(Path::new(path))
}

fn normalize_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let value = text
        .trim_start_matches("./")
        .trim_end_matches('/')
        .to_owned();
    if value.is_empty() {
        ".".to_owned()
    } else {
        value
    }
}

fn relative_text(repository: &Path, path: &Path) -> Result<String> {
    Ok(normalize_path(path.strip_prefix(repository).with_context(
        || format!("path {} is outside repository", path.display()),
    )?))
}

fn external_context(context: &str) -> bool {
    context.contains("://") || context.starts_with("docker-image:") || context.starts_with("git@")
}

fn is_helm_path(path: &str) -> bool {
    path.contains("/deploy/helm/")
        || path.starts_with("deploy/helm/")
        || path.ends_with("Chart.yaml")
        || path.ends_with("values.yaml")
}

fn is_lock_input(path: &str) -> bool {
    matches!(
        Path::new(path).file_name().and_then(|name| name.to_str()),
        Some(
            "Cargo.lock"
                | "uv.lock"
                | "package-lock.json"
                | "requirements.lock"
                | "simulation-runtime.lock.json"
                | "docker-bake.hcl"
        )
    )
}

fn git_text<const N: usize>(repository: &Path, args: [&str; N]) -> Result<String> {
    process::output_text("git", args, Some(repository))
}

fn print_human(plan: &AffectedPlan) {
    println!(
        "Affected surfaces from {} to {} (working tree included)",
        plan.baseline_revision, plan.current_revision
    );
    if plan.image_targets.is_empty() {
        println!("No image targets selected");
    }
    for target in &plan.image_targets {
        println!("{target}:");
        for reason in &plan.reasons[target] {
            println!("  - {reason}");
        }
    }
    println!(
        "Other surfaces: Helm={}, SDK={}, generated contracts={}, lock inputs={}",
        plan.helm_changed,
        plan.sdk_changed,
        plan.generated_contracts_changed,
        plan.lock_inputs_changed
    );
    if plan.broadened {
        println!("Selection was broadened:");
        for reason in &plan.broadening_reasons {
            println!("  - {reason}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_inputs_resolve_against_the_build_context() {
        let temporary = tempfile::tempdir().unwrap();
        let dockerfile = temporary.path().join("context/Dockerfile");
        fs::create_dir_all(dockerfile.parent().unwrap()).unwrap();
        fs::write(
            &dockerfile,
            "COPY --chown=1:1 src config.json /app/\nCOPY --from=builder /out /app\n",
        )
        .unwrap();
        let target = BakeTarget {
            context: "context".to_owned(),
            dockerfile: "Dockerfile".to_owned(),
            ..BakeTarget::default()
        };
        assert_eq!(
            docker_copy_inputs(&target, &dockerfile).unwrap(),
            BTreeSet::from(["context/config.json".to_owned(), "context/src".to_owned()])
        );
    }

    #[test]
    fn affected_dependencies_select_consumers() {
        let definition = BakeDefinition {
            group: BTreeMap::new(),
            target: BTreeMap::from([
                ("base".to_owned(), BakeTarget::default()),
                (
                    "overlay".to_owned(),
                    BakeTarget {
                        contexts: BTreeMap::from([("base".to_owned(), "target:base".to_owned())]),
                        ..BakeTarget::default()
                    },
                ),
            ]),
        };
        assert_eq!(
            consumer_closure(&definition, BTreeSet::from(["base".to_owned()])),
            BTreeSet::from(["base".to_owned(), "overlay".to_owned()])
        );
    }

    #[test]
    fn cargo_integration_tests_do_not_select_runtime_images() {
        assert!(!package_runtime_path_changed(
            "platform/gateway/tests/exposure_probe.rs",
            "platform/gateway"
        ));
        assert!(package_runtime_path_changed(
            "platform/gateway/src/policy.rs",
            "platform/gateway"
        ));
        assert!(package_runtime_path_changed(
            "platform/gateway/Cargo.toml",
            "platform/gateway"
        ));
    }

    #[test]
    fn app_contract_selects_console_consumer() {
        let mut targets = BTreeSet::new();
        let mut reasons = BTreeMap::new();
        apply_contract_consumers(
            &BTreeSet::from(["mcp/apps-extension/src/models.rs".to_owned()]),
            &mut targets,
            &mut reasons,
        );
        assert_eq!(targets, BTreeSet::from(["console-bff".to_owned()]));
        assert!(reasons["console-bff"].contains("MCP App presentation or host contract changed"));
    }
}
