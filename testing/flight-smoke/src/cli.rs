use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "flight-smoke",
    about = "Composed flight acceptance against a running Veoveo installation"
)]
pub(crate) struct Args {
    #[command(subcommand)]
    pub(crate) command: SmokeCommand,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum SmokeCommand {
    /// Verify the independent UAV domain path through flight, live Stream, recording replay, and Reason.
    UavDomainVerify {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Runtime-loaded mission and acceptance parameters.
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        /// Kubernetes context containing the UAV showcase.
        #[arg(long)]
        context: String,
        /// Public installation base URL used for OAuth and MCP.
        #[arg(long)]
        public_base_url: String,
    },
    /// Converge the always-on UAV loop and its simulator-hosted operator cameras.
    UavShowcaseUp {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        /// Kubernetes context containing the composed showcase.
        #[arg(long)]
        context: String,
        /// Namespace containing the platform and showcase releases.
        #[arg(long, default_value = "veoveo")]
        namespace: String,
        /// Public installation base URL used by MCP.
        #[arg(long)]
        public_base_url: String,
    },
    /// Run UAV flight and prove its authoritative live camera in the real Console.
    UavShowcaseVerify {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        /// Kubernetes context containing the composed showcase.
        #[arg(long)]
        context: String,
        /// Namespace containing the platform and showcase releases.
        #[arg(long, default_value = "veoveo")]
        namespace: String,
        /// Public installation base URL used by MCP and the authenticated Console.
        #[arg(long)]
        public_base_url: String,
        /// HTTP discovery or direct ws:// browser endpoint for headed hardware-backed Chrome.
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        /// Root for revision- and run-qualified JSON and PNG evidence.
        #[arg(long, default_value = "output/acceptance/uav")]
        evidence_root: PathBuf,
    },
}
