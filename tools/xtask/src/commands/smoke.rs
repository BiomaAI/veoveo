use std::{
    env,
    ffi::{OsStr, OsString},
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};

use crate::{context::RepositoryContext, process};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CargoBinary {
    package: &'static str,
    binary: &'static str,
}

const SMOKE: CargoBinary = CargoBinary {
    package: "veoveo-smoke",
    binary: "smoke",
};
const DEPLOYMENT_SMOKE: CargoBinary = CargoBinary {
    package: "veoveo-deployment-smoke",
    binary: "deployment-smoke",
};
const BROWSER_SMOKE: CargoBinary = CargoBinary {
    package: "veoveo-browser-smoke",
    binary: "browser-smoke",
};
const FLIGHT_SMOKE: CargoBinary = CargoBinary {
    package: "veoveo-flight-smoke",
    binary: "flight-smoke",
};
const CONFORMANCE: CargoBinary = CargoBinary {
    package: "veoveo-mcp-conformance",
    binary: "conformance",
};
const GATEWAY: CargoBinary = CargoBinary {
    package: "veoveo-mcp-gateway",
    binary: "gateway",
};
const MEDIA: CargoBinary = CargoBinary {
    package: "veoveo-media-mcp",
    binary: "media-mcp",
};
const ARTIFACT_SERVICE: CargoBinary = CargoBinary {
    package: "veoveo-artifact-service",
    binary: "artifact-service",
};
const FRAMES: CargoBinary = CargoBinary {
    package: "veoveo-frames-mcp",
    binary: "frames-mcp",
};
const RECORDING_SPOOLER: CargoBinary = CargoBinary {
    package: "veoveo-recording-hub",
    binary: "spooler",
};
const RECORDING_FORWARDER: CargoBinary = CargoBinary {
    package: "veoveo-recording-forwarder",
    binary: "recording-forwarder",
};
const DUCKDB: CargoBinary = CargoBinary {
    package: "veoveo-duckdb-mcp",
    binary: "duckdb-mcp",
};
const OPTIMIZATION: CargoBinary = CargoBinary {
    package: "veoveo-optimization-mcp",
    binary: "optimization-mcp",
};
const AGENT: CargoBinary = CargoBinary {
    package: "veoveo-agent-kernel",
    binary: "agent",
};

pub(crate) fn run(repository: &RepositoryContext, arguments: &[OsString]) -> Result<()> {
    let dispatcher = dispatcher_binary(arguments)?;
    let build_arguments = cargo_build_arguments(arguments)?;
    process::cargo_status(&build_arguments, Some(repository.root()))?;
    let target = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| repository.root().join("target"));
    let target = if target.is_absolute() {
        target
    } else {
        repository.root().join(target)
    };
    let dependencies = target.join("debug/deps");
    let executable = target.join("debug").join(dispatcher.binary);
    let mut command = Command::new(&executable);
    command
        .args(arguments)
        .current_dir(repository.root())
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    prepend_library_path(&mut command, "LD_LIBRARY_PATH", &dependencies)?;
    prepend_library_path(&mut command, "DYLD_LIBRARY_PATH", &dependencies)?;
    let status = command
        .status()
        .with_context(|| format!("running typed smoke harness {}", executable.display()))?;
    if !status.success() {
        bail!("typed smoke harness failed with {status}");
    }
    Ok(())
}

fn cargo_build_arguments(arguments: &[OsString]) -> Result<Vec<&'static str>> {
    let dispatcher = dispatcher_binary(arguments)?;
    let mut binaries = vec![dispatcher];
    if !requests_help(arguments) && dispatcher == FLIGHT_SMOKE {
        binaries.push(CONFORMANCE);
    } else if !requests_help(arguments) && dispatcher == BROWSER_SMOKE {
        if !matches!(
            arguments.first().and_then(|argument| argument.to_str()),
            Some(
                "map-workspace-browser-verify"
                    | "map-workspace-live-browser-verify"
                    | "console-apps-browser-verify"
                    | "console-artifact-upload-verify"
                    | "console-artifact-upload-resume"
                    | "console-artifact-upload-ux-verify"
                    | "uav-app-hosts-browser-verify"
                    | "uav-agent-instruction-browser-verify"
            )
        ) {
            binaries.push(CONFORMANCE);
        }
    } else if !requests_help(arguments) && dispatcher == SMOKE {
        let scenario = arguments
            .first()
            .context("smoke scenario is required")?
            .to_str()
            .context("smoke scenario is not valid UTF-8")?;
        if !matches!(
            scenario,
            "stream-gpu" | "reason-gpu" | "stream-compiler-startup" | "recording-fixture-finish"
        ) {
            binaries.push(CONFORMANCE);
        }
        for binary in scenario_binaries(scenario)? {
            if !binaries.contains(binary) {
                binaries.push(*binary);
            }
        }
    }

    let mut build_arguments = vec!["build", "--locked"];
    for binary in binaries {
        build_arguments.extend(["--package", binary.package, "--bin", binary.binary]);
    }
    Ok(build_arguments)
}

fn dispatcher_binary(arguments: &[OsString]) -> Result<CargoBinary> {
    if requests_help(arguments) && arguments.is_empty() {
        return Ok(SMOKE);
    }
    let scenario = arguments
        .first()
        .context("smoke scenario is required")?
        .to_str()
        .context("smoke scenario is not valid UTF-8")?;
    if matches!(
        scenario,
        "helm-config"
            | "profile-validate"
            | "profile-registry-up"
            | "profile-cluster-up"
            | "profile-cluster-stop"
            | "profile-cluster-delete"
            | "profile-up"
            | "profile-gpu-verify"
            | "profile-down"
            | "gitops-converge"
            | "gitops-cancel-verify"
            | "component-scope-verify"
    ) {
        Ok(DEPLOYMENT_SMOKE)
    } else if matches!(
        scenario,
        "map-workspace-browser-verify"
            | "map-workspace-live-browser-verify"
            | "console-apps-browser-verify"
            | "console-artifact-upload-verify"
            | "console-artifact-upload-resume"
            | "console-artifact-upload-ux-verify"
            | "uav-app-hosts-browser-verify"
            | "uav-agent-instruction-browser-verify"
            | "uav-showcase-browser-verify"
            | "uav-showcase-live-restart-verify"
            | "uav-recording-browser-verify"
            | "uav-recording-archive-browser-verify"
    ) {
        Ok(BROWSER_SMOKE)
    } else if matches!(
        scenario,
        "uav-domain-verify" | "uav-showcase-up" | "uav-showcase-verify"
    ) {
        Ok(FLIGHT_SMOKE)
    } else {
        Ok(SMOKE)
    }
}

fn requests_help(arguments: &[OsString]) -> bool {
    arguments
        .iter()
        .any(|argument| argument == OsStr::new("--help") || argument == OsStr::new("-h"))
}

fn scenario_binaries(scenario: &str) -> Result<&'static [CargoBinary]> {
    let binaries = match scenario {
        "gateway-suite" => &[
            CONFORMANCE,
            GATEWAY,
            RECORDING_SPOOLER,
            MEDIA,
            ARTIFACT_SERVICE,
        ][..],
        "gateway-platform-store" | "gateway-vault-secrets" => &[GATEWAY],
        "contract-schemas"
        | "sumo-verify"
        | "uav-domain-verify"
        | "uav-showcase-up"
        | "uav-showcase-verify" => &[CONFORMANCE],
        "otel"
        | "gateway-http"
        | "gateway-keycloak"
        | "gateway-two-servers"
        | "gateway-console-stream"
        | "gateway-chart-projection" => &[CONFORMANCE, GATEWAY],
        "media-mcp-auth" | "media-task-run" => &[CONFORMANCE, MEDIA, ARTIFACT_SERVICE],
        "frames-mcp" => &[CONFORMANCE, FRAMES, ARTIFACT_SERVICE],
        "map-mcp" | "datasheet-mcp" => &[CONFORMANCE, ARTIFACT_SERVICE],
        "recording-ingest" => &[CONFORMANCE, GATEWAY, RECORDING_SPOOLER],
        "gateway-authenticated" | "gateway-task-run" => {
            &[CONFORMANCE, MEDIA, GATEWAY, ARTIFACT_SERVICE]
        }
        "agent-kernel" | "agent-sleep-wake" | "agent-kernel-scheduler" => {
            &[CONFORMANCE, MEDIA, GATEWAY, ARTIFACT_SERVICE, AGENT]
        }
        "agent-pilot" => &[
            CONFORMANCE,
            FRAMES,
            OPTIMIZATION,
            GATEWAY,
            ARTIFACT_SERVICE,
            AGENT,
        ],
        "agent-gateway" => &[CONFORMANCE, DUCKDB, GATEWAY, ARTIFACT_SERVICE],
        "stream-gpu" | "reason-gpu" => &[RECORDING_FORWARDER],
        "stream-compiler-startup" | "recording-fixture-finish" => &[],
        "helm-config"
        | "external-simulation-fixture"
        | "profile-validate"
        | "profile-registry-up"
        | "profile-cluster-up"
        | "profile-cluster-stop"
        | "profile-cluster-delete"
        | "profile-up"
        | "profile-gpu-verify"
        | "profile-down"
        | "gitops-converge"
        | "gitops-cancel-verify"
        | "component-scope-verify"
        | "gpu-allocation-verify"
        | "bioma-verify"
        | "artifact-upload-consumers"
        | "surreal-integration"
        | "view-mcp"
        | "view-google-live"
        | "sumo-push"
        | "simulation-certify"
        | "help" => &[],
        unknown => bail!(
            "unknown smoke scenario `{unknown}`; run `cargo xtask smoke help` to list scenarios"
        ),
    };
    Ok(binaries)
}

fn prepend_library_path(command: &mut Command, key: &str, path: &std::path::Path) -> Result<()> {
    let mut paths = vec![path.to_path_buf()];
    if let Some(existing) = env::var_os(key) {
        paths.extend(env::split_paths(&existing));
    }
    command.env(
        key,
        env::join_paths(paths).with_context(|| format!("constructing {key}"))?,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flight_scenarios_build_only_the_focused_client_and_conformance() {
        for scenario in [
            "uav-domain-verify",
            "uav-showcase-up",
            "uav-showcase-verify",
        ] {
            assert_eq!(dispatcher_binary(&[scenario.into()]).unwrap(), FLIGHT_SMOKE);
            assert_eq!(
                cargo_build_arguments(&[scenario.into()]).unwrap(),
                [
                    "build",
                    "--locked",
                    "--package",
                    "veoveo-flight-smoke",
                    "--bin",
                    "flight-smoke",
                    "--package",
                    "veoveo-mcp-conformance",
                    "--bin",
                    "conformance",
                ]
            );
            assert_eq!(
                cargo_build_arguments(&[scenario.into(), "--help".into()]).unwrap(),
                [
                    "build",
                    "--locked",
                    "--package",
                    "veoveo-flight-smoke",
                    "--bin",
                    "flight-smoke"
                ]
            );
        }
    }

    #[test]
    fn gpu_video_scenarios_build_only_their_used_binaries() {
        for scenario in ["stream-gpu", "reason-gpu"] {
            assert_eq!(
                cargo_build_arguments(&[scenario.into()]).unwrap(),
                [
                    "build",
                    "--locked",
                    "--package",
                    "veoveo-smoke",
                    "--bin",
                    "smoke",
                    "--package",
                    "veoveo-recording-forwarder",
                    "--bin",
                    "recording-forwarder",
                ]
            );
        }
        assert_eq!(
            cargo_build_arguments(&["stream-compiler-startup".into()]).unwrap(),
            [
                "build",
                "--locked",
                "--package",
                "veoveo-smoke",
                "--bin",
                "smoke",
            ]
        );
    }

    #[test]
    fn smoke_arguments_remain_lossless_os_strings() {
        let arguments = [
            OsString::from("uav-showcase-verify"),
            OsString::from("--public-base-url"),
            OsString::from("https://installation.example"),
        ];
        let forwarded = arguments
            .iter()
            .map(OsString::as_os_str)
            .collect::<Vec<_>>();
        assert_eq!(
            forwarded,
            vec![
                OsStr::new("uav-showcase-verify"),
                OsStr::new("--public-base-url"),
                OsStr::new("https://installation.example")
            ]
        );
    }

    #[test]
    fn clean_checkout_build_plan_covers_scenario_binaries() {
        assert_eq!(
            scenario_binaries("recording-ingest").unwrap(),
            &[CONFORMANCE, GATEWAY, RECORDING_SPOOLER]
        );
        assert_eq!(
            scenario_binaries("agent-gateway").unwrap(),
            &[CONFORMANCE, DUCKDB, GATEWAY, ARTIFACT_SERVICE]
        );
        assert_eq!(
            scenario_binaries("uav-showcase-up").unwrap(),
            &[CONFORMANCE]
        );
        assert_eq!(
            scenario_binaries("datasheet-mcp").unwrap(),
            &[CONFORMANCE, ARTIFACT_SERVICE]
        );
        assert_eq!(scenario_binaries("profile-up").unwrap(), &[]);
        assert_eq!(scenario_binaries("gpu-allocation-verify").unwrap(), &[]);
        assert!(scenario_binaries("unmapped-scenario").is_err());
    }

    #[test]
    fn profile_commands_build_only_the_focused_deployment_harness() {
        let arguments = [OsString::from("profile-up")];
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            [
                "build",
                "--locked",
                "--package",
                "veoveo-deployment-smoke",
                "--bin",
                "deployment-smoke",
            ]
        );
    }

    #[test]
    fn gitops_convergence_builds_only_the_focused_deployment_harness() {
        let arguments = [OsString::from("gitops-converge")];
        assert_eq!(dispatcher_binary(&arguments).unwrap(), DEPLOYMENT_SMOKE);
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            [
                "build",
                "--locked",
                "--package",
                "veoveo-deployment-smoke",
                "--bin",
                "deployment-smoke",
            ]
        );
    }

    #[test]
    fn gitops_cancellation_uses_the_focused_harness() {
        let arguments = [OsString::from("gitops-cancel-verify")];
        assert_eq!(dispatcher_binary(&arguments).unwrap(), DEPLOYMENT_SMOKE);
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            cargo_build_arguments(&[OsString::from("gitops-converge")]).unwrap()
        );
    }

    #[test]
    fn component_scope_uses_only_the_focused_harness() {
        let arguments = [OsString::from("component-scope-verify")];
        assert_eq!(dispatcher_binary(&arguments).unwrap(), DEPLOYMENT_SMOKE);
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            cargo_build_arguments(&[OsString::from("profile-up")]).unwrap()
        );
    }

    #[test]
    fn repeated_browser_acceptance_builds_only_its_focused_harness() {
        let arguments = [OsString::from("uav-showcase-browser-verify")];
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            [
                "build",
                "--locked",
                "--package",
                "veoveo-browser-smoke",
                "--bin",
                "browser-smoke",
                "--package",
                "veoveo-mcp-conformance",
                "--bin",
                "conformance",
            ]
        );
    }

    #[test]
    fn app_agent_instruction_builds_only_the_focused_browser_harness() {
        let arguments = [OsString::from("uav-agent-instruction-browser-verify")];
        assert_eq!(dispatcher_binary(&arguments).unwrap(), BROWSER_SMOKE);
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            [
                "build",
                "--locked",
                "--package",
                "veoveo-browser-smoke",
                "--bin",
                "browser-smoke",
            ]
        );
    }

    #[test]
    fn app_host_acceptance_builds_only_the_focused_browser_harness() {
        let arguments = [OsString::from("uav-app-hosts-browser-verify")];
        assert_eq!(dispatcher_binary(&arguments).unwrap(), BROWSER_SMOKE);
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            [
                "build",
                "--locked",
                "--package",
                "veoveo-browser-smoke",
                "--bin",
                "browser-smoke",
            ]
        );
    }

    #[test]
    fn complete_app_catalog_acceptance_builds_only_the_focused_browser_harness() {
        let arguments = [OsString::from("console-apps-browser-verify")];
        assert_eq!(dispatcher_binary(&arguments).unwrap(), BROWSER_SMOKE);
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            [
                "build",
                "--locked",
                "--package",
                "veoveo-browser-smoke",
                "--bin",
                "browser-smoke",
            ]
        );
    }

    #[test]
    fn map_workspace_acceptance_builds_only_the_focused_browser_harness() {
        let arguments = [OsString::from("map-workspace-browser-verify")];
        assert_eq!(dispatcher_binary(&arguments).unwrap(), BROWSER_SMOKE);
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            [
                "build",
                "--locked",
                "--package",
                "veoveo-browser-smoke",
                "--bin",
                "browser-smoke",
            ]
        );
    }

    #[test]
    fn live_map_workspace_acceptance_builds_only_the_focused_browser_harness() {
        let arguments = [OsString::from("map-workspace-live-browser-verify")];
        assert_eq!(dispatcher_binary(&arguments).unwrap(), BROWSER_SMOKE);
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            [
                "build",
                "--locked",
                "--package",
                "veoveo-browser-smoke",
                "--bin",
                "browser-smoke",
            ]
        );
    }

    #[test]
    fn recording_browser_acceptance_uses_the_focused_harness() {
        let arguments = [OsString::from("uav-recording-browser-verify")];
        assert_eq!(dispatcher_binary(&arguments).unwrap(), BROWSER_SMOKE);
    }

    #[test]
    fn recording_archive_browser_acceptance_uses_the_focused_harness() {
        let arguments = [OsString::from("uav-recording-archive-browser-verify")];
        assert_eq!(dispatcher_binary(&arguments).unwrap(), BROWSER_SMOKE);
    }

    #[test]
    fn live_restart_acceptance_uses_the_focused_harness() {
        let arguments = [OsString::from("uav-showcase-live-restart-verify")];
        assert_eq!(dispatcher_binary(&arguments).unwrap(), BROWSER_SMOKE);
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            [
                "build",
                "--locked",
                "--package",
                "veoveo-browser-smoke",
                "--bin",
                "browser-smoke",
                "--package",
                "veoveo-mcp-conformance",
                "--bin",
                "conformance",
            ]
        );
    }

    #[test]
    fn one_cargo_invocation_builds_smoke_and_exact_prerequisites() {
        let arguments = [OsString::from("recording-ingest")];
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            [
                "build",
                "--locked",
                "--package",
                "veoveo-smoke",
                "--bin",
                "smoke",
                "--package",
                "veoveo-mcp-conformance",
                "--bin",
                "conformance",
                "--package",
                "veoveo-mcp-gateway",
                "--bin",
                "gateway",
                "--package",
                "veoveo-recording-hub",
                "--bin",
                "spooler",
            ]
        );
    }

    #[test]
    fn help_builds_only_the_dispatcher() {
        let arguments = [OsString::from("agent-gateway"), OsString::from("--help")];
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            [
                "build",
                "--locked",
                "--package",
                "veoveo-smoke",
                "--bin",
                "smoke",
            ]
        );
    }

    #[test]
    fn helm_configuration_uses_only_the_focused_deployment_harness() {
        let arguments = [OsString::from("helm-config")];
        assert_eq!(
            cargo_build_arguments(&arguments).unwrap(),
            [
                "build",
                "--locked",
                "--package",
                "veoveo-deployment-smoke",
                "--bin",
                "deployment-smoke",
            ]
        );
    }
}
