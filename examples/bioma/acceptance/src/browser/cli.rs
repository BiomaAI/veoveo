use std::path::PathBuf;

use clap::{Parser, Subcommand};
use veoveo_recording_contract::RecordingId;

#[derive(Debug, Parser)]
#[command(
    name = "browser-smoke",
    about = "Focused headed-browser acceptance against a running VeoVeo installation"
)]
pub struct Args {
    #[command(subcommand)]
    pub command: SmokeCommand,
}

#[derive(Debug, Subcommand)]
// These are deliberately full, stable xtask dispatch names in a focused UAV
// browser binary; the shared prefix is part of the CLI rather than Rust type noise.
#[allow(clippy::enum_variant_names)]
pub enum SmokeCommand {
    /// Upload Markdown in Workspace and verify its governed download and restored receipt.
    WorkspaceMarkdownVerify {
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value = "output/acceptance/workspace-markdown")]
        evidence_root: PathBuf,
    },
    /// Exercise deployed private dictation and recording Tasks with actual CUDA inference.
    SpeechWorkspaceVerify {
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value = "output/acceptance/speech")]
        evidence_root: PathBuf,
        #[arg(long, default_value = "servers/speech-mcp/testdata/english.wav")]
        audio_fixture: PathBuf,
    },
    /// Render the generated Map workspace App with a hardware GPU and bounded fixture bridge.
    MapWorkspaceBrowserVerify {
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value = "servers/map-mcp/assets/workspace-app.html")]
        app_html: PathBuf,
        #[arg(long, default_value = "output/acceptance/map-workspace")]
        evidence_root: PathBuf,
        #[arg(long, default_value_t = 60)]
        timeout_seconds: u64,
    },
    /// Interact with the deployed Map workspace through the authenticated public Console.
    MapWorkspaceLiveBrowserVerify {
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = "VeoVeo Map live acceptance")]
        composition_title: String,
        #[arg(long, default_value = "VeoVeo GeoPackage live acceptance")]
        layer_title: String,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value = "output/acceptance/map-workspace-live")]
        evidence_root: PathBuf,
        #[arg(long, default_value_t = 120)]
        timeout_seconds: u64,
    },
    /// Verify the Console and standalone UAV App hosts without opening live products.
    UavAppHostsBrowserVerify {
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value_t = 180)]
        timeout_seconds: u64,
    },
    /// Verify the complete grouped first-party App catalog and render every expected App.
    ConsoleAppsBrowserVerify {
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value = "output/acceptance/console-apps")]
        evidence_root: PathBuf,
        #[arg(long, default_value_t = 300)]
        timeout_seconds: u64,
    },
    /// Verify public Console upload selection, durable resume, and a real large transfer.
    ConsoleArtifactUploadVerify {
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value = "output/acceptance/artifact-upload")]
        evidence_root: PathBuf,
        #[arg(long, default_value_t = 10 * 1024 * 1024 * 1024)]
        bytes: u64,
        #[arg(long, default_value_t = 5400)]
        timeout_seconds: u64,
        #[arg(long)]
        preflight_only: bool,
    },
    /// Resume an existing public upload acceptance fixture without retransmitting saved parts.
    ConsoleArtifactUploadResume {
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long)]
        evidence_directory: PathBuf,
        #[arg(long, default_value_t = 5400)]
        timeout_seconds: u64,
    },
    /// Verify upload keyboard, clipboard, filtering, narrow recovery, and cancellation.
    ConsoleArtifactUploadUxVerify {
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long)]
        completed_evidence: PathBuf,
        #[arg(long, default_value = "output/acceptance/artifact-upload")]
        evidence_root: PathBuf,
    },
    /// Repeat headed Console acceptance without restarting or commanding the simulation.
    UavShowcaseBrowserVerify {
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value = "output/acceptance/uav-browser")]
        evidence_root: PathBuf,
    },
    /// Send one UAV App instruction and prove its durable pilot reply through Console.
    UavAgentInstructionBrowserVerify {
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long)]
        agent_id: String,
        #[arg(long)]
        message: String,
        #[arg(long, default_value_t = 300)]
        timeout_seconds: u64,
        #[arg(
            long,
            default_value = "output/acceptance/uav-agent-instruction-browser"
        )]
        evidence_root: PathBuf,
    },
    /// Keep a headed live view mounted while restarting the MCP and simulator containers.
    UavShowcaseLiveRestartVerify {
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value_t = 1_800)]
        restart_timeout_seconds: u64,
        #[arg(long, default_value = "output/acceptance/uav-live-restart")]
        evidence_root: PathBuf,
    },
    /// Verify only governed live Recording playback against the running source.
    UavRecordingBrowserVerify {
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value = "output/acceptance/uav-recording-browser")]
        evidence_root: PathBuf,
    },
    /// Verify one governed sealed Recording through the lazy Redap archive path.
    UavRecordingArchiveBrowserVerify {
        #[arg(long)]
        recording_id: RecordingId,
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(
            long,
            default_value = "output/acceptance/uav-recording-archive-browser"
        )]
        evidence_root: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_browser_commands_require_a_target_before_execution() {
        let scenarios: &[(&str, &[&str])] = &[
            ("workspace-markdown-verify", &[]),
            ("speech-workspace-verify", &[]),
            ("map-workspace-live-browser-verify", &[]),
            ("uav-app-hosts-browser-verify", &[]),
            ("console-apps-browser-verify", &[]),
            ("console-artifact-upload-verify", &[]),
            (
                "console-artifact-upload-resume",
                &["--evidence-directory", "receipt"],
            ),
            (
                "console-artifact-upload-ux-verify",
                &["--completed-evidence", "receipt.json"],
            ),
            ("uav-showcase-browser-verify", &[]),
            (
                "uav-agent-instruction-browser-verify",
                &["--agent-id", "pilot", "--message", "inspect"],
            ),
            ("uav-showcase-live-restart-verify", &[]),
            ("uav-recording-browser-verify", &[]),
            (
                "uav-recording-archive-browser-verify",
                &["--recording-id", "01900000-0000-7000-8000-000000000001"],
            ),
        ];
        for (command, extra) in scenarios {
            let mut args = vec!["browser-smoke", command];
            args.extend_from_slice(extra);
            assert!(
                Args::try_parse_from(&args).is_err(),
                "{command} must require a target"
            );
            args.extend(["--installation", "fork.json"]);
            assert!(
                Args::try_parse_from(&args).is_ok(),
                "{command} must accept a target"
            );
            for flag in ["--public-base-url", "--context", "--namespace"] {
                let mut overridden = args.clone();
                overridden.extend([flag, "override"]);
                assert!(
                    Args::try_parse_from(overridden).is_err(),
                    "{command} accepted {flag}"
                );
            }
        }
        assert!(Args::try_parse_from(["browser-smoke", "map-workspace-browser-verify"]).is_ok());
    }
}
