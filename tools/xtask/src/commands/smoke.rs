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
    let executable = target.join("debug/smoke");
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
    let mut binaries = vec![SMOKE];
    if !requests_help(arguments) {
        binaries.push(CONFORMANCE);
        let scenario = arguments
            .first()
            .context("smoke scenario is required")?
            .to_str()
            .context("smoke scenario is not valid UTF-8")?;
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
        | "uav-showcase-verify"
        | "simulation-view-verify" => &[CONFORMANCE],
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
        | "gpu-allocation-verify"
        | "bioma-verify"
        | "surreal-integration"
        | "view-mcp"
        | "view-google-live"
        | "sumo-push"
        | "simulation-certify"
        | "simulation-view-visual-compare"
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
            scenario_binaries("datasheet-mcp").unwrap(),
            &[CONFORMANCE, ARTIFACT_SERVICE]
        );
        assert_eq!(scenario_binaries("profile-up").unwrap(), &[]);
        assert_eq!(scenario_binaries("gpu-allocation-verify").unwrap(), &[]);
        assert!(scenario_binaries("unmapped-scenario").is_err());
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
    fn real_scenarios_keep_the_smoke_conformance_build_unit_stable() {
        let arguments = [OsString::from("helm-config")];
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
            ]
        );
    }
}
