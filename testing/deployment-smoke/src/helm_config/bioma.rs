//! Reference-installation values, GitOps ownership, and edge assertions.
use super::*;
use std::collections::BTreeMap;

pub(super) fn check() -> Result<()> {
    gitops::check()?;
    let bioma = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "bioma".into(),
            "deploy/helm/veoveo".into(),
            "--values".into(),
            "testing/fixtures/platform-selection/platform-values.yaml".into(),
            "--namespace".into(),
            "veoveo".into(),
            "--values".into(),
            "examples/bioma/values.yaml".into(),
            "--values".into(),
            "examples/bioma/k3d-values.yaml".into(),
            "--values".into(),
            "examples/bioma/images/veoveo.lock.yaml".into(),
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
        "name: speech-mcp",
        "name: otel-collector",
        "name: bioma-otel-collector",
        "http://otel-collector:4318/v1/logs",
        "--expected-control-plane",
        "/etc/veoveo/gateway/gateway.json",
        "value: \"computers,artifact,media,timeseries,optimization,duckdb,frames,map,recording,stream,reason,speech,datasheet,uav-sim\"",
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
    let installation = run_checked(
        Path::new("kubectl"),
        ["kustomize".into(), "examples/bioma".into()],
        [],
    )?;
    let mut bundles = Vec::new();
    let mut manager_bundles = Vec::new();
    for document in serde_yaml_ng::Deserializer::from_str(&installation) {
        let object = Value::deserialize(document)?;
        if object["kind"] == "ConfigMap"
            && object["metadata"]["name"] == "bioma-gateway-control-plane"
        {
            let data = serde_json::from_value::<BTreeMap<String, String>>(object["data"].clone())?;
            match object["metadata"]["namespace"].as_str() {
                Some("veoveo") => bundles.push(data),
                Some("veoveo-agents") => manager_bundles.push(data),
                _ => bail!("unexpected namespace for the Bioma control-plane configuration"),
            }
        }
    }
    ensure!(
        bundles.len() == 1,
        "expected one complete Bioma gateway ConfigMap"
    );
    ensure!(
        manager_bundles.len() == 1,
        "expected one managed-agent catalog"
    );
    ensure!(
        manager_bundles[0].len() == 1
            && manager_bundles[0].get("gateway.json") == bundles[0].get("gateway.json"),
        "the manager must receive the same public catalog without gateway JWK files"
    );
    let bundle_digest = veoveo_deploy_contract::gateway_bundle_digest(&bundles[0])?;
    for config in [
        bundles[0]["gateway.json"].clone(),
        fs::read_to_string("configs/gateway.local.json")?,
    ] {
        let config: Value = serde_json::from_str(&config)?;
        let speech = config["servers"]
            .as_array()
            .and_then(|servers| servers.iter().find(|server| server["slug"] == "speech"))
            .context("Speech registration missing")?;
        ensure!(
            speech["referenced_resource_schemes"] == serde_json::json!(["artifact"]),
            "Speech must preserve source Artifact URIs through gateway projection"
        );
    }
    let control_plane_revision = bundle_digest
        .as_str()
        .strip_prefix("sha256:")
        .expect("validated SHA-256 digest");
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
    not_contains(&bioma, "secretName: bioma-ingress-tls")?;
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
            "--values".into(),
            "testing/fixtures/platform-selection/platform-values.yaml".into(),
            "--namespace".into(),
            "veoveo".into(),
            "--values".into(),
            "examples/bioma/values.yaml".into(),
            "--values".into(),
            "examples/bioma/k3d-values.yaml".into(),
            "--values".into(),
            "examples/bioma/lan-values.yaml".into(),
            "--values".into(),
            "examples/bioma/images/veoveo.lock.yaml".into(),
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
            "examples/bioma/images/uav-sim.lock.yaml".into(),
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
        "name: veoveo-uav-sim-adapter",
        "key: bearer-token",
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
        "name: UAV_SIM_OPERATOR_RTSP_PORT_BASE",
        "name: UAV_SIM_PUBLIC_STREAM_URL",
        "value: \"wss://veoveo.bioma.ai/uav-sim/live\"",
        "name: UAV_SIM_RUNTIME_STREAM_URL",
        "value: \"ws://uav-sim-runtime:8810/v1/live-streams\"",
        "name: uav-sim-live-stream",
        "http://127.0.0.1:8810/healthz",
        "http://127.0.0.1:8810/readyz",
        "nvidia.com/gpu: 1",
        "name: uav-sim-runtime",
        "value: \"http://uav-sim-runtime:8810/\"",
    ] {
        contains(&uav_sim, expected)?;
    }
    let world_digest = hex::encode(Sha256::digest(fs::read(
        "examples/bioma/uav-sim-world.json",
    )?));
    contains(
        &uav_sim,
        &format!("checksum/world-bootstrap: \"{world_digest}\""),
    )?;
    for forbidden in [
        "veoveo.ai/chart-revision",
        "GOOGLE_MAPS_API_KEY",
        "UAV_SIM_POSE_",
        "simulation-view",
        "\n  name: uav-sim-live\n",
        "path: /webrtc",
        "name: stream-signal",
        "name: stream-media",
        "UAV_SIM_RUNTIME_EVENT_SOCKET",
        "name: ROS_DISTRO",
        "name: RMW_IMPLEMENTATION",
        "isaacsim.ros2.core",
    ] {
        if uav_sim.contains(forbidden) {
            bail!("UAV simulation render must not contain `{forbidden}`");
        }
    }
    ensure!(
        uav_sim.matches("name: CESIUM_ION_ACCESS_TOKEN").count() == 1,
        "interactive UAV render must inject the Cesium ion token exactly once"
    );
    let simulator = uav_sim
        .find("- name: isaac-sim")
        .context("UAV render omits the authoritative simulator")?;
    let forwarder = uav_sim
        .find("- name: recording-forwarder")
        .context("UAV render omits the recording forwarder")?;
    let mcp = uav_sim
        .find("- name: uav-sim-mcp")
        .context("UAV render omits the UAV MCP server")?;
    ensure!(
        simulator < forwarder && forwarder < mcp,
        "authoritative simulator and recording forwarder must precede the independent MCP deployment"
    );
    ensure!(
        uav_sim.matches("kind: Deployment").count() == 2
            && uav_sim.matches("runtimeClassName: nvidia").count() == 1
            && uav_sim.matches("nvidia.com/gpu: 1").count() == 2,
        "UAV chart must render one GPU runtime and one GPU-independent MCP deployment"
    );

    agent_template::verify(&uav_sim)?;

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

    let bioma_cluster = fs::read_to_string("examples/bioma/k3d.yaml")?;
    contains(&bioma_cluster, "name: veoveo-bioma")?;
    contains(&bioma_cluster, "127.0.0.1:8781:80")?;
    contains(&bioma_cluster, "k3d-veoveo-registry.localhost:5001")?;
    not_contains(&bioma_cluster, "create:")?;
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

    ensure!(
        !Path::new("examples/bioma/deployment.json").exists(),
        "Bioma must use its enterprise GitOps contract rather than a deployment profile"
    );
    let bioma_root = fs::read_to_string("examples/bioma/gitops/bootstrap.yaml")?;
    for expected in [
        "kind: GitRepository",
        "url: ssh://git@github.com/BiomaAI/veoveo.git",
        "kind: Kustomization",
        "path: ./examples/bioma",
        "deletionPolicy: Orphan",
        "prune: true",
        "wait: true",
    ] {
        contains(&bioma_root, expected)?;
    }
    let bioma_platform = fs::read_to_string("examples/bioma/platform/flux/kustomization.yaml")?;
    for expected in [
        "manifests/bases/source-controller?ref=v2.9.5",
        "manifests/bases/kustomize-controller?ref=v2.9.5",
        "manifests/bases/helm-controller?ref=v2.9.5",
    ] {
        contains(&bioma_platform, expected)?;
    }
    not_contains(&bioma_platform, "notification-controller?ref=")?;
    for source in [
        "examples/bioma/gitops/sources/veoveo.yaml",
        "examples/bioma/gitops/sources/uav-sim.yaml",
    ] {
        let source = fs::read_to_string(source)?;
        for expected in [
            "kind: OCIRepository",
            "charts-registry.flux-system.svc.cluster.local:5000/charts/",
            "insecure: true",
            "application/vnd.cncf.helm.chart.content.v1.tar+gzip",
            "operation: copy",
        ] {
            contains(&source, expected)?;
        }
    }
    for release in [
        "examples/bioma/gitops/releases/veoveo.yaml",
        "examples/bioma/gitops/releases/uav-sim.yaml",
    ] {
        let release = fs::read_to_string(release)?;
        for expected in [
            "kind: HelmRelease",
            "targetNamespace: veoveo",
            "storageNamespace: veoveo",
            "chartRef:",
            "serverSideApply: true",
            "serverSideApply: enabled",
            "driftDetection:",
            "mode: enabled",
            "valuesKey: images.lock.yaml",
        ] {
            contains(&release, expected)?;
        }
    }
    Ok(())
}
