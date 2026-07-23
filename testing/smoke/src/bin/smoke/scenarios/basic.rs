use anyhow::ensure;

use super::*;

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
    println!("surreal integration smoke ok");
    Ok(())
}

pub(crate) async fn helm_config() -> Result<()> {
    for chart in [
        "deploy/helm/veoveo",
        "showcase/sumo/deploy/helm",
        "showcase/uav-sim/deploy/helm",
    ] {
        run_checked(Path::new("helm"), ["lint".into(), chart.into()], [])
            .with_context(|| format!("linting Helm chart {chart}"))?;
    }

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
        "port: 9878",
        "host: localhost",
        "path: /s",
        "mountPath: /etc/veoveo/gateway",
        "runAsUser: 65532",
        "runAsUser: 10001",
    ] {
        contains(&platform, expected)?;
    }
    for forbidden in ["OTEL_EXPORTER_OTLP_ENDPOINT", "port: 9876"] {
        if platform.contains(forbidden) {
            bail!("canonical Helm render must not contain `{forbidden}`");
        }
    }

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
        "host: objects-veoveo.bioma.ai",
        "https://veoveo.bioma.ai",
        "name: bioma-gateway-control-plane",
        "name: recording-hub",
        "name: frames-mcp-bootstrap",
        "bioma-uav-origin",
        r#""view_coordinates":{"x":"right","y":"forward","z":"up"}"#,
        "name: view-mcp",
        "name: perception-mcp",
        "name: reason-mcp",
        "value: \"artifact,media,timeseries,optimization,duckdb,frames,map,recording,perception,reason,datasheet\"",
        "checksum/reason-runtime:",
        "checksum/control-plane: \"unresolved\"",
    ] {
        contains(&bioma, expected)?;
    }
    for forbidden in ["name: otel-collector", "secretName: bioma-ingress-tls"] {
        if bioma.contains(forbidden) {
            bail!("Bioma k3d render must not contain `{forbidden}`");
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
        "name: UAV_SIM_FOLLOW_CAMERA_WIDTH",
        "value: \"1280\"",
        "name: UAV_SIM_LIVE_STREAM_SIGNALING_URL",
        "value: \"ws://127.0.0.1:49101/webrtc\"",
        "name: UAV_SIM_LIVE_STREAM_PUBLIC_IP",
        "name: uav-sim-live",
        "name: stream-signal",
        "containerPort: 49101",
        "name: stream-media",
        "containerPort: 47998",
        "nodePort: 30910",
        "nodePort: 30998",
        "name: ROS_DISTRO",
        "value: jazzy",
        "name: RMW_IMPLEMENTATION",
        "value: rmw_fastrtps_cpp",
        "name: LD_LIBRARY_PATH",
        "value: /isaac-sim/exts/isaacsim.ros2.core/jazzy/lib",
        "http://127.0.0.1:8810/healthz",
        "http://127.0.0.1:8810/readyz",
        "nvidia.com/gpu: 1",
    ] {
        contains(&uav_sim, expected)?;
    }
    for forbidden in ["GOOGLE_MAPS_API_KEY"] {
        if uav_sim.contains(forbidden) {
            bail!("UAV simulation render must not contain `{forbidden}`");
        }
    }
    ensure!(
        uav_sim.matches("name: CESIUM_ION_ACCESS_TOKEN").count() == 1,
        "interactive UAV render must inject the Cesium ion token exactly once"
    );

    let uav_batch = run_checked(
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
            "--set".into(),
            "interactive.enabled=false".into(),
            "--set".into(),
            "batch.enabled=true".into(),
        ],
        [],
    )?;
    for expected in [
        "kind: Job",
        "name: uav-sim-bioma-uav-batch",
        "name: UAV_SIM_EXIT_AFTER_SECONDS",
        "runtimeClassName: nvidia",
        "name: CESIUM_ION_ACCESS_TOKEN",
        "name: uav-sim-batch-runtime-cache",
        "claimName: uav-sim-batch-runtime-cache",
        "name: uav-sim-batch-recording-forwarder",
        "claimName: uav-sim-batch-recording-forwarder",
        "image: k3d-veoveo-registry.localhost:5000/veoveo/recording-forwarder@sha256:",
    ] {
        contains(&uav_batch, expected)?;
    }
    for forbidden in ["kind: Service", "name: uav-sim-mcp"] {
        if uav_batch.contains(forbidden) {
            bail!("batch UAV render must not contain `{forbidden}`");
        }
    }

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
            .pointer("/components/isaac_sim/version")
            .and_then(Value::as_str)
            == Some("6.0.1")
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
                == Some("6.0.2"),
        "UAV dependency lock omitted a canonical release or Google tiles identity"
    );
    let uav_runtime_dockerfile = fs::read_to_string("showcase/uav-sim/runtime/Dockerfile")?;
    for expected in [
        "nvcr.io/nvidia/isaac-sim:6.0.1@sha256:",
        "px4io/px4-dev:v1.17.0@sha256:",
        "PX4_COMMIT=d6f12ad1c4f70ad3230afd7d86e971421e02fef4",
        "PEGASUS_COMMIT=644da37e9d5268e5f9a34e78bdcfd57a8bab82b4",
        "CESIUM_VERSION=0.29.0",
        "sha256sum --check --strict",
        "cesium-0.29.0-preinstalled-vendor.patch",
        "lxml-6.0.2-cp312-cp312",
        "git -C pegasus apply --unidiff-zero --check",
        "rerun-sdk==${RERUN_SDK_VERSION}",
        "AS runtime-base",
        "ARG UAV_SIM_BASE_IMAGE=veoveo/uav-sim-base:",
        "FROM ${UAV_SIM_BASE_IMAGE} AS runtime",
        "org.opencontainers.image.revision=",
        "USER 10001:10001",
    ] {
        contains(&uav_runtime_dockerfile, expected)?;
    }
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
        "cesium_interface.on_update_frame([cesium_viewport], False)",
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
    contains(&gpu_device_plugin, "replicas: 4")?;
    contains(
        &gpu_device_plugin,
        "veoveo.ai/device-plugin-config: time-slicing-4",
    )?;

    let gateway_dockerfile = fs::read_to_string("platform/gateway/Dockerfile")?;
    contains(&gateway_dockerfile, "find /app/target -name 'libduckdb.so'")?;
    contains(
        &gateway_dockerfile,
        "id=veoveo-rust-1.97.1-trixie-release,target=/app/target,sharing=locked",
    )?;
    contains(
        &gateway_dockerfile,
        "COPY --from=builder /out/lib/libduckdb.so /usr/local/lib/libduckdb.so",
    )?;
    let uav_mcp_dockerfile = fs::read_to_string("servers/uav-sim-mcp/Dockerfile")?;
    for expected in [
        "--bin uav-sim-mcp",
        "@nvidia/ov-web-rtc@6.6.0",
        "77be78cd4799f797d320d386461834737f5a8368deacfb3b27ae26612f39c9a5",
        "UAV_SIM_WEBRTC_CLIENT_BUNDLE=/tmp/ov-web-rtc.umd.cjs",
    ] {
        contains(&uav_mcp_dockerfile, expected)?;
    }
    let bake = fs::read_to_string("docker-bake.hcl")?;
    for expected in [
        "group \"platform-core\"",
        "group \"platform-full\"",
        "group \"showcase-sumo-base\"",
        "group \"showcase-sumo\"",
        "group \"showcase-uav-sim-base\"",
        "group \"showcase-uav-sim\"",
        "sumo-base = \"target:sumo-base\"",
        "uav-sim-base = \"target:uav-sim-base\"",
        "VEOVEO_REGISTRY",
        "VEOVEO_IMAGE_TAG",
    ] {
        contains(&bake, expected)?;
    }
    let justfile = fs::read_to_string("Justfile")?;
    for expected in [
        "profile-validate profile:",
        "profile-publish profile revision='HEAD':",
        "profile-up profile revision='HEAD':",
        "profile-cluster-up profile:",
        "charts-publish registry version revision='HEAD' plain_http='false':",
    ] {
        contains(&justfile, expected)?;
    }
    for forbidden in ["k3d image import", "docker save", "bioma-build:"] {
        not_contains(&justfile, forbidden)?;
    }
    ensure!(
        !Path::new("examples/bioma/deployment.json").exists(),
        "Bioma must use its enterprise GitOps contract rather than a deployment profile"
    );
    crate::deployment::profile_validate(Path::new("showcase/sumo/deploy/deployment.json"))?;
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
        contains(&application, "targetRevision: 0.1.0-92ba57cdf93d")?;
        not_contains(&application, "ServerSideApply=true")?;
    }
    let uav_scenario: Value = serde_json::from_str(&fs::read_to_string(
        "showcase/uav-sim/scenarios/bioma-aerial.json",
    )?)?;
    ensure!(
        uav_scenario.get("schema").and_then(Value::as_str) == Some("veoveo.uav-sim-acceptance/v4")
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
                == Some(8),
        "runtime-loaded UAV scenario omitted the canonical mission"
    );
    for dockerfile in [
        "agents/kernel/Dockerfile",
        "apps/console/bff/Dockerfile",
        "mcp/bridges/stdio/Dockerfile",
        "platform/artifacts/service/Dockerfile",
        "platform/gateway/Dockerfile",
        "platform/recordings/hub/Dockerfile",
        "servers/artifact-mcp/Dockerfile",
        "servers/frames-mcp/Dockerfile",
        "servers/duckdb-mcp/Dockerfile",
        "servers/media-mcp/Dockerfile",
        "servers/optimization-mcp/Dockerfile",
        "servers/perception-mcp/Dockerfile",
        "servers/recording-mcp/Dockerfile",
        "servers/timeseries-mcp/Dockerfile",
        "servers/time-mcp/Dockerfile",
        "servers/uav-sim-mcp/Dockerfile",
        "showcase/sumo/sumo-mcp/Dockerfile",
    ] {
        let contents = fs::read_to_string(dockerfile)?;
        for workspace_root in [
            "COPY agents ./agents",
            "COPY apps/console/bff ./apps/console/bff",
            "COPY mcp ./mcp",
            "COPY platform ./platform",
            "COPY servers ./servers",
            "COPY testing ./testing",
            "COPY showcase/sumo/sumo-mcp ./showcase/sumo/sumo-mcp",
        ] {
            contains(&contents, workspace_root)
                .with_context(|| format!("{dockerfile} must copy every Cargo workspace root"))?;
        }
    }

    let dockerignore = fs::read_to_string(".dockerignore")?;
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
    let frame = assert_schema_title(
        &schemas.join("rrd-frame-definition.schema.json"),
        "RrdFrameDefinition",
    )?;
    for property in ["frame_id", "kind", "view_coordinates"] {
        if !frame
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("frame definition schema has no object `{property}` property");
        }
    }
    assert_schema_title(
        &schemas.join("coordinate-point.schema.json"),
        "CoordinatePoint",
    )?;
    assert_schema_title(
        &schemas.join("coordinate-operation-provenance.schema.json"),
        "CoordinateOperationProvenance",
    )?;
    assert_schema_title(
        &schemas.join("rrd-geofence-geometry.schema.json"),
        "RrdGeofenceGeometry",
    )?;
    let batch_transform = assert_schema_title(
        &schemas.join("batch-transform-output.schema.json"),
        "BatchTransformOutput",
    )?;
    for property in ["result", "artifact"] {
        if !batch_transform
            .get("properties")
            .and_then(|properties| properties.get(property))
            .is_some_and(Value::is_object)
        {
            bail!("batch transform schema has no object `{property}` property");
        }
    }
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
