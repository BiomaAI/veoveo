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
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use crate::{
    ImageSelectionArgs, PlanFormat,
    commands::{builder, source},
    context::RepositoryContext,
    process,
};

const PLAN_SCHEMA: &str = "veoveo.io/image-build-plan/v1";
const RUN_SCHEMA: &str = "veoveo.io/image-build-run/v1";
const SOURCE_DATE_EPOCH: &str = "0";
const MODE_LABEL: &str = "io.veoveo.build.mode";
const PACKAGE_LABEL: &str = "io.veoveo.build.package";
const BINARIES_LABEL: &str = "io.veoveo.build.binaries";
const FAMILY_LABEL: &str = "io.veoveo.build.family";
const AUXILIARY_LABEL: &str = "io.veoveo.build.auxiliary";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Selection {
    pub(crate) kind: SelectionKind,
    pub(crate) name: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SelectionKind {
    Target,
    Group,
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
        })
    }

    pub(crate) fn group(group: &str) -> Result<Self> {
        validate_identifier("Bake group", group)?;
        Ok(Self {
            kind: SelectionKind::Group,
            name: group.to_owned(),
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BuildPlanV1 {
    schema_version: &'static str,
    selection: Selection,
    source: SourceRevision,
    source_date_epoch: &'static str,
    targets: Vec<ImageTarget>,
    families: Vec<FamilyPlan>,
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
    RustUavBookwormV1,
    RustDeepstreamV1,
    RustVllmV1,
    RustSumoBullseyeV1,
}

impl BuilderFamily {
    fn name(self) -> &'static str {
        match self {
            Self::RustTrixieV1 => "rust-trixie-v1",
            Self::RustBookwormV1 => "rust-bookworm-v1",
            Self::RustUavBookwormV1 => "rust-uav-bookworm-v1",
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
    target_cache_id: String,
}

pub(crate) struct PreparedPlan {
    pub(crate) plan: BuildPlanV1,
    override_file: NamedTempFile,
}

impl PreparedPlan {
    pub(crate) fn image_references(&self) -> Vec<(String, String)> {
        self.plan
            .targets
            .iter()
            .flat_map(|target| {
                target
                    .tags
                    .iter()
                    .cloned()
                    .map(|tag| (target.name.clone(), tag))
            })
            .collect()
    }
}

pub(crate) struct EvidenceRun {
    operation: String,
    directory: PathBuf,
    metadata: PathBuf,
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
    source_date_epoch: &'static str,
    started_at_unix_millis: u64,
    elapsed_millis: u64,
    result: BuildRunResult,
    exit_code: Option<i32>,
    error: Option<&'a str>,
    plan_file: &'static str,
    buildx_metadata_file: Option<&'static str>,
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

#[derive(Debug, Deserialize)]
struct BakeTarget {
    #[serde(default)]
    args: BTreeMap<String, String>,
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

pub(crate) fn build_command(
    repository: &RepositoryContext,
    args: &ImageSelectionArgs,
) -> Result<()> {
    let selection = Selection::from_args(args)?;
    builder::ensure(repository)?;
    let prepared = prepare(repository, selection, &BTreeMap::new())?;
    let evidence = evidence_run(repository, &prepared.plan, "build")?;
    execute(
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
    let checked = bake_print(repository.root(), &selection, environment, &[])?;
    let direct_targets = selected_targets(&checked, &selection)?;
    let metadata = cargo_metadata(repository.root())?;
    let source_hash = source::source_hash(repository)?;
    let source_revision = SourceRevision {
        revision: git_output(repository.root(), ["rev-parse", "HEAD"])?
            .trim()
            .to_owned(),
        dirty: !git_output(
            repository.root(),
            ["status", "--porcelain=v1", "--untracked-files=all"],
        )?
        .trim()
        .is_empty(),
    };
    let package_index = metadata
        .packages
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect::<BTreeMap<_, _>>();

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

    let mut families = Vec::new();
    for (family, units) in &family_units {
        validate_family_modes(*family, units)?;
        let mut packages = BTreeSet::new();
        let mut binaries = BTreeSet::new();
        let mut auxiliary = BTreeSet::new();
        for (_, unit) in units {
            packages.insert(unit.package.clone());
            binaries.extend(unit.binaries.iter().cloned());
            auxiliary.extend(unit.auxiliary.iter().copied());
        }
        let fingerprint = family_fingerprint(repository.root(), *family, units, &checked)?;
        let target_cache_id = format!(
            "veoveo-target-v1-{}-{}-linux-amd64-release",
            &source_hash[..12],
            &fingerprint[..12]
        );
        families.push(FamilyPlan {
            family: *family,
            packages: packages.into_iter().collect(),
            binaries: binaries.into_iter().collect(),
            auxiliary: auxiliary.into_iter().collect(),
            target_cache_id,
        });
    }

    let plan = BuildPlanV1 {
        schema_version: PLAN_SCHEMA,
        selection,
        source: source_revision,
        source_date_epoch: SOURCE_DATE_EPOCH,
        targets,
        families,
    };
    let override_definition = make_override(&plan)?;
    let mut override_file = NamedTempFile::new().context("creating Bake override")?;
    serde_json::to_writer_pretty(&mut override_file, &override_definition)?;
    override_file.flush()?;

    let resolved = bake_print(
        repository.root(),
        &plan.selection,
        environment,
        &[override_file.path()],
    )?;
    verify_override(&plan, &resolved)?;
    Ok(PreparedPlan {
        plan,
        override_file,
    })
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum OutputMode {
    Load,
    Push,
}

pub(crate) fn execute(
    repository: &RepositoryContext,
    prepared: &PreparedPlan,
    environment: &BTreeMap<String, String>,
    mode: OutputMode,
    evidence: &EvidenceRun,
) -> Result<()> {
    let bake = repository.root().join("docker-bake.hcl");
    let mut command = builder::buildx_command(repository)?;
    command
        .current_dir(repository.root())
        .args(["bake", "--builder", builder::BUILDER_NAME, "-f"])
        .arg(&bake)
        .arg("-f")
        .arg(prepared.override_file.path())
        .arg(&prepared.plan.selection.name)
        .arg("--metadata-file")
        .arg(evidence.metadata_path())
        .env("SOURCE_DATE_EPOCH", SOURCE_DATE_EPOCH)
        .envs(environment)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    for target in &prepared.plan.targets {
        let exporter = match mode {
            OutputMode::Load => "type=docker,rewrite-timestamp=true",
            OutputMode::Push => "type=registry,rewrite-timestamp=true",
        };
        command
            .arg("--set")
            .arg(format!("{}.output={exporter}", target.name));
    }
    match command.status() {
        Ok(status) => {
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

    fn finish(
        &self,
        plan: &BuildPlanV1,
        output_mode: OutputMode,
        result: BuildRunResult,
        exit_code: Option<i32>,
        error: Option<&str>,
    ) -> Result<()> {
        let elapsed_millis = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let run = BuildRunV1 {
            schema_version: RUN_SCHEMA,
            operation: &self.operation,
            output_mode,
            selection: &plan.selection,
            source: &plan.source,
            source_date_epoch: SOURCE_DATE_EPOCH,
            started_at_unix_millis: self.started_at_unix_millis,
            elapsed_millis,
            result,
            exit_code,
            error,
            plan_file: "plan.json",
            buildx_metadata_file: self.metadata.exists().then_some("buildx-metadata.json"),
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
    }
}

fn parse_rust_unit(
    name: &str,
    target: &BakeTarget,
    packages: &BTreeMap<&str, &CargoPackage>,
) -> Result<Option<RustBuildUnit>> {
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
        return Ok(None);
    }
    ensure!(
        present == labels.len(),
        "Rust image target {name} must declare all canonical build labels"
    );
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

fn parse_family(value: &str) -> Result<BuilderFamily> {
    match value {
        "rust-trixie-v1" => Ok(BuilderFamily::RustTrixieV1),
        "rust-bookworm-v1" => Ok(BuilderFamily::RustBookwormV1),
        "rust-uav-bookworm-v1" => Ok(BuilderFamily::RustUavBookwormV1),
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

fn family_fingerprint(
    repository: &Path,
    family: BuilderFamily,
    units: &[(String, RustBuildUnit)],
    definition: &BakeDefinition,
) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(family.name());
    hasher.update(b"\0linux/amd64\0release\0");
    hasher.update(fs::read(repository.join("rust-toolchain.toml"))?);
    hasher.update(fs::read(repository.join("Cargo.toml"))?);
    hasher.update(fs::read(repository.join(".cargo/config.toml"))?);
    if let Some(artifact) = family.shared_artifact_target() {
        let target = &definition.target[artifact];
        hasher.update(fs::read(repository.join(&target.dockerfile))?);
        for (key, value) in &target.args {
            hasher.update(key);
            hasher.update(b"=");
            hasher.update(value);
            hasher.update(b"\0");
        }
    } else {
        for (name, _) in units {
            let target = &definition.target[name];
            hasher.update(name);
            hasher.update(fs::read(repository.join(&target.dockerfile))?);
            for (key, value) in &target.args {
                hasher.update(key);
                hasher.update(b"=");
                hasher.update(value);
                hasher.update(b"\0");
            }
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

fn make_override(plan: &BuildPlanV1) -> Result<BakeOverride> {
    let mut target = BTreeMap::new();
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
                    "VEOVEO_TARGET_CACHE_ID".to_owned(),
                    family.target_cache_id.clone(),
                ),
            ]);
            target.insert(artifact.to_owned(), BakeTargetOverride { args });
            continue;
        } else {
            BTreeMap::from([(
                "VEOVEO_TARGET_CACHE_ID".to_owned(),
                family.target_cache_id.clone(),
            )])
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
        target.insert(image.name.clone(), BakeTargetOverride { args });
    }
    Ok(BakeOverride { target })
}

fn verify_override(plan: &BuildPlanV1, definition: &BakeDefinition) -> Result<()> {
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
    repository: &Path,
    selection: &Selection,
    environment: &BTreeMap<String, String>,
    extra_files: &[&Path],
) -> Result<BakeDefinition> {
    let mut command = builder::buildx_command(&RepositoryContext::discover(repository)?)?;
    command
        .current_dir(repository)
        .args(["bake", "--builder", builder::BUILDER_NAME, "-f"])
        .arg(repository.join("docker-bake.hcl"));
    for file in extra_files {
        command.arg("-f").arg(file);
    }
    let output = command
        .arg(&selection.name)
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
        },
        plan.selection.name,
        plan.source.revision,
        if plan.source.dirty { " (dirty)" } else { "" }
    );
    for family in &plan.families {
        println!(
            "{}: {} packages, {} binaries, cache {}",
            family.family.name(),
            family.packages.len(),
            family.binaries.len(),
            family.target_cache_id
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
