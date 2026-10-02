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
    /// Verify live UAV camera inference and preview without flight or recording replay.
    UavStreamVerify {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        #[arg(long)]
        installation: PathBuf,
    },
    /// Verify live Recording replay and grounded Reason without flight commands.
    UavRecordingVerify {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        #[arg(long)]
        installation: PathBuf,
    },
    /// Verify the scenario's current Map route admission without flight commands.
    UavRouteVerify {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        #[arg(long)]
        installation: PathBuf,
    },
    /// Publish the scenario in Frames and write the admitted simulator startup binding.
    UavWorldPublish {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        #[arg(long)]
        installation: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Verify UAV flight, live Stream, recording replay and Artifact isolation.
    UavDomainVerify {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Runtime-loaded mission and acceptance parameters.
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        /// Installation-owned cluster, OAuth identity, scopes, and public origin.
        #[arg(long)]
        installation: PathBuf,
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
        /// Installation-owned cluster, OAuth identity, scopes, and public origin.
        #[arg(long)]
        installation: PathBuf,
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
        /// Installation-owned cluster, OAuth identity, scopes, and public origin.
        #[arg(long)]
        installation: PathBuf,
        /// HTTP discovery or direct ws:// browser endpoint for headed hardware-backed Chrome.
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        /// Root for revision- and run-qualified JSON and PNG evidence.
        #[arg(long, default_value = "output/acceptance/uav")]
        evidence_root: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_publication_requires_installation_and_explicit_output() {
        for missing in [
            vec!["flight-smoke", "uav-world-publish"],
            vec![
                "flight-smoke",
                "uav-world-publish",
                "--installation",
                "fork.json",
            ],
            vec![
                "flight-smoke",
                "uav-world-publish",
                "--output",
                "world.json",
            ],
        ] {
            assert!(Args::try_parse_from(missing).is_err());
        }
        assert!(
            Args::try_parse_from([
                "flight-smoke",
                "uav-world-publish",
                "--installation",
                "fork.json",
                "--output",
                "world.json"
            ])
            .is_ok()
        );
    }

    #[test]
    fn installed_flight_commands_require_one_target_and_reject_coordinate_overrides() {
        for command in [
            "uav-stream-verify",
            "uav-recording-verify",
            "uav-route-verify",
            "uav-domain-verify",
            "uav-showcase-up",
            "uav-showcase-verify",
        ] {
            assert!(Args::try_parse_from(["flight-smoke", command]).is_err());
            assert!(
                Args::try_parse_from(["flight-smoke", command, "--installation", "fork.json"])
                    .is_ok()
            );
            for flag in ["--context", "--namespace", "--public-base-url"] {
                assert!(
                    Args::try_parse_from([
                        "flight-smoke",
                        command,
                        "--installation",
                        "fork.json",
                        flag,
                        "override"
                    ])
                    .is_err()
                );
            }
        }
    }
}
