use anyhow::ensure;
use sha2::{Digest, Sha256};
use veoveo_extension_contract::SimulationRuntimeBuildLock;

use super::*;

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

pub(crate) async fn surreal_integration() -> Result<()> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let name = format!("veoveo-surreal-smoke-{}", uuid::Uuid::new_v4().simple());
    run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "--detach".into(),
            "--rm".into(),
            "--name".into(),
            name.clone().into(),
            "--publish".into(),
            format!("127.0.0.1:{port}:8000").into(),
            "--tmpfs".into(),
            "/data:rw,size=1073741824,uid=65532,gid=65532,mode=0700".into(),
            "surrealdb/surrealdb:v3.2.1".into(),
            "start".into(),
            "--bind".into(),
            "0.0.0.0:8000".into(),
            "--user".into(),
            "root".into(),
            "--pass".into(),
            "root".into(),
            "rocksdb:/data/veoveo.db".into(),
        ],
        [],
    )?;
    let _container = ContainerGuard::new(name);
    let ready_url = format!("http://127.0.0.1:{port}/ready");
    let mut ready = false;
    for _ in 0..120 {
        if http_ok(&ready_url).await? {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    if !ready {
        bail!("timed out waiting for SurrealDB 3.2.1 at {ready_url}");
    }

    let endpoint = format!("ws://127.0.0.1:{port}");
    let environment = [
        ("VEOVEO_SURREAL_INTEGRATION", "1".into()),
        ("VEOVEO_SURREAL_URL", endpoint.clone().into()),
        ("VEOVEO_SURREAL_ENDPOINT", endpoint.into()),
        ("VEOVEO_SURREAL_USER", "root".into()),
        ("VEOVEO_SURREAL_USERNAME", "root".into()),
        ("VEOVEO_SURREAL_PASSWORD", "root".into()),
    ];
    for (package, test) in [
        ("veoveo-platform-store", "surreal_integration"),
        ("veoveo-task-runtime", "surreal_integration"),
        ("veoveo-agent-runtime", "surreal_integration"),
        ("veoveo-mcp-gateway", "control_store"),
        ("veoveo-mcp-gateway", "gateway_state"),
        ("veoveo-media-mcp", "surreal_integration"),
    ] {
        println!("==> live SurrealDB test: {package}/{test}");
        run_checked(
            Path::new("cargo"),
            [
                "test".into(),
                "-p".into(),
                package.into(),
                "--test".into(),
                test.into(),
                "--".into(),
                "--nocapture".into(),
                "--test-threads=1".into(),
            ],
            environment.clone(),
        )?;
    }
    let _ = environment;
    println!("surreal integration smoke ok");
    Ok(())
}

pub(crate) async fn helm_config() -> Result<()> {
    for chart in [
        "deploy/helm/veoveo-extension",
        "deploy/helm/veoveo",
        "showcase/sumo/deploy/helm",
        "showcase/uav-sim/deploy/helm",
        "testing/fixtures/extension-helm-consumer",
        "testing/fixtures/external-simulation-extension/deploy/helm",
    ] {
        run_checked(Path::new("helm"), ["lint".into(), chart.into()], [])
            .with_context(|| format!("linting Helm chart {chart}"))?;
    }

    let extension = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "separate-extension-release".into(),
            "testing/fixtures/extension-helm-consumer".into(),
            "--namespace".into(),
            "veoveo".into(),
        ],
        [],
    )?;
    for expected in [
        "app.kubernetes.io/instance: \"separate-extension-release\"",
        "veoveo.ai/installation: \"veoveo\"",
        "app.kubernetes.io/component: \"anonymous-mcp\"",
        "app.kubernetes.io/component: \"gateway\"",
        "app.kubernetes.io/component: \"artifact-service\"",
        "app.kubernetes.io/component: \"recording\"",
        "runAsUser: 10001",
        "readOnlyRootFilesystem: true",
        "name: VEOVEO_INTERNAL_TRUST_JWKS",
    ] {
        contains(&extension, expected)?;
    }
    not_contains(&extension, "app.kubernetes.io/instance: \"veoveo\"")?;
    let production_extension = Command::new("helm")
        .args([
            "template",
            "separate-extension-release",
            "testing/fixtures/extension-helm-consumer",
            "--set",
            "veoveo.production=true",
        ])
        .output()
        .context("rendering the production extension fixture without an image digest")?;
    ensure!(
        !production_extension.status.success(),
        "production extension render must reject mutable image tags"
    );

    let external_simulation = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "anonymous-simulation".into(),
            "testing/fixtures/external-simulation-extension/deploy/helm".into(),
            "--namespace".into(),
            "veoveo".into(),
            "--values".into(),
            "testing/fixtures/external-simulation-extension/deploy/helm/values.test.yaml".into(),
        ],
        [],
    )?;
    for expected in [
        "name: anonymous-simulation-mcp",
        "registry.example.internal/extensions/anonymous-simulation-mcp@sha256:1111111111111111111111111111111111111111111111111111111111111111",
        "veoveo.ai/simulator-hosted-live-view: \"true\"",
        "name: ANONYMOUS_SIMULATION_PUBLIC_SIGNALING_URL",
        "value: \"wss://simulation.example/anonymous-simulation/signaling\"",
        "name: ANONYMOUS_SIMULATION_PUBLIC_MEDIA_HOST",
        "value: \"192.0.2.10\"",
        "runAsUser: 10001",
        "readOnlyRootFilesystem: true",
        "port: 8812",
        "port: 48030",
    ] {
        contains(&external_simulation, expected)?;
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
        not_contains(&external_simulation, forbidden)?;
    }
    let external_simulation_without_digest = Command::new("helm")
        .args([
            "template",
            "anonymous-simulation",
            "testing/fixtures/external-simulation-extension/deploy/helm",
            "--set",
            "veoveo.production=true",
        ])
        .output()
        .context("rendering the external simulation chart without an image digest")?;
    ensure!(
        !external_simulation_without_digest.status.success(),
        "production external simulation render must reject mutable image tags"
    );

    let platform = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "veoveo".into(),
            "deploy/helm/veoveo".into(),
            "--namespace".into(),
            "veoveo".into(),
            "--values".into(),
            "deploy/local/k3d/values.yaml".into(),
            "--values".into(),
            "showcase/sumo/deploy/platform-values.yaml".into(),
        ],
        [],
    )?;
    for expected in [
        "image: surrealdb/surrealdb:v3.2.1",
        "image: rustfs/rustfs:1.0.0-beta.8",
        "image: amazon/aws-cli:2.35.23",
        "name: mcp-gateway",
        "name: artifact-service",
        "name: recording-hub",
        "name: console-bff",
        "name: VEOVEO_CONSOLE_MCP_TRANSPORT_URL",
        "value: \"http://mcp-gateway:8788/mcp/admin\"",
        "value: \"operator:use admin:manage uav-sim:read uav-sim:write uav-sim:stream map:admin map:dataset:read map:feature:admin map:feature:publish map:feature:read map:feature:write map:raster:derive map:spatial:derive time:read view:read view:write view:capture\"",
        "host: localhost",
        "path: /s",
        "mountPath: /etc/veoveo/gateway",
        "runAsUser: 65532",
        "runAsUser: 10001",
    ] {
        contains(&platform, expected)?;
    }
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
            "--namespace".into(),
            "veoveo".into(),
            "--values".into(),
            "deploy/local/k3d/values.yaml".into(),
            "--values".into(),
            "showcase/sumo/deploy/platform-values.yaml".into(),
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

    let bioma = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "bioma".into(),
            "deploy/helm/veoveo".into(),
            "--namespace".into(),
            "veoveo".into(),
            "--values".into(),
            "examples/bioma/values.yaml".into(),
            "--values".into(),
            "examples/bioma/k3d-values.yaml".into(),
            "--values".into(),
            "examples/bioma/images.lock.yaml".into(),
        ],
        [],
    )?;
    for expected in [
        "host: veoveo.bioma.ai",
        "https://veoveo.bioma.ai",
        "name: bioma-gateway-control-plane",
        "name: recording-hub",
        "name: view-mcp",
        "name: stream-mcp",
        "name: reason-mcp",
        "value: \"artifact,media,timeseries,optimization,duckdb,frames,map,recording,stream,reason,datasheet,uav-sim\"",
        "checksum/reason-runtime:",
    ] {
        contains(&bioma, expected)?;
    }
    for forbidden in [
        "objects-veoveo",
        "objects.veoveo",
        "ARTIFACT_S3_PUBLIC_ENDPOINT",
        "objectStoreHost",
        "publicEndpoint",
        "name: NVIDIA_VISIBLE_DEVICES",
    ] {
        not_contains(&bioma, forbidden)?;
    }
    not_contains(&bioma, "name: frames-mcp-bootstrap")?;
    not_contains(&bioma, "frames://frame/")?;
    let control_plane = fs::read("examples/bioma/gateway.json")?;
    let control_plane_revision = hex::encode(Sha256::digest(control_plane));
    let bioma_values = fs::read_to_string("examples/bioma/values.yaml")?;
    contains(
        &bioma_values,
        &format!("controlPlaneRevision: {control_plane_revision}"),
    )?;
    contains(
        &bioma,
        &format!("checksum/control-plane: \"{control_plane_revision}\""),
    )?;
    contains(&bioma, "veoveo.ai/bootstrap-revision:")?;
    not_contains(&bioma, "veoveo.ai/bootstrap-revision: \"bootstrap-1\"")?;
    for forbidden in ["name: otel-collector", "secretName: bioma-ingress-tls"] {
        if bioma.contains(forbidden) {
            bail!("Bioma k3d render must not contain `{forbidden}`");
        }
    }
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
        "map-mcp",
        "view-mcp",
        "time-mcp",
        "datasheet-mcp",
        "chart-mcp",
        "rerun-bridge",
        "recording",
    ] {
        let deployment = bioma
            .split("\n---\n")
            .find(|document| {
                document.contains("kind: Deployment")
                    && document.contains(&format!("name: {component}\n"))
            })
            .with_context(|| format!("finding rendered {component} deployment"))?;
        contains(deployment, "replicas: 1")?;
        contains(deployment, "strategy:\n    type: Recreate")?;
        contains(deployment, "veoveo.ai/chart-revision: \"0.1.0\"")?;
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
    let bioma_tunnel = fs::read_to_string("examples/bioma/gitops/cloudflared.yaml")?;
    contains(&bioma_tunnel, "name: TUNNEL_TOKEN")?;
    for forbidden in ["--token", "$(TUNNEL_TOKEN)"] {
        if bioma_tunnel.contains(forbidden) {
            bail!("Bioma tunnel must not expose its token through `{forbidden}`");
        }
    }

    let bioma_lan = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "bioma".into(),
            "deploy/helm/veoveo".into(),
            "--namespace".into(),
            "veoveo".into(),
            "--values".into(),
            "examples/bioma/values.yaml".into(),
            "--values".into(),
            "examples/bioma/k3d-values.yaml".into(),
            "--values".into(),
            "examples/bioma/lan-values.yaml".into(),
            "--values".into(),
            "examples/bioma/images.lock.yaml".into(),
        ],
        [],
    )?;
    contains(&bioma_lan, "secretName: bioma-lan-ingress-tls")?;
    contains(&bioma_lan, "host: veoveo.bioma.ai")?;

    let uav_sim = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "uav-sim".into(),
            "showcase/uav-sim/deploy/helm".into(),
            "--namespace".into(),
            "veoveo".into(),
            "--values".into(),
            "examples/bioma/uav-sim-values.yaml".into(),
            "--values".into(),
            "examples/bioma/images.lock.yaml".into(),
        ],
        [],
    )?;
    for expected in [
        "name: uav-sim-mcp",
        "name: isaac-sim",
        "image: k3d-veoveo-registry.localhost:5000/veoveo/uav-sim-runtime@sha256:",
        "image: k3d-veoveo-registry.localhost:5000/veoveo/uav-sim-mcp@sha256:",
        "runtimeClassName: nvidia",
        "name: CESIUM_ION_ACCESS_TOKEN",
        "name: veoveo-uav-sim-secrets",
        "key: cesium-ion-access-token",
        "name: UAV_SIM_CESIUM_ION_ASSET_ID",
        "value: \"2275207\"",
        "name: UAV_SIM_TILE_CACHE_POLICY",
        "value: \"persistent\"",
        "name: XDG_CACHE_HOME",
        "/var/lib/veoveo/runtime-cache/isaac-6.0.1-cesium-0.29.0-v1",
        "mountPath: /isaac-sim/kit/cache",
        "mountPath: /isaac-sim/kit/data",
        "kind: PersistentVolumeClaim",
        "name: uav-sim-runtime-cache",
        "claimName: uav-sim-runtime-cache",
        "name: uav-sim-recording-forwarder",
        "claimName: uav-sim-recording-forwarder",
        "image: k3d-veoveo-registry.localhost:5000/veoveo/recording-forwarder@sha256:",
        "http://mcp-gateway:8788/",
        "name: UAV_SIM_CAMERA_FOCAL_LENGTH_MM",
        "value: \"8\"",
        "name: UAV_SIM_CAMERA_ORIENTATION_W",
        "value: \"0.7071067811865476\"",
        "name: UAV_SIM_RECORDING_TENANT_KEY",
        "value: \"bioma\"",
        "veoveo.ai/simulator-hosted-live-view: \"true\"",
        "name: UAV_SIM_OPERATOR_CAMERAS_JSON",
        "name: UAV_SIM_LIVE_SIGNALING_PORT_BASE",
        "name: UAV_SIM_LIVE_MEDIA_PORT_BASE",
        "name: UAV_SIM_PUBLIC_SIGNALING_URL",
        "value: \"wss://veoveo.bioma.ai/uav-sim/signaling\"",
        "name: UAV_SIM_NATIVE_SIGNALING_URL",
        "value: \"ws://127.0.0.1:49100/webrtc\"",
        "name: UAV_SIM_LIVE_VIEW_MAXIMUM_VIEWERS",
        "name: uav-sim-media",
        "name: uav-sim-signaling",
        "name: ROS_DISTRO",
        "value: jazzy",
        "name: RMW_IMPLEMENTATION",
        "value: rmw_fastrtps_cpp",
        "name: LD_LIBRARY_PATH",
        "value: /isaac-sim/exts/isaacsim.ros2.core/jazzy/lib",
        "http://127.0.0.1:8810/healthz",
        "http://127.0.0.1:8810/readyz",
        "nvidia.com/gpu: 1",
        "veoveo.ai/chart-revision: \"0.1.0\"",
    ] {
        contains(&uav_sim, expected)?;
    }
    for forbidden in [
        "GOOGLE_MAPS_API_KEY",
        "UAV_SIM_POSE_",
        "simulation-view",
        "name: uav-sim-live",
        "path: /webrtc",
        "name: stream-signal",
        "name: stream-media",
    ] {
        if uav_sim.contains(forbidden) {
            bail!("UAV simulation render must not contain `{forbidden}`");
        }
    }
    ensure!(
        uav_sim.matches("name: CESIUM_ION_ACCESS_TOKEN").count() == 1,
        "interactive UAV render must inject the Cesium ion token exactly once"
    );

    let production_without_digests = Command::new("helm")
        .args([
            "template",
            "uav-sim",
            "showcase/uav-sim/deploy/helm",
            "--values",
            "examples/bioma/uav-sim-values.yaml",
            "--set",
            "global.production=true",
        ])
        .output()
        .context("rendering the production UAV chart without image digests")?;
    ensure!(
        !production_without_digests.status.success(),
        "production UAV render must reject mutable image tags"
    );

    let sumo_cluster = fs::read_to_string("deploy/local/k3d/cluster.yaml")?;
    contains(&sumo_cluster, "name: veoveo-sumo")?;
    contains(&sumo_cluster, "127.0.0.1:8780:80")?;

    let bioma_cluster = fs::read_to_string("examples/bioma/k3d.yaml")?;
    contains(&bioma_cluster, "name: veoveo-bioma")?;
    contains(&bioma_cluster, "127.0.0.1:8781:80")?;
    contains(&bioma_cluster, "k3d-veoveo-registry.localhost:5001")?;
    not_contains(&bioma_cluster, "create:")?;
    let registry: Value =
        serde_json::from_str(&fs::read_to_string("deploy/local/k3d/registry.json")?)?;
    ensure!(
        registry.get("schemaVersion").and_then(Value::as_str)
            == Some("veoveo.io/local-registry/v1")
            && registry.get("name").and_then(Value::as_str) == Some("veoveo-registry.localhost")
            && registry
                .get("image")
                .and_then(Value::as_str)
                .is_some_and(|image| image.contains("registry:3.1.1@sha256:")),
        "local registry config must identify the shared immutable registry"
    );
    let tunnel: Value = serde_json::from_str(&fs::read_to_string(
        "examples/bioma/cloudflare-tunnel.json",
    )?)?;
    let ingress = tunnel
        .pointer("/config/ingress")
        .and_then(Value::as_array)
        .context("Bioma Cloudflare configuration omitted ingress")?;
    ensure!(
        ingress.iter().any(|route| {
            route.get("hostname").and_then(Value::as_str) == Some("veoveo.bioma.ai")
                && route.get("service").and_then(Value::as_str)
                    == Some("http://traefik.kube-system.svc.cluster.local:80")
        }),
        "Bioma tunnel must route the public hostname to in-cluster Traefik"
    );
    let public_hosts = ingress
        .iter()
        .filter_map(|route| route.get("hostname").and_then(Value::as_str))
        .collect::<Vec<_>>();
    ensure!(
        public_hosts == ["veoveo.bioma.ai"],
        "Bioma tunnel must declare exactly one public hostname: {public_hosts:?}"
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
        "veoveo.ai/chart-revision: \"0.1.0\"",
    ] {
        contains(&sumo, expected)?;
    }
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
            .pointer("/components/simulation_runtime/compatibility_release")
            .and_then(Value::as_str)
            == Some("2026.07.0")
            && uav_dependencies
                .pointer("/components/simulation_runtime/build_target")
                .and_then(Value::as_str)
                == Some("simulation-runtime")
            && uav_dependencies
                .pointer("/components/cesium_for_omniverse/version")
                .and_then(Value::as_str)
                == Some("0.29.0")
            && uav_dependencies
                .pointer("/components/pegasus_simulator/version")
                .and_then(Value::as_str)
                == Some("5.1.0")
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
                == Some("0.35.0")
            && uav_dependencies
                .pointer("/components/python_runtime/rerun_sdk")
                .and_then(Value::as_str)
                == Some("0.35.0"),
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
    for dockerfile in [
        "platform/runtimes/simulation/Dockerfile",
        overlay_dockerfiles[0],
        overlay_dockerfiles[1],
    ] {
        assert_revision_metadata_follows_payload(dockerfile)?;
    }
    for dockerfile in overlay_dockerfiles {
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
            &format!("io.veoveo.simulation.base-lock=\"{simulation_lock_digest}\""),
        )?;
    }
    for expected in [
        "nvcr.io/nvidia/isaac-sim:6.0.1@sha256:",
        "ISAAC_LAB_REVISION=ffff603eafc6b74264a5261cc0183d6a65390d78",
        "WARP_WHEEL_SHA256=95c169f28bd7d6c78ac4ad62e2df1e61a096033748f757157fa4551aed80d010",
        "NEWTON_WHEEL_SHA256=0e11343cc51b86647d9afcd191a21ca4d0d5e410d84072a60ef84af908c72577",
        "--require-hashes",
        "sha256sum --check --strict",
        "/isaac-sim/extscache/omni.warp.core-1.13.0+lx64",
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
    let uav_runtime_dockerfile = fs::read_to_string("showcase/uav-sim/runtime/Dockerfile")?;
    for expected in [
        "ARG SIMULATION_RUNTIME_IMAGE=veoveo/simulation-runtime:2026.07.0",
        "px4io/px4-dev:v1.17.0@sha256:",
        "PX4_COMMIT=d6f12ad1c4f70ad3230afd7d86e971421e02fef4",
        "PEGASUS_COMMIT=644da37e9d5268e5f9a34e78bdcfd57a8bab82b4",
        "CESIUM_VERSION=0.29.0",
        "sha256sum --check --strict",
        "cesium-0.29.0-preinstalled-vendor.patch",
        "lxml-6.0.2-cp312-cp312",
        "git -C pegasus apply --unidiff-zero --check",
        "ARG RERUN_SDK_VERSION=0.35.0",
        "rerun-sdk==${RERUN_SDK_VERSION}",
        "FROM --platform=${TARGETPLATFORM} ${SIMULATION_RUNTIME_IMAGE} AS uav-overlay",
        "FROM uav-overlay AS runtime",
        "uav-overlay.identity.json",
        "org.opencontainers.image.revision=",
        "USER 10001:10001",
    ] {
        contains(&uav_runtime_dockerfile, expected)?;
    }
    for removed in ["UAV_SIM_BASE_IMAGE", "ISAAC_SIM_IMAGE", "AS runtime-base"] {
        not_contains(&uav_runtime_dockerfile, removed)?;
    }
    contains(
        &fs::read_to_string("platform/recordings/hub/Dockerfile")?,
        "rerun-sdk==0.35.0",
    )?;
    let stdio_bridge_dockerfile = fs::read_to_string("mcp/bridges/stdio/Dockerfile")?;
    contains(&stdio_bridge_dockerfile, "ARG RERUN_VERSION=0.35.0")?;
    contains(
        &stdio_bridge_dockerfile,
        r#"rerun --version | grep -F "rerun-cli ${RERUN_VERSION} ""#,
    )?;
    for required in ["libegl1", "libvulkan1", "libx11-6", "libxext6"] {
        contains(&stdio_bridge_dockerfile, required)?;
    }
    not_contains(&stdio_bridge_dockerfile, "mesa-vulkan-drivers")?;
    not_contains(&stdio_bridge_dockerfile, "lavapipe")?;
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
    contains(&gpu_device_plugin, "replicas: 6")?;
    contains(
        &gpu_device_plugin,
        "veoveo.ai/device-plugin-config: time-slicing-7",
    )?;

    let gateway_dockerfile = fs::read_to_string("platform/gateway/Dockerfile")?;
    contains(
        &gateway_dockerfile,
        "COPY --from=veoveo-rust-artifacts /lib/libduckdb.so",
    )?;
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
        "testing/fixtures/external-simulation-installation/Dockerfile.anonymous-simulation-mcp",
    )?;
    for expected in [
        "--locked",
        "--no-emit-package veoveo-mcp",
        "--require-hashes",
        "sdk/python/src/veoveo_mcp",
        "external-simulation-extension/src/anonymous_simulation_mcp",
        "external-simulation-extension/AGENTS.md",
        "external-simulation-extension/DESIGN.md",
    ] {
        contains(&anonymous_simulation_adapter, expected)?;
    }
    not_contains(&anonymous_simulation_adapter, "veoveo-python-index")?;
    let workspace_builder = fs::read_to_string("tools/image-build/rust-workspace.Dockerfile")?;
    for expected in [
        "@nvidia/ov-web-rtc-6.6.0.tgz",
        "77be78cd4799f797d320d386461834737f5a8368deacfb3b27ae26612f39c9a5",
        "UAV_SIM_WEBRTC_CLIENT_BUNDLE=",
    ] {
        contains(&workspace_builder, expected)?;
    }
    let bake = fs::read_to_string("docker-bake.hcl")?;
    for expected in [
        "group \"platform-core\"",
        "group \"platform-full\"",
        "group \"external-extension-platform\"",
        "group \"external-simulation-platform\"",
        "group \"external-simulation-extension-fixture\"",
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
        "\"io.veoveo.build.mode\"",
        "\"io.veoveo.build.package\"",
        "\"io.veoveo.build.binaries\"",
        "\"io.veoveo.build.family\"",
        "VEOVEO_REGISTRY",
        "VEOVEO_IMAGE_TAG",
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
    ensure!(
        !Path::new("examples/bioma/deployment.json").exists(),
        "Bioma must use its enterprise GitOps contract rather than a deployment profile"
    );
    crate::deployment::profile_validate(Path::new("showcase/sumo/deploy/deployment.json"))?;
    crate::deployment::profile_validate(Path::new(
        "testing/fixtures/external-simulation-installation/deployment.json",
    ))?;
    let bioma_root = fs::read_to_string("examples/bioma/gitops/bootstrap.yaml")?;
    for expected in [
        "kind: Application",
        "repoURL: https://github.com/BiomaAI/veoveo.git",
        "path: examples/bioma",
        "ServerSideApply=true",
    ] {
        contains(&bioma_root, expected)?;
    }
    let bioma_platform = fs::read_to_string("examples/bioma/platform/argocd/kustomization.yaml")?;
    contains(
        &bioma_platform,
        "argoproj/argo-cd/v3.4.5/manifests/install.yaml",
    )?;
    let mut chart_revision = None;
    let mut configuration_revision = None;
    for application in [
        "examples/bioma/gitops/applications/veoveo.yaml",
        "examples/bioma/gitops/applications/uav-sim.yaml",
    ] {
        let application = fs::read_to_string(application)?;
        contains(
            &application,
            "charts-registry.argocd.svc.cluster.local/charts",
        )?;
        contains(
            &application,
            "$configuration/examples/bioma/images.lock.yaml",
        )?;
        let revision = application
            .lines()
            .find_map(|line| {
                line.trim()
                    .strip_prefix("targetRevision: ")
                    .filter(|value| value.starts_with("0.1.0-"))
            })
            .context("Bioma application omitted its immutable chart revision")?
            .to_owned();
        if let Some(expected) = &chart_revision {
            ensure!(
                &revision == expected,
                "Bioma applications must use one chart revision: {expected} != {revision}"
            );
        } else {
            chart_revision = Some(revision);
        }
        let configuration_revision_value = application
            .lines()
            .filter_map(|line| line.trim().strip_prefix("targetRevision: "))
            .find(|value| value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .context("Bioma application omitted its immutable configuration revision")?
            .to_owned();
        if let Some(expected) = &configuration_revision {
            ensure!(
                &configuration_revision_value == expected,
                "Bioma applications must use one configuration revision: \
                 {expected} != {configuration_revision_value}"
            );
        } else {
            configuration_revision = Some(configuration_revision_value);
        }
        not_contains(&application, "targetRevision: main")?;
        not_contains(&application, "ServerSideApply=true")?;
    }
    let uav_scenario: Value = serde_json::from_str(&fs::read_to_string(
        "showcase/uav-sim/scenarios/new-york-aerial.json",
    )?)?;
    ensure!(
        uav_scenario.get("schema").and_then(Value::as_str) == Some("veoveo.uav-sim-acceptance/v9")
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
                == Some(300.0)
            && uav_scenario
                .pointer("/mission/speed_mps")
                .and_then(Value::as_f64)
                == Some(3.0)
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
        "type=bind,source=.,target=/src,readonly",
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
    for dockerfile in [
        "servers/stream-mcp/Dockerfile",
        "servers/reason-mcp/Dockerfile",
        "showcase/sumo/sumo-mcp/Dockerfile",
    ] {
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

pub(crate) async fn gateway_platform_store(gateway: &Path, control_plane: &Path) -> Result<()> {
    assert_executable(gateway)?;
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let platform_store = spawn_gateway_platform_store(gateway, control_plane).await?;
    let validate = run_checked(
        gateway,
        ["control-plane-validate".into()],
        platform_store.runtime_env(),
    )?;
    contains(&validate, "ok: revision")?;
    contains(&validate, "1 server(s), 2 profile(s)")?;

    cleanup.remove_on_drop();
    println!("gateway platform store smoke ok");
    Ok(())
}

pub(crate) fn contract_schemas(conformance: &Path) -> Result<()> {
    assert_executable(conformance)?;
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());
    let schemas = tmpdir.join("schemas");

    run_checked(
        conformance,
        [
            "contract-schemas".into(),
            "--output-dir".into(),
            schemas.as_os_str().to_os_string(),
        ],
        [],
    )?;

    assert_schema_title(
        &schemas.join("gateway-control-plane.schema.json"),
        "GatewayControlPlane",
    )?;
    let control_plane_revision = assert_schema_title(
        &schemas.join("gateway-control-plane-revision.schema.json"),
        "GatewayControlPlaneRevision",
    )?;
    for property in ["revision_id", "sha256", "source", "control_plane"] {
        if !control_plane_revision
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("control-plane revision schema has no object `{property}` property");
        }
    }
    assert_schema_title(
        &schemas.join("gateway-server-fragment.schema.json"),
        "GatewayServerFragment",
    )?;
    assert_schema_title(
        &schemas.join("gateway-binding.schema.json"),
        "GatewayBinding",
    )?;
    assert_schema_title(
        &schemas.join("gateway-composition-provenance.schema.json"),
        "GatewayCompositionProvenance",
    )?;
    assert_schema_title(
        &schemas.join("resource-authorization-server.schema.json"),
        "ResourceAuthorizationServer",
    )?;
    assert_schema_title(
        &schemas.join("oauth-client-registration.schema.json"),
        "OAuthClientRegistration",
    )?;
    assert_schema_title(
        &schemas.join("gateway-resource-subscription.schema.json"),
        "GatewayResourceSubscription",
    )?;
    assert_schema_title(
        &schemas.join("gateway-internal-identity.schema.json"),
        "GatewayInternalIdentity",
    )?;
    assert_schema_title(
        &schemas.join("principal-audit-attributes.schema.json"),
        "PrincipalAuditAttributes",
    )?;
    assert_schema_title(
        &schemas.join("data-label-definition.schema.json"),
        "DataLabelDefinition",
    )?;
    assert_schema_title(
        &schemas.join("tenant-definition.schema.json"),
        "TenantDefinition",
    )?;
    let auth_audit = assert_schema_title(
        &schemas.join("auth-audit-event.schema.json"),
        "AuthAuditEvent",
    )?;
    for property in ["outcome", "reason", "method", "protected_resource"] {
        if !auth_audit
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("auth audit schema has no object `{property}` property");
        }
    }
    let deployment = assert_schema_title(
        &schemas.join("self-hosted-deployment-plan.schema.json"),
        "SelfHostedDeploymentPlan",
    )?;
    if !deployment
        .get("properties")
        .and_then(|properties| properties.get("profiles"))
        .is_some_and(Value::is_object)
    {
        bail!("deployment plan schema has no object profiles property");
    }
    let deployment_profile = assert_schema_title(
        &schemas.join("self-hosted-deployment-profile.schema.json"),
        "SelfHostedDeploymentProfile",
    )?;
    for property in [
        "service_to_service",
        "platform_store",
        "analytical_runtime",
        "telemetry",
        "tenant_model",
    ] {
        if !deployment_profile
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("deployment profile schema has no object `{property}` property");
        }
    }
    let platform_store = assert_schema_title(
        &schemas.join("platform-store-deployment.schema.json"),
        "PlatformStoreDeployment",
    )?;
    for property in [
        "engine",
        "version",
        "storage_engine",
        "topology",
        "database_ha",
        "changefeed_source_of_truth",
    ] {
        if !platform_store
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("platform store schema has no object `{property}` property");
        }
    }
    let artifact = assert_schema_title(
        &schemas.join("artifact-metadata.schema.json"),
        "ArtifactMetadata",
    )?;
    if !artifact
        .get("properties")
        .and_then(|properties| properties.get("compliance"))
        .is_some_and(Value::is_object)
    {
        bail!("artifact metadata schema has no object compliance property");
    }
    assert_schema_title(
        &schemas.join("coordinate-operation-provenance.schema.json"),
        "CoordinateOperationProvenance",
    )?;
    let usage = assert_schema_title(&schemas.join("usage-report.schema.json"), "UsageReport")?;
    if !usage
        .get("properties")
        .and_then(|properties| properties.get("records"))
        .is_some_and(Value::is_object)
    {
        bail!("usage report schema has no object records property");
    }

    cleanup.remove_on_drop();
    println!("contract schemas smoke ok");
    Ok(())
}

pub(crate) async fn otel(conformance: &Path, gateway: &Path, control_plane: &Path) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(gateway)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let gateway_port = 18804u16;
    let otlp_port = 18805u16;
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let otlp_base = format!("http://127.0.0.1:{otlp_port}");
    let gateway_log = tmpdir.join("gateway.log");
    let otlp_log = tmpdir.join("otlp.log");
    let otlp_ready = tmpdir.join("otlp.ready");
    let otlp_hits = tmpdir.join("otlp.hits");

    let mut otlp = ChildGuard::spawn(
        conformance,
        [
            "otlp-http-sink".into(),
            "--port".into(),
            otlp_port.to_string().into(),
            "--ready-file".into(),
            otlp_ready.as_os_str().to_os_string(),
            "--hits-file".into(),
            otlp_hits.as_os_str().to_os_string(),
        ],
        [],
        &otlp_log,
    )?;
    wait_for_file(&otlp_ready).await?;

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let platform_store = spawn_gateway_platform_store(gateway, control_plane).await?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, &platform_store),
        [
            ("OTEL_EXPORTER_OTLP_ENDPOINT", otlp_base.into()),
            (
                "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
                INTERNAL_SIGNING_KEY_DER_B64.into(),
            ),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.trim().into(),
            ),
        ],
        &gateway_log,
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;
    let ready: Value = reqwest::get(format!("{gateway_base}/readyz"))
        .await?
        .error_for_status()?
        .json()
        .await?;
    if ready.get("profiles").and_then(Value::as_u64) != Some(1) {
        bail!("gateway readyz did not report one profile: {ready}");
    }

    wait_for_file_contains(&otlp_hits, "logs ", "traces ").await?;

    gateway_child.stop();
    otlp.stop();
    cleanup.remove_on_drop();
    println!("otel smoke ok");
    Ok(())
}
