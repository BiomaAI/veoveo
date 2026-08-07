use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{
    ImageSelectionArgs, PlanFormat,
    commands::{builder, source},
    context::RepositoryContext,
    process,
};

mod affected;
mod buildkit;

const PLAN_SCHEMA: &str = "veoveo.io/image-build-plan/v2";
const RUN_SCHEMA: &str = "veoveo.io/image-build-run/v2";
const MODE_LABEL: &str = "io.veoveo.build.mode";
const PACKAGE_LABEL: &str = "io.veoveo.build.package";
const BINARIES_LABEL: &str = "io.veoveo.build.binaries";
const FAMILY_LABEL: &str = "io.veoveo.build.family";
const AUXILIARY_LABEL: &str = "io.veoveo.build.auxiliary";
// SOURCE_DATE_EPOCH is a predefined BuildKit argument and therefore part of
// every stage's cache key. Keep it stable across source revisions. Bump this
// cache ABI only when an admitted pinned parent image contains newer metadata.
const REPRODUCIBLE_BUILD_EPOCH: u64 = 1_786_076_699;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Selection {
    pub(crate) kind: SelectionKind,
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) targets: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SelectionKind {
    Target,
    Group,
    Exact,
}

impl Selection {
    pub(crate) fn from_args(args: &ImageSelectionArgs) -> Result<Self> {
        match (&args.target, &args.group) {
            (Some(target), None) => Self::target(target),
            (None, Some(group)) => Self::group(group),
            _ => bail!("select exactly one --target or --group"),
        }
    }

    pub(crate) fn target(target: &str) -> Result<Self> {
        validate_identifier("Bake target", target)?;
        Ok(Self {
            kind: SelectionKind::Target,
            name: target.to_owned(),
            targets: Vec::new(),
        })
    }

    pub(crate) fn group(group: &str) -> Result<Self> {
        validate_identifier("Bake group", group)?;
        Ok(Self {
            kind: SelectionKind::Group,
            name: group.to_owned(),
            targets: Vec::new(),
        })
    }

    pub(crate) fn exact(name: &str, targets: impl IntoIterator<Item = String>) -> Result<Self> {
        validate_identifier("exact Bake selection", name)?;
        let targets = targets.into_iter().collect::<BTreeSet<_>>();
        ensure!(!targets.is_empty(), "exact Bake selection cannot be empty");
        for target in &targets {
            validate_identifier("Bake target", target)?;
        }
        Ok(Self {
            kind: SelectionKind::Exact,
            name: name.to_owned(),
            targets: targets.into_iter().collect(),
        })
    }

    fn bake_patterns(&self) -> Vec<&str> {
        match self.kind {
            SelectionKind::Target | SelectionKind::Group => vec![self.name.as_str()],
            SelectionKind::Exact => self.targets.iter().map(String::as_str).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BuildPlanV1 {
    schema_version: &'static str,
    selection: Selection,
    source: SourceRevision,
    source_date_epoch: u64,
    build_date_epoch: u64,
    planning: PlanningTimings,
    source_revision_targets: Vec<String>,
    targets: Vec<ImageTarget>,
    families: Vec<FamilyPlan>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanningTimings {
    graph_resolution_millis: u64,
    cargo_metadata_millis: u64,
    source_identity_millis: u64,
    validation_millis: u64,
    total_millis: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceRevision {
    revision: String,
    dirty: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageTarget {
    name: String,
    tags: Vec<String>,
    platform: String,
    rust: Option<RustBuildUnit>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RustBuildUnit {
    mode: BuildMode,
    package: String,
    binaries: Vec<String>,
    family: BuilderFamily,
    auxiliary: Vec<AuxiliaryArtifact>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
enum BuildMode {
    RustShared,
    RustStandalone,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
enum BuilderFamily {
    RustTrixieV1,
    RustBookwormV1,
    RustDeepstreamV1,
    RustVllmV1,
    RustSumoBullseyeV1,
}

impl BuilderFamily {
    fn name(self) -> &'static str {
        match self {
            Self::RustTrixieV1 => "rust-trixie-v1",
            Self::RustBookwormV1 => "rust-bookworm-v1",
            Self::RustDeepstreamV1 => "rust-deepstream-v1",
            Self::RustVllmV1 => "rust-vllm-v1",
            Self::RustSumoBullseyeV1 => "rust-sumo-bullseye-v1",
        }
    }

    fn shared_artifact_target(self) -> Option<&'static str> {
        match self {
            Self::RustTrixieV1 => Some("rust-trixie-artifacts"),
            Self::RustBookwormV1 => Some("rust-bookworm-artifacts"),
            _ => None,
        }
    }

    fn cargo_cache_id(self) -> String {
        format!("veoveo-cargo-{}", self.name())
    }

    /// Explicit compatibility epoch for Cargo target artifacts.
    ///
    /// Bump this only when the builder ABI, toolchain, target, or Cargo
    /// profile becomes incompatible. Cargo owns source and flag freshness
    /// inside the namespace, so ordinary Dockerfile edits do not invalidate it.
    fn target_cache_epoch(self) -> &'static str {
        match self {
            Self::RustTrixieV1 => "9b79bf6f1617",
            Self::RustBookwormV1 => "d793280d4d65",
            Self::RustDeepstreamV1 => "33878ce751d7",
            Self::RustVllmV1 => "b6332ab4fe25",
            Self::RustSumoBullseyeV1 => "79132306a5b6",
        }
    }

    fn target_cache_id(self, source_hash: &str) -> String {
        format!(
            "veoveo-target-v1-{}-{}-linux-amd64-release",
            &source_hash[..12],
            self.target_cache_epoch()
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
enum AuxiliaryArtifact {
    Libduckdb,
    DuckdbSpatial,
}

impl AuxiliaryArtifact {
    fn name(self) -> &'static str {
        match self {
            Self::Libduckdb => "libduckdb",
            Self::DuckdbSpatial => "duckdb-spatial",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FamilyPlan {
    family: BuilderFamily,
    packages: Vec<String>,
    binaries: Vec<String>,
    auxiliary: Vec<AuxiliaryArtifact>,
    cargo_cache_id: String,
    target_cache_epoch: &'static str,
    target_cache_id: String,
}

pub(crate) struct PreparedPlan {
    pub(crate) plan: BuildPlanV1,
    override_file: NamedTempFile,
}

#[derive(Clone, Debug)]
pub(crate) struct ImageOutput {
    pub(crate) target: String,
    pub(crate) reference: String,
    pub(crate) platform: String,
}

impl PreparedPlan {
    pub(crate) fn image_references(&self) -> Vec<(String, String)> {
        self.image_outputs()
            .into_iter()
            .map(|output| (output.target, output.reference))
            .collect()
    }

    pub(crate) fn image_outputs(&self) -> Vec<ImageOutput> {
        self.plan
            .targets
            .iter()
            .flat_map(|target| {
                target.tags.iter().cloned().map(|tag| ImageOutput {
                    target: target.name.clone(),
                    reference: tag,
                    platform: target.platform.clone(),
                })
            })
            .collect()
    }
}

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
enum BuildRunResult {
    Succeeded,
    Failed,
}

#[derive(Debug, Deserialize)]
struct BakeDefinition {
    #[serde(default)]
    group: BTreeMap<String, BakeGroup>,
    target: BTreeMap<String, BakeTarget>,
}

#[derive(Debug, Deserialize)]
struct BakeGroup {
    targets: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct BakeTarget {
    #[serde(default)]
    args: BTreeMap<String, String>,
    #[serde(default)]
    context: String,
    #[serde(default)]
    contexts: BTreeMap<String, String>,
    #[serde(default)]
    dockerfile: String,
    #[serde(default)]
    labels: BTreeMap<String, String>,
    #[serde(default)]
    platforms: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: String,
    targets: Vec<CargoTarget>,
}

#[derive(Debug, Deserialize)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BakeOverride {
    target: BTreeMap<String, BakeTargetOverride>,
}

#[derive(Debug, Serialize)]
struct BakeTargetOverride {
    args: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct BakeAttestationOverride {
    target: BTreeMap<String, BakeTargetAttestation>,
}

#[derive(Debug, Serialize)]
struct BakeTargetAttestation {
    attest: [&'static str; 2],
}

pub(crate) fn plan_command(
    repository: &RepositoryContext,
    args: &ImageSelectionArgs,
    format: PlanFormat,
) -> Result<()> {
    let selection = Selection::from_args(args)?;
    let prepared = prepare(repository, selection, &BTreeMap::new())?;
    match format {
        PlanFormat::Human => print_human(&prepared.plan),
        PlanFormat::Json => {
            serde_json::to_writer_pretty(std::io::stdout(), &prepared.plan)?;
            println!();
        }
    }
    Ok(())
}

pub(crate) fn affected_command(
    repository: &RepositoryContext,
    since: &str,
    format: PlanFormat,
) -> Result<()> {
    affected::command(repository, since, format)
}

pub(crate) fn build_command(
    repository: &RepositoryContext,
    args: &ImageSelectionArgs,
) -> Result<()> {
    let selection = Selection::from_args(args)?;
    let _builder = builder::ensure(repository)?;
    let prepared = prepare(repository, selection, &BTreeMap::new())?;
    let evidence = evidence_run(repository, &prepared.plan, "build")?;
    execute(
        repository,
        repository,
        &prepared,
        &BTreeMap::new(),
        OutputMode::Load,
        &evidence,
    )?;
    println!("Build evidence: {}", evidence.directory().display());
    Ok(())
}

pub(crate) fn prepare(
    repository: &RepositoryContext,
    selection: Selection,
    environment: &BTreeMap<String, String>,
) -> Result<PreparedPlan> {
    prepare_with_builder(repository, repository, selection, environment)
}

pub(crate) fn prepare_with_builder(
    builder_repository: &RepositoryContext,
    source_repository: &RepositoryContext,
    selection: Selection,
    environment: &BTreeMap<String, String>,
) -> Result<PreparedPlan> {
    let total_started = Instant::now();
    let mut planning = PlanningTimings::default();
    let graph_started = Instant::now();
    let checked = bake_print(
        builder_repository,
        source_repository.root(),
        &selection,
        environment,
        &[],
    )?;
    planning.graph_resolution_millis = elapsed_millis(graph_started);
    let validation_started = Instant::now();
    let direct_targets = selected_targets(&checked, &selection)?;
    let source_revision_targets = target_dependency_closure(&checked, &direct_targets)?;
    let needs_cargo_metadata = direct_targets.iter().try_fold(false, |needed, name| {
        let target = checked
            .target
            .get(name)
            .with_context(|| format!("Bake selection references missing target {name}"))?;
        Ok::<_, anyhow::Error>(needed | rust_labels_present(name, target)?)
    })?;
    planning.validation_millis = elapsed_millis(validation_started);
    let metadata_started = Instant::now();
    let metadata = needs_cargo_metadata
        .then(|| cargo_metadata(source_repository.root()))
        .transpose()?;
    planning.cargo_metadata_millis = elapsed_millis(metadata_started);
    let identity_started = Instant::now();
    let source_hash = source::source_hash(source_repository)?;
    let source_date_epoch = git_output(
        source_repository.root(),
        ["show", "-s", "--format=%ct", "HEAD"],
    )?
    .trim()
    .parse::<u64>()
    .context("source commit timestamp is not a positive Unix epoch")?;
    ensure!(
        source_date_epoch > 0,
        "source commit timestamp must be after the Unix epoch"
    );
    let source_revision = SourceRevision {
        revision: git_output(source_repository.root(), ["rev-parse", "HEAD"])?
            .trim()
            .to_owned(),
        dirty: !git_output(
            source_repository.root(),
            ["status", "--porcelain=v1", "--untracked-files=all"],
        )?
        .trim()
        .is_empty(),
    };
    planning.source_identity_millis = elapsed_millis(identity_started);
    let validation_started = Instant::now();
    let package_index = metadata
        .as_ref()
        .map(|metadata| {
            metadata
                .packages
                .iter()
                .map(|package| (package.name.as_str(), package))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    let mut targets = Vec::new();
    let mut family_units = BTreeMap::<BuilderFamily, Vec<(String, RustBuildUnit)>>::new();
    for name in direct_targets {
        let target = checked
            .target
            .get(&name)
            .with_context(|| format!("Bake selection references missing target {name}"))?;
        ensure!(
            target.platforms == ["linux/amd64"],
            "target {name} must resolve exactly platform linux/amd64"
        );
        ensure!(
            !target.tags.is_empty(),
            "target {name} produces no image tag"
        );
        let rust = parse_rust_unit(&name, target, &package_index)?;
        if let Some(unit) = &rust {
            if unit.mode == BuildMode::RustStandalone {
                validate_standalone_source_boundary(source_repository.root(), &name, target)?;
            }
            family_units
                .entry(unit.family)
                .or_default()
                .push((name.clone(), unit.clone()));
        }
        targets.push(ImageTarget {
            name,
            tags: target.tags.clone(),
            platform: "linux/amd64".to_owned(),
            rust,
        });
    }

    let all_targets = if family_units
        .keys()
        .any(|family| family.shared_artifact_target().is_some())
    {
        let started = Instant::now();
        let result = Some(bake_print_all(
            builder_repository,
            source_repository.root(),
            environment,
        )?);
        planning.graph_resolution_millis = planning
            .graph_resolution_millis
            .saturating_add(elapsed_millis(started));
        result
    } else {
        None
    };
    let mut families = Vec::new();
    for (family, selected_units) in &family_units {
        validate_family_modes(*family, selected_units)?;
        let units = if family.shared_artifact_target().is_some() {
            let canonical_definition = all_targets
                .as_ref()
                .expect("the complete Bake graph was loaded for a shared family");
            let mut canonical_units = Vec::new();
            for (name, target) in &canonical_definition.target {
                if let Some(unit) = parse_rust_unit(name, target, &package_index)?
                    && unit.family == *family
                {
                    canonical_units.push((name.clone(), unit));
                }
            }
            let canonical_names = canonical_units
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<BTreeSet<_>>();
            for (name, _) in selected_units {
                ensure!(
                    canonical_names.contains(name.as_str()),
                    "Rust target {name} is missing from the complete Bake family catalog"
                );
            }
            validate_family_modes(*family, &canonical_units)?;
            canonical_units
        } else {
            selected_units.clone()
        };
        let mut packages = BTreeSet::new();
        let mut binaries = BTreeSet::new();
        let mut auxiliary = BTreeSet::new();
        for (_, unit) in &units {
            packages.insert(unit.package.clone());
            binaries.extend(unit.binaries.iter().cloned());
            auxiliary.extend(unit.auxiliary.iter().copied());
        }
        families.push(FamilyPlan {
            family: *family,
            packages: packages.into_iter().collect(),
            binaries: binaries.into_iter().collect(),
            auxiliary: auxiliary.into_iter().collect(),
            cargo_cache_id: family.cargo_cache_id(),
            target_cache_epoch: family.target_cache_epoch(),
            target_cache_id: family.target_cache_id(&source_hash),
        });
    }

    planning.validation_millis = planning
        .validation_millis
        .saturating_add(elapsed_millis(validation_started));
    let mut plan = BuildPlanV1 {
        schema_version: PLAN_SCHEMA,
        selection,
        source: source_revision,
        source_date_epoch,
        build_date_epoch: REPRODUCIBLE_BUILD_EPOCH,
        planning,
        source_revision_targets,
        targets,
        families,
    };
    let override_definition = make_override(&plan)?;
    let mut override_file = NamedTempFile::new().context("creating Bake override")?;
    serde_json::to_writer_pretty(&mut override_file, &override_definition)?;
    override_file.flush()?;

    let graph_started = Instant::now();
    let resolved = bake_print(
        builder_repository,
        source_repository.root(),
        &plan.selection,
        environment,
        &[override_file.path()],
    )?;
    plan.planning.graph_resolution_millis = plan
        .planning
        .graph_resolution_millis
        .saturating_add(elapsed_millis(graph_started));
    let validation_started = Instant::now();
    verify_override(&plan, &resolved)?;
    plan.planning.validation_millis = plan
        .planning
        .validation_millis
        .saturating_add(elapsed_millis(validation_started));
    plan.planning.total_millis = elapsed_millis(total_started);
    Ok(PreparedPlan {
        plan,
        override_file,
    })
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum OutputMode {
    Load,
    Staged,
    Qualified,
}

impl OutputMode {
    fn exporter(self) -> &'static str {
        match self {
            // Local images are disposable developer artifacts. Rewriting every
            // inherited layer makes a tiny Isaac overlay re-export the full
            // multi-gigabyte base without strengthening release evidence.
            Self::Load => "type=docker",
            // The source commit timestamp is later than every admitted pinned
            // parent image. BuildKit therefore retains inherited layer blobs and
            // normalizes only newer entries produced by this source build.
            Self::Staged | Self::Qualified => "type=registry,rewrite-timestamp=true",
        }
    }
}

pub(crate) fn execute(
    builder_repository: &RepositoryContext,
    source_repository: &RepositoryContext,
    prepared: &PreparedPlan,
    environment: &BTreeMap<String, String>,
    mode: OutputMode,
    evidence: &EvidenceRun,
) -> Result<()> {
    let bake = source_repository.root().join("docker-bake.hcl");
    let mut attestation_override = match mode {
        OutputMode::Load | OutputMode::Staged => None,
        OutputMode::Qualified => {
            let definition = BakeAttestationOverride {
                target: prepared
                    .plan
                    .targets
                    .iter()
                    .map(|target| {
                        (
                            target.name.clone(),
                            BakeTargetAttestation {
                                attest: ["type=provenance,mode=max", "type=sbom"],
                            },
                        )
                    })
                    .collect(),
            };
            let mut file = NamedTempFile::new().context("creating release attestation override")?;
            serde_json::to_writer_pretty(&mut file, &definition)?;
            file.flush()?;
            Some(file)
        }
    };
    let mut command = builder::buildx_command(builder_repository)?;
    command
        .current_dir(source_repository.root())
        .args(["bake", "--builder", builder::BUILDER_NAME, "-f"])
        .arg(&bake)
        .arg("-f")
        .arg(prepared.override_file.path());
    if let Some(override_file) = attestation_override.as_mut() {
        command.arg("-f").arg(override_file.path());
    }
    if matches!(mode, OutputMode::Staged) {
        command.args(["--provenance=false", "--sbom=false"]);
    }
    command
        .args(prepared.plan.selection.bake_patterns())
        .arg("--metadata-file")
        .arg(evidence.metadata_path())
        .env(
            "SOURCE_DATE_EPOCH",
            prepared.plan.build_date_epoch.to_string(),
        )
        .envs(environment)
        .stdin(Stdio::null());
    for target in &prepared.plan.targets {
        command
            .arg("--set")
            .arg(format!("{}.output={}", target.name, mode.exporter()));
    }
    match buildkit::execute(&mut command, evidence.buildkit_trace_path()) {
        Ok((status, phases)) => {
            let error =
                (!status.success()).then(|| format!("Docker Buildx Bake failed with {status}"));
            evidence.finish(
                &prepared.plan,
                mode,
                if status.success() {
                    BuildRunResult::Succeeded
                } else {
                    BuildRunResult::Failed
                },
                status.code(),
                error.as_deref(),
                phases,
            )?;
            ensure!(status.success(), "Docker Buildx Bake failed with {status}");
            Ok(())
        }
        Err(error) => {
            let message = format!("running Docker Buildx Bake: {error}");
            evidence.finish(
                &prepared.plan,
                mode,
                BuildRunResult::Failed,
                None,
                Some(&message),
                buildkit::PhaseTimings::default(),
            )?;
            Err(error).context("running Docker Buildx Bake")
        }
    }
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

    fn metadata_path(&self) -> &Path {
        &self.metadata
    }

    fn buildkit_trace_path(&self) -> &Path {
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

    fn finish(
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

fn parse_publication_index_digests(
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

fn selected_targets(definition: &BakeDefinition, selection: &Selection) -> Result<Vec<String>> {
    match selection.kind {
        SelectionKind::Target => {
            ensure!(
                definition.target.contains_key(&selection.name),
                "unknown Bake target {}",
                selection.name
            );
            Ok(vec![selection.name.clone()])
        }
        SelectionKind::Group => definition
            .group
            .get(&selection.name)
            .map(|group| group.targets.clone())
            .with_context(|| format!("unknown Bake group {}", selection.name)),
        SelectionKind::Exact => {
            for target in &selection.targets {
                ensure!(
                    definition.target.contains_key(target),
                    "unknown Bake target {target}"
                );
            }
            Ok(selection.targets.clone())
        }
    }
}

fn target_dependency_closure(
    definition: &BakeDefinition,
    direct_targets: &[String],
) -> Result<Vec<String>> {
    let mut pending = direct_targets.to_vec();
    let mut closure = BTreeSet::new();
    while let Some(name) = pending.pop() {
        if !closure.insert(name.clone()) {
            continue;
        }
        let target = definition
            .target
            .get(&name)
            .with_context(|| format!("Bake target dependency {name} does not exist"))?;
        for dependency in target
            .contexts
            .values()
            .filter_map(|context| context.strip_prefix("target:"))
        {
            ensure!(
                definition.target.contains_key(dependency),
                "Bake target {name} references missing target context {dependency}"
            );
            pending.push(dependency.to_owned());
        }
    }
    Ok(closure.into_iter().collect())
}

fn parse_rust_unit(
    name: &str,
    target: &BakeTarget,
    packages: &BTreeMap<&str, &CargoPackage>,
) -> Result<Option<RustBuildUnit>> {
    if !rust_labels_present(name, target)? {
        return Ok(None);
    }
    let mode = match target.labels[MODE_LABEL].as_str() {
        "rust-shared" => BuildMode::RustShared,
        "rust-standalone" => BuildMode::RustStandalone,
        value => bail!("target {name} has unknown Rust build mode {value}"),
    };
    let package = target.labels[PACKAGE_LABEL].clone();
    let binaries = parse_collection(&target.labels[BINARIES_LABEL], false)?;
    let family = parse_family(&target.labels[FAMILY_LABEL])
        .with_context(|| format!("target {name} has invalid builder family"))?;
    let auxiliary = parse_collection(&target.labels[AUXILIARY_LABEL], true)?
        .into_iter()
        .map(|value| match value.as_str() {
            "libduckdb" => Ok(AuxiliaryArtifact::Libduckdb),
            "duckdb-spatial" => Ok(AuxiliaryArtifact::DuckdbSpatial),
            _ => bail!("target {name} has unknown auxiliary artifact {value}"),
        })
        .collect::<Result<Vec<_>>>()?;
    let cargo_package = packages
        .get(package.as_str())
        .with_context(|| format!("target {name} references unknown Cargo package {package}"))?;
    let cargo_bins = cargo_package
        .targets
        .iter()
        .filter(|target| target.kind.iter().any(|kind| kind == "bin"))
        .map(|target| target.name.as_str())
        .collect::<BTreeSet<_>>();
    for binary in &binaries {
        ensure!(
            cargo_bins.contains(binary.as_str()),
            "target {name} references missing binary {binary} in package {package}"
        );
    }
    Ok(Some(RustBuildUnit {
        mode,
        package,
        binaries,
        family,
        auxiliary,
    }))
}

fn rust_labels_present(name: &str, target: &BakeTarget) -> Result<bool> {
    let labels = [
        MODE_LABEL,
        PACKAGE_LABEL,
        BINARIES_LABEL,
        FAMILY_LABEL,
        AUXILIARY_LABEL,
    ];
    let present = labels
        .iter()
        .filter(|label| target.labels.contains_key(**label))
        .count();
    if present == 0 {
        return Ok(false);
    }
    ensure!(
        present == labels.len(),
        "Rust image target {name} must declare all canonical build labels"
    );
    Ok(true)
}

fn parse_family(value: &str) -> Result<BuilderFamily> {
    match value {
        "rust-trixie-v1" => Ok(BuilderFamily::RustTrixieV1),
        "rust-bookworm-v1" => Ok(BuilderFamily::RustBookwormV1),
        "rust-deepstream-v1" => Ok(BuilderFamily::RustDeepstreamV1),
        "rust-vllm-v1" => Ok(BuilderFamily::RustVllmV1),
        "rust-sumo-bullseye-v1" => Ok(BuilderFamily::RustSumoBullseyeV1),
        _ => bail!("unknown builder family {value}"),
    }
}

fn parse_collection(value: &str, allow_empty: bool) -> Result<Vec<String>> {
    if value.is_empty() && allow_empty {
        return Ok(Vec::new());
    }
    let values = value.split(',').map(str::to_owned).collect::<Vec<_>>();
    ensure!(!values.is_empty(), "collection cannot be empty");
    for value in &values {
        validate_identifier("collection item", value)?;
    }
    let unique = values.iter().collect::<BTreeSet<_>>();
    ensure!(
        unique.len() == values.len(),
        "collection contains duplicates"
    );
    Ok(values)
}

fn validate_family_modes(family: BuilderFamily, units: &[(String, RustBuildUnit)]) -> Result<()> {
    let expected = if family.shared_artifact_target().is_some() {
        BuildMode::RustShared
    } else {
        BuildMode::RustStandalone
    };
    for (target, unit) in units {
        ensure!(
            unit.mode == expected,
            "target {target} uses mode {:?}, incompatible with family {}",
            unit.mode,
            family.name()
        );
    }
    if expected == BuildMode::RustStandalone {
        ensure!(
            units.len() == 1,
            "standalone family {} selected more than one build unit",
            family.name()
        );
    }
    Ok(())
}

fn validate_standalone_source_boundary(
    repository: &Path,
    name: &str,
    target: &BakeTarget,
) -> Result<()> {
    ensure!(
        target.context == ".",
        "standalone Rust target {name} must use the repository root as its build context"
    );
    ensure!(
        !target.dockerfile.is_empty(),
        "standalone Rust target {name} has no Dockerfile"
    );
    let dockerfile_path = repository.join(&target.dockerfile);
    let dockerfile = fs::read_to_string(&dockerfile_path)
        .with_context(|| format!("reading {}", dockerfile_path.display()))?;
    validate_standalone_builder_stage(&dockerfile)
        .with_context(|| format!("standalone Rust target {name} source boundary"))?;
    Ok(())
}

fn validate_standalone_builder_stage(dockerfile: &str) -> Result<()> {
    let mut builder = Vec::new();
    let mut in_builder = false;
    for line in dockerfile.lines() {
        let line = line.trim();
        if line.starts_with("FROM ") {
            if in_builder {
                break;
            }
            in_builder = true;
            continue;
        }
        if in_builder {
            builder.push(line);
        }
    }
    ensure!(!builder.is_empty(), "Dockerfile has no builder stage");
    ensure!(
        builder
            .iter()
            .any(|line| line.contains("--mount=type=bind,source=.,target=/src,readonly")),
        "builder must read the complete workspace through the canonical read-only source mount"
    );
    ensure!(
        !builder.iter().any(|line| line.starts_with("COPY ")),
        "builder must not maintain a second handwritten workspace COPY list"
    );
    Ok(())
}

fn make_override(plan: &BuildPlanV1) -> Result<BakeOverride> {
    let mut target = plan
        .source_revision_targets
        .iter()
        .map(|name| {
            (
                name.clone(),
                BakeTargetOverride {
                    args: BTreeMap::from([(
                        "SOURCE_REVISION".to_owned(),
                        plan.source.revision.clone(),
                    )]),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    for family in &plan.families {
        let args = if let Some(artifact) = family.family.shared_artifact_target() {
            let args = BTreeMap::from([
                (
                    "VEOVEO_CARGO_PACKAGES".to_owned(),
                    family.packages.join(","),
                ),
                (
                    "VEOVEO_CARGO_BINARIES".to_owned(),
                    family.binaries.join(","),
                ),
                (
                    "VEOVEO_AUXILIARY".to_owned(),
                    family
                        .auxiliary
                        .iter()
                        .filter(|value| **value != AuxiliaryArtifact::DuckdbSpatial)
                        .map(|value| value.name())
                        .collect::<Vec<_>>()
                        .join(","),
                ),
                (
                    "VEOVEO_CARGO_CACHE_ID".to_owned(),
                    family.cargo_cache_id.clone(),
                ),
                (
                    "VEOVEO_TARGET_CACHE_ID".to_owned(),
                    family.target_cache_id.clone(),
                ),
            ]);
            target
                .entry(artifact.to_owned())
                .or_insert_with(|| BakeTargetOverride {
                    args: BTreeMap::new(),
                })
                .args
                .extend(args);
            continue;
        } else {
            BTreeMap::from([
                (
                    "VEOVEO_CARGO_CACHE_ID".to_owned(),
                    family.cargo_cache_id.clone(),
                ),
                (
                    "VEOVEO_TARGET_CACHE_ID".to_owned(),
                    family.target_cache_id.clone(),
                ),
            ])
        };
        let image = plan
            .targets
            .iter()
            .find(|target| {
                target
                    .rust
                    .as_ref()
                    .is_some_and(|unit| unit.family == family.family)
            })
            .context("standalone family has no image target")?;
        target
            .get_mut(&image.name)
            .expect("direct image target was seeded")
            .args
            .extend(args);
    }
    Ok(BakeOverride { target })
}

fn verify_override(plan: &BuildPlanV1, definition: &BakeDefinition) -> Result<()> {
    for name in &plan.source_revision_targets {
        let target = definition
            .target
            .get(name)
            .with_context(|| format!("resolved Bake graph omitted source target {name}"))?;
        ensure!(
            target.args.get("SOURCE_REVISION") == Some(&plan.source.revision),
            "resolved Bake graph changed source revision for {name}"
        );
    }
    for family in &plan.families {
        let target_name = family.family.shared_artifact_target().unwrap_or_else(|| {
            plan.targets
                .iter()
                .find(|target| {
                    target
                        .rust
                        .as_ref()
                        .is_some_and(|unit| unit.family == family.family)
                })
                .map_or("", |target| target.name.as_str())
        });
        let target = definition
            .target
            .get(target_name)
            .with_context(|| format!("resolved Bake graph omitted target {target_name}"))?;
        ensure!(
            target.args.get("VEOVEO_TARGET_CACHE_ID") == Some(&family.target_cache_id),
            "resolved Bake graph changed cache identity for {}",
            family.family.name()
        );
        ensure!(
            target.args.get("VEOVEO_CARGO_CACHE_ID") == Some(&family.cargo_cache_id),
            "resolved Bake graph changed Cargo download cache identity for {}",
            family.family.name()
        );
        if family.family.shared_artifact_target().is_some() {
            ensure!(
                target.args.get("VEOVEO_CARGO_PACKAGES") == Some(&family.packages.join(",")),
                "resolved Bake graph changed package membership for {}",
                family.family.name()
            );
            ensure!(
                target.args.get("VEOVEO_CARGO_BINARIES") == Some(&family.binaries.join(",")),
                "resolved Bake graph changed binary membership for {}",
                family.family.name()
            );
        }
    }
    Ok(())
}

fn bake_print(
    builder_repository: &RepositoryContext,
    source_repository: &Path,
    selection: &Selection,
    environment: &BTreeMap<String, String>,
    extra_files: &[&Path],
) -> Result<BakeDefinition> {
    bake_print_patterns(
        builder_repository,
        source_repository,
        &selection.bake_patterns(),
        environment,
        extra_files,
    )
}

fn bake_print_all(
    builder_repository: &RepositoryContext,
    source_repository: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<BakeDefinition> {
    bake_print_patterns(
        builder_repository,
        source_repository,
        &["*"],
        environment,
        &[],
    )
}

fn bake_print_patterns(
    builder_repository: &RepositoryContext,
    source_repository: &Path,
    patterns: &[&str],
    environment: &BTreeMap<String, String>,
    extra_files: &[&Path],
) -> Result<BakeDefinition> {
    let mut command = builder::buildx_command(builder_repository)?;
    command
        .current_dir(source_repository)
        .args(["bake", "--builder", builder::BUILDER_NAME, "-f"])
        .arg(source_repository.join("docker-bake.hcl"));
    for file in extra_files {
        command.arg("-f").arg(file);
    }
    let output = command
        .args(patterns)
        .arg("--print")
        .envs(environment)
        .output()
        .context("resolving the Docker Bake graph")?;
    ensure!(
        output.status.success(),
        "Docker Bake graph resolution failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).context("decoding resolved Docker Bake graph")
}

fn cargo_metadata(repository: &Path) -> Result<CargoMetadata> {
    let output = process::output(
        "cargo",
        ["metadata", "--no-deps", "--format-version", "1", "--locked"],
        Some(repository),
    )?;
    serde_json::from_slice(&output.stdout).context("decoding Cargo metadata")
}

fn git_output<const N: usize>(repository: &Path, args: [&str; N]) -> Result<String> {
    process::output_text("git", args, Some(repository))
}

fn print_human(plan: &BuildPlanV1) {
    println!(
        "Image plan {} {} at {}{}",
        match plan.selection.kind {
            SelectionKind::Target => "target",
            SelectionKind::Group => "group",
            SelectionKind::Exact => "exact",
        },
        plan.selection.name,
        plan.source.revision,
        if plan.source.dirty { " (dirty)" } else { "" }
    );
    for family in &plan.families {
        println!(
            "{}: {} packages, {} binaries, Cargo cache {}, target cache {} (epoch {})",
            family.family.name(),
            family.packages.len(),
            family.binaries.len(),
            family.cargo_cache_id,
            family.target_cache_id,
            family.target_cache_epoch
        );
    }
    let non_rust = plan
        .targets
        .iter()
        .filter(|target| target.rust.is_none())
        .count();
    println!(
        "{} image targets ({} non-Rust)",
        plan.targets.len(),
        non_rust
    );
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn validate_identifier(kind: &str, value: &str) -> Result<()> {
    ensure!(!value.is_empty(), "{kind} cannot be empty");
    ensure!(
        value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        }),
        "{kind} contains invalid characters: {value}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AUXILIARY_LABEL, BINARIES_LABEL, BakeDefinition, BakeTarget, BuilderFamily, FAMILY_LABEL,
        MODE_LABEL, PACKAGE_LABEL, REPRODUCIBLE_BUILD_EPOCH, Selection,
        parse_publication_index_digests, rust_labels_present, target_dependency_closure,
        validate_standalone_builder_stage,
    };
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn cargo_download_caches_are_isolated_by_builder_family() {
        let families = [
            BuilderFamily::RustTrixieV1,
            BuilderFamily::RustBookwormV1,
            BuilderFamily::RustDeepstreamV1,
            BuilderFamily::RustVllmV1,
            BuilderFamily::RustSumoBullseyeV1,
        ];
        let identities = families
            .into_iter()
            .map(BuilderFamily::cargo_cache_id)
            .collect::<BTreeSet<_>>();

        assert_eq!(identities.len(), families.len());
        assert!(identities.contains("veoveo-cargo-rust-trixie-v1"));
        assert!(identities.contains("veoveo-cargo-rust-bookworm-v1"));
    }

    #[test]
    fn target_cache_epochs_are_explicit_stable_and_unique() {
        let families = [
            BuilderFamily::RustTrixieV1,
            BuilderFamily::RustBookwormV1,
            BuilderFamily::RustDeepstreamV1,
            BuilderFamily::RustVllmV1,
            BuilderFamily::RustSumoBullseyeV1,
        ];
        let source_hash = "a".repeat(64);
        let identities = families
            .into_iter()
            .map(|family| family.target_cache_id(&source_hash))
            .collect::<BTreeSet<_>>();

        assert_eq!(identities.len(), families.len());
        assert!(
            identities.contains("veoveo-target-v1-aaaaaaaaaaaa-9b79bf6f1617-linux-amd64-release")
        );
    }

    #[test]
    fn image_build_epoch_is_stable_across_source_revisions() {
        assert_eq!(REPRODUCIBLE_BUILD_EPOCH, 1_786_076_699);
    }

    #[test]
    fn exact_selection_is_one_sorted_multi_target_bake_invocation() {
        let selection = Selection::exact(
            "platform",
            [
                "optimization-mcp".to_owned(),
                "mcp-gateway".to_owned(),
                "optimization-mcp".to_owned(),
            ],
        )
        .unwrap();

        assert_eq!(
            selection.bake_patterns(),
            ["mcp-gateway", "optimization-mcp"]
        );
    }

    #[test]
    fn cargo_metadata_is_required_only_for_completely_rust_labelled_targets() {
        let docker_only = BakeTarget::default();
        assert!(!rust_labels_present("python-runtime", &docker_only).unwrap());

        let rust = BakeTarget {
            labels: BTreeMap::from([
                (MODE_LABEL.to_owned(), "rust-standalone".to_owned()),
                (PACKAGE_LABEL.to_owned(), "example".to_owned()),
                (BINARIES_LABEL.to_owned(), "example".to_owned()),
                (FAMILY_LABEL.to_owned(), "rust-trixie-v1".to_owned()),
                (AUXILIARY_LABEL.to_owned(), String::new()),
            ]),
            ..BakeTarget::default()
        };
        assert!(rust_labels_present("rust-runtime", &rust).unwrap());

        let partial = BakeTarget {
            labels: BTreeMap::from([(MODE_LABEL.to_owned(), "rust-standalone".to_owned())]),
            ..BakeTarget::default()
        };
        assert!(
            rust_labels_present("invalid-runtime", &partial)
                .unwrap_err()
                .to_string()
                .contains("must declare all canonical build labels")
        );
    }

    #[test]
    fn source_revision_reaches_transitive_target_contexts() {
        let definition = BakeDefinition {
            group: BTreeMap::new(),
            target: BTreeMap::from([
                (
                    "overlay".to_owned(),
                    BakeTarget {
                        contexts: BTreeMap::from([(
                            "runtime".to_owned(),
                            "target:runtime".to_owned(),
                        )]),
                        ..BakeTarget::default()
                    },
                ),
                (
                    "runtime".to_owned(),
                    BakeTarget {
                        contexts: BTreeMap::from([(
                            "artifacts".to_owned(),
                            "target:artifacts".to_owned(),
                        )]),
                        ..BakeTarget::default()
                    },
                ),
                ("artifacts".to_owned(), BakeTarget::default()),
            ]),
        };

        assert_eq!(
            target_dependency_closure(&definition, &["overlay".to_owned()]).unwrap(),
            ["artifacts", "overlay", "runtime"]
        );
    }

    #[test]
    fn reads_publication_index_digest_from_buildx_metadata() {
        let expected = BTreeMap::from([(
            "example".to_owned(),
            "registry.internal/veoveo/example:revision".to_owned(),
        )]);
        let metadata = br#"{
          "example": {
            "containerimage.digest": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "image.name": "registry.internal/veoveo/example:revision"
          }
        }"#;

        let digests = parse_publication_index_digests(metadata, &expected).unwrap();

        assert_eq!(
            digests["example"],
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn rejects_metadata_for_an_unexpected_published_reference() {
        let expected = BTreeMap::from([(
            "example".to_owned(),
            "registry.internal/veoveo/example:expected".to_owned(),
        )]);
        let metadata = br#"{
          "example": {
            "containerimage.digest": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "image.name": "registry.internal/veoveo/example:other"
          }
        }"#;

        let error = parse_publication_index_digests(metadata, &expected).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("does not contain expected image")
        );
    }

    #[test]
    fn standalone_builder_requires_the_complete_workspace_source_mount() {
        validate_standalone_builder_stage(
            "FROM rust:1 AS builder\n\
             WORKDIR /src\n\
             RUN --mount=type=bind,source=.,target=/src,readonly cargo build\n\
             FROM scratch\n\
             COPY --from=builder /out/bin /bin\n",
        )
        .unwrap();

        let error = validate_standalone_builder_stage(
            "FROM rust:1 AS builder\n\
             COPY Cargo.toml Cargo.lock ./\n\
             COPY servers ./servers\n\
             RUN cargo build\n\
             FROM scratch\n",
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("complete workspace through the canonical read-only source mount")
        );
    }
}
