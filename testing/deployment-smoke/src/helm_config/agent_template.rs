//! The showcase packages a reviewed template; the manager owns every pilot instance.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, fs};
use veoveo_mcp_contract::agent_management::{RuntimeTemplate, runtime_config_revision};

pub(super) fn verify(rendered: &str) -> Result<()> {
    let documents: Vec<Value> = serde_yaml_ng::Deserializer::from_str(rendered)
        .map(Value::deserialize)
        .collect::<std::result::Result<_, _>>()?;
    let templates: Vec<_> = documents
        .iter()
        .filter(|document| {
            document["kind"] == "ConfigMap"
                && document["metadata"]["labels"]["app.kubernetes.io/component"] == "agent-template"
        })
        .collect();
    ensure!(
        templates.len() == 1,
        "Bioma must install one reviewed pilot template"
    );
    let object = templates[0];
    ensure!(
        object["immutable"] == true,
        "pilot template must be immutable"
    );
    let data: BTreeMap<String, String> = serde_json::from_value(object["data"].clone())?;
    ensure!(
        data.len() == 2
            && data.contains_key("manifest.json")
            && data.contains_key("0001_mission_state.sql"),
        "unexpected pilot template contents"
    );
    let revision = runtime_config_revision(&data);
    let values: Value =
        serde_yaml_ng::from_str(&fs::read_to_string("examples/bioma/values.yaml")?)?;
    let approved: Vec<RuntimeTemplate> =
        serde_json::from_value(values["gateway"]["agents"]["templates"].clone())?;
    let approved = approved
        .iter()
        .find(|template| template.id.as_str() == "uav-pilot")
        .context("Bioma's approved pilot runtime template")?;
    ensure!(
        object["metadata"]["name"] == approved.workload.config_map
            && object["metadata"]["namespace"] == approved.workload.namespace
            && revision == approved.workload.config_digest,
        "packaged pilot data differs from the approved managed template"
    );
    for document in &documents {
        let kind = document["kind"].as_str().unwrap_or_default();
        let name = document["metadata"]["name"].as_str().unwrap_or_default();
        ensure!(
            !matches!(kind, "Secret" | "ServiceAccount"),
            "UAV packaging must not provision pilot credentials or identities"
        );
        if kind == "Deployment" {
            ensure!(
                matches!(name, "uav-sim" | "uav-sim-mcp"),
                "UAV chart still owns a pilot workload"
            );
        }
        if kind == "PersistentVolumeClaim" {
            ensure!(
                matches!(
                    name,
                    "uav-sim-runtime-cache" | "uav-sim-recording-forwarder"
                ),
                "UAV chart still owns pilot memory"
            );
        }
    }
    ensure!(
        !rendered.contains("UAV_SIM_AGENT_MESSAGE_TARGETS")
            && !rendered.contains("veoveo/agent-kernel@"),
        "pilot instances or App message targets still come from Helm"
    );
    Ok(())
}
