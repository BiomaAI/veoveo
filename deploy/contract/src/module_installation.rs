//! Public installation inputs; composition admission belongs to the image host.

use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_modules::{ModulePlanDocument, ModuleSelectionDocument};

use crate::{FirstPartyMcpServer, PlatformComponent, ResolvedPlatformSelection};

/// Configuration consumed by the offline composition producer and rendered Jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleInstallationSpec {
    /// Installation-relative public ModuleSelectionDocument file.
    pub selection: PathBuf,
    /// Actual composition-generated plan for pre-image, non-installed validation.
    /// Locked compilation always generates its own plan from the qualified image.
    pub development_plan: Option<PathBuf>,
}

/// Checks a producer's public bindings without registering owner vocabulary here.
/// The caller binds the producer to its qualified image and checks exact rendering.
pub fn validate_module_plan(
    plan: &ModulePlanDocument,
    selection: &ModuleSelectionDocument,
    composition: &str,
    platform: &ResolvedPlatformSelection,
) -> Result<()> {
    ensure!(
        plan.composition().as_str() == composition,
        "module plan composition differs from selected image"
    );
    ensure!(
        plan.selection()? == *selection,
        "module plan differs from installation selection"
    );
    ensure!(
        !plan.lanes().is_empty(),
        "module plan must contain selected lanes"
    );
    for lane in plan.lanes() {
        ensure!(
            lane.image.as_str() == "gateway",
            "module lane selects an unsupported composition image"
        );
        ensure!(
            lane.command.argv()
                == [
                    "/usr/local/bin/gateway",
                    "module-migrate",
                    "--module",
                    lane.module.as_str()
                ],
            "module lane command differs from the supported gateway execution profile"
        );
    }
    for binding in plan.runtime_bindings() {
        ensure!(
            binding.component.is_some() || binding.mcp_server.is_some(),
            "module plan runtime binding requires a concrete consumer"
        );
        let component = binding
            .component
            .as_ref()
            .map(|key| {
                serde_json::from_value::<PlatformComponent>(serde_json::Value::String(
                    key.as_str().into(),
                ))
                .context("module plan names an unsupported platform component")
            })
            .transpose()?;
        let server = binding
            .mcp_server
            .as_ref()
            .map(|key| {
                serde_json::from_value::<FirstPartyMcpServer>(serde_json::Value::String(
                    key.as_str().into(),
                ))
                .context("module plan names an unsupported hosted MCP server")
            })
            .transpose()?;
        let enabled = (component.is_some() || server.is_some())
            && component.is_none_or(|key| platform.components.contains(&key))
            && server.is_none_or(|key| platform.mcp_servers.contains(&key));
        ensure!(
            !enabled
                || plan
                    .lanes()
                    .iter()
                    .any(|lane| lane.module == binding.module),
            "enabled runtime consumer requires selected module {}",
            binding.module
        );
    }
    Ok(())
}

impl ModuleInstallationSpec {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.selection.as_os_str().is_empty(),
            "module selection path is required"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn actual_fixture() -> Value {
        serde_json::from_slice(
            &std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../testing/fixtures/module-schema-consumer/module-plan.json"),
            )
            .unwrap(),
        )
        .unwrap()
    }
    fn platform(components: &[&str], servers: &[&str]) -> ResolvedPlatformSelection {
        serde_json::from_value(json!({"components":components,"mcpServers":servers,"artifactAudiences":[],"gpuScheduling":null})).unwrap()
    }
    fn decode(value: &Value) -> ModulePlanDocument {
        serde_json::from_value(value.clone()).unwrap()
    }

    #[test]
    fn actual_generated_plan_checks_identity_generation_and_execution_profile() {
        let fixture = actual_fixture();
        let plan = decode(&fixture);
        let selection = plan.selection().unwrap();
        let platform = platform(&["gateway", "platform-store"], &["map"]);
        validate_module_plan(&plan, &selection, plan.composition().as_str(), &platform).unwrap();
        assert!(
            validate_module_plan(
                &plan,
                &selection,
                &format!("sha256:{}", "0".repeat(64)),
                &platform
            )
            .is_err()
        );
        let mut changed_selection = serde_json::to_value(&selection).unwrap();
        changed_selection["generation"] = "2".into();
        assert!(
            validate_module_plan(
                &plan,
                &serde_json::from_value(changed_selection).unwrap(),
                plan.composition().as_str(),
                &platform
            )
            .is_err()
        );
        let mut changed = fixture.clone();
        changed["lanes"][0]["command"] = json!(["/bin/sh", "-c", "echo bypass"]);
        let invalid = decode(&changed);
        assert!(
            validate_module_plan(&invalid, &selection, plan.composition().as_str(), &platform)
                .is_err()
        );
    }

    #[test]
    fn enabled_hosts_require_lanes_without_enabling_unrequested_hosts() {
        let mut fixture = actual_fixture();
        fixture["enabled"]
            .as_array_mut()
            .unwrap()
            .retain(|module| module != "map");
        fixture["lanes"]
            .as_array_mut()
            .unwrap()
            .retain(|lane| lane["module"] != "map");
        let plan = decode(&fixture);
        let selection = plan.selection().unwrap();
        assert!(
            validate_module_plan(
                &plan,
                &selection,
                plan.composition().as_str(),
                &platform(&["gateway"], &["map"])
            )
            .is_err()
        );
        validate_module_plan(
            &plan,
            &selection,
            plan.composition().as_str(),
            &platform(&["gateway"], &[]),
        )
        .unwrap();
        // Both present keys form one conjunction. Separate rows express alternatives.
        fixture["runtimeBindings"] =
            json!([{"module":"map","component":"console","mcpServer":"map"}]);
        let plan = decode(&fixture);
        validate_module_plan(
            &plan,
            &selection,
            plan.composition().as_str(),
            &platform(&["gateway"], &["map"]),
        )
        .unwrap();
        assert!(
            validate_module_plan(
                &plan,
                &selection,
                plan.composition().as_str(),
                &platform(&["gateway", "console"], &["map"])
            )
            .is_err()
        );
        fixture["runtimeBindings"][0]["component"] = "invented-host".into();
        let invalid = decode(&fixture);
        assert!(
            validate_module_plan(
                &invalid,
                &selection,
                invalid.composition().as_str(),
                &platform(&[], &[])
            )
            .is_err()
        );
    }
}
