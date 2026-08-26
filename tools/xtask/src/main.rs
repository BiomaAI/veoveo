mod commands;
mod context;
mod process;

use std::{ffi::OsString, path::PathBuf};

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::{
    commands::{builder, doctor, enforce, image, release, release_preflight, smoke, test_report},
    context::RepositoryContext,
};

#[derive(Debug, Parser)]
#[command(name = "cargo xtask", about = "Veoveo repository operations")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Verify repository tools and pinned versions.
    Doctor,
    /// Run canonical repository enforcement.
    Enforce {
        #[command(subcommand)]
        scope: Option<EnforceScope>,
    },
    /// Plan and build OCI images.
    Image {
        #[command(subcommand)]
        command: ImageCommand,
    },
    /// Publish immutable release artifacts.
    Release {
        #[command(subcommand)]
        command: ReleaseCommand,
    },
    /// Build and dispatch the typed Rust smoke harness.
    Smoke(SmokeArgs),
    /// Record and display locally executed build and test results.
    TestReport {
        #[command(subcommand)]
        command: TestReportCommand,
    },
}

#[derive(Debug, Subcommand)]
enum TestReportCommand {
    /// Run one existing command and record its result in the committed report.
    Run(TestReportRunArgs),
    /// Display the committed report and fail when it is stale or reports a failure.
    Show(TestReportShowArgs),
}

#[derive(Debug, Args)]
struct TestReportRunArgs {
    /// Stable name shown for this check in the report.
    #[arg(long)]
    name: String,
    /// Command and arguments to execute without a shell.
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<OsString>,
}

#[derive(Debug, Args)]
struct TestReportShowArgs {
    /// Also append the Markdown report to GITHUB_STEP_SUMMARY when available.
    #[arg(long)]
    github_summary: bool,
}

#[derive(Debug, Subcommand)]
enum EnforceScope {
    /// Run Rust formatting, linting, tests, and documentation checks.
    Rust,
    /// Run the locked Python SDK and released-package template checks.
    Python,
}

#[derive(Debug, Subcommand)]
enum ImageCommand {
    /// Inspect and manage the repository Buildx builder.
    Builder {
        #[command(subcommand)]
        command: BuilderCommand,
    },
    /// Remove digest-keyed local Docker materializations retained by simulation certification.
    CertificationCachePrune(CertificationCachePruneArgs),
    /// Resolve and validate an image build plan.
    Plan(ImagePlanArgs),
    /// Explain the image and release surfaces affected since one source revision.
    Affected(ImageAffectedArgs),
    /// Build selected images from the current checkout and load them into Docker.
    Build(ImageSelectionArgs),
    /// Publish immutable runtime images for a development cluster without release attestations.
    Stage(ImageStageArgs),
    /// Merge staged runtime identities into a non-release, digest-locked GitOps closure.
    DevelopmentLock(ImageDevelopmentLockArgs),
}

#[derive(Debug, Subcommand)]
enum ReleaseCommand {
    /// Check host and optional Kubernetes headroom before an expensive release.
    Preflight(ReleasePreflightArgs),
    /// Publish images from one exact committed revision.
    Images(ReleaseImagesArgs),
    /// Build, verify, and optionally publish the Python SDK.
    PythonSdk(ReleasePythonSdkArgs),
    /// Build, verify, and optionally publish the private Helm chart set.
    HelmCharts(ReleaseHelmChartsArgs),
    /// Publish paired hardware evidence for the canonical simulation runtime.
    SimulationRuntime(ReleaseSimulationRuntimeArgs),
    /// Generate one compatibility release from immutable publication evidence.
    Compatibility(ReleaseCompatibilityArgs),
}

#[derive(Debug, Args)]
struct ReleasePreflightArgs {
    /// Estimated peak additional disk use for the planned build.
    #[arg(long, default_value_t = 320)]
    expected_growth_gib: u64,
    /// Free filesystem percentage retained after the estimated build growth.
    #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u8).range(1..=99))]
    reserve_free_percent: u8,
    /// Kubernetes node whose Ready and DiskPressure conditions must be healthy.
    #[arg(long)]
    kubernetes_node: Option<String>,
    /// Namespace inspected for historical Evicted pod objects.
    #[arg(long, default_value = "veoveo", requires = "kubernetes_node")]
    namespace: String,
}

#[derive(Debug, Subcommand)]
enum BuilderCommand {
    /// Report the builder and tool versions without changing them.
    Status,
    /// Create a missing builder and verify its complete contract.
    Ensure,
    /// Apply the checked-in builder configuration without deleting build state.
    Reconfigure(BuilderConfirmationArgs),
    /// Remove and recreate the managed builder.
    Recreate(BuilderConfirmationArgs),
}

#[derive(Debug, Args)]
struct SmokeArgs {
    /// Smoke scenario and its typed arguments.
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    arguments: Vec<OsString>,
}

#[derive(Debug, Args)]
struct BuilderConfirmationArgs {
    /// Required builder-name confirmation.
    #[arg(long)]
    confirm: String,
}

#[derive(Debug, Args)]
struct CertificationCachePruneArgs {
    /// Required certification cache repository confirmation.
    #[arg(long)]
    confirm: String,
}

#[derive(Clone, Debug, Args)]
struct ImageSelectionArgs {
    /// One Docker Bake image target.
    #[arg(long, conflicts_with = "group", required_unless_present = "group")]
    target: Option<String>,
    /// One Docker Bake image group.
    #[arg(long, conflicts_with = "target", required_unless_present = "target")]
    group: Option<String>,
}

#[derive(Debug, Args)]
struct ImagePlanArgs {
    #[command(flatten)]
    selection: ImageSelectionArgs,
    /// Plan output encoding.
    #[arg(long, value_enum, default_value_t = PlanFormat::Human)]
    format: PlanFormat,
}

#[derive(Debug, Args)]
struct ImageAffectedArgs {
    /// Baseline Git revision compared with the current working tree.
    #[arg(long)]
    since: String,
    /// Plan output encoding.
    #[arg(long, value_enum, default_value_t = PlanFormat::Human)]
    format: PlanFormat,
}

#[derive(Debug, Args)]
struct ImageStageArgs {
    #[command(flatten)]
    selection: ImageSelectionArgs,
    /// OCI registry endpoint reachable by the publication host.
    #[arg(long)]
    push_registry: String,
    /// OCI registry endpoint used by Kubernetes image pulls.
    #[arg(long)]
    pull_registry: String,
    /// Explicit transport used by the registry endpoints.
    #[arg(long, value_enum)]
    registry_transport: RegistryTransportArg,
    /// Exact committed source revision to stage.
    #[arg(long)]
    revision: String,
    /// Create-only staging evidence output.
    #[arg(long)]
    evidence_output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ImageDevelopmentLockArgs {
    /// Qualified deployment lock that supplies the complete unchanged image closure.
    #[arg(long)]
    base_lock: PathBuf,
    /// Runtime-only staging evidence to merge; repeat for independently staged selections.
    #[arg(long, required = true)]
    stage_evidence: Vec<PathBuf>,
    /// Typed non-release development image lock output.
    #[arg(long)]
    output: PathBuf,
    /// Helm-compatible JSON values containing the merged registry and image digests.
    #[arg(long)]
    values_output: PathBuf,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PlanFormat {
    Human,
    Json,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum RegistryTransportArg {
    Tls,
    InsecureHttp,
}

impl From<RegistryTransportArg> for veoveo_deploy_contract::RegistryTransport {
    fn from(value: RegistryTransportArg) -> Self {
        match value {
            RegistryTransportArg::Tls => Self::Tls,
            RegistryTransportArg::InsecureHttp => Self::InsecureHttp,
        }
    }
}

#[derive(Debug, Args)]
struct ReleaseImagesArgs {
    /// Deployment profile path, interpreted inside the selected profile revision.
    #[arg(long, conflicts_with_all = ["target", "group", "push_registry", "pull_registry", "registry_transport"])]
    profile: Option<PathBuf>,
    /// Exact configuration-repository revision containing the deployment profile.
    #[arg(long, requires = "profile", conflicts_with = "revision")]
    profile_revision: Option<String>,
    /// One Docker Bake image target.
    #[arg(long, conflicts_with_all = ["profile", "group"])]
    target: Option<String>,
    /// One Docker Bake image group.
    #[arg(long, conflicts_with_all = ["profile", "target"])]
    group: Option<String>,
    /// OCI registry endpoint reachable by the publication host for a direct release.
    #[arg(long)]
    push_registry: Option<String>,
    /// OCI registry endpoint used by Kubernetes image pulls for a direct release.
    #[arg(long)]
    pull_registry: Option<String>,
    /// Explicit transport used by direct-release registry endpoints.
    #[arg(long, value_enum)]
    registry_transport: Option<RegistryTransportArg>,
    /// Exact source revision for a direct target or group release.
    #[arg(long)]
    revision: Option<String>,
    /// Immutable deployment lock output for a profile release.
    #[arg(long, requires = "profile")]
    lock_output: Option<PathBuf>,
    /// Immutable image release evidence output for a direct release.
    #[arg(long, conflicts_with = "profile")]
    evidence_output: Option<PathBuf>,
    /// Staged-image evidence whose runnable digests must survive qualification unchanged.
    #[arg(long, conflicts_with = "profile")]
    stage_evidence: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ReleasePythonSdkArgs {
    /// Exact Git revision or ref to resolve.
    #[arg(long)]
    revision: String,
    /// Parent directory for the revision-addressed release bundle.
    #[arg(long, default_value = "output/releases/python-sdk")]
    output_dir: PathBuf,
    /// Private Python package upload endpoint. Credentials come from UV_PUBLISH_*.
    #[arg(long)]
    publish_url: Option<String>,
    /// Private simple-index URL used to skip an artifact that already exists.
    #[arg(long, requires = "publish_url")]
    check_url: Option<String>,
    /// Validate the upload without changing the package index.
    #[arg(long, requires = "publish_url")]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ReleaseHelmChartsArgs {
    /// Exact Git revision or ref to resolve.
    #[arg(long)]
    revision: String,
    /// Semantic chart release version.
    #[arg(long)]
    version: String,
    /// Parent directory for revision- and version-addressed release bundles.
    #[arg(long, default_value = "output/releases/helm")]
    output_dir: PathBuf,
    /// Private OCI registry host and repository prefix, without oci://.
    #[arg(long)]
    registry: Option<String>,
    /// Permit unencrypted OCI transport for an explicitly selected internal registry.
    #[arg(long, requires = "registry")]
    plain_http: bool,
}

#[derive(Debug, Args)]
struct ReleaseSimulationRuntimeArgs {
    /// Exact Veoveo source revision that produced both certified overlays.
    #[arg(long)]
    revision: String,
    /// Semantic simulation-runtime release version.
    #[arg(long)]
    version: String,
    /// Validated deployment lock authorizing the registry identity and transport.
    #[arg(long)]
    deployment_lock: PathBuf,
    /// Hardware result for the existing first-party UAV overlay.
    #[arg(long)]
    first_party_result: PathBuf,
    /// Hardware result for the repository-neutral external overlay fixture.
    #[arg(long)]
    anonymous_result: PathBuf,
    /// Parent directory for revision- and version-addressed release bundles.
    #[arg(long, default_value = "output/releases/simulation-runtime")]
    output_dir: PathBuf,
}

#[derive(Debug, Args)]
struct ReleaseCompatibilityArgs {
    /// Exact Veoveo source revision that owns the compatibility release.
    #[arg(long)]
    revision: String,
    /// Semantic compatibility release identity.
    #[arg(long)]
    release: String,
    /// Semantic Veoveo platform version.
    #[arg(long)]
    platform_version: String,
    /// Python SDK release-evidence JSON.
    #[arg(long)]
    python_evidence: PathBuf,
    /// Credential-free private Python artifact base using python://.
    #[arg(long)]
    python_artifact_base: String,
    /// OCI Helm release-evidence JSON.
    #[arg(long)]
    helm_evidence: PathBuf,
    /// Extension-support image release-evidence JSON.
    #[arg(long)]
    image_evidence: PathBuf,
    /// Optional published simulation-runtime release evidence.
    #[arg(long)]
    simulation_evidence: Option<PathBuf>,
    /// Revision-addressed compatibility release output parent.
    #[arg(long, default_value = "output/releases/compatibility")]
    output_dir: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repository = RepositoryContext::discover(&PathBuf::from("."))?;
    match cli.command {
        Command::Doctor => doctor::run(&repository),
        Command::Enforce { scope } => match scope.unwrap_or(EnforceScope::Rust) {
            EnforceScope::Rust => enforce::rust(&repository),
            EnforceScope::Python => enforce::python(&repository),
        },
        Command::Image { command } => match command {
            ImageCommand::Builder { command } => match command {
                BuilderCommand::Status => builder::status(&repository),
                BuilderCommand::Ensure => builder::ensure(&repository).map(drop),
                BuilderCommand::Reconfigure(args) => {
                    builder::reconfigure(&repository, &args.confirm)
                }
                BuilderCommand::Recreate(args) => builder::recreate(&repository, &args.confirm),
            },
            ImageCommand::CertificationCachePrune(args) => {
                builder::prune_certification_cache(&repository, &args.confirm)
            }
            ImageCommand::Plan(args) => {
                image::plan_command(&repository, &args.selection, args.format)
            }
            ImageCommand::Affected(args) => {
                image::affected_command(&repository, &args.since, args.format)
            }
            ImageCommand::Build(selection) => image::build_command(&repository, &selection),
            ImageCommand::Stage(args) => release::stage_images(&repository, &args),
            ImageCommand::DevelopmentLock(args) => {
                release::development_image_lock(&repository, &args)
            }
        },
        Command::Release { command } => match command {
            ReleaseCommand::Preflight(args) => release_preflight::run(&repository, &args),
            ReleaseCommand::Images(args) => release::images(&repository, &args),
            ReleaseCommand::PythonSdk(args) => release::python_sdk(&repository, &args),
            ReleaseCommand::HelmCharts(args) => release::helm_charts(&repository, &args),
            ReleaseCommand::SimulationRuntime(args) => {
                release::simulation_runtime(&repository, &args)
            }
            ReleaseCommand::Compatibility(args) => release::compatibility(&repository, &args),
        },
        Command::Smoke(args) => smoke::run(&repository, &args.arguments),
        Command::TestReport { command } => match command {
            TestReportCommand::Run(args) => {
                test_report::run(&repository, &args.name, &args.command)
            }
            TestReportCommand::Show(args) => test_report::show(&repository, args.github_summary),
        },
    }
}
