//! Recover Rust compilation from a compiler cache with empty Cargo target trees.

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
use veoveo_image_build_control::cpu_snapshot;

use super::{PreparedPlan, Selection, benchmark::compiled_packages, buildkit, prepare};
use crate::{BuilderCacheBenchmarkArgs, commands::builder, context::RepositoryContext};

// Latest stable upstream release, verified through the GitHub release and asset digest.
const SCCACHE_VERSION: &str = "0.17.0";
const SCCACHE_URL: &str = "https://github.com/mozilla/sccache/releases/download/v0.17.0/sccache-v0.17.0-x86_64-unknown-linux-musl.tar.gz";
const SCCACHE_SHA256: &str = "67c4a96dd237c1f518f6b36083f270f9976d516f1e57fce891755ea782e50006";
const TARGET: &str = "veoveo-compiler-cache-experiment";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Case {
    Baseline,
    Populate,
    Restore,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Populate => "populate",
            Self::Restore => "restore",
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Comparison {
    schema: &'static str,
    source_revision: String,
    source_dirty: bool,
    sccache_version: &'static str,
    sccache_archive_sha256: &'static str,
    client_side_mode: bool,
    cache_mount_id: String,
    reference_artifact_sha256: BTreeMap<PathBuf, String>,
    samples: Vec<Sample>,
    accepted: bool,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sample {
    case: Case,
    elapsed_millis: u64,
    phases: buildkit::PhaseTimings,
    cpu_usage_micros: u64,
    throttled_periods: u64,
    cpu_periods: u64,
    compiled_packages: BTreeSet<String>,
    artifact_sha256: BTreeMap<PathBuf, String>,
    compiler_identity: Option<String>,
    cache_before: Option<CacheInfo>,
    cache: Option<CacheInfo>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CacheInfo {
    version: String,
    cache_location: String,
    cache_size: Option<u64>,
    max_cache_size: Option<u64>,
    stats: CacheStats,
}

#[derive(Debug, Deserialize, Serialize)]
struct CacheStats {
    compile_requests: u64,
    requests_unsupported_compiler: u64,
    cache_hits: LanguageCounts,
    cache_misses: LanguageCounts,
    cache_errors: LanguageCounts,
    cache_timeouts: u64,
    cache_read_errors: u64,
    cache_write_errors: u64,
    compile_fails: u64,
    not_cached: BTreeMap<String, u64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct LanguageCounts {
    counts: BTreeMap<String, u64>,
}

impl LanguageCounts {
    fn rust(&self) -> u64 {
        self.counts.get("Rust").copied().unwrap_or(0)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InputEvidence {
    target_initially_empty: bool,
    incremental: bool,
    compiler_cache_initially_empty: bool,
    case: Case,
}

#[derive(Serialize)]
struct Overrides {
    target: BTreeMap<String, TargetOverride>,
}

#[derive(Default, Serialize)]
struct TargetOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dockerfile: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    platforms: Vec<&'static str>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    contexts: BTreeMap<&'static str, String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    args: BTreeMap<&'static str, String>,
}

pub(crate) fn run(repository: &RepositoryContext, args: &BuilderCacheBenchmarkArgs) -> Result<()> {
    let _lease = builder::ensure(repository)?;
    let selection = Selection::exact("compiler-cache-benchmark", args.target.clone())?;
    let prepared = prepare(repository, selection, &BTreeMap::new())?;
    ensure!(
        prepared.plan.families.len() == 1,
        "cache benchmark requires one shared Rust family"
    );
    let family = &prepared.plan.families[0];
    let artifact = family
        .family
        .shared_artifact_target()
        .context("cache benchmark requires an admitted shared compiler family")?;
    ensure!(
        prepared
            .plan
            .targets
            .iter()
            .all(|target| target.rust.is_some() && target.platform == "linux/amd64"),
        "cache benchmark requires Linux amd64 Rust targets"
    );
    let output = if args.output.is_absolute() {
        args.output.clone()
    } else {
        repository.root().join(&args.output)
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&output).context("cache benchmark requires a new output directory")?;
    fs::write(
        output.join("plan.json"),
        serde_json::to_vec_pretty(&prepared.plan)?,
    )?;
    let cache_mount_id = format!(
        "veoveo-sccache-experiment-{}",
        chrono::Utc::now()
            .timestamp_nanos_opt()
            .context("benchmark time overflow")?
    );
    let overrides = Overrides {
        target: BTreeMap::from([
            (
                artifact.to_owned(),
                TargetOverride {
                    target: Some("compile"),
                    ..Default::default()
                },
            ),
            (
                TARGET.to_owned(),
                TargetOverride {
                    target: Some("artifacts"),
                    context: Some(repository.root().to_owned()),
                    dockerfile: Some("tools/image-build/compiler-cache.Dockerfile"),
                    platforms: vec!["linux/amd64"],
                    contexts: BTreeMap::from([
                        ("compiler-environment", format!("target:{artifact}")),
                        (
                            "veoveo-rust-source",
                            family
                                .context_path
                                .to_str()
                                .context("compiler context path is not UTF-8")?
                                .to_owned(),
                        ),
                    ]),
                    args: BTreeMap::from([
                        ("VEOVEO_CARGO_PACKAGES", family.packages.join(",")),
                        ("VEOVEO_CARGO_BINARIES", family.binaries.join(",")),
                        (
                            "VEOVEO_AUXILIARY",
                            family
                                .auxiliary
                                .iter()
                                .filter(|artifact| {
                                    **artifact != super::AuxiliaryArtifact::DuckdbSpatial
                                })
                                .map(|artifact| artifact.name())
                                .collect::<Vec<_>>()
                                .join(","),
                        ),
                        ("VEOVEO_CARGO_CACHE_ID", family.cargo_cache_id.clone()),
                        ("VEOVEO_COMPILER_CACHE_ID", cache_mount_id.clone()),
                        ("SCCACHE_VERSION", SCCACHE_VERSION.to_owned()),
                        ("SCCACHE_ARCHIVE_URL", SCCACHE_URL.to_owned()),
                        ("SCCACHE_ARCHIVE_SHA256", SCCACHE_SHA256.to_owned()),
                    ]),
                },
            ),
        ]),
    };
    let overrides_path = output.join("bake.json");
    fs::write(&overrides_path, serde_json::to_vec_pretty(&overrides)?)?;
    let mut comparison = Comparison {
        schema: "veoveo.io/compiler-cache-comparison/v1",
        source_revision: prepared.plan.source.revision.clone(),
        source_dirty: prepared.plan.source.dirty,
        sccache_version: SCCACHE_VERSION,
        sccache_archive_sha256: SCCACHE_SHA256,
        client_side_mode: true,
        cache_mount_id,
        reference_artifact_sha256: BTreeMap::new(),
        samples: Vec::new(),
        accepted: false,
        error: None,
    };
    let outcome = (|| -> Result<()> {
        // Establish the existing compiler environment outside the measured samples.
        let warmup = output.join("environment");
        fs::create_dir(&warmup)?;
        let mut command = bake_command(repository, &prepared)?;
        command.arg(artifact).arg("--set").arg(format!(
            "{artifact}.output=type=local,dest={}",
            warmup.join("artifacts").display()
        ));
        let (status, _) = buildkit::execute(&mut command, &warmup.join("buildkit-events.jsonl"))?;
        ensure!(
            status.success(),
            "compiler environment preparation failed: {status}"
        );
        comparison.reference_artifact_sha256 = artifact_digests(&warmup.join("artifacts"))?;
        for case in [Case::Baseline, Case::Populate, Case::Restore] {
            println!("Compiler cache benchmark: {}", case.name());
            let directory = output.join(case.name());
            fs::create_dir(&directory)?;
            let mut sample = Sample {
                case,
                elapsed_millis: 0,
                phases: buildkit::PhaseTimings::default(),
                cpu_usage_micros: 0,
                throttled_periods: 0,
                cpu_periods: 0,
                compiled_packages: BTreeSet::new(),
                artifact_sha256: BTreeMap::new(),
                compiler_identity: None,
                cache_before: None,
                cache: None,
                error: None,
            };
            let result = solve(
                repository,
                &prepared,
                &overrides_path,
                &directory,
                &mut sample,
            );
            sample.error = result.as_ref().err().map(|error| format!("{error:#}"));
            comparison.samples.push(sample);
            fs::write(
                output.join("comparison.json"),
                serde_json::to_vec_pretty(&comparison)?,
            )?;
            result?;
        }
        validate_samples(&comparison.samples)?;
        ensure!(
            comparison.samples[0].artifact_sha256 == comparison.reference_artifact_sha256,
            "fresh-target compilation differs from the ordinary artifact target; inspect source freshness and compiler flags before accepting cache reuse"
        );
        comparison.accepted = true;
        Ok(())
    })();
    comparison.error = outcome.as_ref().err().map(|error| format!("{error:#}"));
    fs::write(
        output.join("comparison.json"),
        serde_json::to_vec_pretty(&comparison)?,
    )?;
    outcome?;
    println!(
        "Compiler cache comparison: {}",
        output.join("comparison.json").display()
    );
    Ok(())
}

fn bake_command(
    repository: &RepositoryContext,
    prepared: &PreparedPlan,
) -> Result<std::process::Command> {
    let mut command = builder::buildx_command(repository)?;
    command
        .current_dir(repository.root())
        .args(["bake", "--builder", builder::BUILDER_NAME, "-f"])
        .arg(repository.root().join("docker-bake.hcl"))
        .arg("-f")
        .arg(prepared.override_file.path())
        .env(
            "SOURCE_DATE_EPOCH",
            prepared.plan.build_date_epoch.to_string(),
        )
        .stdin(Stdio::null());
    Ok(command)
}

fn solve(
    repository: &RepositoryContext,
    prepared: &PreparedPlan,
    overrides: &Path,
    directory: &Path,
    sample: &mut Sample,
) -> Result<()> {
    let mut command = bake_command(repository, prepared)?;
    command
        .arg("-f")
        .arg(overrides)
        .arg(TARGET)
        .arg("--set")
        .arg(format!(
            "{TARGET}.args.VEOVEO_CACHE_CASE={}",
            sample.case.name()
        ))
        .arg("--set")
        .arg(format!(
            "{TARGET}.output=type=local,dest={}",
            directory.join("artifacts").display()
        ))
        .arg("--metadata-file")
        .arg(directory.join("buildkit-metadata.json"));
    let before = cpu_snapshot(repository.root())?;
    let started = Instant::now();
    let result = buildkit::execute(&mut command, &directory.join("buildkit-events.jsonl"));
    sample.elapsed_millis = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let after = cpu_snapshot(repository.root());
    let (status, phases) = result?;
    sample.phases = phases;
    let delta = after?
        .checked_delta(before)
        .context("CPU counters reset during compiler cache comparison")?;
    sample.cpu_usage_micros = delta.usage_us;
    sample.throttled_periods = delta.throttled_periods;
    sample.cpu_periods = delta.periods;
    ensure!(status.success(), "compiler cache solve failed: {status}");
    sample.compiled_packages = compiled_packages(&fs::read_to_string(
        directory.join("buildkit-events.jsonl"),
    )?)?;
    ensure!(
        !sample.compiled_packages.is_empty(),
        "compiler cache sample did not execute Cargo compilation"
    );
    let evidence = directory.join("artifacts/evidence");
    let inputs: InputEvidence = serde_json::from_slice(&fs::read(evidence.join("inputs.json"))?)?;
    ensure!(
        inputs.target_initially_empty && !inputs.incremental && inputs.case == sample.case,
        "compiler cache sample has invalid target or incremental inputs"
    );
    sample.compiler_identity = Some(fs::read_to_string(evidence.join("compiler.txt"))?);
    let before: CacheInfo =
        serde_json::from_slice(&fs::read(evidence.join("sccache-before.json"))?)?;
    ensure!(
        before.stats.compile_requests == 0,
        "compiler cache statistics were not reset"
    );
    ensure!(
        before
            .max_cache_size
            .is_some_and(|size| size > 0 && size <= 8 * 1024 * 1024 * 1024),
        "compiler cache exceeds the experiment's storage budget"
    );
    match sample.case {
        Case::Baseline | Case::Populate => ensure!(
            inputs.compiler_cache_initially_empty,
            "baseline and populate require an initially empty compiler cache"
        ),
        Case::Restore => ensure!(
            !inputs.compiler_cache_initially_empty,
            "restore requires a populated compiler cache"
        ),
    }
    sample.cache_before = Some(before);
    sample.cache = Some(serde_json::from_slice(&fs::read(
        evidence.join("sccache.json"),
    )?)?);
    sample.artifact_sha256 = artifact_digests(&directory.join("artifacts"))?;
    validate_cache(
        sample.case,
        sample.cache.as_ref().context("missing cache statistics")?,
    )
}

fn validate_cache(case: Case, info: &CacheInfo) -> Result<()> {
    ensure!(
        info.version == SCCACHE_VERSION,
        "sccache version differs from the admitted experiment pin"
    );
    let stats = &info.stats;
    ensure!(
        stats.requests_unsupported_compiler == 0
            && stats.cache_read_errors == 0
            && stats.cache_write_errors == 0
            && stats.cache_timeouts == 0
            && stats.cache_errors.counts.values().all(|count| *count == 0),
        "compiler cache sample reports cache errors"
    );
    match case {
        Case::Baseline => ensure!(
            stats.compile_requests == 0,
            "baseline unexpectedly used the compiler wrapper"
        ),
        Case::Populate => ensure!(
            stats.cache_misses.rust() > 0,
            "populate sample did not cache any Rust compilation"
        ),
        Case::Restore => ensure!(
            stats.cache_hits.rust() > 0,
            "fresh-target restore sample has no Rust cache hits"
        ),
    }
    Ok(())
}

fn validate_samples(samples: &[Sample]) -> Result<()> {
    ensure!(
        samples.len() == 3,
        "compiler cache comparison requires all three samples"
    );
    ensure!(
        samples
            .windows(2)
            .all(|pair| pair[0].artifact_sha256 == pair[1].artifact_sha256
                && pair[0].compiler_identity == pair[1].compiler_identity
                && pair[0].compiled_packages == pair[1].compiled_packages),
        "compiler cache samples differ in compiler, compiled packages, or binary bytes"
    );
    Ok(())
}

pub(super) fn artifact_digests(root: &Path) -> Result<BTreeMap<PathBuf, String>> {
    fn collect(
        root: &Path,
        directory: &Path,
        result: &mut BTreeMap<PathBuf, String>,
    ) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(root)?;
            if relative == Path::new("evidence") {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_dir() {
                collect(root, &path, result)?;
            } else {
                ensure!(
                    kind.is_file(),
                    "compiled artifacts must be regular files: {}",
                    path.display()
                );
                result.insert(
                    relative.to_owned(),
                    format!("sha256:{}", hex::encode(Sha256::digest(fs::read(&path)?))),
                );
            }
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    collect(root, root, &mut result)?;
    ensure!(!result.is_empty(), "compiler exported no artifacts");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_info() -> CacheInfo {
        let empty = || LanguageCounts {
            counts: BTreeMap::new(),
        };
        CacheInfo {
            version: SCCACHE_VERSION.to_owned(),
            cache_location: "/compiler-cache".to_owned(),
            cache_size: Some(0),
            max_cache_size: Some(8_000_000_000),
            stats: CacheStats {
                compile_requests: 0,
                requests_unsupported_compiler: 0,
                cache_hits: empty(),
                cache_misses: empty(),
                cache_errors: empty(),
                cache_timeouts: 0,
                cache_read_errors: 0,
                cache_write_errors: 0,
                compile_fails: 0,
                not_cached: BTreeMap::new(),
            },
        }
    }

    #[test]
    fn cache_evidence_distinguishes_baseline_population_and_recovery() {
        let mut info = cache_info();
        validate_cache(Case::Baseline, &info).unwrap();
        assert!(validate_cache(Case::Populate, &info).is_err());
        assert!(validate_cache(Case::Restore, &info).is_err());
        info.stats.compile_requests = 10;
        info.stats.cache_misses.counts.insert("Rust".to_owned(), 8);
        validate_cache(Case::Populate, &info).unwrap();
        assert!(validate_cache(Case::Baseline, &info).is_err());
        info.stats.cache_hits.counts.insert("Rust".to_owned(), 8);
        validate_cache(Case::Restore, &info).unwrap();
        validate_cache(Case::Populate, &info).unwrap();
        // Native configure probes deliberately invoke compilers on unsupported inputs.
        // The complete Cargo action must succeed, but this diagnostic need not be zero.
        info.stats.compile_fails = 5;
        validate_cache(Case::Restore, &info).unwrap();
        info.stats.cache_write_errors = 1;
        assert!(validate_cache(Case::Restore, &info).is_err());
    }

    #[test]
    fn artifact_identity_includes_auxiliary_libraries_and_excludes_case_statistics() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        for name in ["bin", "lib", "evidence"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        fs::write(root.join("bin/server"), "binary").unwrap();
        fs::write(root.join("lib/libduckdb.so"), "native library").unwrap();
        fs::write(root.join("evidence/sccache.json"), "first statistics").unwrap();
        let first = artifact_digests(root).unwrap();
        assert_eq!(first.len(), 2);
        fs::write(root.join("evidence/sccache.json"), "second statistics").unwrap();
        assert_eq!(first, artifact_digests(root).unwrap());
        fs::write(root.join("lib/libduckdb.so"), "changed native library").unwrap();
        assert_ne!(first, artifact_digests(root).unwrap());
    }
}
