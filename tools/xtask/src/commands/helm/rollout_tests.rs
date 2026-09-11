//! Rendered rollout contracts, including the installation-owned Flux handoff.

use std::{collections::BTreeMap, path::Path, process::Command};

use serde::Deserialize;
use serde_json::Value;

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("xtask lives in tools/xtask")
}

fn output(command: &mut Command) -> Vec<u8> {
    let output = command.output().expect("run chart tool");
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn objects(bytes: &[u8]) -> Vec<Value> {
    serde_yaml_ng::Deserializer::from_slice(bytes)
        .map(|document| Value::deserialize(document).expect("rendered YAML object"))
        .filter(|object| !object.is_null())
        .collect()
}

fn release_values<'a>(rendered: &'a [Value], release_name: &str) -> &'a Value {
    let release = rendered
        .iter()
        .find(|object| {
            object["kind"] == "HelmRelease" && object["metadata"]["name"] == release_name
        })
        .unwrap();
    let reference = release["spec"]["valuesFrom"]
        .as_array()
        .unwrap()
        .iter()
        .find(|reference| reference["valuesKey"] == "images.lock.yaml")
        .unwrap();
    assert_eq!(reference["kind"], "ConfigMap");
    let config = rendered
        .iter()
        .find(|object| {
            object["kind"] == "ConfigMap"
                && object["metadata"]["namespace"] == release["metadata"]["namespace"]
                && object["metadata"]["name"] == reference["name"]
        })
        .unwrap();
    assert_eq!(config["immutable"], true);
    config
}

fn render(chart: &Path, extension: bool, settings: &[&str]) -> Vec<Value> {
    let mut command = Command::new("helm");
    command
        .current_dir(repository())
        .args(["template", "veoveo"])
        .arg(chart)
        .args(["--namespace", "veoveo"]);
    let values = if extension {
        &[
            "examples/bioma/uav-sim-values.yaml",
            "examples/bioma/images/uav-sim.lock.yaml",
        ][..]
    } else {
        &[
            "examples/bioma/values.yaml",
            "examples/bioma/k3d-values.yaml",
            "examples/bioma/images/veoveo.lock.yaml",
        ][..]
    };
    for path in values {
        command.args(["-f", path]);
    }
    for setting in settings {
        command.args(["--set", setting]);
    }
    objects(&output(&mut command))
}

fn pod_templates(objects: &[Value]) -> BTreeMap<String, Value> {
    objects
        .iter()
        .filter(|object| object["kind"] == "Deployment")
        .map(|object| {
            (
                object["metadata"]["name"].as_str().unwrap().to_owned(),
                object["spec"]["template"].clone(),
            )
        })
        .collect()
}

fn changed_pods(before: &[Value], after: &[Value]) -> Vec<String> {
    let before = pod_templates(before);
    let after = pod_templates(after);
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>()
    );
    before
        .into_iter()
        .filter_map(|(name, template)| (after[&name] != template).then_some(name))
        .collect()
}

#[test]
fn computers_configuration_and_artifact_dependency_match_the_service_profile() {
    let chart = repository().join("deploy/helm/veoveo");
    let rendered = render(
        &chart,
        false,
        &[
            "computerCapacity=unconfigured",
            "computers.existingConfigMap=",
            "computers.existingTrustSecret=",
            "computers.configurationRevision=",
            "computers.host.existingConfigMap=",
            "computers.host.existingTrustSecret=",
            "computers.host.configurationRevision=",
        ],
    );
    let document = rendered
        .iter()
        .find(|object| {
            object["kind"] == "ConfigMap" && object["metadata"]["name"] == "computers-configuration"
        })
        .unwrap();
    let configuration: Value =
        serde_json::from_str(document["data"]["computers.json"].as_str().unwrap()).unwrap();
    assert_eq!(configuration["schema"], "veoveo.io/computers-service/v3");
    assert_eq!(configuration["capacity"]["kind"], "unconfigured");
    let denied = Command::new("helm")
        .args(["template", "computers"])
        .arg(&chart)
        .args([
            "--set",
            "installationPreset=custom",
            "--set",
            "computerCapacity=openshell-docker",
            "--set-json",
            r#"components=["gateway","platform-store"]"#,
            "--set-json",
            r#"mcpServers=["computers"]"#,
        ])
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(
        String::from_utf8_lossy(&denied.stderr).contains("artifact-service for command outputs")
    );
    let denied = Command::new("helm")
        .args(["template", "computers"])
        .arg(&chart)
        .args([
            "--set",
            "computerCapacity=openshell-docker",
            "--set-json",
            "artifactService.allowedAudiences=[\"artifact\"]",
        ])
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(
        String::from_utf8_lossy(&denied.stderr)
            .contains("computers in artifactService.allowedAudiences")
    );
}

#[test]
fn bioma_compute_host_admits_every_control_plane_template() {
    let read = |path| -> Value {
        serde_json::from_slice(&std::fs::read(repository().join(path)).unwrap()).unwrap()
    };
    let service = read("examples/bioma/computers/computers.json");
    let host = read("examples/bioma/computers/host.json");
    assert_eq!(service["providerInstanceId"], host["providerId"]);
    let templates = service["capacity"]["templates"].as_array().unwrap();
    for template in templates {
        assert!(
            host["images"]
                .as_array()
                .unwrap()
                .contains(&template["image"]),
            "compute host must preload {}",
            template["id"]
        );
        let allocation = host["templates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["fingerprint"] == template["fingerprint"])
            .expect("retained storage must admit every selected template");
        assert_eq!(
            allocation["capacityBytes"].as_u64().unwrap(),
            template["homeCapacityMib"].as_u64().unwrap() * 1024 * 1024
        );
    }
    let default = templates
        .iter()
        .find(|entry| entry["fingerprint"] == service["capacity"]["defaultTemplate"])
        .unwrap();
    assert_eq!(host["defaultImage"], default["image"]);
}

#[test]
fn chart_publication_metadata_preserves_every_bioma_pod_template() {
    for (chart, name, extension, expected_pods) in [
        ("deploy/helm/veoveo", "veoveo", false, 20),
        ("showcase/uav-sim/deploy/helm", "uav-sim", true, 6),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut renders = Vec::new();
        for (version, revision) in [
            ("0.1.0-rollout.1", "source-one"),
            ("0.1.0-rollout.2", "source-two"),
        ] {
            output(
                Command::new("helm")
                    .current_dir(repository())
                    .args([
                        "package",
                        chart,
                        "--version",
                        version,
                        "--app-version",
                        revision,
                        "--destination",
                    ])
                    .arg(directory.path()),
            );
            renders.push(render(
                &directory.path().join(format!("{name}-{version}.tgz")),
                extension,
                &[],
            ));
        }
        assert_eq!(pod_templates(&renders[0]).len(), expected_pods);
        assert!(
            changed_pods(&renders[0], &renders[1]).is_empty(),
            "{name} restarts Pods for chart metadata"
        );
    }
}

#[test]
fn runtime_catalog_updates_roll_only_their_consumer() {
    let chart = repository().join("deploy/helm/veoveo");
    let before = render(&chart, false, &[]);
    for (setting, name, checksum, configmap) in [
        (
            "stream.liveInput.width=1920",
            "stream-mcp",
            "checksum/stream-runtime",
            "stream-runtime",
        ),
        (
            "reason.model.title=Changed model",
            "reason-mcp",
            "checksum/reason-runtime",
            "reason-runtime",
        ),
    ] {
        let after = render(&chart, false, &[setting]);
        assert_eq!(changed_pods(&before, &after), [name]);
        let find_data = |objects: &[Value]| {
            objects
                .iter()
                .find(|object| {
                    object["kind"] == "ConfigMap" && object["metadata"]["name"] == configmap
                })
                .unwrap()["data"]
                .clone()
        };
        assert_ne!(find_data(&before), find_data(&after));
        let before = pod_templates(&before);
        let after = pod_templates(&after);
        assert_ne!(
            before[name]["metadata"]["annotations"][checksum],
            after[name]["metadata"]["annotations"][checksum]
        );
    }
}

#[test]
fn one_image_digest_rolls_only_its_workload() {
    for (chart, extension, image, deployment) in [
        ("deploy/helm/veoveo", false, "console-bff", "console-bff"),
        (
            "showcase/uav-sim/deploy/helm",
            true,
            "uav-sim-mcp",
            "uav-sim-mcp",
        ),
    ] {
        let chart = repository().join(chart);
        let before = render(&chart, extension, &[]);
        let setting = format!(
            "global.imageDigests.veoveo/{image}=sha256:{}",
            "a".repeat(64)
        );
        let after = render(&chart, extension, &[&setting]);
        assert_eq!(changed_pods(&before, &after), [deployment]);
    }
}

#[test]
fn generated_helm_values_are_selected_by_flux_watch_labels() {
    let rendered = objects(&output(
        Command::new("kubectl")
            .current_dir(repository())
            .args(["kustomize", "examples/bioma"]),
    ));
    for name in ["veoveo", "uav-sim"] {
        let values = release_values(&rendered, name);
        assert_eq!(
            values["metadata"]["labels"]["reconcile.fluxcd.io/watch"],
            "Enabled"
        );
        assert!(
            values["metadata"]["annotations"]
                .get("reconcile.fluxcd.io/watch")
                .is_none()
        );
    }
}

#[test]
fn each_release_receives_exactly_its_consumed_image_digests() {
    fn images(value: &Value, references: &mut std::collections::BTreeSet<String>) {
        match value {
            Value::Object(object) => {
                if let Some(image) = object.get("image").and_then(Value::as_str)
                    && image.starts_with("k3d-veoveo-registry.localhost:5000/veoveo/")
                {
                    references.insert(image.to_owned());
                }
                for child in object.values() {
                    images(child, references);
                }
            }
            Value::Array(array) => {
                for child in array {
                    images(child, references);
                }
            }
            _ => {}
        }
    }
    let rendered = objects(&output(
        Command::new("kubectl")
            .current_dir(repository())
            .args(["kustomize", "examples/bioma"]),
    ));
    for (name, chart, extension) in [
        ("veoveo", "deploy/helm/veoveo", false),
        ("uav-sim", "showcase/uav-sim/deploy/helm", true),
    ] {
        let config = release_values(&rendered, name);
        let values: Value =
            serde_yaml_ng::from_str(config["data"]["images.lock.yaml"].as_str().unwrap()).unwrap();
        let registry = values["global"]["veoveoRegistry"].as_str().unwrap();
        let declared = values["global"]["imageDigests"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(repository, digest)| {
                format!("{registry}/{repository}@{}", digest.as_str().unwrap())
            })
            .collect::<std::collections::BTreeSet<_>>();
        let mut consumed = std::collections::BTreeSet::new();
        for object in render(&repository().join(chart), extension, &[]) {
            images(&object, &mut consumed);
            // The private host pulls its default guest image from installation
            // configuration. It is a runnable dependency outside Pod image fields.
            if object["kind"] == "Deployment" && object["metadata"]["name"] == "computer-host" {
                let config_name = object["spec"]["template"]["spec"]["volumes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|volume| volume["name"] == "configuration")
                    .unwrap()["configMap"]["name"]
                    .as_str()
                    .unwrap();
                let config = rendered
                    .iter()
                    .find(|object| {
                        object["kind"] == "ConfigMap" && object["metadata"]["name"] == config_name
                    })
                    .unwrap();
                let host: Value =
                    serde_json::from_str(config["data"]["host.json"].as_str().unwrap()).unwrap();
                let image = host["defaultImage"].as_str().unwrap();
                assert!(
                    host["images"]
                        .as_array()
                        .unwrap()
                        .contains(&Value::String(image.into()))
                );
                consumed.insert(image.to_owned());
            }
        }
        assert_eq!(
            declared, consumed,
            "{name} must not receive unrelated image-lock keys"
        );
    }
}

#[test]
fn chart_selection_packages_only_the_requested_chart_once() {
    let output = tempfile::tempdir().unwrap();
    let selection = [super::Chart::UavSim, super::Chart::UavSim];
    let release = super::build(
        repository(),
        output.path(),
        "0.1.0-selection",
        "selected-source",
        &selection,
    )
    .unwrap();
    assert_eq!(release.artifacts.len(), 1);
    assert_eq!(release.artifacts[0].name, "uav-sim");
    assert_eq!(super::selection_name(&selection), "uav-sim");
    assert!(!output.path().join("veoveo-0.1.0-selection.tgz").exists());
}

#[test]
fn external_world_content_rolls_only_its_bootstrap_consumer() {
    let chart = repository().join("showcase/uav-sim/deploy/helm");
    let before = render(&chart, true, &[]);
    let digest = "b".repeat(64);
    let after = render(
        &chart,
        true,
        &[&format!("world.bootstrap.contentSha256={digest}")],
    );
    assert_eq!(changed_pods(&before, &after), ["uav-sim-mcp"]);
    assert_eq!(
        pod_templates(&after)["uav-sim-mcp"]["metadata"]["annotations"]["checksum/world-bootstrap"],
        digest
    );
    for digest in ["", "invalid"] {
        let output = Command::new("helm")
            .current_dir(repository())
            .args([
                "template",
                "uav-sim",
                "showcase/uav-sim/deploy/helm",
                "-f",
                "examples/bioma/uav-sim-values.yaml",
                "--set",
                &format!("world.bootstrap.contentSha256={digest}"),
            ])
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "an external bootstrap requires its exact content digest"
        );
    }
}

#[test]
fn sumo_chart_metadata_preserves_pod_templates() {
    let directory = tempfile::tempdir().unwrap();
    let mut renders = Vec::new();
    for version in ["0.1.0-rollout.1", "0.1.0-rollout.2"] {
        output(
            Command::new("helm")
                .current_dir(repository())
                .args([
                    "package",
                    "showcase/sumo/deploy/helm",
                    "--version",
                    version,
                    "--destination",
                ])
                .arg(directory.path()),
        );
        renders.push(objects(&output(
            Command::new("helm")
                .args(["template", "sumo"])
                .arg(directory.path().join(format!("veoveo-sumo-{version}.tgz"))),
        )));
    }
    assert_eq!(pod_templates(&renders[0]).len(), 2);
    assert!(changed_pods(&renders[0], &renders[1]).is_empty());
}
