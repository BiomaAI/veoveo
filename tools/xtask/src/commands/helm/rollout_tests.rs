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
            "examples/bioma/modules-values.yaml",
        ][..]
    };
    for path in values {
        command.args(["-f", path]);
    }
    if !extension {
        // Explicit fixture input; reference issuer networks are installation-owned.
        command.args(["--set", "computers.host.issuerEgress[0]=198.51.100.10/32"]);
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
    assert_eq!(configuration["schema"], "veoveo.ai/computers-service/v3");
    assert_eq!(configuration["capacity"]["kind"], "unconfigured");
    let denied = Command::new("helm")
        .current_dir(repository())
        .args(["template", "computers"])
        .arg(&chart)
        .args([
            "--set-file",
            "moduleInstallation.planJson=testing/fixtures/module-schema-consumer/module-plan.json",
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
        .current_dir(repository())
        .args(["template", "computers"])
        .arg(&chart)
        .args([
            "--set-file",
            "moduleInstallation.planJson=testing/fixtures/module-schema-consumer/module-plan.json",
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
        ("deploy/helm/veoveo", "veoveo", false, 25),
        ("showcase/uav-sim/deploy/helm", "uav-sim", true, 2),
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
            // The manager launches runtime images from its JSON template catalog.
            // They are consumed at instance creation, outside this chart's Pod fields.
            if object["kind"] == "ConfigMap" && object["metadata"]["name"] == "veoveo-agent-manager"
            {
                let config: Value =
                    serde_json::from_str(object["data"]["manager.json"].as_str().unwrap()).unwrap();
                images(&config, &mut consumed);
            }
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
fn external_world_content_recreates_the_immutable_runtime_and_its_companion() {
    let chart = repository().join("showcase/uav-sim/deploy/helm");
    let before = render(&chart, true, &[]);
    let digest = "b".repeat(64);
    let after = render(
        &chart,
        true,
        &[&format!("world.bootstrap.contentSha256={digest}")],
    );
    assert_eq!(changed_pods(&before, &after), ["uav-sim", "uav-sim-mcp"]);
    for name in ["uav-sim", "uav-sim-mcp"] {
        let templates = pod_templates(&after);
        let pod = &templates[name];
        assert_eq!(
            pod["metadata"]["annotations"]["checksum/world-bootstrap"],
            digest
        );
        let container = &pod["spec"]["containers"][0];
        assert!(container["env"].as_array().unwrap().iter().any(|env| {
            env["name"] == "UAV_SIM_WORLD_BOOTSTRAP_FILE"
                && env["value"] == "/etc/veoveo/uav-sim-world/world.json"
        }));
        assert!(
            container["volumeMounts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|mount| { mount["name"] == "world-bootstrap" && mount["readOnly"] == true })
        );
        assert!(
            pod["spec"]["volumes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|volume| {
                    volume["name"] == "world-bootstrap"
                        && volume["configMap"]["name"] == "uav-sim-world-binding"
                })
        );
    }
    let runtime = after
        .iter()
        .find(|object| object["kind"] == "Deployment" && object["metadata"]["name"] == "uav-sim")
        .unwrap();
    assert_eq!(runtime["spec"]["strategy"]["type"], "Recreate");
    let claims = |objects: &[Value]| {
        objects
            .iter()
            .filter(|object| object["kind"] == "PersistentVolumeClaim")
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(claims(&before), claims(&after));
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

#[test]
fn helm_evidence_enrichment_admits_only_current_selected_release() {
    use super::{HelmArtifact, HelmRelease, OciPublication, write_evidence};
    let directory = tempfile::tempdir().unwrap();
    let mut release = HelmRelease {
        output: directory.path().to_owned(),
        artifacts: vec![HelmArtifact {
            name: "veoveo",
            archive: directory.path().join("veoveo-1.2.3.tgz"),
            filename: "veoveo-1.2.3.tgz".to_owned(),
            sha256: veoveo_deploy_contract::ArtifactDigest::parse(format!(
                "sha256:{}",
                "a".repeat(64)
            ))
            .unwrap(),
            oci: None,
        }],
    };
    write_evidence(&release, "1.2.3", "selected-source", "v4.3.0").unwrap();
    let target = directory.path().join("release-evidence.json");
    let original = std::fs::read(&target).unwrap();
    release.artifacts[0].oci = Some(OciPublication {
        coordinate: veoveo_deploy_contract::ArtifactCoordinate::new(
            "oci://registry.test/charts/veoveo:1.2.3",
        )
        .unwrap(),
        digest: format!("sha256:{}", "b".repeat(64)),
    });
    let current: serde_json::Value = serde_json::from_slice(&original).unwrap();
    for (path, old) in [
        ("/schemaVersion", "schema_version"),
        ("/sourceRevision", "source_revision"),
        ("/artifacts/0/mediaType", "media_type"),
    ] {
        for mixed in [false, true] {
            let mut bad = current.clone();
            let (parent, field) = path.rsplit_once('/').unwrap();
            let object = bad.pointer_mut(parent).unwrap().as_object_mut().unwrap();
            let value = object[field].clone();
            if !mixed {
                object.remove(field);
            }
            object.insert(old.to_owned(), value);
            let bytes = serde_json::to_vec(&bad).unwrap();
            std::fs::write(&target, &bytes).unwrap();
            assert!(write_evidence(&release, "1.2.3", "selected-source", "v4.3.0").is_err());
            assert_eq!(std::fs::read(&target).unwrap(), bytes);
        }
    }
    for (path, value) in [
        (
            "/schemaVersion",
            serde_json::json!("veoveo.ai/helm-chart-release-evidence/v0"),
        ),
        ("/sourceRevision", serde_json::json!("foreign-source")),
        ("/version", serde_json::json!("9.9.9")),
        (
            "/artifacts/0/sha256",
            serde_json::json!(format!("sha256:{}", "c".repeat(64))),
        ),
        ("/artifacts/0/sha256", serde_json::json!("c".repeat(64))),
        (
            "/artifacts/0/sha256",
            serde_json::json!(format!("sha256:{}", "C".repeat(64))),
        ),
        ("/artifacts/0/sha256", serde_json::json!("sha256:bad")),
        (
            "/artifacts/0/filename",
            serde_json::json!("foreign-1.2.3.tgz"),
        ),
    ] {
        let mut bad = current.clone();
        *bad.pointer_mut(path).unwrap() = value;
        let bytes = serde_json::to_vec(&bad).unwrap();
        std::fs::write(&target, &bytes).unwrap();
        assert!(write_evidence(&release, "1.2.3", "selected-source", "v4.3.0").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), bytes);
    }
    std::fs::write(&target, &original).unwrap();
    write_evidence(&release, "1.2.3", "selected-source", "v4.3.0").unwrap();
    let enriched: super::HelmReleaseEvidence =
        serde_json::from_slice(&std::fs::read(&target).unwrap()).unwrap();
    assert_eq!(
        enriched.artifacts[0].oci.as_ref().unwrap(),
        release.artifacts[0].oci.as_ref().unwrap()
    );
    let settled = std::fs::read(&target).unwrap();
    write_evidence(&release, "1.2.3", "selected-source", "v4.3.0").unwrap();
    release.artifacts[0].oci.as_mut().unwrap().digest = format!("sha256:{}", "d".repeat(64));
    assert!(write_evidence(&release, "1.2.3", "selected-source", "v4.3.0").is_err());
    assert_eq!(std::fs::read(&target).unwrap(), settled);
}

#[test]
fn helm_push_preflight_refuses_before_helm_effects() {
    use super::{HelmArtifact, HelmRelease, push, sha256_file, write_evidence};
    const CHILD: &str = "VEOVEO_HELM_PREFLIGHT_CASE";
    if let Ok(case) = std::env::var(CHILD) {
        let directory = Path::new(&case);
        let mut release = HelmRelease {
            output: directory.to_owned(),
            artifacts: vec![HelmArtifact {
                name: "veoveo",
                archive: directory.join("veoveo-1.2.3.tgz"),
                filename: "veoveo-1.2.3.tgz".to_owned(),
                sha256: veoveo_deploy_contract::ArtifactDigest::parse(
                    std::env::var("VEOVEO_HELM_PREFLIGHT_SELECTED_DIGEST").unwrap(),
                )
                .unwrap(),
                oci: None,
            }],
        };
        let result = push(
            &mut release,
            "registry.test/charts",
            false,
            "1.2.3",
            "selected-source",
        );
        let expected = std::env::var("VEOVEO_HELM_PREFLIGHT_ALLOWED").unwrap() == "yes";
        assert_eq!(result.is_ok(), expected, "{result:?}");
        assert_eq!(directory.join("effects").exists(), expected);
        return;
    }
    for mutation in [
        "current",
        "wrong_chart",
        "wrong_version",
        "noncanonical",
        "retired",
        "foreign_source",
        "mixed",
        "bare_digest",
        "invalid_digest",
        "changed_archive",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("veoveo-1.2.3.tgz");
        std::fs::write(&archive, b"selected archive").unwrap();
        let release = HelmRelease {
            output: directory.path().to_owned(),
            artifacts: vec![HelmArtifact {
                name: "veoveo",
                filename: "veoveo-1.2.3.tgz".to_owned(),
                sha256: sha256_file(&archive).unwrap(),
                archive,
                oci: None,
            }],
        };
        write_evidence(&release, "1.2.3", "selected-source", "v4.3.0").unwrap();
        let target = directory.path().join("release-evidence.json");
        let mut evidence: Value = serde_json::from_slice(&std::fs::read(&target).unwrap()).unwrap();
        assert_eq!(
            evidence["artifacts"][0]["sha256"],
            serde_json::to_value(sha256_file(&release.artifacts[0].archive).unwrap()).unwrap()
        );
        assert!(
            evidence["artifacts"][0]["sha256"]
                .as_str()
                .unwrap()
                .starts_with("sha256:")
        );
        match mutation {
            "wrong_chart" | "wrong_version" | "noncanonical" => {
                let coordinate = match mutation {
                    "wrong_chart" => "oci://registry.test/charts/uav-sim:1.2.3",
                    "wrong_version" => "oci://registry.test/charts/veoveo:9.9.9",
                    _ => "oci://registry.test/charts/%76eoveo:1.2.3",
                };
                evidence["artifacts"][0]["oci"] = serde_json::json!({"coordinate": coordinate, "digest": format!("sha256:{}", "b".repeat(64))});
            }
            "retired" => {
                evidence["schemaVersion"] =
                    serde_json::json!("veoveo.ai/helm-chart-release-evidence/v0")
            }
            "foreign_source" => evidence["sourceRevision"] = serde_json::json!("foreign-source"),
            "mixed" => evidence["source_revision"] = evidence["sourceRevision"].clone(),
            "bare_digest" => evidence["artifacts"][0]["sha256"] = serde_json::json!("a".repeat(64)),
            "invalid_digest" => {
                evidence["artifacts"][0]["sha256"] = serde_json::json!("sha256:bad")
            }
            "changed_archive" => {
                std::fs::write(
                    directory.path().join("veoveo-1.2.3.tgz"),
                    b"changed archive",
                )
                .unwrap();
            }
            _ => {}
        }
        std::fs::write(&target, serde_json::to_vec(&evidence).unwrap()).unwrap();
        let before = std::fs::read(&target).unwrap();
        let driver = directory.path().join("helm");
        std::fs::write(&driver, format!("#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{}'\nif [ \"$1\" = version ]; then printf 'v4.3.0\\n'; else printf 'Digest: sha256:{}\\n'; fi\n", directory.path().join("effects").display(), "b".repeat(64))).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&driver, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let status = Command::new("timeout")
            .args(["10s"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "commands::helm::rollout_tests::helm_push_preflight_refuses_before_helm_effects",
                "--nocapture",
            ])
            .env(CHILD, directory.path())
            .env(
                "VEOVEO_HELM_PREFLIGHT_SELECTED_DIGEST",
                release.artifacts[0].sha256.as_str(),
            )
            .env(
                "VEOVEO_HELM_PREFLIGHT_ALLOWED",
                if mutation == "current" { "yes" } else { "no" },
            )
            .env(
                "PATH",
                std::env::join_paths(std::iter::once(directory.path().to_owned()).chain(
                    std::env::split_paths(
                        &std::env::var_os("PATH").expect("GNU timeout prerequisite PATH"),
                    ),
                ))
                .unwrap(),
            )
            .status()
            .unwrap();
        assert!(status.success(), "preflight case {mutation}: {status}");
        if mutation != "current" {
            assert_eq!(std::fs::read(&target).unwrap(), before);
        }
    }
}
