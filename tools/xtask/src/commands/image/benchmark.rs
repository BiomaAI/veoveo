//! Compiler-only source-edit comparisons under an exclusive worker lease.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_image_build_control::{CpuQuotaLease, CpuSnapshot, cpu_snapshot};

use super::{PreparedPlan, Selection, buildkit, prepare};
use crate::{BuilderBenchmarkArgs, commands::builder, context::RepositoryContext};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Comparison {
    schema: &'static str,
    started_at: String,
    source_revision: String,
    source_dirty: bool,
    artifact_target: String,
    source_paths: Vec<PathBuf>,
    samples: Vec<Sample>,
    restored_declared_quota: bool,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sample {
    name: String,
    cpus: u32,
    warmup: bool,
    elapsed_millis: u64,
    phases: buildkit::PhaseTimings,
    cpu_usage_micros: u64,
    cpu_periods: u64,
    throttled_periods: u64,
    throttled_micros: u64,
    source_sha256: BTreeMap<PathBuf, String>,
    compiled_packages: BTreeSet<String>,
    binary_sha256: BTreeMap<String, String>,
    success: bool,
    error: Option<String>,
}

pub(crate) fn run(repository: &RepositoryContext, args: &BuilderBenchmarkArgs) -> Result<()> {
    ensure!(
        args.cpus.len() >= 2,
        "benchmark requires at least two CPU samples"
    );
    for path in &args.source {
        validate_source_path(path)?;
    }
    let selection = Selection::exact("cpu-benchmark", args.target.clone())?;
    let lease = builder::ensure(repository)?;
    let prepared = prepare(repository, selection, &BTreeMap::new())?;
    ensure!(
        prepared.plan.families.len() == 1,
        "benchmark requires one shared Rust compiler family"
    );
    let family = &prepared.plan.families[0];
    let artifact = family
        .family
        .shared_artifact_target()
        .context("benchmark requires an admitted shared artifact target")?;
    ensure!(
        prepared
            .plan
            .targets
            .iter()
            .all(|target| target.rust.is_some()),
        "benchmark targets must all be Rust images"
    );
    let original_sources = args
        .source
        .iter()
        .map(|path| {
            let source = family.context_path.join(path);
            ensure!(
                fs::symlink_metadata(&source)?.is_file(),
                "benchmark source must be a regular admitted compiler input: {}",
                path.display()
            );
            Ok((path.clone(), fs::read(source)?))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let output = if args.output.is_absolute() {
        args.output.clone()
    } else {
        repository.root().join(&args.output)
    };
    ensure!(
        !output.exists(),
        "benchmark output already exists: {}",
        output.display()
    );
    fs::create_dir_all(&output)?;
    fs::write(
        output.join("plan.json"),
        serde_json::to_vec_pretty(&prepared.plan)?,
    )?;
    let started = chrono::Utc::now();
    let mut comparison = Comparison {
        schema: "veoveo.io/compiler-cpu-comparison/v1",
        started_at: started.to_rfc3339(),
        source_revision: prepared.plan.source.revision.clone(),
        source_dirty: prepared.plan.source.dirty,
        artifact_target: artifact.to_owned(),
        source_paths: original_sources.keys().cloned().collect(),
        samples: Vec::new(),
        restored_declared_quota: false,
        error: None,
    };
    let mut quota = CpuQuotaLease::new(repository.root(), &lease)?;
    let result = (|| -> Result<()> {
        let declared = veoveo_image_build_control::RESOURCES;
        let warmup_cpus = u32::try_from(declared.cpu_quota_us / declared.cpu_period_us)?;
        let mut cases = vec![("warmup".to_owned(), warmup_cpus, true)];
        cases.extend(
            args.cpus
                .iter()
                .enumerate()
                .map(|(i, cpus)| (format!("sample-{}-{cpus}cpu", i + 1), *cpus, false)),
        );
        for (name, cpus, warmup) in cases {
            quota.set(cpus)?;
            let mut sources = BTreeMap::new();
            for (path, original) in &original_sources {
                let bytes = if warmup {
                    original.clone()
                } else {
                    varied_source(
                        original,
                        &format!(
                            "{}-{name}",
                            started
                                .timestamp_nanos_opt()
                                .context("benchmark time overflow")?
                        ),
                    )
                };
                // The generated context owns independent files. Never edit the worktree.
                if !warmup {
                    fs::write(family.context_path.join(path), &bytes)?;
                }
                sources.insert(
                    path.clone(),
                    format!("sha256:{}", hex::encode(Sha256::digest(&bytes))),
                );
            }
            println!("Compiler benchmark {name}: {cpus} CPUs");
            let directory = output.join(&name);
            fs::create_dir(&directory)?;
            let mut sample = Sample {
                name,
                cpus,
                warmup,
                elapsed_millis: 0,
                phases: buildkit::PhaseTimings::default(),
                cpu_usage_micros: 0,
                cpu_periods: 0,
                throttled_periods: 0,
                throttled_micros: 0,
                source_sha256: sources,
                compiled_packages: BTreeSet::new(),
                binary_sha256: BTreeMap::new(),
                success: false,
                error: None,
            };
            let outcome = solve(repository, &prepared, artifact, &directory, &mut sample);
            sample.error = outcome.as_ref().err().map(|error| format!("{error:#}"));
            comparison.samples.push(sample);
            fs::write(
                output.join("comparison.json"),
                serde_json::to_vec_pretty(&comparison)?,
            )?;
            outcome?;
        }
        let measured = comparison
            .samples
            .iter()
            .filter(|sample| !sample.warmup)
            .collect::<Vec<_>>();
        ensure!(
            measured
                .windows(2)
                .all(|pair| pair[0].compiled_packages == pair[1].compiled_packages),
            "measured samples compiled different package sets; inspect the warm dependency baseline"
        );
        ensure!(
            measured
                .windows(2)
                .all(|pair| pair[0].binary_sha256 == pair[1].binary_sha256),
            "measured samples produced different binary bytes; the fixture is not a controlled artifact comparison"
        );
        Ok(())
    })();
    let restoration = quota.restore();
    comparison.restored_declared_quota = restoration.is_ok();
    comparison.error = result
        .as_ref()
        .err()
        .or_else(|| restoration.as_ref().err())
        .map(|error| format!("{error:#}"));
    fs::write(
        output.join("comparison.json"),
        serde_json::to_vec_pretty(&comparison)?,
    )?;
    restoration.context("restoring declared builder resources after benchmark")?;
    result?;
    println!(
        "Compiler comparison: {}",
        output.join("comparison.json").display()
    );
    Ok(())
}

fn solve(
    repository: &RepositoryContext,
    prepared: &PreparedPlan,
    artifact: &str,
    directory: &Path,
    sample: &mut Sample,
) -> Result<()> {
    let mut command = builder::buildx_command(repository)?;
    command
        .current_dir(repository.root())
        .args(["bake", "--builder", builder::BUILDER_NAME, "-f"])
        .arg(repository.root().join("docker-bake.hcl"))
        .arg("-f")
        .arg(prepared.override_file.path())
        .arg(artifact)
        .arg("--set")
        .arg(format!(
            "{artifact}.output=type=local,dest={}",
            directory.join("artifacts").display()
        ))
        .arg("--metadata-file")
        .arg(directory.join("buildkit-metadata.json"))
        .env(
            "SOURCE_DATE_EPOCH",
            prepared.plan.build_date_epoch.to_string(),
        )
        .stdin(Stdio::null());
    let before = cpu_snapshot(repository.root())?;
    let started = Instant::now();
    let result = buildkit::execute(&mut command, &directory.join("buildkit-events.jsonl"));
    sample.elapsed_millis = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let after = cpu_snapshot(repository.root());
    let (status, phases) = result?;
    sample.phases = phases;
    sample.compiled_packages = compiled_packages(&fs::read_to_string(
        directory.join("buildkit-events.jsonl"),
    )?)?;
    let delta: CpuSnapshot = after?
        .checked_delta(before)
        .context("builder CPU counters reset during benchmark")?;
    sample.cpu_usage_micros = delta.usage_us;
    sample.cpu_periods = delta.periods;
    sample.throttled_periods = delta.throttled_periods;
    sample.throttled_micros = delta.throttled_us;
    ensure!(
        status.success(),
        "benchmark BuildKit solve failed: {status}"
    );
    ensure!(
        sample.warmup || !sample.compiled_packages.is_empty(),
        "source-edit sample did not compile any Cargo package"
    );
    for binary in &prepared.plan.families[0].binaries {
        let bytes = fs::read(directory.join("artifacts/bin").join(binary))?;
        sample.binary_sha256.insert(
            binary.clone(),
            format!("sha256:{}", hex::encode(Sha256::digest(bytes))),
        );
    }
    sample.success = true;
    Ok(())
}

pub(super) fn validate_source_path(path: &Path) -> Result<()> {
    ensure!(
        path.components()
            .all(|part| matches!(part, Component::Normal(_)))
            && path.extension().is_some_and(|extension| extension == "rs"),
        "benchmark source must be a repository-relative Rust file: {}",
        path.display()
    );
    Ok(())
}

pub(super) fn varied_source(original: &[u8], identity: &str) -> Vec<u8> {
    let mut result = original.to_vec();
    // Keep source span offsets constant across quota labels and sample numbers.
    let identity = hex::encode(Sha256::digest(identity.as_bytes()));
    result.extend_from_slice(
        format!("\n// Veoveo compiler benchmark input: {identity}\n").as_bytes(),
    );
    result
}

#[derive(Deserialize)]
struct LogEvent {
    #[serde(default)]
    logs: Vec<LogData>,
}

#[derive(Deserialize)]
struct LogData {
    data: String,
}

pub(super) fn compiled_packages(trace: &str) -> Result<BTreeSet<String>> {
    let mut bytes = Vec::new();
    for line in trace.lines() {
        let Ok(event) = serde_json::from_str::<LogEvent>(line) else {
            continue;
        };
        for log in event.logs {
            bytes.extend(STANDARD.decode(log.data)?);
        }
    }
    Ok(String::from_utf8(bytes)?
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("Compiling ")?
                .split_whitespace()
                .next()
                .map(str::to_owned)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_changes_do_not_accumulate_or_rewrite_source_lines() {
        let original = b"fn main() {}\n";
        let first = varied_source(original, "first");
        let second = varied_source(original, "second");
        assert!(first.starts_with(original));
        assert!(second.starts_with(original));
        assert_ne!(first, second);
        assert_eq!(first.len(), second.len());
        assert!(!String::from_utf8(second).unwrap().contains("first"));
    }

    #[test]
    fn fixture_paths_cannot_escape_the_compiler_context() {
        assert!(validate_source_path(Path::new("servers/example/src/main.rs")).is_ok());
        for path in [
            "/tmp/main.rs",
            "../main.rs",
            "servers/../main.rs",
            "Cargo.toml",
            "",
        ] {
            assert!(validate_source_path(Path::new(path)).is_err(), "{path}");
        }
    }

    #[test]
    fn compiler_observation_handles_fragmented_logs_and_rejects_freshness_only() {
        let trace = [
            "   Compi",
            "ling example v1.0.0 (/src/example)\n    Finished release\n",
        ]
        .map(|part| serde_json::json!({"logs": [{"data": STANDARD.encode(part)}]}).to_string())
        .join("\n");
        assert_eq!(
            compiled_packages(&trace).unwrap(),
            BTreeSet::from(["example".to_owned()])
        );
        let trace =
            serde_json::json!({"logs": [{"data": STANDARD.encode("Finished release in 1s\n")}]});
        assert!(compiled_packages(&trace.to_string()).unwrap().is_empty());
    }

    #[test]
    #[ignore = "uses the managed BuildKit worker to verify quota restoration after a rejected measurement"]
    fn rejected_measurement_restores_the_declared_worker_quota() {
        let repository = RepositoryContext::discover(&std::env::current_dir().unwrap()).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("comparison");
        let args = BuilderBenchmarkArgs {
            target: vec!["console-bff".to_owned(), "mcp-gateway".to_owned()],
            // Cargo needs this workspace entrypoint for discovery, but neither
            // selected binary depends on xtask. Its edit must compile no package.
            source: vec![PathBuf::from("tools/xtask/src/main.rs")],
            cpus: vec![4, 12],
            output: output.clone(),
        };
        let error = run(&repository, &args).unwrap_err();
        assert!(format!("{error:#}").contains("did not compile any Cargo package"));
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RestorationEvidence {
            restored_declared_quota: bool,
        }
        let evidence: RestorationEvidence =
            serde_json::from_slice(&fs::read(output.join("comparison.json")).unwrap()).unwrap();
        assert!(evidence.restored_declared_quota);
        builder::status(&repository).unwrap();
    }
}
