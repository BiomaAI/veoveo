//! Actual Helm wiring inputs and owner revisions; no cluster or image qualification.
use super::*;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn render(root: &Path, arguments: &[&str]) -> Result<Vec<Value>> {
    let output = Command::new("timeout")
        .args(["25s", "helm", "template"])
        .args(arguments)
        .current_dir(root)
        .output()
        .context("Helm and GNU timeout required")?;
    ensure!(
        output.status.success(),
        "Helm render: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_yaml_ng::Deserializer::from_slice(&output.stdout)
        .map(|doc| Value::deserialize(doc).map_err(Into::into))
        .collect()
}
fn config_map(objects: &[Value], name: Option<&str>) -> Result<ConfigMap> {
    let object = objects
        .iter()
        .find(|v| v["kind"] == "ConfigMap" && name.is_none_or(|name| v["metadata"]["name"] == name))
        .context("rendered ConfigMap")?;
    serde_json::from_value(object.clone()).context("typed ConfigMap")
}

#[test]
fn rendered_manager_inputs_bind_complete_owner_revisions() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scratch = Scratch(
        std::env::temp_dir().join(format!("veoveo-manager-render-{}", uuid::Uuid::now_v7())),
    );
    std::fs::create_dir(&scratch.0)?;
    let source: Value =
        serde_yaml_ng::from_slice(&std::fs::read(root.join("examples/bioma/values.yaml"))?)?;
    // This existing scoped Helm profile qualifies wiring, not production image/composition admission.
    let scoped = json!({"global":{"publicBaseUrl":source["global"]["publicBaseUrl"]},
        "gateway":{"agents":source["gateway"]["agents"]}, "agentManager":source["agentManager"],
        "consoleBff":{"workspace":source["consoleBff"]["workspace"]}});
    let values = scratch.0.join("values.json");
    std::fs::write(&values, serde_json::to_vec(&scoped)?)?;
    let plan = format!(
        "moduleInstallation.planJson={}",
        root.join("testing/fixtures/module-schema-consumer/module-plan.json")
            .display()
    );
    let manager_objects = render(
        &root,
        &[
            "manager-wiring",
            "deploy/helm/veoveo",
            "--namespace",
            "manager-wiring",
            "--kube-version",
            "1.36.0",
            "--values",
            values.to_str().context("values path")?,
            "--set",
            "installationPreset=custom",
            "--set",
            "components={agent-runtime-support,gateway,platform-store}",
            "--set",
            "gateway.controlPlaneRevision=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,gateway.auditRetentionDays=1,knowledge.existingConfigMap=knowledge-test,knowledge.existingSigningSecret=knowledge-test,knowledge.configurationRevision=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "--set-file",
            &plan,
        ],
    )?;
    let manager = config_map(&manager_objects, Some("veoveo-agent-manager"))?;
    let manager_json = manager.data.get("manager.json").context("manager.json")?;
    let config: Config = serde_json::from_str(manager_json)?;
    let template_objects = render(
        &root,
        &[
            "agent-template-wiring",
            "showcase/uav-sim/deploy/helm",
            "--set",
            "agentTemplate.enabled=true",
            "--show-only",
            "templates/agent-template.yaml",
        ],
    )?;
    let template_map = config_map(&template_objects, None)?;
    let digest = wire::runtime_config_revision(&template_map.data);
    ensure!(
        template_map.metadata.name == format!("uav-pilot-{}", &digest.as_str()[7..19]),
        "chart and owner ConfigMap digest differ"
    );
    let mut template = config
        .templates
        .iter()
        .find(|t| t.id.as_str() == "uav-pilot")
        .context("actual UAV template")?
        .clone();
    // Bind only the wiring fixture to the actual rendered producer; do not rewrite installation pins.
    template.workload.config_map = template_map.metadata.name.clone();
    template.workload.config_digest = digest.clone();
    resources::configuration_items(&template_map, &template)?;
    let template_revision = wire::runtime_template_revision(&template);
    let mut changed_map = template_map.clone();
    changed_map
        .data
        .get_mut("manifest.json")
        .context("manifest")?
        .push(' ');
    ensure!(
        resources::configuration_items(&changed_map, &template).is_err(),
        "changed ConfigMap must refuse"
    );
    let mut changed_template = template.clone();
    changed_template.name.push_str(" changed");
    ensure!(
        wire::runtime_template_revision(&changed_template) != template_revision,
        "complete template revision"
    );
    let models = config
        .models
        .iter()
        .map(|model| {
            let mut changed = model.clone();
            changed.model.push_str("-changed");
            assert_ne!(changed.revision(), model.revision());
            json!({"value":model, "revision":model.revision()})
        })
        .collect::<Vec<_>>();
    let current: Value = serde_json::from_str(manager_json)?;
    for (canonical, retired) in [
        ("gatewayUrl", "gateway_url"),
        ("storeEndpoint", "store_endpoint"),
        ("databaseCredentialRevision", "database_credential_revision"),
    ] {
        for keep_current in [false, true] {
            let mut bad = current.clone();
            let value = bad[canonical].clone();
            if !keep_current {
                bad.as_object_mut().unwrap().remove(canonical);
            }
            bad[retired] = value;
            ensure!(
                serde_json::from_value::<Config>(bad).is_err(),
                "retired/mixed manager member admitted"
            );
        }
    }
    let (_, native, _) = fixture();
    let native_bytes = serde_json::to_vec(&native.revision.content)?;
    let native_roundtrip: veoveo_agent_runtime::persistence::AgentContent =
        serde_json::from_slice(&native_bytes)?;
    ensure!(
        native_roundtrip.digest()? == native.revision.digest,
        "native content revision changed"
    );
    let captured = json!({"profile":"helm-wiring-fixture", "managerData":manager.data,
        "templateData":template_map.data, "configRevision":digest, "template":template,
        "templateRevision":template_revision, "models":models,
        "nativeContent":native.revision.content, "nativeContentRevision":native.revision.digest});
    let expected =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/rendered-installation.json");
    if let Some(path) = std::env::var_os("VEOVEO_AGENT_MANAGER_RENDER_CAPTURE") {
        let path = PathBuf::from(path);
        ensure!(path.is_absolute(), "capture path must be absolute");
        std::fs::write(path, serde_json::to_vec_pretty(&captured)?)?;
    } else {
        let published: Value = serde_json::from_slice(&std::fs::read(expected)?)?;
        ensure!(
            published == captured,
            "rendered manager inputs changed; recapture with the owner test"
        );
    }
    Ok(())
}
