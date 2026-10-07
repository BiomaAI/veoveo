//! Public installation inputs; composition admission belongs to the image host.

use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_modules::{ModulePlanDocument, ModuleSelectionDocument};
use veoveo_types::Vocabulary;

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
                PlatformComponent::ALL
                    .iter()
                    .copied()
                    .find(|component| component.helm_value() == key.as_str())
                    .context("module plan names an unsupported platform component")
            })
            .transpose()?;
        let server = binding
            .mcp_server
            .as_ref()
            .map(|key| {
                FirstPartyMcpServer::from_wire(key.as_str())
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

    fn actual_fixture(name: &str) -> Value {
        serde_json::from_slice(
            &std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../testing/fixtures/module-schema-consumer")
                    .join(name),
            )
            .unwrap(),
        )
        .unwrap()
    }
    fn platform(
        components: &[PlatformComponent],
        servers: &[FirstPartyMcpServer],
    ) -> ResolvedPlatformSelection {
        ResolvedPlatformSelection {
            computer_capacity: Default::default(),
            components: components.iter().copied().collect(),
            mcp_servers: servers.iter().copied().collect(),
            artifact_audiences: Default::default(),
            workloads: Default::default(),
            gpu_scheduling: None,
        }
    }
    fn decode(value: &Value) -> ModulePlanDocument {
        serde_json::from_value(value.clone()).unwrap()
    }

    #[test]
    fn actual_generated_plan_checks_identity_generation_and_execution_profile() {
        let fixture = actual_fixture("module-plan.json");
        let plan = decode(&fixture);
        let selection = plan.selection().unwrap();
        let platform = platform(
            &[
                PlatformComponent::Gateway,
                PlatformComponent::PlatformStore,
                PlatformComponent::AgentRuntimeSupport,
                PlatformComponent::RecordingDataPlane,
            ],
            &[FirstPartyMcpServer::Map, FirstPartyMcpServer::Recording],
        );
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
        let mut fixture = actual_fixture("module-plan.json");
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
                &platform(&[PlatformComponent::Gateway], &[FirstPartyMcpServer::Map])
            )
            .is_err()
        );
        validate_module_plan(
            &plan,
            &selection,
            plan.composition().as_str(),
            &platform(&[PlatformComponent::Gateway], &[]),
        )
        .unwrap();
        let kernels = decode(&actual_fixture("kernels-plan.json"));
        let kernel_selection = kernels.selection().unwrap();
        validate_module_plan(
            &kernels,
            &kernel_selection,
            kernels.composition().as_str(),
            &platform(&[PlatformComponent::Gateway], &[]),
        )
        .unwrap();
        for component in [
            PlatformComponent::AgentRuntimeSupport,
            PlatformComponent::RecordingDataPlane,
        ] {
            let error = validate_module_plan(
                &kernels,
                &kernel_selection,
                kernels.composition().as_str(),
                &platform(&[component], &[]),
            )
            .unwrap_err();
            assert!(error.to_string().contains("requires selected module"));
        }
        // Both present keys form one conjunction. Separate rows express alternatives.
        fixture["runtimeBindings"] =
            json!([{"module":"map","component":"console","mcpServer":"map"}]);
        let plan = decode(&fixture);
        validate_module_plan(
            &plan,
            &selection,
            plan.composition().as_str(),
            &platform(&[PlatformComponent::Gateway], &[FirstPartyMcpServer::Map]),
        )
        .unwrap();
        assert!(
            validate_module_plan(
                &plan,
                &selection,
                plan.composition().as_str(),
                &platform(
                    &[PlatformComponent::Gateway, PlatformComponent::Console],
                    &[FirstPartyMcpServer::Map],
                )
            )
            .is_err()
        );
        for key in [
            "invented-host",
            "agent_runtime_support",
            "recording_data_plane",
        ] {
            fixture["runtimeBindings"][0]["component"] = key.into();
            let invalid = decode(&fixture);
            let error = validate_module_plan(
                &invalid,
                &selection,
                invalid.composition().as_str(),
                &platform(&[], &[]),
            )
            .unwrap_err();
            assert!(error.to_string().contains("unsupported platform component"));
        }
        fixture["runtimeBindings"][0]["component"] = "console".into();
        for key in ["invented-server", "Map"] {
            fixture["runtimeBindings"][0]["mcpServer"] = key.into();
            let invalid = decode(&fixture);
            let error = validate_module_plan(
                &invalid,
                &selection,
                invalid.composition().as_str(),
                &platform(&[], &[]),
            )
            .unwrap_err();
            assert!(error.to_string().contains("unsupported hosted MCP server"));
        }
    }
}
