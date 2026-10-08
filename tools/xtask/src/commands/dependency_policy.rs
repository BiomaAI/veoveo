//! Independent Cargo consumers admit dependency roles without workspace feature unions.
use super::contract_docs;
use crate::{
    context::RepositoryContext,
    discovery::{budget::Budget, cargo::CargoMetadata, features},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, Debug)]
enum Profile {
    Contract,
    Kernel,
}
struct Selection {
    owner: PathBuf,
    name: String,
    features: Vec<String>,
    profile: Profile,
}

pub(crate) fn enforce(repository: &RepositoryContext) -> Result<()> {
    let root = repository.root();
    let owners = contract_docs::discover_owners(root)?;
    let mut selections = Vec::new();
    let mut failures = Vec::new();
    for owner in owners.iter().filter(|p| p.join("Cargo.toml").is_file()) {
        let manifest = manifest(owner)?;
        let name = package_name(&manifest)?;
        if manifest
            .get("features")
            .and_then(|f| f.get("contract"))
            .is_none()
        {
            failures.push(format!(
                "{}: hosted owner lacks contract feature",
                owner.display()
            ));
            continue;
        }
        if !owner.join("src/lib.rs").is_file() && manifest.get("lib").is_none() {
            failures.push(format!(
                "{}: hosted owner lacks public library",
                owner.display()
            ));
            continue;
        }
        selections.push(Selection {
            owner: owner.clone(),
            name,
            features: vec!["contract".into()],
            profile: Profile::Contract,
        });
    }
    for (path, features) in [
        ("platform/store", vec!["runtime".into()]),
        ("platform/audit", vec![]),
        ("platform/gateway", vec![]),
        ("servers/knowledge-mcp", vec!["runtime".into()]),
    ] {
        let owner = root.join(path);
        selections.push(Selection {
            name: package_name(&manifest(&owner)?)?,
            owner,
            features,
            profile: Profile::Kernel,
        });
    }
    let budget = Budget::new(&root.join("target"), 600, 10)?;
    for selection in &selections {
        if let Err(error) = inspect(root, selection, &owners, &budget).with_context(|| {
            format!(
                "{} {:?}: defaults OFF, features {:?}, normal/build host profile",
                selection.owner.display(),
                selection.profile,
                selection.features
            )
        }) {
            failures.push(format!("{error:#}"));
        }
    }
    ensure!(
        failures.is_empty(),
        "dependency boundaries refused:\n{}",
        failures.join("\n")
    );
    budget.completed();
    println!(
        "{} independent hosted contract and 4 reusable kernel normal/build profiles verified",
        selections.len() - 4
    );
    Ok(())
}
fn manifest(owner: &Path) -> Result<toml::Value> {
    Ok(toml::from_str(&fs::read_to_string(
        owner.join("Cargo.toml"),
    )?)?)
}
fn package_name(manifest: &toml::Value) -> Result<String> {
    Ok(manifest
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(toml::Value::as_str)
        .context("owner lacks package name")?
        .to_owned())
}

fn command(directory: &Path, args: &[&str]) -> Command {
    let mut c = Command::new("cargo");
    c.current_dir(directory).args(args);
    crate::process::remove_parent_cargo_package_environment(&mut c);
    c
}
#[derive(serde::Serialize)]
struct ConsumerManifest<'a> {
    package: ConsumerPackage,
    workspace: ConsumerWorkspace,
    dependencies: std::collections::BTreeMap<&'a str, ConsumerDependency<'a>>,
}
#[derive(serde::Serialize)]
struct ConsumerPackage {
    name: &'static str,
    version: &'static str,
    edition: &'static str,
}
#[derive(serde::Serialize)]
struct ConsumerWorkspace {
    resolver: &'static str,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "kebab-case")]
struct ConsumerDependency<'a> {
    package: &'a str,
    path: &'a Path,
    default_features: bool,
    features: &'a [String],
}

fn inspect(root: &Path, selection: &Selection, owners: &[PathBuf], budget: &Budget) -> Result<()> {
    let temporary = tempfile::tempdir()?;
    fs::create_dir(temporary.path().join("src"))?;
    fs::write(
        temporary.path().join("src/lib.rs"),
        "// Metadata-only independent consumer.\n",
    )?;
    let input = ConsumerManifest {
        package: ConsumerPackage {
            name: "veoveo-boundary-consumer",
            version: "0.0.0",
            edition: "2024",
        },
        workspace: ConsumerWorkspace { resolver: "3" },
        dependencies: std::collections::BTreeMap::from([(
            "owner",
            ConsumerDependency {
                package: &selection.name,
                path: &selection.owner,
                default_features: false,
                features: &selection.features,
            },
        )]),
    };
    fs::write(
        temporary.path().join("Cargo.toml"),
        toml::to_string(&input)?,
    )?;
    let original_lock = fs::read(root.join("Cargo.lock"))?;
    fs::write(temporary.path().join("Cargo.lock"), &original_lock)?;
    // Cargo may add/prune the disposable consumer entry. It owns resolution;
    // admission below requires all selected identities/checksums to match the original lock.
    let output = budget.output(command(
        temporary.path(),
        &["metadata", "--offline", "--format-version", "1"],
    ))?;
    ensure!(
        output.stdout.len() <= 64 * 1024 * 1024,
        "isolated metadata exceeds 64MiB"
    );
    let metadata: CargoMetadata = serde_json::from_slice(&output.stdout)?;
    let owner = metadata
        .packages
        .iter()
        .find(|p| p.manifest_path == selection.owner.join("Cargo.toml"))
        .context("Cargo omitted the selected source owner")?;
    ensure!(
        owner
            .targets
            .iter()
            .any(|t| t.kind.iter().any(|k| k == "lib")
                && t.src_path.starts_with(&selection.owner)
                && t.src_path.is_file()),
        "selected owner lacks a contained library source target"
    );
    admit_lock(
        &original_lock,
        &fs::read(temporary.path().join("Cargo.lock"))?,
    )?;
    let output = budget.output(command(
        temporary.path(),
        &[
            "tree",
            "--offline",
            "--locked",
            "--target",
            "host-tuple",
            "--edges",
            "normal,build",
            "--prefix",
            "depth",
            "--format",
            "{p}|veoveo-features|{f}",
        ],
    ))?;
    ensure!(
        output.stdout.len() <= 64 * 1024 * 1024,
        "isolated tree exceeds 64MiB"
    );
    admit_tree(
        root,
        selection,
        owners,
        &metadata,
        &output.stdout,
        &temporary.path().join("Cargo.toml"),
    )?;
    ensure!(
        fs::read(root.join("Cargo.lock"))? == original_lock,
        "repository lock changed during dependency inspection"
    );
    Ok(())
}
#[derive(serde::Deserialize)]
struct Lock {
    package: Vec<LockedPackage>,
}
#[derive(serde::Deserialize, PartialEq, Eq, PartialOrd, Ord)]
struct LockedPackage {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
}
fn admit_lock(original: &[u8], selected: &[u8]) -> Result<()> {
    let original: Lock = toml::from_str(std::str::from_utf8(original)?)?;
    let selected: Lock = toml::from_str(std::str::from_utf8(selected)?)?;
    ensure!(
        original
            .package
            .iter()
            .all(|p| p.name != "veoveo-boundary-consumer"),
        "repository lock collides with the private consumer identity"
    );
    let pins: BTreeSet<_> = original.package.into_iter().collect();
    for p in selected.package {
        if p.name == "veoveo-boundary-consumer" && p.source.is_none() && p.version == "0.0.0" {
            continue;
        }
        ensure!(
            pins.contains(&p),
            "isolated selection changed unqualified dependency identity {} v{} {:?}",
            p.name,
            p.version,
            p.source
        );
    }
    Ok(())
}
struct TreeNode {
    package: usize,
    features: BTreeSet<String>,
    label: String,
    children: BTreeSet<usize>,
}
fn admit_tree(
    root: &Path,
    selection: &Selection,
    owners: &[PathBuf],
    metadata: &CargoMetadata,
    bytes: &[u8],
    consumer_manifest: &Path,
) -> Result<()> {
    let mut stack: Vec<usize> = Vec::new();
    let mut nodes: Vec<TreeNode> = Vec::new();
    let mut rows = std::collections::BTreeMap::new();
    let mut selected_root = false;
    for line in std::str::from_utf8(bytes)?
        .lines()
        .filter(|l| !l.is_empty())
    {
        let offset = line
            .find(|c: char| !c.is_ascii_digit())
            .context("tree lacks package identity")?;
        let depth: usize = line[..offset].parse()?;
        let label = line[offset..]
            .strip_suffix(" (*)")
            .unwrap_or(&line[offset..]);
        ensure!(
            depth <= stack.len() && depth <= 256,
            "invalid dependency tree depth"
        );
        stack.truncate(depth);
        let index = if let Some(index) = rows.get(label) {
            *index
        } else {
            let effective = features::decode(metadata, label.as_bytes())?;
            let (id, sets) = effective.iter().next().context("empty package row")?;
            let package = metadata
                .packages
                .iter()
                .position(|p| &p.id == id)
                .context("tree identity missing")?;
            let active = sets.iter().next().context("feature row missing")?.clone();
            let index = nodes.len();
            ensure!(
                index < 65536,
                "Cargo tree exceeds 65536 source/feature instances"
            );
            nodes.push(TreeNode {
                package,
                features: active,
                label: label.split("|veoveo-features|").next().unwrap().to_owned(),
                children: BTreeSet::new(),
            });
            rows.insert(label, index);
            index
        };
        if let Some(parent) = stack.last() {
            nodes[*parent].children.insert(index);
        }
        stack.push(index);
        let package = &metadata.packages[nodes[index].package];
        if package.manifest_path.parent() == Some(selection.owner.as_path()) {
            selected_root = true;
            ensure!(
                selection
                    .features
                    .iter()
                    .all(|f| nodes[index].features.contains(f)),
                "selected owner feature missing"
            );
        }
    }
    ensure!(
        selected_root && !nodes.is_empty(),
        "Cargo omitted selected owner root"
    );
    // Cargo's (*) row retains an edge to the already emitted source/feature
    // instance. Traverse each instance in both admission scopes, not every path.
    let mut pending = std::collections::VecDeque::from([(0, false, Vec::new())]);
    let mut visited = BTreeSet::new();
    while let Some((index, inherited_contract, mut chain)) = pending.pop_front() {
        if !visited.insert((index, inherited_contract)) {
            continue;
        }
        let node = &nodes[index];
        let package = &metadata.packages[node.package];
        chain.push(node.label.clone());
        let wrapper = package.manifest_path == consumer_manifest;
        let contract = matches!(selection.profile, Profile::Contract)
            || inherited_contract
            || (!wrapper
                && role(root, owners, package) == Role::OptionalOwner
                && package.manifest_path.parent() != Some(selection.owner.as_path())
                && pure_owner(&node.features));
        if !wrapper {
            let effective = Selection {
                owner: selection.owner.clone(),
                name: selection.name.clone(),
                features: selection.features.clone(),
                profile: if contract {
                    Profile::Contract
                } else {
                    selection.profile
                },
            };
            if let Some(reason) = refused(root, &effective, owners, package, &node.features) {
                anyhow::bail!(
                    "{reason}; selected {:?}, offending identity {}, active features {:?}; chain: {}",
                    selection.profile,
                    package.id,
                    node.features,
                    chain.join(" -> ")
                );
            }
        }
        for child in &node.children {
            pending.push_back((*child, contract, chain.clone()));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Port,
    Kernel,
    OptionalOwner,
    Composition,
    RequiredEmbedding,
    Unknown,
}
fn role(root: &Path, owners: &[PathBuf], p: &crate::discovery::cargo::CargoPackage) -> Role {
    let Ok(path) = p.manifest_path.strip_prefix(root) else {
        return Role::Unknown;
    };
    if path.starts_with("platform/gateway/catalog")
        || path.starts_with("platform/gateway/composition")
    {
        return Role::Composition;
    }
    if path.components().any(|c| c.as_os_str() == "contract") {
        return Role::Port;
    }
    if owners
        .iter()
        .any(|o| p.manifest_path.parent() == Some(o.as_path()))
    {
        return Role::OptionalOwner;
    }
    if [
        "platform/types",
        "platform/macros",
        "platform/modules",
        "mcp/knowledge-extension",
    ]
    .iter()
    .any(|base| path.starts_with(base))
    {
        return Role::Port;
    }
    if path.starts_with("platform/runtimes/embedding/client") {
        return Role::RequiredEmbedding;
    }
    if [
        "agents/runtime",
        "agents/manager",
        "agents/kernel",
        "platform/computers",
        "platform/workspace",
        "platform/recordings",
        "platform/runtimes",
    ]
    .iter()
    .any(|base| path.starts_with(base))
    {
        return Role::OptionalOwner;
    }
    if [
        "platform/store",
        "platform/audit",
        "platform/gateway",
        "platform/policy",
        "platform/task-runtime",
        "mcp/apps-extension",
    ]
    .iter()
    .any(|base| path.starts_with(base))
    {
        return Role::Kernel;
    }
    Role::Unknown
}
fn pure_owner(active: &BTreeSet<String>) -> bool {
    active.contains("contract")
        && active
            .iter()
            .all(|f| matches!(f.as_str(), "contract" | "schema" | "catalog"))
}
fn refused(
    root: &Path,
    selection: &Selection,
    owners: &[PathBuf],
    p: &crate::discovery::cargo::CargoPackage,
    active: &BTreeSet<String>,
) -> Option<&'static str> {
    // Explicit implementation exclusions cover the currently qualified protocol,
    // database, provider, hardware and async profiles; this is not a third-party source purity proof.
    let implementation = matches!(
        p.name.as_str(),
        "rmcp"
            | "tokio"
            | "axum"
            | "reqwest"
            | "tonic"
            | "wgpu"
            | "wgpu-core"
            | "wgpu-hal"
            | "cudarc"
            | "ash"
            | "duckdb"
            | "async-openai"
            | "opencv"
            | "re_sdk"
            | "traci-rs"
    ) || p.name.starts_with("surrealdb");
    if matches!(selection.profile, Profile::Contract) && implementation {
        return Some("MCP/database/provider/GPU/async implementation in public contract");
    }
    if p.source.is_some() {
        return None;
    }
    // The disposable consumer is excluded before classification by admit_tree.
    let role = role(root, owners, p);
    match role {
        Role::Composition => Some("catalog/composition back-edge"),
        Role::Unknown => {
            Some("unclassified first-party owner/path; declare its supported role before admission")
        }
        Role::OptionalOwner
            if p.manifest_path.parent() != Some(selection.owner.as_path())
                && !pure_owner(active) =>
        {
            Some("optional owner implementation in reusable/contract profile")
        }
        Role::OptionalOwner
            if matches!(selection.profile, Profile::Contract) && !pure_owner(active) =>
        {
            Some("hosted implementation feature in public contract")
        }
        Role::Kernel if matches!(selection.profile, Profile::Contract) => {
            Some("kernel implementation package in public contract")
        }
        Role::RequiredEmbedding
            if matches!(selection.profile, Profile::Contract)
                || !selection.owner.ends_with("servers/knowledge-mcp") =>
        {
            Some("embedding implementation is admitted only by the Knowledge reusable runtime")
        }
        Role::Port
            if matches!(selection.profile, Profile::Contract)
                && active.iter().any(|f| {
                    matches!(f.as_str(), "runtime" | "mcp" | "server" | "managed-clients")
                }) =>
        {
            Some("implementation feature on contract/extension port")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn package(root: &Path, path: &str, name: &str, extra: &str, docs: bool) -> PathBuf {
        let directory = root.join(path);
        fs::create_dir_all(directory.join("src")).unwrap();
        fs::write(
            directory.join("Cargo.toml"),
            format!(
                "[package]\nname = {name:?}\nversion = \"0.0.1\"\nedition = \"2024\"\n{extra}\n"
            ),
        )
        .unwrap();
        fs::write(
            directory.join("src/lib.rs"),
            if docs {
                "fn docs() { server_docs!(\"independent\"); }"
            } else {
                ""
            },
        )
        .unwrap();
        directory
    }
    fn seed(root: &Path, members: &[&str], budget: &Budget) {
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "[workspace]\nresolver = \"3\"\nmembers = {}\n",
                serde_json::to_string(members).unwrap()
            ),
        )
        .unwrap();
        budget
            .output(command(
                root,
                &["metadata", "--offline", "--format-version", "1"],
            ))
            .unwrap();
    }
    #[test]
    fn independent_owner_is_discovered_and_dev_only_runtime_is_excluded() {
        let root = tempfile::tempdir().unwrap();
        let budget = Budget::new(&root.path().join("target"), 30, 1).unwrap();
        package(root.path(), "external/runtime", "tokio", "", false);
        let owner = package(
            root.path(),
            "servers/independent",
            "independent-mcp",
            "[features]\ndefault = [\"runtime\"]\ncontract = []\nruntime = [\"dep:tokio\"]\n[dependencies]\ntokio = { path = \"../../external/runtime\", optional = true }\n[dev-dependencies]\ntokio = { path = \"../../external/runtime\" }",
            true,
        );
        seed(
            root.path(),
            &["servers/independent", "external/runtime"],
            &budget,
        );
        let owners = contract_docs::discover_owners(root.path()).unwrap();
        assert_eq!(owners, vec![owner.clone()]);
        inspect(
            root.path(),
            &Selection {
                owner,
                name: "independent-mcp".into(),
                features: vec!["contract".into()],
                profile: Profile::Contract,
            },
            &owners,
            &budget,
        )
        .unwrap();
        budget.completed();
    }
    #[test]
    fn indirect_feature_runtime_and_build_leaks_report_the_dependency_chain() {
        for edge in ["dependencies", "build-dependencies"] {
            let root = tempfile::tempdir().unwrap();
            let budget = Budget::new(&root.path().join("target"), 30, 1).unwrap();
            package(root.path(), "external/runtime", "tokio", "", false);
            package(
                root.path(),
                "platform/types",
                "fixture-port",
                &format!(
                    "[{edge}]\ntokio = {{ path = \"../../external/runtime\", optional = true }}\n[features]\nleak = [\"dep:tokio\"]"
                ),
                false,
            );
            let owner = package(
                root.path(),
                "servers/independent",
                "independent-mcp",
                "[dependencies]\nport = { package = \"fixture-port\", path = \"../../platform/types\", default-features = false }\n[features]\ncontract = [\"port/leak\"]",
                true,
            );
            seed(
                root.path(),
                &["servers/independent", "platform/types", "external/runtime"],
                &budget,
            );
            let owners = contract_docs::discover_owners(root.path()).unwrap();
            let error = inspect(
                root.path(),
                &Selection {
                    owner,
                    name: "independent-mcp".into(),
                    features: vec!["contract".into()],
                    profile: Profile::Contract,
                },
                &owners,
                &budget,
            )
            .unwrap_err()
            .to_string();
            assert!(
                error.contains("tokio")
                    && error.contains("fixture-port")
                    && error.contains("chain:"),
                "{edge}: {error}"
            );
            budget.completed();
        }
    }
    #[test]
    fn kernel_composition_back_edge_and_unknown_owner_are_refused() {
        for path in ["platform/gateway/catalog", "platform/unknown-owner"] {
            let root = tempfile::tempdir().unwrap();
            let budget = Budget::new(&root.path().join("target"), 30, 1).unwrap();
            package(root.path(), path, "fixture-composition", "", false);
            let owner = package(
                root.path(),
                "platform/store",
                "fixture-kernel",
                &format!(
                    "[dependencies]\nrecipe = {{ package = \"fixture-composition\", path = {:?} }}",
                    format!("../../{path}")
                ),
                false,
            );
            seed(root.path(), &["platform/store", path], &budget);
            let error = inspect(
                root.path(),
                &Selection {
                    owner,
                    name: "fixture-kernel".into(),
                    features: vec![],
                    profile: Profile::Kernel,
                },
                &[],
                &budget,
            )
            .unwrap_err()
            .to_string();
            assert!(
                error.contains(if path.ends_with("catalog") {
                    "back-edge"
                } else {
                    "unclassified"
                }),
                "{error}"
            );
            budget.completed();
        }
    }
    #[test]
    fn repeated_shared_subtree_is_rechecked_under_contract_scope() {
        let root = tempfile::tempdir().unwrap();
        let budget = Budget::new(&root.path().join("target"), 30, 1).unwrap();
        package(root.path(), "platform/types/runtime", "tokio", "", false);
        package(
            root.path(),
            "platform/types",
            "fixture-port",
            "[dependencies]\ntokio = { path = \"runtime\" }",
            false,
        );
        package(
            root.path(),
            "agents/runtime",
            "fixture-agent",
            "[features]\ncontract = []\n[dependencies]\nport = { package = \"fixture-port\", path = \"../../platform/types\" }",
            false,
        );
        let owner = package(
            root.path(),
            "platform/store",
            "fixture-kernel",
            "[dependencies]\nport = { package = \"fixture-port\", path = \"../types\" }\nagent = { package = \"fixture-agent\", path = \"../../agents/runtime\", default-features = false, features = [\"contract\"] }",
            false,
        );
        seed(
            root.path(),
            &[
                "platform/store",
                "platform/types",
                "agents/runtime",
                "platform/types/runtime",
            ],
            &budget,
        );
        let error = inspect(
            root.path(),
            &Selection {
                owner,
                name: "fixture-kernel".into(),
                features: vec![],
                profile: Profile::Kernel,
            },
            &[],
            &budget,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("tokio") && error.contains("fixture-agent"),
            "{error}"
        );
        budget.completed();
    }
    #[test]
    fn host_build_feature_instance_cannot_hide_behind_pure_target_instance() {
        let root = tempfile::tempdir().unwrap();
        let budget = Budget::new(&root.path().join("target"), 30, 1).unwrap();
        package(root.path(), "external/runtime", "tokio", "", false);
        package(
            root.path(),
            "platform/types",
            "fixture-port",
            "[dependencies]\ntokio = { path = \"../../external/runtime\", optional = true }\n[features]\nleak = [\"dep:tokio\"]",
            false,
        );
        let owner = package(
            root.path(),
            "servers/independent",
            "independent-mcp",
            "[features]\ncontract = []\n[dependencies]\nport = { package = \"fixture-port\", path = \"../../platform/types\", default-features = false }\n[build-dependencies]\nport = { package = \"fixture-port\", path = \"../../platform/types\", default-features = false, features = [\"leak\"] }",
            true,
        );
        seed(
            root.path(),
            &["servers/independent", "platform/types", "external/runtime"],
            &budget,
        );
        let owners = contract_docs::discover_owners(root.path()).unwrap();
        let error = inspect(
            root.path(),
            &Selection {
                owner,
                name: "independent-mcp".into(),
                features: vec!["contract".into()],
                profile: Profile::Contract,
            },
            &owners,
            &budget,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("tokio") && error.contains("fixture-port"),
            "{error}"
        );
        budget.completed();
    }
    #[test]
    fn lock_admission_rejects_changed_version_source_and_checksum() {
        let seed = b"[[package]]\nname='port'\nversion='1.0.0'\nsource='registry+https://example.invalid'\nchecksum='abc'\n";
        for replacement in ["1.1.0", "registry+https://foreign.invalid", "def"] {
            let original = std::str::from_utf8(seed).unwrap();
            let field = if replacement == "1.1.0" {
                "1.0.0"
            } else if replacement == "def" {
                "abc"
            } else {
                "registry+https://example.invalid"
            };
            assert!(admit_lock(seed, original.replace(field, replacement).as_bytes()).is_err());
        }
    }
}
