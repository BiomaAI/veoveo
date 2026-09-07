use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::{
    BuildPlanV1, OutputMode, PreparedPlan, Selection, SelectionKind, SourceRevision, buildkit,
    operation, validate_identifier,
};
use crate::context::RepositoryContext;

const RUN_SCHEMA: &str = "veoveo.io/image-build-run/v2";

pub(crate) struct EvidenceRun {
    operation: String,
    directory: PathBuf,
    metadata: PathBuf,
    buildkit_trace: PathBuf,
    record: PathBuf,
    started_at_unix_millis: u64,
    started: Instant,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildRunV1<'a> {
    schema_version: &'static str,
    operation: &'a str,
    output_mode: OutputMode,
    selection: &'a Selection,
    source: &'a SourceRevision,
    source_date_epoch: u64,
    build_date_epoch: u64,
    started_at_unix_millis: u64,
    elapsed_millis: u64,
    result: BuildRunResult,
    exit_code: Option<i32>,
    error: Option<&'a str>,
    plan_file: &'static str,
    buildx_metadata_file: Option<&'static str>,
    buildkit_trace_file: Option<&'static str>,
    phases: buildkit::PhaseTimings,
}

#[derive(Debug, Deserialize)]
struct BuildxTargetMetadata {
    #[serde(rename = "containerimage.digest")]
    digest: Option<String>,
    #[serde(rename = "image.name")]
    image_name: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum BuildRunResult {
    Succeeded,
    Failed,
}

pub(crate) fn evidence_run(
    repository: &RepositoryContext,
    plan: &BuildPlanV1,
    operation: &str,
) -> Result<EvidenceRun> {
    validate_identifier("evidence operation", operation)?;
    let root = repository
        .root()
        .join("target/veoveo-xtask/evidence")
        .join(&plan.source.revision);
    fs::create_dir_all(&root)
        .with_context(|| format!("creating evidence directory {}", root.display()))?;
    let selection_kind = match plan.selection.kind {
        SelectionKind::Target => "target",
        SelectionKind::Group => "group",
        SelectionKind::Exact => "exact",
    };
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?;
    let started_at_unix_millis =
        u64::try_from(started_at.as_millis()).context("system time exceeds u64 milliseconds")?;
    let base_name = format!(
        "{operation}-{selection_kind}-{}-{}-{}",
        plan.selection.name,
        started_at.as_nanos(),
        std::process::id()
    );
    let directory = (0_u16..)
        .find_map(|sequence| {
            let name = if sequence == 0 {
                base_name.clone()
            } else {
                format!("{base_name}-{sequence}")
            };
            let candidate = root.join(name);
            match fs::create_dir(&candidate) {
                Ok(()) => Some(Ok(candidate)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .context("exhausted image evidence run identifiers")?
        .with_context(|| format!("creating image evidence run under {}", root.display()))?;
    let plan_path = directory.join("plan.json");
    let mut plan_file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&plan_path)
        .with_context(|| format!("creating image plan evidence {}", plan_path.display()))?;
    serde_json::to_writer_pretty(&mut plan_file, plan)?;
    plan_file.write_all(b"\n")?;
    operation::solve_evidence(&directory);
    Ok(EvidenceRun {
        operation: operation.to_owned(),
        metadata: directory.join("buildx-metadata.json"),
        buildkit_trace: directory.join("buildkit-events.jsonl"),
        record: directory.join("run.json"),
        directory,
        started_at_unix_millis,
        started: Instant::now(),
    })
}

impl EvidenceRun {
    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(super) fn metadata_path(&self) -> &Path {
        &self.metadata
    }

    pub(super) fn buildkit_trace_path(&self) -> &Path {
        &self.buildkit_trace
    }

    pub(crate) fn publication_index_digests(
        &self,
        prepared: &PreparedPlan,
    ) -> Result<BTreeMap<String, String>> {
        let bytes = fs::read(&self.metadata)
            .with_context(|| format!("reading Buildx metadata {}", self.metadata.display()))?;
        let expected = prepared
            .image_references()
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        parse_publication_index_digests(&bytes, &expected)
    }

    pub(super) fn finish(
        &self,
        plan: &BuildPlanV1,
        output_mode: OutputMode,
        result: BuildRunResult,
        exit_code: Option<i32>,
        error: Option<&str>,
        phases: buildkit::PhaseTimings,
    ) -> Result<()> {
        let elapsed_millis = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let run = BuildRunV1 {
            schema_version: RUN_SCHEMA,
            operation: &self.operation,
            output_mode,
            selection: &plan.selection,
            source: &plan.source,
            source_date_epoch: plan.source_date_epoch,
            build_date_epoch: plan.build_date_epoch,
            started_at_unix_millis: self.started_at_unix_millis,
            elapsed_millis,
            result,
            exit_code,
            error,
            plan_file: "plan.json",
            buildx_metadata_file: self.metadata.exists().then_some("buildx-metadata.json"),
            buildkit_trace_file: self
                .buildkit_trace
                .exists()
                .then_some("buildkit-events.jsonl"),
            phases,
        };
        let mut record = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&self.record)
            .with_context(|| format!("creating image run evidence {}", self.record.display()))?;
        serde_json::to_writer_pretty(&mut record, &run)?;
        record.write_all(b"\n")?;
        Ok(())
    }
}

pub(super) fn parse_publication_index_digests(
    bytes: &[u8],
    expected: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let metadata = serde_json::from_slice::<BTreeMap<String, BuildxTargetMetadata>>(bytes)
        .context("decoding Buildx publication metadata")?;
    expected
        .iter()
        .map(|(target, reference)| {
            let target_metadata = metadata
                .get(target)
                .with_context(|| format!("Buildx metadata omitted image target {target}"))?;
            let published_names = target_metadata
                .image_name
                .as_deref()
                .with_context(|| format!("Buildx did not publish image target {target}"))?;
            ensure!(
                published_names
                    .split(',')
                    .map(str::trim)
                    .any(|name| name == reference),
                "Buildx metadata for target {target} does not contain expected image {reference}"
            );
            let digest = target_metadata.digest.as_deref().with_context(|| {
                format!("Buildx did not report a digest for image target {target}")
            })?;
            ensure!(
                digest.len() == 71
                    && digest.starts_with("sha256:")
                    && digest[7..].bytes().all(|byte| byte.is_ascii_hexdigit()),
                "Buildx reported invalid OCI digest {digest} for image target {target}"
            );
            Ok((target.clone(), digest.to_owned()))
        })
        .collect()
}
