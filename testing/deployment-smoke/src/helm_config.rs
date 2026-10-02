//! Configuration-only Helm and GitOps smoke assertions, shared with the full suite.
use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use veoveo_simulation_contract::SimulationRuntimeBuildLock;

#[path = "helm_config/agent_template.rs"]
mod agent_template;
#[path = "helm_config/bioma.rs"]
mod bioma;
#[path = "helm_config/commands.rs"]
mod commands;
#[path = "helm_config/gitops.rs"]
mod gitops;
#[path = "helm_config/jobs.rs"]
mod jobs;
#[path = "helm_config/object_store.rs"]
mod object_store;
use commands::{contains, not_contains, run_checked};

fn assert_revision_metadata_follows_payload(path: &str) -> Result<()> {
    let dockerfile =
        fs::read_to_string(path).with_context(|| format!("reading Dockerfile {path}"))?;
    let revision_argument = dockerfile
        .rfind("\nARG SOURCE_REVISION=")
        .with_context(|| format!("{path} has no SOURCE_REVISION build argument"))?;
    let last_payload_instruction = dockerfile
        .rfind("\nRUN ")
        .into_iter()
        .chain(dockerfile.rfind("\nCOPY "))
        .max()
        .with_context(|| format!("{path} has no payload-producing RUN or COPY instruction"))?;
    ensure!(
        revision_argument > last_payload_instruction,
        "{path} declares SOURCE_REVISION before its final payload instruction; changing an OCI \
         label revision would invalidate an unchanged image payload"
    );
    ensure!(
        dockerfile.matches("ARG SOURCE_REVISION=").count() == 1,
        "{path} must have one canonical SOURCE_REVISION argument"
    );
    contains(
        &dockerfile[revision_argument..],
        r#"org.opencontainers.image.revision="${SOURCE_REVISION}""#,
    )
    .with_context(|| format!("{path} must consume SOURCE_REVISION only in trailing metadata"))?;
    Ok(())
}

pub(crate) fn helm_config() -> Result<()> {
    bioma::check()?;
    jobs::check()?;
    object_store::check()?;
    for chart in [
        "deploy/helm/common",
        "deploy/helm/veoveo",
        "showcase/sumo/deploy/helm",
        "showcase/uav-sim/deploy/helm",
        "testing/fixtures/chart-library-consumer",
        "testing/fixtures/fork-workload/deploy/helm",
    ] {
        run_checked(Path::new("helm"), ["lint".into(), chart.into()], [])
            .with_context(|| format!("linting Helm chart {chart}"))?;
    }

    let workload = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "separate-workload-release".into(),
            "testing/fixtures/chart-library-consumer".into(),
            "--namespace".into(),
            "veoveo".into(),
        ],
        [],
    )?;
    for expected in [
        "app.kubernetes.io/instance: \"separate-workload-release\"",
        "veoveo.ai/installation: \"veoveo\"",
        "app.kubernetes.io/component: \"anonymous-mcp\"",
        "app.kubernetes.io/component: \"gateway\"",
        "app.kubernetes.io/component: \"artifact-service\"",
        "app.kubernetes.io/component: \"recording\"",
        "runAsUser: 10001",
        "readOnlyRootFilesystem: true",
        "name: VEOVEO_INTERNAL_TRUST_JWKS",
    ] {
        contains(&workload, expected)?;
    }
    not_contains(&workload, "app.kubernetes.io/instance: \"veoveo\"")?;
    let production_workload = Command::new("helm")
        .args([
            "template",
            "separate-workload-release",
            "testing/fixtures/chart-library-consumer",
            "--set",
            "veoveo.production=true",
        ])
        .output()
        .context("rendering the production workload fixture without an image digest")?;
    ensure!(
        !production_workload.status.success(),
        "production workload render must reject mutable image tags"
    );

    let fork_simulation = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "anonymous-simulation".into(),
            "testing/fixtures/fork-workload/deploy/helm".into(),
            "--namespace".into(),
            "veoveo".into(),
            "--values".into(),
            "testing/fixtures/fork-workload/deploy/helm/values.test.yaml".into(),
        ],
        [],
    )?;
    for expected in [
        "name: anonymous-simulation-mcp",
        "registry.example.internal/veoveo/anonymous-simulation-mcp@sha256:1111111111111111111111111111111111111111111111111111111111111111",
        "veoveo.ai/simulator-hosted-live-view: \"true\"",
        "name: ANONYMOUS_SIMULATION_PUBLIC_STREAM_URL",
        "value: \"wss://simulation.example/anonymous-simulation/live\"",
        "runAsUser: 10001",
        "readOnlyRootFilesystem: true",
        "port: 8812",
    ] {
        contains(&fork_simulation, expected)?;
    }
    for forbidden in [
        "nvidia.com/gpu",
        "runtimeClassName:",
        "simulation-view",
        "POSE_",
        "stream-media",
        "stream-signal",
        "name: camera",
    ] {
        not_contains(&fork_simulation, forbidden)?;
    }
    let fork_simulation_without_digest = Command::new("helm")
        .args([
            "template",
            "anonymous-simulation",
            "testing/fixtures/fork-workload/deploy/helm",
            "--set",
            "global.production=true",
        ])
        .output()
        .context("rendering the fork simulation chart without an image digest")?;
    ensure!(
        !fork_simulation_without_digest.status.success(),
        "production fork simulation render must reject mutable image tags"
    );

    let platform = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "veoveo".into(),
            "deploy/helm/veoveo".into(),
            "--values".into(),
            "testing/fixtures/platform-selection/platform-values.yaml".into(),
            "--namespace".into(),
            "veoveo".into(),
            "--values".into(),
            "testing/fixtures/platform-selection/full-values.yaml".into(),
            "--values".into(),
            "testing/fixtures/fork-installation/platform-values.yaml".into(),
        ],
        [],
    )?;
    recording_caches(&platform)?;
    for component in [
        "mcp-gateway",
        "artifact-mcp",
        "media-mcp",
        "stream-mcp",
        "reason-mcp",
        "timeseries-mcp",
        "duckdb-mcp",
        "optimization-mcp",
        "frames-mcp",
        "knowledge-mcp",
        "map-mcp",
        "view-mcp",
        "time-mcp",
        "datasheet-mcp",
        "chart-mcp",
        "rerun-bridge",
        "recording",
    ] {
        let deployment = platform
            .split("\n---\n")
            .find(|document| {
                document.contains("kind: Deployment")
                    && document.contains(&format!("name: {component}\n"))
            })
            .with_context(|| format!("finding rendered {component} deployment"))?;
        contains(deployment, "replicas: 1")?;
        if component == "mcp-gateway" {
            not_contains(deployment, "strategy:\n    type: Recreate")?;
        } else {
            contains(deployment, "strategy:\n    type: Recreate")?;
        }
        not_contains(deployment, "veoveo.ai/chart-revision")?;
        let required_driver_capabilities = match component {
            "reason-mcp" | "stream-mcp" => Some("compute,utility,video"),
            "optimization-mcp" => Some("compute,utility"),
            "view-mcp" | "rerun-bridge" => Some("graphics,compute,utility"),
            _ => None,
        };
        if let Some(capabilities) = required_driver_capabilities {
            contains(deployment, "nvidia.com/gpu: \"1\"")?;
            contains(deployment, "name: NVIDIA_DRIVER_CAPABILITIES")?;
            contains(deployment, &format!("value: {capabilities}"))?;
            not_contains(deployment, "name: NVIDIA_VISIBLE_DEVICES")?;
        }
        if component == "map-mcp" {
            contains(deployment, "startupProbe:")?;
            contains(deployment, "failureThreshold: 60")?;
        }
        if component == "optimization-mcp" {
            contains(deployment, "runtimeClassName: nvidia")?;
            contains(deployment, "name: cuopt-executor")?;
            contains(deployment, "name: VEOVEO_CUOPT_SOCKET")?;
            contains(deployment, "nvidia.com/gpu: \"1\"")?;
        }
        if component == "rerun-bridge" {
            contains(deployment, "runtimeClassName: nvidia")?;
            contains(deployment, "name: WGPU_BACKEND")?;
            contains(deployment, "value: vulkan")?;
        }
    }

    let gateway_deployment = platform
        .split("\n---\n")
        .find(|document| {
            document.contains("kind: Deployment") && document.contains("name: mcp-gateway\n")
        })
        .context("finding rendered mcp-gateway deployment")?;
    let gateway_service = platform
        .split("\n---\n")
        .find(|document| {
            document.contains("kind: Service") && document.contains("name: mcp-gateway\n")
        })
        .context("finding rendered mcp-gateway service")?;
    let recording_deployment = platform
        .split("\n---\n")
        .find(|document| {
            document.contains("kind: Deployment") && document.contains("name: recording\n")
        })
        .context("finding rendered recording deployment")?;
    let recording_service = platform
        .split("\n---\n")
        .find(|document| {
            document.contains("kind: Service") && document.contains("name: recording-mcp\n")
        })
        .context("finding rendered recording service")?;
    contains(
        recording_service,
        "traefik.ingress.kubernetes.io/service.serversscheme: h2c",
    )?;
    let redap_ingresses = platform
        .split("\n---\n")
        .filter(|document| document.contains("path: /rerun.cloud.v1alpha1.RerunCloudService"))
        .collect::<Vec<_>>();
    ensure!(
        redap_ingresses.len() == 1 && redap_ingresses[0].contains("kind: Ingress"),
        "native Redap must have one dedicated Ingress"
    );
    contains(redap_ingresses[0], "name: recording-mcp")?;
    contains(gateway_service, "sessionAffinity: ClientIP")?;
    contains(gateway_service, "timeoutSeconds: 10800")?;
    contains(gateway_deployment, "startupProbe:")?;
    contains(gateway_deployment, "failureThreshold: 24")?;
    contains(
        recording_deployment,
        "- name: recordings\n              mountPath: /recordings\n              readOnly: false\n            - name: recording-mcp-tmp",
    )
    .context("Recording MCP must remove Artifact-backed Blueprint staging files")?;
    for expected in [
        "image: surrealdb/surrealdb@sha256:681c6c22c287421b5c7d99e0fde79b6e0d32c36c1ddeaab2762a1661cb04cd20",
        "image: rustfs/rustfs@sha256:8cc9801755448b71a786705ce76692c77e14936cccd87cf2fc31842e58f4d1ff",
        "image: amazon/aws-cli:2.35.23",
        "name: mcp-gateway",
        "name: artifact-service",
        "name: recording-hub",
        "name: console-bff",
        "name: VEOVEO_CONSOLE_MCP_TRANSPORT_URL",
        "value: \"http://mcp-gateway:8788/mcp/admin\"",
        "value: \"operator:use admin:manage audit:read recording:seal uav-sim:admin uav-sim:stream map:admin map:dataset:read map:feature:admin map:feature:publish map:feature:read map:feature:write map:raster:derive map:spatial:derive time:read view:read view:write view:capture\"",
        "host: localhost",
        "path: /s",
        "mountPath: /etc/veoveo/gateway",
        "runAsUser: 65532",
        "runAsUser: 10001",
        "rtpjitterbuffer mode=none latency=50",
        "max-dropout-time=1000 max-misorder-time=100 faststart-min-packets=2",
    ] {
        contains(&platform, expected)?;
    }
    let stream_catalog = fs::read_to_string("configs/stream/catalog.example.json")?;
    contains(&stream_catalog, "rtpjitterbuffer mode=none latency=50")?;
    contains(
        &stream_catalog,
        "max-dropout-time=1000 max-misorder-time=100 faststart-min-packets=2",
    )?;
    for forbidden in [
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "name: renderer-control",
        "port: 9876",
    ] {
        if platform.contains(forbidden) {
            bail!("canonical Helm render must not contain `{forbidden}`");
        }
    }

    let console_ca = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "veoveo".into(),
            "deploy/helm/veoveo".into(),
            "--values".into(),
            "testing/fixtures/platform-selection/platform-values.yaml".into(),
            "--namespace".into(),
            "veoveo".into(),
            "--values".into(),
            "testing/fixtures/platform-selection/full-values.yaml".into(),
            "--values".into(),
            "testing/fixtures/fork-installation/platform-values.yaml".into(),
            "--set".into(),
            "consoleBff.outboundCa.existingConfigMap=corporate-ca".into(),
            "--set".into(),
            "consoleBff.outboundCa.key=roots.pem".into(),
        ],
        [],
    )?;
    let console_deployment = console_ca
        .split("\n---\n")
        .find(|document| {
            document.contains("kind: Deployment") && document.contains("name: console-bff\n")
        })
        .context("finding Console BFF deployment with installation CA")?;
    for expected in [
        "name: VEOVEO_CONSOLE_OUTBOUND_CA_BUNDLE",
        "value: /etc/veoveo/console-outbound-ca/ca.pem",
        "name: console-outbound-ca",
        "mountPath: /etc/veoveo/console-outbound-ca",
        "readOnly: true",
        "name: \"corporate-ca\"",
        "defaultMode: 0444",
        "key: \"roots.pem\"",
        "path: ca.pem",
    ] {
        contains(console_deployment, expected)?;
    }
    not_contains(console_deployment, "optional: true")?;

    let malformed_console_transport = Command::new("helm")
        .args([
            "template",
            "veoveo",
            "deploy/helm/veoveo",
            "--set",
            "consoleBff.mcpTransportUrl=relative/mcp/admin",
        ])
        .output()
        .context("rendering the platform chart with a malformed Console MCP transport")?;
    ensure!(
        !malformed_console_transport.status.success(),
        "Helm schema must reject a non-absolute Console MCP transport URL"
    );
    contains(
        &String::from_utf8_lossy(&malformed_console_transport.stderr),
        "/consoleBff/mcpTransportUrl",
    )?;

    let console_mapbox = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "veoveo".into(),
            "deploy/helm/veoveo".into(),
            "--values".into(),
            "testing/fixtures/platform-selection/platform-values.yaml".into(),
            "--set".into(),
            "consoleBff.rerunMap.provider=mapbox".into(),
            "--set".into(),
            "consoleBff.rerunMap.mapbox.accessToken.existingSecret=console-browser-map".into(),
            "--set".into(),
            "consoleBff.rerunMap.mapbox.accessToken.key=access-token".into(),
        ],
        [],
    )?;
    let console_deployment = console_mapbox
        .split("\n---\n")
        .find(|document| {
            document.contains("kind: Deployment") && document.contains("name: console-bff\n")
        })
        .context("finding Console BFF deployment with Mapbox configuration")?;
    for expected in [
        "name: VEOVEO_CONSOLE_RERUN_MAP_PROVIDER",
        "value: \"mapbox\"",
        "name: RERUN_MAPBOX_ACCESS_TOKEN",
        "name: \"console-browser-map\"",
        "key: \"access-token\"",
        "optional: true",
    ] {
        contains(console_deployment, expected)?;
    }
    not_contains(console_deployment, "pk.")?;

    let missing_console_mapbox_secret = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "veoveo".into(),
            "deploy/helm/veoveo".into(),
            "--values".into(),
            "testing/fixtures/platform-selection/platform-values.yaml".into(),
            "--set".into(),
            "consoleBff.rerunMap.provider=mapbox".into(),
        ],
        [],
    )?;
    let missing_secret_deployment = missing_console_mapbox_secret
        .split("\n---\n")
        .find(|document| {
            document.contains("kind: Deployment") && document.contains("name: console-bff\n")
        })
        .context("finding Console BFF deployment without a Mapbox Secret")?;
    contains(missing_secret_deployment, "value: \"mapbox\"")?;
    not_contains(missing_secret_deployment, "name: RERUN_MAPBOX_ACCESS_TOKEN")?;

    let sumo_cluster = fs::read_to_string("deploy/local/k3d/cluster.yaml")?;
    contains(&sumo_cluster, "name: veoveo-sumo")?;
    contains(&sumo_cluster, "127.0.0.1:8780:80")?;

    let registry: Value =
        serde_json::from_str(&fs::read_to_string("deploy/local/k3d/registry.json")?)?;
    ensure!(
        registry.get("schemaVersion").and_then(Value::as_str)
            == Some("veoveo.ai/local-registry/v1")
            && registry.get("name").and_then(Value::as_str) == Some("veoveo-registry.localhost")
            && registry
                .get("image")
                .and_then(Value::as_str)
                .is_some_and(|image| image.contains("registry:3.1.1@sha256:")),
        "local registry config must identify the shared immutable registry"
    );
    let sumo = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "sumo".into(),
            "showcase/sumo/deploy/helm".into(),
            "--namespace".into(),
            "veoveo".into(),
        ],
        [],
    )?;
    for expected in [
        "image: veoveo/sumo-sim:1.27.1",
        "image: veoveo/sumo-mcp:0.1.0",
        "image: veoveo/recording-forwarder:0.1.0",
        "nodePort: 30895",
        "value: sumo-mcp:8795",
        "http://mcp-gateway:8788/",
        "http://localhost:8780/ingest/recordings",
        "name: recording-producer-key",
        "name: sumo-recording-forwarder",
        "claimName: sumo-recording-forwarder",
        "runAsUser: 10001",
    ] {
        contains(&sumo, expected)?;
    }
    not_contains(&sumo, "veoveo.ai/chart-revision")?;
    if sumo.contains("tcpSocket:") {
        bail!("SUMO chart must not probe the single-client TraCI socket");
    }
    if sumo.contains("OTEL_EXPORTER_OTLP_ENDPOINT") {
        bail!("SUMO chart must not export telemetry when its profile disables telemetry");
    }

    let uav_dependencies: Value = serde_json::from_str(&fs::read_to_string(
        "showcase/uav-sim/dependencies.lock.json",
    )?)?;
    ensure!(
        uav_dependencies
            .pointer("/components/simulation_runtime/build_target")
            .and_then(Value::as_str)
            == Some("simulation-runtime")
            && uav_dependencies
                .pointer("/components/cesium_for_omniverse/version")
                .and_then(Value::as_str)
                == Some("0.29.0")
            && uav_dependencies
                .pointer("/components/px4_autopilot/version")
                .and_then(Value::as_str)
                == Some("1.17.0")
            && uav_dependencies
                .pointer("/components/google_photorealistic_3d_tiles/cesium_ion_asset_id")
                .and_then(Value::as_u64)
                == Some(2_275_207)
            && uav_dependencies
                .pointer("/components/google_photorealistic_3d_tiles/persistence")
                .and_then(Value::as_str)
                == Some("versioned_runtime_cache")
            && uav_dependencies
                .pointer("/components/oci_distribution_registry/version")
                .and_then(Value::as_str)
                == Some("3.1.1")
            && uav_dependencies
                .pointer("/components/python_runtime/lxml")
                .and_then(Value::as_str)
                == Some("6.0.2")
            && uav_dependencies
                .pointer("/components/rerun/version")
                .and_then(Value::as_str)
                == Some("0.38.1")
            && uav_dependencies
                .pointer("/components/python_runtime/rerun_sdk")
                .and_then(Value::as_str)
                == Some("0.38.1"),
        "UAV dependency lock omitted a canonical release or Google tiles identity"
    );
    let simulation_lock_bytes =
        fs::read("platform/runtimes/simulation/simulation-runtime.lock.json")?;
    let simulation_lock: SimulationRuntimeBuildLock =
        serde_json::from_slice(&simulation_lock_bytes)?;
    simulation_lock.validate()?;
    let simulation_lock_digest = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(&simulation_lock_bytes))
    );
    for identity_path in [
        "showcase/uav-sim/runtime/uav-overlay.identity.json",
        "testing/fixtures/simulation-overlay/identity.json",
    ] {
        let identity: Value = serde_json::from_slice(&fs::read(identity_path)?)?;
        ensure!(
            identity.get("baseLockDigest").and_then(Value::as_str) == Some(&simulation_lock_digest),
            "{identity_path} does not identify the current canonical simulation lock"
        );
    }
    let simulation_runtime_dockerfile =
        fs::read_to_string("platform/runtimes/simulation/Dockerfile")?;
    let overlay_dockerfiles = [
        "showcase/uav-sim/runtime/Dockerfile",
        "testing/fixtures/simulation-overlay/Dockerfile",
    ];
    let python_path_dockerfiles = [
        "showcase/uav-sim/runtime/Dockerfile.dependencies",
        "testing/fixtures/simulation-overlay/Dockerfile",
    ];
    for dockerfile in [
        "platform/runtimes/simulation/Dockerfile",
        overlay_dockerfiles[0],
        overlay_dockerfiles[1],
    ] {
        assert_revision_metadata_follows_payload(dockerfile)?;
    }
    for dockerfile in python_path_dockerfiles {
        let contents = fs::read_to_string(dockerfile)?;
        contains(&contents, "${PYTHONPATH}")?;
        for platform_root in [
            "/isaac-sim/extsDeprecated/omni.isaac.ml_archive/pip_prebundle",
            "/opt/veoveo/python",
            "/opt/veoveo/isaaclab/source/isaaclab",
        ] {
            not_contains(&contents, platform_root)?;
        }
    }
    for dockerfile in [
        "showcase/uav-sim/runtime/Dockerfile",
        "testing/fixtures/simulation-overlay/Dockerfile",
    ] {
        contains(
            &fs::read_to_string(dockerfile)?,
            &format!("ai.veoveo.simulation.base-lock=\"{simulation_lock_digest}\""),
        )?;
    }
    contains(
        &simulation_runtime_dockerfile,
        &format!(
            "{}  /opt/veoveo/simulation-base/simulation-runtime.lock.json",
            simulation_lock_digest.trim_start_matches("sha256:")
        ),
    )?;
    for expected in [
        "nvcr.io/nvidia/isaac-sim:6.1.0@sha256:",
        "ISAAC_LAB_REVISION=ae37b028ea415c91ea2bc32609efcd759ed2b974",
        "NEWTON_WHEEL_SHA256=b432b0db9ee963c1fe570b88952d913b9eb98c9ca190c58f80882f28dd0dd5d6",
        "--require-hashes",
        "sha256sum --check --strict",
        "/isaac-sim/exts/isaacsim.pip.newton/pip_prebundle",
        "VEOVEO_SIMULATION_RUNTIME_PROFILE=",
        "USER 10001:10001",
    ] {
        contains(&simulation_runtime_dockerfile, expected)?;
    }
    for probe in [
        "platform/runtimes/simulation/probes/identity.py",
        "platform/runtimes/simulation/probes/gpu.py",
    ] {
        ensure!(
            Path::new(probe).is_file(),
            "missing simulation runtime probe {probe}"
        );
    }
    let uav_runtime_dockerfile = format!(
        "{}\n{}",
        fs::read_to_string("showcase/uav-sim/runtime/Dockerfile.dependencies")?,
        fs::read_to_string("showcase/uav-sim/runtime/Dockerfile")?,
    );
    for expected in [
        "ARG SIMULATION_RUNTIME_IMAGE=veoveo/simulation-runtime:2026.08.0",
        "px4io/px4-dev:v1.17.0@sha256:",
        "PX4_COMMIT=d6f12ad1c4f70ad3230afd7d86e971421e02fef4",
        "cesium-0.29.0-preinstalled-vendor.patch",
        "lxml-6.0.2-cp312-cp312",
        "ARG RERUN_SDK_VERSION=0.38.1",
        "rerun-sdk==${RERUN_SDK_VERSION}",
        "FROM --platform=${TARGETPLATFORM} ${SIMULATION_RUNTIME_IMAGE} AS uav-overlay",
        "FROM uav-sim-dependencies AS runtime",
        "uav-overlay.identity.json",
        "org.opencontainers.image.revision=",
        "USER 10001:10001",
    ] {
        contains(&uav_runtime_dockerfile, expected)?;
    }
    let overlay = fs::read_to_string("showcase/uav-sim/runtime/Dockerfile")?;
    contains(
        &overlay,
        "docker/dockerfile:1.27.0@sha256:bde3983e9c939224420ddaf6b784cc30e09b035a4dea01f581230c50809f372e",
    )?;
    let runtime_overlay = overlay
        .split_once("FROM uav-sim-dependencies AS runtime")
        .context("UAV runtime stage is missing")?
        .1;
    let copies = runtime_overlay
        .lines()
        .filter(|line| line.starts_with("COPY "))
        .collect::<Vec<_>>();
    ensure!(
        copies.len() == 4
            && copies
                .iter()
                .all(|line| line.starts_with("COPY --link ") && !line.contains("--chmod")),
        "UAV runtime copies must retain BuildKit's independent-layer optimization; chmod belongs in the scratch entrypoint stage"
    );
    contains(
        &overlay,
        "FROM scratch AS entrypoint\nCOPY --chmod=0555 entrypoint.py /entrypoint.py",
    )?;
    contains(
        runtime_overlay,
        "COPY --link --from=entrypoint --chown=10001:10001 /entrypoint.py",
    )?;
    for removed in [
        "UAV_SIM_BASE_IMAGE",
        "ISAAC_SIM_IMAGE",
        "AS runtime-base",
        "pegasus.simulator",
    ] {
        not_contains(&uav_runtime_dockerfile, removed)?;
    }
    contains(
        &fs::read_to_string("platform/recordings/hub/Dockerfile")?,
        "ARG RERUN_VERSION=0.38.1",
    )?;
    let stdio_bridge_dockerfile = fs::read_to_string("mcp/bridges/stdio/Dockerfile")?;
    contains(&stdio_bridge_dockerfile, "ARG RERUN_VERSION=0.38.1")?;
    contains(&stdio_bridge_dockerfile, r#""rerun-sdk==${RERUN_VERSION}""#)?;
    contains(&stdio_bridge_dockerfile, "ARG PYARROW_VERSION=25.0.1")?;
    contains(&stdio_bridge_dockerfile, "rerun analytics disable")?;
    contains(
        &stdio_bridge_dockerfile,
        r#"rerun --version | grep -F "rerun-cli ${RERUN_VERSION} ""#,
    )?;
    for required in ["libegl1", "libvulkan1", "libx11-6", "libxext6"] {
        contains(&stdio_bridge_dockerfile, required)?;
    }
    not_contains(&stdio_bridge_dockerfile, "mesa-vulkan-drivers")?;
    not_contains(&stdio_bridge_dockerfile, "lavapipe")?;
    not_contains(&stdio_bridge_dockerfile, "releases/download")?;
    let cesium_patch = fs::read_to_string(
        "showcase/uav-sim/runtime/patches/cesium-0.29.0-preinstalled-vendor.patch",
    )?;
    contains(&cesium_patch, "metadata.version(\"lxml\")")?;
    contains(&cesium_patch, "never mutate a Kit installation")?;
    let uav_runtime = fs::read_to_string("showcase/uav-sim/runtime/veoveo_uav_sim/app.py")?;
    for expected in [
        "/CesiumServers/IonOfficial",
        "https://api.cesium.com/",
        "cesium_data.GetSelectedIonServerRel().SetTargets",
        "cesium_interface.on_stage_change(0)",
        "cesium_interface.on_update_frame(cesium_viewports, False)",
    ] {
        contains(&uav_runtime, expected)?;
    }
    let px4_commander = fs::read_to_string("showcase/uav-sim/runtime/veoveo_uav_sim/px4.py")?;
    for expected in [
        "udpin:127.0.0.1:{14_550 + self.instance}",
        "self._connection.clients.add((\"127.0.0.1\", 18_570 + self.instance))",
        "GCS_HEARTBEAT_INTERVAL_SECONDS = 1.0",
    ] {
        contains(&px4_commander, expected)?;
    }
    let gpu_device_plugin = fs::read_to_string("deploy/local/k3d/node/nvidia-device-plugin.yaml")?;
    contains(&gpu_device_plugin, "replicas: 8")?;
    contains(
        &gpu_device_plugin,
        "veoveo.ai/device-plugin-config: time-slicing-8",
    )?;

    let gateway_dockerfile = fs::read_to_string("platform/gateway/Dockerfile")?;
    not_contains(&gateway_dockerfile, "libduckdb")?;
    contains(
        &gateway_dockerfile,
        "COPY --from=veoveo-rust-artifacts /bin/gateway",
    )?;
    let uav_mcp_dockerfile = fs::read_to_string("servers/uav-sim-mcp/Dockerfile")?;
    contains(
        &uav_mcp_dockerfile,
        "--from=veoveo-rust-artifacts /bin/uav-sim-mcp",
    )?;
    for forbidden in ["@nvidia/ov-web-rtc", "WEBRTC_CLIENT_BUNDLE"] {
        not_contains(&uav_mcp_dockerfile, forbidden)?;
    }
    let view_mcp_dockerfile = fs::read_to_string("servers/view-mcp/Dockerfile")?;
    contains(
        &view_mcp_dockerfile,
        "NVIDIA_DRIVER_CAPABILITIES=graphics,compute,utility",
    )?;
    not_contains(&view_mcp_dockerfile, "NVIDIA_VISIBLE_DEVICES")?;
    let anonymous_simulation_adapter = fs::read_to_string(
        "testing/fixtures/fork-installation/Dockerfile.anonymous-simulation-mcp",
    )?;
    for expected in [
        "--locked",
        "--no-editable",
        "COPY sdk/python /src/sdk/python",
        "fork-workload/src src",
        "fork-workload/AGENTS.md",
        "fork-workload/DESIGN.md",
    ] {
        contains(&anonymous_simulation_adapter, expected)?;
    }
    not_contains(&anonymous_simulation_adapter, "veoveo-python-index")?;
    let workspace_builder = fs::read_to_string("tools/image-build/rust-workspace.Dockerfile")?;
    for forbidden in ["@nvidia/ov-web-rtc", "UAV_SIM_WEBRTC_CLIENT_BUNDLE"] {
        not_contains(&workspace_builder, forbidden)?;
    }
    let bake = fs::read_to_string("docker-bake.hcl")?;
    for expected in [
        "group \"platform-core\"",
        "group \"platform-full\"",
        "group \"domain-platform\"",
        "group \"simulation-platform\"",
        "group \"fork-workload-fixture\"",
        "group \"showcase-sumo-base\"",
        "group \"showcase-sumo\"",
        "group \"simulation-runtime\"",
        "group \"showcase-uav-sim\"",
        "group \"showcase-uav-sim-overlay-acceptance\"",
        "target \"simulation-runtime-payload\"",
        "target \"simulation-overlay-acceptance\"",
        "sumo-base = \"target:sumo-base\"",
        "simulation-runtime = \"target:simulation-runtime-payload\"",
        "veoveo-rust-artifacts = \"target:rust-trixie-artifacts\"",
        "veoveo-rust-artifacts = \"target:rust-bookworm-artifacts\"",
        "\"ai.veoveo.build.mode\"",
        "\"ai.veoveo.build.package\"",
        "\"ai.veoveo.build.binaries\"",
        "\"ai.veoveo.build.family\"",
        "VEOVEO_REGISTRY",
        "VEOVEO_IMAGE_TAG",
        "function \"registry_cache\"",
        "function \"registry_cache_export\"",
        "cache-from = registry_cache(\"uav-sim-runtime\")",
        "cache-to   = registry_cache_export(\"uav-sim-runtime\")",
    ] {
        contains(&bake, expected)?;
    }
    not_contains(&bake, "simulation-runtime = \"target:simulation-runtime\"")?;
    let image_orchestration = fs::read_to_string("tools/xtask/src/commands/image.rs")?;
    contains(&image_orchestration, "type=provenance,mode=max")?;
    contains(&image_orchestration, "type=sbom")?;
    ensure!(
        !Path::new("Justfile").exists(),
        "the retired Justfile command surface must not return"
    );
    let xtask = fs::read_to_string("tools/xtask/src/main.rs")?;
    for expected in [
        "Command::Smoke(args)",
        "ReleaseCommand::HelmCharts(args)",
        "ImageCommand::Build(selection)",
    ] {
        contains(&xtask, expected)?;
    }
    let smoke_dispatch = fs::read_to_string("tools/xtask/src/commands/smoke.rs")?;
    for expected in [
        "veoveo-smoke",
        "veoveo-mcp-conformance",
        "veoveo-duckdb-mcp",
        "veoveo-recording-hub",
        "veoveo-artifact-service",
        "scenario_binaries",
        ".args(arguments)",
    ] {
        contains(&smoke_dispatch, expected)?;
    }
    for forbidden in [
        "process::status(\"kubectl\"",
        "process::status(\"helm\"",
        "process::status(\"k3d\"",
        "process::status(\"docker\"",
        "Command::new(\"kubectl\"",
        "Command::new(\"helm\"",
        "Command::new(\"k3d\"",
        "Command::new(\"docker\"",
        "reqwest",
        "serde_json",
        "tokio",
        "retry",
        "evidence",
        "cleanup",
    ] {
        not_contains(&smoke_dispatch, forbidden)?;
    }
    for forbidden in [
        "k3d image import",
        "docker save",
        "bioma-build:",
        "profile-publish",
        "docker build -f servers/",
    ] {
        not_contains(&xtask, forbidden)?;
        not_contains(&smoke_dispatch, forbidden)?;
    }
    veoveo_deploy_runtime::profile_validate_working_tree(Path::new(
        "showcase/sumo/deploy/deployment.json",
    ))?;
    veoveo_deploy_runtime::profile_validate_working_tree(Path::new(
        "testing/fixtures/fork-installation/deployment.json",
    ))?;
    let uav_scenario: Value = serde_json::from_str(&fs::read_to_string(
        "showcase/uav-sim/scenarios/new-york-aerial.json",
    )?)?;
    ensure!(
        uav_scenario.get("schema").and_then(Value::as_str)
            == Some("veoveo.ai/uav-sim-acceptance/v12")
            && uav_scenario
                .pointer("/world/tree/frames/1/parent_transform/origin/latitude_degrees")
                .and_then(Value::as_f64)
                == Some(40.758)
            && uav_scenario
                .pointer("/world/tree/frames/1/parent_transform/origin/longitude_degrees")
                .and_then(Value::as_f64)
                == Some(-73.9855)
            && uav_scenario
                .pointer("/takeoff/relative_altitude_m")
                .and_then(Value::as_f64)
                == Some(197.0)
            && uav_scenario
                .pointer("/mission/speed_mps")
                .and_then(Value::as_f64)
                == Some(20.0)
            && uav_scenario
                .pointer("/mission/task_timeout_seconds")
                .and_then(Value::as_u64)
                == Some(1_800)
            && uav_scenario
                .pointer("/reason/maximum_frames")
                .and_then(Value::as_u64)
                == Some(6),
        "runtime-loaded UAV scenario omitted the canonical mission"
    );
    for dockerfile in [
        "agents/kernel/Dockerfile",
        "apps/console/bff/Dockerfile",
        "mcp/bridges/stdio/Dockerfile",
        "platform/artifacts/service/Dockerfile",
        "platform/gateway/Dockerfile",
        "platform/recordings/forwarder/Dockerfile",
        "platform/recordings/hub/Dockerfile",
        "servers/artifact-mcp/Dockerfile",
        "servers/frames-mcp/Dockerfile",
        "servers/duckdb-mcp/Dockerfile",
        "servers/map-mcp/Dockerfile",
        "servers/media-mcp/Dockerfile",
        "servers/optimization-mcp/Dockerfile",
        "servers/recording-mcp/Dockerfile",
        "servers/stream-mcp/Dockerfile",
        "servers/reason-mcp/Dockerfile",
        "servers/timeseries-mcp/Dockerfile",
        "servers/time-mcp/Dockerfile",
        "servers/uav-sim-mcp/Dockerfile",
        "servers/view-mcp/Dockerfile",
    ] {
        let contents = fs::read_to_string(dockerfile)?;
        contains(&contents, "--from=veoveo-rust-artifacts")
            .with_context(|| format!("{dockerfile} must consume the family artifact context"))?;
        not_contains(&contents, "cargo build")
            .with_context(|| format!("{dockerfile} must not compile Rust independently"))?;
        not_contains(&contents, "COPY agents ./agents")
            .with_context(|| format!("{dockerfile} must not copy the Cargo workspace"))?;
    }
    for expected in [
        "type=bind,source=.,target=/src,rw",
        "veoveo-source-freshness /src /target/.veoveo-inputs",
        "id=${VEOVEO_CARGO_CACHE_ID}-registry-v1",
        "id=${VEOVEO_CARGO_CACHE_ID}-git-v1",
        "id=${VEOVEO_TARGET_CACHE_ID}",
        "target=/usr/local/cargo/registry,sharing=locked",
        "target=/usr/local/cargo/git,sharing=locked",
        "VEOVEO_CARGO_PACKAGES",
        "VEOVEO_CARGO_BINARIES",
        "cargo_args=(build --release --locked)",
        "--features veoveo-recording-mcp/redap",
    ] {
        contains(&workspace_builder, expected)?;
    }
    not_contains(&workspace_builder, "--jobs 4")?;
    {
        let dockerfile = "showcase/sumo/sumo-mcp/Dockerfile";
        let contents = fs::read_to_string(dockerfile)?;
        contains(&contents, "id=${VEOVEO_CARGO_CACHE_ID}-registry-v1")?;
        contains(&contents, "id=${VEOVEO_CARGO_CACHE_ID}-git-v1")?;
        contains(&contents, "id=${VEOVEO_TARGET_CACHE_ID}")?;
        contains(&contents, "target=/usr/local/cargo/registry,sharing=locked")?;
        contains(&contents, "target=/usr/local/cargo/git,sharing=locked")?;
        not_contains(&contents, "--jobs 4")?;
    }

    let dockerignore = fs::read_to_string(".dockerignore")?;
    contains(&dockerignore, "docs")?;
    contains(&dockerignore, "**/.venv")?;
    contains(&dockerignore, "**/node_modules")?;
    contains(&dockerignore, "**/dist")?;

    println!("helm config smoke ok");
    Ok(())
}

fn recording_caches(rendered: &str) -> Result<()> {
    let objects = serde_yaml_ng::Deserializer::from_str(rendered)
        .map(Value::deserialize)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for server in ["stream", "reason"] {
        let name = format!("{server}-mcp");
        let claim = format!("{server}-recording-cache");
        let deployment = objects
            .iter()
            .find(|object| object["kind"] == "Deployment" && object["metadata"]["name"] == name)
            .with_context(|| format!("missing {name} deployment"))?;
        let spec = &deployment["spec"]["template"]["spec"];
        let container = spec["containers"]
            .as_array()
            .context("missing containers")?
            .iter()
            .find(|container| container["name"] == name)
            .context("missing server container")?;
        let args = container["args"].as_array().context("missing args")?;
        for (flag, value) in [
            ("--catalog-cache-dir", "/recording-cache"),
            ("--catalog-cache-managed-bytes", "8589934592"),
            ("--catalog-cache-minimum-free-bytes", "1073741824"),
        ] {
            ensure!(
                args.windows(2)
                    .any(|pair| pair[0] == flag && pair[1] == value),
                "{name} cache configuration missing {flag}"
            );
        }
        ensure!(
            container["volumeMounts"]
                .as_array()
                .context("missing mounts")?
                .iter()
                .any(|mount| mount["name"] == "recording-cache"
                    && mount["mountPath"] == "/recording-cache"
                    && mount["readOnly"] != true),
            "{name} cache mount is not writable"
        );
        ensure!(
            spec["volumes"]
                .as_array()
                .context("missing volumes")?
                .iter()
                .any(|volume| volume["name"] == "recording-cache"
                    && volume["persistentVolumeClaim"]["claimName"] == claim),
            "{name} does not retain its cache across pod replacement"
        );
        ensure!(
            objects
                .iter()
                .any(|object| object["kind"] == "PersistentVolumeClaim"
                    && object["metadata"]["name"] == claim
                    && object["spec"]["resources"]["requests"]["storage"] == "10Gi"),
            "{name} cache PVC is missing"
        );
    }
    Ok(())
}
