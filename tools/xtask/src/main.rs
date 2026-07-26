mod commands;
mod context;
mod process;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::{
    commands::{builder, doctor, enforce, image, release},
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
    /// Resolve and validate an image build plan.
    Plan(ImagePlanArgs),
    /// Build selected images from the current checkout and load them into Docker.
    Build(ImageSelectionArgs),
}

#[derive(Debug, Subcommand)]
enum ReleaseCommand {
    /// Publish images from one exact committed revision.
    Images(ReleaseImagesArgs),
    /// Build, verify, and optionally publish the Python SDK.
    PythonSdk(ReleasePythonSdkArgs),
    /// Build, verify, and optionally publish the private Helm chart set.
    HelmCharts(ReleaseHelmChartsArgs),
}

#[derive(Debug, Subcommand)]
enum BuilderCommand {
    /// Report the builder and tool versions without changing them.
    Status,
    /// Create a missing builder and verify its complete contract.
    Ensure,
    /// Remove and recreate the managed builder.
    Recreate(RecreateArgs),
}

#[derive(Debug, Args)]
struct RecreateArgs {
    /// Required destructive-action confirmation.
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

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PlanFormat {
    Human,
    Json,
}

#[derive(Debug, Args)]
struct ReleaseImagesArgs {
    /// Deployment profile path, interpreted inside the selected profile revision.
    #[arg(long, conflicts_with_all = ["target", "group", "registry"])]
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
    /// OCI registry for a direct target or group release.
    #[arg(long)]
    registry: Option<String>,
    /// Exact source revision for a direct target or group release.
    #[arg(long)]
    revision: Option<String>,
    /// Immutable deployment lock output for a profile release.
    #[arg(long, requires = "profile")]
    lock_output: Option<PathBuf>,
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
                BuilderCommand::Ensure => builder::ensure(&repository),
                BuilderCommand::Recreate(args) => builder::recreate(&repository, &args.confirm),
            },
            ImageCommand::Plan(args) => {
                image::plan_command(&repository, &args.selection, args.format)
            }
            ImageCommand::Build(selection) => image::build_command(&repository, &selection),
        },
        Command::Release { command } => match command {
            ReleaseCommand::Images(args) => release::images(&repository, &args),
            ReleaseCommand::PythonSdk(args) => release::python_sdk(&repository, &args),
            ReleaseCommand::HelmCharts(args) => release::helm_charts(&repository, &args),
        },
    }
}
