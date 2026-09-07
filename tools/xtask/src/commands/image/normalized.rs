//! Publish dependency payloads once and use their immutable normalized digest.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use super::{
    BakeDefinition, BakeTarget, EvidenceRun, OutputMode, PreparedPlan, REPRODUCIBLE_BUILD_EPOCH,
    buildkit, operation, source_context, target_dependency_closure, validate_identifier,
};
use crate::{
    commands::{builder, image_manifest},
    context::RepositoryContext,
};

const PARENT_LABEL: &str = "io.veoveo.build.normalized-parent";
const INPUTS_LABEL: &str = "io.veoveo.build.input-paths";
const RECEIPT_SCHEMA: &str = "veoveo.io/normalized-parent/v1";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ParentPlan {
    consumer: String,
    target: String,
    context: String,
    recipe_digest: String,
    inputs: BTreeMap<String, InputIdentity>,
}

#[derive(Clone, Debug, Serialize)]
struct InputIdentity {
    digest: String,
    files: usize,
}

pub(super) struct PreparedParent {
    pub plan: ParentPlan,
    contexts: BTreeMap<String, source_context::SourceContext>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema_version: String,
    recipe_digest: String,
    reference: String,
    runtime_digest: String,
    source_revision: String,
}

#[derive(Deserialize)]
struct BuildMetadata {
    #[serde(rename = "containerimage.digest")]
    digest: Option<String>,
}

#[derive(Default, Serialize)]
struct Override {
    target: BTreeMap<String, TargetOverride>,
}

#[derive(Default, Serialize)]
struct TargetOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<PathBuf>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    contexts: BTreeMap<String, String>,
}

pub(super) fn prepare(
    repository: &Path,
    definition: &BakeDefinition,
    selected: &[String],
) -> Result<Vec<PreparedParent>> {
    let mut parents = Vec::new();
    for consumer in selected {
        let target = &definition.target[consumer];
        let Some(parent) = target.labels.get(PARENT_LABEL) else {
            continue;
        };
        validate_identifier("normalized parent", parent)?;
        let contexts = target
            .contexts
            .iter()
            .filter(|(_, value)| *value == &format!("target:{parent}"))
            .collect::<Vec<_>>();
        ensure!(
            contexts.len() == 1,
            "{consumer} must consume normalized parent {parent} through exactly one named context"
        );
        let context = contexts[0].0.clone();
        let closure = target_dependency_closure(definition, std::slice::from_ref(parent))?;
        let mut prepared = BTreeMap::new();
        let mut graph = BTreeMap::new();
        for name in closure {
            let target = &definition.target[&name];
            for value in target.contexts.values() {
                ensure!(
                    value.starts_with("target:")
                        || (value.starts_with("docker-image://") && value.contains("@sha256:")),
                    "normalized dependency {name} has a mutable or undeclared external context: {value}"
                );
            }
            let root = safe_relative(&target.context)?;
            let root = repository.join(root);
            ensure!(
                fs::canonicalize(&root)?.starts_with(fs::canonicalize(repository)?),
                "normalized context must remain inside its source repository"
            );
            let paths = target.labels.get(INPUTS_LABEL).with_context(|| {
                format!("normalized dependency {name} must declare {INPUTS_LABEL}")
            })?;
            let available = source_context::tracked_files(repository)?;
            let prefix = if target.context == "." {
                Path::new("")
            } else {
                Path::new(&target.context)
            };
            let available = available
                .into_iter()
                .filter_map(|path| path.strip_prefix(prefix).ok().map(Path::to_owned))
                .collect::<BTreeSet<_>>();
            let files = admitted_files(&root, paths, &target.dockerfile, &available)?;
            prepared.insert(
                name.clone(),
                source_context::materialize(&root, files, Vec::new())?,
            );
            graph.insert(name, recipe_target(target.clone()));
        }
        let inputs = prepared
            .iter()
            .map(|(name, context)| {
                (
                    name.clone(),
                    InputIdentity {
                        digest: context.identity.digest.clone(),
                        files: context.identity.files,
                    },
                )
            })
            .collect();
        let recipe_digest = recipe_digest(&graph, &inputs)?;
        parents.push(PreparedParent {
            plan: ParentPlan {
                consumer: consumer.clone(),
                target: parent.clone(),
                context,
                recipe_digest,
                inputs,
            },
            contexts: prepared,
        });
    }
    Ok(parents)
}

fn safe_relative(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    ensure!(
        !value.is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_) | Component::CurDir)),
        "normalized input path must be relative without traversal: {value}"
    );
    Ok(path)
}

fn admitted_files(
    root: &Path,
    paths: &str,
    dockerfile: &str,
    available: &BTreeSet<PathBuf>,
) -> Result<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    for value in paths.split(',') {
        let path = safe_relative(value)?;
        let selected = available
            .iter()
            .filter(|file| file.starts_with(path))
            .cloned()
            .collect::<Vec<_>>();
        ensure!(
            !selected.is_empty(),
            "normalized input {value} is absent or ignored in {}",
            root.display()
        );
        files.extend(selected);
    }
    ensure!(
        files.contains(safe_relative(dockerfile)?),
        "normalized dependency must admit its Dockerfile {dockerfile}"
    );
    for path in [
        PathBuf::from(".dockerignore"),
        PathBuf::from(format!("{dockerfile}.dockerignore")),
    ] {
        if available.contains(&path) {
            files.insert(path);
        }
    }
    Ok(files)
}

fn recipe_target(mut target: BakeTarget) -> BakeTarget {
    // These publication destinations and cache hints do not alter filesystem output.
    target.tags.clear();
    for field in ["cache-from", "cache-to", "output", "attest"] {
        target.options.remove(field);
    }
    target
}

fn recipe_digest(
    graph: &BTreeMap<String, BakeTarget>,
    inputs: &BTreeMap<String, InputIdentity>,
) -> Result<String> {
    let mut digest = Sha256::new();
    digest.update(b"veoveo.io/normalized-parent-recipe/v1\0");
    digest.update(serde_json::to_vec(&(
        graph,
        inputs,
        REPRODUCIBLE_BUILD_EPOCH,
        veoveo_image_build_control::BUILDKIT_IMAGE,
        OutputMode::Staged.exporter(),
    ))?);
    Ok(format!("sha256:{}", hex::encode(digest.finalize())))
}

pub(super) fn resolve(
    builder_repository: &RepositoryContext,
    source: &RepositoryContext,
    prepared: &PreparedPlan,
    environment: &BTreeMap<String, String>,
    evidence: &EvidenceRun,
    insecure: bool,
) -> Result<Option<NamedTempFile>> {
    if prepared.parents.is_empty() {
        return Ok(None);
    }
    let _timing = operation::span(operation::Phase::ParentResolution);
    let registry = environment
        .get("VEOVEO_REGISTRY")
        .filter(|value| !value.is_empty())
        .context("normalized parent publication requires a registry")?;
    let mut overrides = Override::default();
    for parent in &prepared.parents {
        let plan = &parent.plan;
        let repository = format!("{registry}/veoveo/{}", plan.target);
        let reference = format!(
            "{repository}:recipe-{}",
            plan.recipe_digest.trim_start_matches("sha256:")
        );
        let cache = builder_repository
            .root()
            .join("target/veoveo-xtask/normalized")
            .join(hex::encode(Sha256::digest(registry.as_bytes())))
            .join(plan.recipe_digest.trim_start_matches("sha256:"));
        fs::create_dir_all(&cache)?;
        let receipt_path = cache.join("receipt.json");
        let directory = evidence.directory().join("parents").join(&plan.target);
        fs::create_dir_all(&directory)?;
        let receipt = if receipt_path.exists() {
            let receipt: Receipt = serde_json::from_slice(&fs::read(&receipt_path)?)?;
            ensure!(
                receipt.schema_version == RECEIPT_SCHEMA
                    && receipt.recipe_digest == plan.recipe_digest
                    && receipt.reference == reference,
                "normalized parent receipt does not match the admitted recipe: {}",
                receipt_path.display()
            );
            receipt
        } else {
            let receipt = if let Some(runtime_digest) =
                image_manifest::resolve_staged_reference(&reference, insecure)?
            {
                Receipt {
                    schema_version: RECEIPT_SCHEMA.to_owned(),
                    recipe_digest: plan.recipe_digest.clone(),
                    reference: reference.clone(),
                    runtime_digest,
                    source_revision: super::git_output(source.root(), ["rev-parse", "HEAD"])?
                        .trim()
                        .to_owned(),
                }
            } else {
                publish(
                    builder_repository,
                    source,
                    parent,
                    environment,
                    &reference,
                    &directory,
                )?
            };
            image_manifest::inspect_staged(
                &repository,
                &receipt.runtime_digest,
                "linux/amd64",
                insecure,
            )?;
            write_json(&receipt_path, &receipt)?;
            receipt
        };
        // Fail closed if a retained immutable input has disappeared. A registry failure
        // is not permission to silently rebuild or replace an admitted parent.
        image_manifest::inspect_staged(
            &repository,
            &receipt.runtime_digest,
            "linux/amd64",
            insecure,
        )?;
        write_json(&directory.join("receipt.json"), &receipt)?;
        println!(
            "Normalized parent {}: {}@{}",
            plan.target, repository, receipt.runtime_digest
        );
        overrides
            .target
            .entry(plan.consumer.clone())
            .or_default()
            .contexts
            .insert(
                plan.context.clone(),
                format!("docker-image://{repository}@{}", receipt.runtime_digest),
            );
    }
    Ok(Some(override_file(&overrides)?))
}

fn publish(
    builder_repository: &RepositoryContext,
    source: &RepositoryContext,
    parent: &PreparedParent,
    environment: &BTreeMap<String, String>,
    reference: &str,
    directory: &Path,
) -> Result<Receipt> {
    let _timing = operation::span(operation::Phase::ParentPublication);
    let mut overrides = Override::default();
    for (name, context) in &parent.contexts {
        overrides.target.insert(
            name.clone(),
            TargetOverride {
                context: Some(context.path().to_owned()),
                ..Default::default()
            },
        );
    }
    let file = override_file(&overrides)?;
    let target = &parent.plan.target;
    let metadata = directory.join("buildx-metadata.json");
    let mut command = builder::buildx_command(builder_repository)?;
    command
        .current_dir(source.root())
        .args(["bake", "--builder", builder::BUILDER_NAME, "-f"])
        .arg(source.root().join("docker-bake.hcl"))
        .arg("-f")
        .arg(file.path())
        .arg(target)
        .args(["--provenance=false", "--sbom=false", "--metadata-file"])
        .arg(&metadata)
        .arg("--set")
        .arg(format!("{target}.tags={reference}"))
        .arg("--set")
        .arg(format!("{target}.output={}", OutputMode::Staged.exporter()))
        .arg("--set")
        .arg(format!(
            "{target}.labels.io.veoveo.build.recipe={}",
            parent.plan.recipe_digest
        ))
        .envs(environment)
        .env("SOURCE_DATE_EPOCH", REPRODUCIBLE_BUILD_EPOCH.to_string());
    let cpu_before = veoveo_image_build_control::cpu_snapshot(builder_repository.root());
    let result = buildkit::execute(&mut command, &directory.join("buildkit-events.jsonl"));
    operation::cpu_delta(
        veoveo_image_build_control::cpu_snapshot(builder_repository.root()),
        cpu_before,
    );
    let (status, phases) = result?;
    write_json(&directory.join("phases.json"), &phases)?;
    ensure!(
        status.success(),
        "normalizing dependency {target} failed with {status}"
    );
    let metadata: BTreeMap<String, BuildMetadata> = serde_json::from_slice(&fs::read(metadata)?)?;
    let digest = metadata
        .get(target)
        .and_then(|metadata| metadata.digest.clone())
        .context("normalized publication omitted its image digest")?;
    Ok(Receipt {
        schema_version: RECEIPT_SCHEMA.to_owned(),
        recipe_digest: parent.plan.recipe_digest.clone(),
        reference: reference.to_owned(),
        runtime_digest: digest,
        source_revision: super::git_output(source.root(), ["rev-parse", "HEAD"])?
            .trim()
            .to_owned(),
    })
}

fn override_file(overrides: &Override) -> Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    serde_json::to_writer_pretty(&mut file, overrides)?;
    file.flush()?;
    Ok(file)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = NamedTempFile::new_in(path.parent().context("evidence has no parent")?)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn auxiliary_target_metadata_does_not_require_an_exported_image() {
        let metadata: BTreeMap<String, BuildMetadata> = serde_json::from_value(json!({
            "parent": {"containerimage.digest":"sha256:parent"},
            "payload": {"buildx.build.ref":"worker/reference"}
        }))
        .unwrap();
        assert_eq!(metadata["parent"].digest.as_deref(), Some("sha256:parent"));
        assert!(metadata["payload"].digest.is_none());
    }

    #[test]
    fn payload_options_and_input_bytes_define_recipe_not_publication_destinations() {
        let original: BakeTarget = serde_json::from_value(json!({
            "context":"runtime", "dockerfile":"Dockerfile.dependencies", "target":"dependencies",
            "args":{"BASE_IMAGE":"example@sha256:abc"}, "platforms":["linux/amd64"],
            "tags":["registry/old"], "cache-to":[{"ref":"old-cache"}],
            "network":"none"
        }))
        .unwrap();
        let inputs = BTreeMap::from([(
            "dependency".into(),
            InputIdentity {
                digest: "sha256:input".into(),
                files: 3,
            },
        )]);
        let key = |target| {
            recipe_digest(
                &BTreeMap::from([("dependency".into(), recipe_target(target))]),
                &inputs,
            )
            .unwrap()
        };
        let initial = key(original.clone());
        let mut changed = original.clone();
        changed.tags = vec!["registry/new".into()];
        changed
            .options
            .insert("cache-to".into(), json!([{"ref":"new-cache"}]));
        assert_eq!(initial, key(changed));
        for (name, value) in [("target", json!("other-stage")), ("network", json!("host"))] {
            let mut changed = original.clone();
            changed.options.insert(name.into(), value);
            assert_ne!(initial, key(changed));
        }
        let mut changed = original.clone();
        changed
            .args
            .insert("BASE_IMAGE".into(), "new-base@sha256:def".into());
        assert_ne!(initial, key(changed));
        let mut changed = inputs.clone();
        changed.get_mut("dependency").unwrap().digest = "sha256:changed".into();
        assert_ne!(
            initial,
            recipe_digest(
                &BTreeMap::from([("dependency".into(), recipe_target(original))]),
                &changed
            )
            .unwrap()
        );
    }

    #[test]
    fn app_source_is_unavailable_to_dependency_builds_and_patches_change_identity() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("patches")).unwrap();
        for (name, bytes) in [
            ("Dockerfile.dependencies", "FROM scratch"),
            ("patches/fix.patch", "patch-one"),
            ("app.py", "application-one"),
        ] {
            fs::write(root.path().join(name), bytes).unwrap();
        }
        let available = ["Dockerfile.dependencies", "patches/fix.patch", "app.py"]
            .map(PathBuf::from)
            .into_iter()
            .collect();
        let prepare = || {
            let files = admitted_files(
                root.path(),
                "Dockerfile.dependencies,patches",
                "Dockerfile.dependencies",
                &available,
            )
            .unwrap();
            source_context::materialize(root.path(), files, Vec::new()).unwrap()
        };
        let first = prepare();
        assert!(!first.path().join("app.py").exists());
        fs::write(root.path().join("app.py"), "application-two").unwrap();
        assert_eq!(first.identity.digest, prepare().identity.digest);
        fs::write(root.path().join("patches/fix.patch"), "patch-two").unwrap();
        assert_ne!(first.identity.digest, prepare().identity.digest);
        for paths in ["../escape", "/absolute", "missing", "patches"] {
            assert!(
                admitted_files(root.path(), paths, "Dockerfile.dependencies", &available).is_err()
            );
        }
    }
}
