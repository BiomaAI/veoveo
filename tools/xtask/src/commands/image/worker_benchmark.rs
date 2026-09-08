//! Compare complete BuildKit results with incremental Cargo state on a fresh worker.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_image_build_control::{ExperimentWorker, cpu_snapshot};

use super::{
    PreparedPlan, Selection,
    benchmark::{compiled_packages, validate_source_path, varied_source},
    buildkit,
    cache_benchmark::artifact_digests,
    operation, prepare,
};
use crate::{
    BuilderWorkerBenchmarkArgs, ReleasePreflightArgs,
    commands::{builder, release_preflight},
    context::RepositoryContext,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Case {
    Export,
    Import,
    PrimaryEdit,
    SecondaryEdit,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Export => "export",
            Self::Import => "import",
            Self::PrimaryEdit => "primary-edit",
            Self::SecondaryEdit => "secondary-edit",
        }
    }
    fn secondary(self) -> bool {
        matches!(self, Self::Import | Self::SecondaryEdit)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Comparison {
    schema: &'static str,
    source_revision: String,
    source_dirty: bool,
    buildkit_image: &'static str,
    same_host: bool,
    second_worker: String,
    second_worker_identity: String,
    initial_cache_records: u64,
    cache_mounts_after_import: Option<u64>,
    cache_manifest_digest: Option<String>,
    cache_export_bytes: Option<u64>,
    samples: Vec<Sample>,
    worker_removed: bool,
    export_removed: bool,
    accepted: bool,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sample {
    case: Case,
    worker: String,
    elapsed_millis: u64,
    phases: buildkit::PhaseTimings,
    cpu_usage_micros: u64,
    throttled_periods: u64,
    source_sha256: BTreeMap<PathBuf, String>,
    compiled_packages: BTreeSet<String>,
    artifact_sha256: BTreeMap<PathBuf, String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct CacheIndex {
    manifests: Vec<CacheManifest>,
}
#[derive(Deserialize)]
struct CacheManifest {
    digest: veoveo_extension_contract::ArtifactDigest,
}

pub(crate) fn run(repository: &RepositoryContext, args: &BuilderWorkerBenchmarkArgs) -> Result<()> {
    for source in &args.source {
        validate_source_path(source)?;
    }
    let lease = builder::ensure(repository)?;
    // Covers a fresh compiler target, exported layers, and local binary snapshots.
    resource_preflight(repository, 40)?;
    let prepared = prepare(
        repository,
        Selection::exact("worker-benchmark", args.target.clone())?,
        &BTreeMap::new(),
    )?;
    ensure!(
        prepared.plan.families.len() == 1
            && prepared
                .plan
                .targets
                .iter()
                .all(|target| target.rust.is_some() && target.platform == "linux/amd64"),
        "worker benchmark requires one Linux amd64 Rust family"
    );
    let family = &prepared.plan.families[0];
    let artifact = family
        .family
        .shared_artifact_target()
        .context("worker benchmark requires an admitted shared artifact target")?;
    let original = args
        .source
        .iter()
        .map(|path| {
            let source = family.context_path.join(path);
            ensure!(
                fs::symlink_metadata(&source)?.is_file(),
                "benchmark source is not a regular admitted input"
            );
            Ok((path.clone(), fs::read(source)?))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let output = if args.output.is_absolute() {
        args.output.clone()
    } else {
        repository.root().join(&args.output)
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&output).context("worker benchmark requires a new output directory")?;
    fs::write(
        output.join("plan.json"),
        serde_json::to_vec_pretty(&prepared.plan)?,
    )?;
    // The exported OCI cache is a transport input, removed after the experiment.
    let cache = tempfile::Builder::new()
        .prefix("worker-cache-")
        .tempdir_in(&output)?;
    let mut worker = ExperimentWorker::create(repository.root(), &lease)?;
    let mut comparison = Comparison {
        schema: "veoveo.io/compiler-worker-comparison/v1",
        source_revision: prepared.plan.source.revision.clone(),
        source_dirty: prepared.plan.source.dirty,
        buildkit_image: veoveo_image_build_control::BUILDKIT_IMAGE,
        same_host: true,
        second_worker: worker.name().into(),
        second_worker_identity: worker.identity()?,
        initial_cache_records: worker.cache_records(None)?,
        cache_mounts_after_import: None,
        cache_manifest_digest: None,
        cache_export_bytes: None,
        samples: Vec::new(),
        worker_removed: false,
        export_removed: false,
        accepted: false,
        error: None,
    };
    let outcome = (|| -> Result<()> {
        for case in [
            Case::Export,
            Case::Import,
            Case::PrimaryEdit,
            Case::SecondaryEdit,
        ] {
            let mut source_hashes = BTreeMap::new();
            for (path, bytes) in &original {
                let bytes = if matches!(case, Case::PrimaryEdit | Case::SecondaryEdit) {
                    varied_source(bytes, worker.name())
                } else {
                    bytes.clone()
                };
                fs::write(family.context_path.join(path), &bytes)?;
                source_hashes.insert(
                    path.clone(),
                    format!("sha256:{}", hex::encode(Sha256::digest(&bytes))),
                );
            }
            resource_preflight(repository, if case.secondary() { 24 } else { 0 })?;
            println!("Compiler worker benchmark: {}", case.name());
            let directory = output.join(case.name());
            fs::create_dir(&directory)?;
            let mut sample = Sample {
                case,
                worker: if case.secondary() {
                    worker.name()
                } else {
                    builder::BUILDER_NAME
                }
                .into(),
                elapsed_millis: 0,
                phases: buildkit::PhaseTimings::default(),
                cpu_usage_micros: 0,
                throttled_periods: 0,
                source_sha256: source_hashes,
                compiled_packages: BTreeSet::new(),
                artifact_sha256: BTreeMap::new(),
                error: None,
            };
            let result = solve(
                &Solve {
                    repository,
                    prepared: &prepared,
                    artifact,
                    directory: &directory,
                    cache: cache.path(),
                    cache_digest: comparison.cache_manifest_digest.as_deref(),
                    worker: &worker,
                },
                &mut sample,
            );
            sample.error = result.as_ref().err().map(|error| format!("{error:#}"));
            comparison.samples.push(sample);
            fs::write(
                output.join("comparison.json"),
                serde_json::to_vec_pretty(&comparison)?,
            )?;
            result?;
            if case == Case::Export {
                let index = fs::read(cache.path().join("index.json"))?;
                comparison.cache_manifest_digest = Some(cache_digest(&index)?);
                comparison.cache_export_bytes = Some(directory_bytes(cache.path())?);
                fs::write(output.join("cache-index.json"), index)?;
            }
            if case == Case::Import {
                comparison.cache_mounts_after_import =
                    Some(worker.cache_records(Some("type==exec.cachemount"))?);
                ensure!(
                    comparison.cache_mounts_after_import == Some(0),
                    "cache import unexpectedly populated Cargo execution mounts"
                );
                ensure!(
                    comparison.samples[1].compiled_packages.is_empty()
                        && comparison.samples[1].phases.compile_millis == 0,
                    "unchanged cache import executed compilation"
                );
            }
        }
        validate_samples(&comparison.samples)?;
        Ok(())
    })();
    let removal = worker.remove();
    comparison.worker_removed = removal.is_ok();
    let export_removal = cache.close();
    comparison.export_removed = export_removal.is_ok();
    comparison.error = outcome
        .as_ref()
        .err()
        .map(|error| format!("{error:#}"))
        .or_else(|| removal.as_ref().err().map(|error| format!("{error:#}")))
        .or_else(|| {
            export_removal
                .as_ref()
                .err()
                .map(|error| format!("{error:#}"))
        });
    comparison.accepted = comparison.error.is_none();
    fs::write(
        output.join("comparison.json"),
        serde_json::to_vec_pretty(&comparison)?,
    )?;
    removal?;
    export_removal?;
    outcome?;
    println!(
        "Compiler worker comparison: {}",
        output.join("comparison.json").display()
    );
    Ok(())
}

struct Solve<'a, 'w> {
    repository: &'a RepositoryContext,
    prepared: &'a PreparedPlan,
    artifact: &'a str,
    directory: &'a Path,
    cache: &'a Path,
    cache_digest: Option<&'a str>,
    worker: &'a ExperimentWorker<'w>,
}

fn solve(inputs: &Solve<'_, '_>, sample: &mut Sample) -> Result<()> {
    let mut command = builder::buildx_command(inputs.repository)?;
    command
        .current_dir(inputs.repository.root())
        .args(["bake", "--builder", &sample.worker, "-f"])
        .arg(inputs.repository.root().join("docker-bake.hcl"))
        .arg("-f")
        .arg(inputs.prepared.override_file.path())
        .arg(inputs.artifact)
        .arg("--set")
        .arg(format!(
            "{}.output=type=local,dest={}",
            inputs.artifact,
            inputs.directory.join("artifacts").display()
        ))
        .arg("--metadata-file")
        .arg(inputs.directory.join("buildkit-metadata.json"))
        .env(
            "SOURCE_DATE_EPOCH",
            inputs.prepared.plan.build_date_epoch.to_string(),
        )
        .stdin(Stdio::null());
    if sample.case == Case::Export {
        command.arg("--set").arg(format!(
            "{}.cache-to=type=local,dest={},mode=max",
            inputs.artifact,
            inputs.cache.display()
        ));
    }
    if sample.case.secondary() {
        command.arg("--set").arg(format!(
            "{}.cache-from=type=local,src={},digest={}",
            inputs.artifact,
            inputs.cache.display(),
            inputs
                .cache_digest
                .context("second worker has no exported cache identity")?
        ));
    }
    let snapshot = || {
        if sample.case.secondary() {
            inputs.worker.cpu_snapshot()
        } else {
            cpu_snapshot(inputs.repository.root())
        }
    };
    let before = snapshot()?;
    let started = Instant::now();
    let trace = inputs.directory.join("buildkit-events.jsonl");
    operation::solve_evidence(&trace);
    let _span = operation::span(operation::Phase::Solve);
    let result = buildkit::execute(&mut command, &trace);
    sample.elapsed_millis = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let after = snapshot()?;
    let delta = after
        .checked_delta(before)
        .context("worker CPU counters reset during solve")?;
    operation::cpu_delta(Ok(after), Ok(before));
    sample.cpu_usage_micros = delta.usage_us;
    sample.throttled_periods = delta.throttled_periods;
    let (status, phases) = result?;
    sample.phases = phases;
    ensure!(status.success(), "worker benchmark solve failed: {status}");
    sample.compiled_packages = compiled_packages(&fs::read_to_string(trace)?)?;
    sample.artifact_sha256 = artifact_digests(&inputs.directory.join("artifacts"))?;
    Ok(())
}

fn resource_preflight(repository: &RepositoryContext, expected_growth_gib: u64) -> Result<()> {
    release_preflight::run(
        repository,
        &ReleasePreflightArgs {
            expected_growth_gib,
            reserve_free_percent: 20,
            kubernetes_node: None,
            namespace: "veoveo".into(),
        },
    )
}

fn cache_digest(bytes: &[u8]) -> Result<String> {
    let index: CacheIndex =
        serde_json::from_slice(bytes).context("decoding exported OCI cache index")?;
    ensure!(
        index.manifests.len() == 1,
        "exported cache must have one exact root manifest"
    );
    Ok(index.manifests[0].digest.as_str().into())
}

fn directory_bytes(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        let bytes = if metadata.is_dir() {
            directory_bytes(&entry.path())?
        } else {
            ensure!(
                metadata.is_file(),
                "exported cache contains a non-file entry"
            );
            metadata.len()
        };
        total = total.checked_add(bytes).context("cache size overflow")?;
    }
    Ok(total)
}

fn validate_samples(samples: &[Sample]) -> Result<()> {
    ensure!(
        samples.iter().all(|sample| sample.error.is_none()
            && !sample.artifact_sha256.is_empty()
            && !sample.source_sha256.is_empty()),
        "worker sample has missing or failed artifact evidence"
    );
    ensure!(
        samples.len() == 4
            && samples
                .iter()
                .zip([
                    Case::Export,
                    Case::Import,
                    Case::PrimaryEdit,
                    Case::SecondaryEdit
                ])
                .all(|(sample, case)| sample.case == case),
        "worker comparison requires all four ordered cases"
    );
    for (left, right) in [(0, 1), (2, 3)] {
        ensure!(
            samples[left].source_sha256 == samples[right].source_sha256
                && samples[left].artifact_sha256 == samples[right].artifact_sha256,
            "matching worker inputs produced different artifact bytes"
        );
    }
    ensure!(
        samples[0].source_sha256 != samples[2].source_sha256,
        "source-edit sample did not change compiler inputs"
    );
    ensure!(
        samples[1].compiled_packages.is_empty() && samples[1].phases.compile_millis == 0,
        "unchanged second worker compiled Rust"
    );
    ensure!(
        !samples[2].compiled_packages.is_empty()
            && samples[2]
                .compiled_packages
                .is_subset(&samples[3].compiled_packages),
        "source-edit comparison has inconsistent compiler package evidence"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_import_requires_one_immutable_manifest() {
        assert!(cache_digest(br#"{"manifests":[]}"#).is_err());
        assert!(cache_digest(br#"{"manifests":[{"digest":"latest"}]}"#).is_err());
        let digest = format!("sha256:{}", "a".repeat(64));
        let value = serde_json::json!({"manifests":[{"digest":digest}]});
        assert_eq!(
            cache_digest(&serde_json::to_vec(&value).unwrap()).unwrap(),
            digest
        );
        let repeated = value["manifests"][0].clone();
        let mut doubled = value;
        doubled["manifests"].as_array_mut().unwrap().push(repeated);
        assert!(cache_digest(&serde_json::to_vec(&doubled).unwrap()).is_err());
    }

    fn samples() -> Vec<Sample> {
        [
            Case::Export,
            Case::Import,
            Case::PrimaryEdit,
            Case::SecondaryEdit,
        ]
        .into_iter()
        .map(|case| Sample {
            case,
            worker: case.name().into(),
            elapsed_millis: 1,
            phases: buildkit::PhaseTimings::default(),
            cpu_usage_micros: 1,
            throttled_periods: 0,
            source_sha256: BTreeMap::from([(
                PathBuf::from("main.rs"),
                if matches!(case, Case::Export | Case::Import) {
                    "original"
                } else {
                    "edited"
                }
                .into(),
            )]),
            compiled_packages: if matches!(case, Case::PrimaryEdit | Case::SecondaryEdit) {
                BTreeSet::from(["console-bff".into()])
            } else {
                BTreeSet::new()
            },
            artifact_sha256: BTreeMap::from([(
                PathBuf::from("bin/console-bff"),
                "same-artifact".into(),
            )]),
            error: None,
        })
        .collect()
    }

    #[test]
    fn worker_evidence_rejects_false_hits_mismatched_outputs_and_missing_source_edits() {
        validate_samples(&samples()).unwrap();
        let mut changed = samples();
        changed[1]
            .compiled_packages
            .insert("unexpected-rebuild".into());
        assert!(validate_samples(&changed).is_err());
        let mut changed = samples();
        changed[3]
            .artifact_sha256
            .values_mut()
            .for_each(|digest| *digest = "different".into());
        assert!(validate_samples(&changed).is_err());
        let mut changed = samples();
        changed[3]
            .source_sha256
            .values_mut()
            .for_each(|digest| *digest = "different-input".into());
        assert!(validate_samples(&changed).is_err());
        let mut changed = samples();
        changed.iter_mut().for_each(|sample| {
            sample
                .source_sha256
                .values_mut()
                .for_each(|digest| *digest = "unchanged".into())
        });
        assert!(validate_samples(&changed).is_err());
        let mut changed = samples();
        changed[3].error = Some("failed solve".into());
        assert!(validate_samples(&changed).is_err());
        let mut changed = samples();
        changed.pop();
        assert!(validate_samples(&changed).is_err());
    }
}
