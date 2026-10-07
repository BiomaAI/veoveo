//! Published naming surfaces use the complete maintained MCP descriptor types.
use super::{NamingInspection, controlled_name};
use crate::schema_evidence::{NamingEvidence, SchemaEvidenceOrigin, ToolNameProjection};
use anyhow::{Result, ensure};
use rmcp::model::{Prompt, Resource, ResourceTemplate, Tool};
use schemars::Schema;
use serde_json::Value;
use std::collections::BTreeSet;

pub fn inspect_discovery(
    tools: &[Tool],
    resources: &[Resource],
    templates: &[ResourceTemplate],
    prompts: &[Prompt],
    extensions: Option<&rmcp::model::ExtensionCapabilities>,
    evidence: &NamingEvidence<'_>,
) -> Result<NamingInspection> {
    inspect_discovery_since(
        std::time::Instant::now(),
        tools,
        resources,
        templates,
        prompts,
        extensions,
        evidence,
    )
}

pub(crate) fn inspect_discovery_since(
    started: std::time::Instant,
    tools: &[Tool],
    resources: &[Resource],
    templates: &[ResourceTemplate],
    prompts: &[Prompt],
    extensions: Option<&rmcp::model::ExtensionCapabilities>,
    evidence: &NamingEvidence<'_>,
) -> Result<NamingInspection> {
    let mut inspection = NamingInspection {
        started,
        ..Default::default()
    };
    inspection.tick()?;

    // Selection admission is part of the original run budget.
    ensure!(
        evidence.bodies.len() <= super::MAX_NAMING_ROOTS
            && evidence.required_observations.len() <= super::MAX_NAMING_ROOTS,
        "owner evidence selection exceeds root limit"
    );
    for label in &evidence.required_observations {
        inspection.charge(&Value::String(label.as_str().into()))?;
    }
    for body in evidence.bodies {
        inspection.charge(&Value::String(body.label().into()))?;
    }
    evidence.validate()?;
    inspection.tick()?;
    let mut tool_names = BTreeSet::new();
    for tool in tools {
        inspection.charge(&Value::String(tool.name.to_string()))?;
        ensure!(
            tool_names.insert(tool.name.as_ref()),
            "duplicate discovered tool identity"
        );
        let local = match evidence.tool_names {
            ToolNameProjection::Local => veoveo_types::LocalToolName::parse(tool.name.as_ref())?,
            ToolNameProjection::Gateway => {
                veoveo_gateway_contract::GatewayToolName::parse(tool.name.as_ref())?
                    .parts()?
                    .1
            }
        };
        ensure!(
            controlled_name(local.as_str()),
            "tool local name violates D2 grammar"
        );
        inspection.schema(
            &format!("tools/{}/inputSchema", tool.name),
            &Schema::try_from(Value::Object(tool.input_schema.as_ref().clone()))?,
            None,
            SchemaEvidenceOrigin::Remote,
        )?;
        let output = tool
            .output_schema
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tool output schema is missing"))?;
        inspection.schema(
            &format!("tools/{}/outputSchema", tool.name),
            &Schema::try_from(Value::Object(output.as_ref().clone()))?,
            None,
            SchemaEvidenceOrigin::Remote,
        )?;
        if let Some(meta) = tool.meta.as_ref() {
            inspection.charge(&serde_json::to_value(meta)?)?;
            inspection.value_work(&serde_json::to_value(meta)?, 0)?;
            if let Some(value) = meta.get(veoveo_mcp_apps_extension::UI_META_KEY) {
                adopted::<veoveo_mcp_apps_extension::ToolUiMeta>(
                    value,
                    &mut inspection,
                    "tools/ui",
                )?;
            }
            if let Some(value) = meta.get(veoveo_mcp_knowledge_extension::EXTENSION_ID) {
                adopted::<veoveo_mcp_knowledge_extension::SearchDeclaration>(
                    value,
                    &mut inspection,
                    "tools/knowledge",
                )?;
            }
            if let Some(value) = meta.get(veoveo_gateway_contract::APP_TOOL_DEPENDENCIES_META_KEY) {
                adopted::<Vec<veoveo_gateway_contract::AppToolDependency>>(
                    value,
                    &mut inspection,
                    "tools/appDependencies",
                )?;
            }
        }
    }
    let mut prompt_names = BTreeSet::new();
    for prompt in prompts {
        inspection.charge(&Value::String(prompt.name.clone()))?;
        ensure!(
            prompt_names.insert(prompt.name.as_str()),
            "duplicate discovered prompt identity"
        );
        ensure!(
            controlled_name(&prompt.name),
            "prompt name violates unqualified D2 grammar"
        );
        let mut arguments = BTreeSet::new();
        for argument in prompt.arguments.as_deref().unwrap_or_default() {
            inspection.charge(&Value::String(argument.name.clone()))?;
            ensure!(
                arguments.insert(argument.name.as_str()) && controlled_name(&argument.name),
                "duplicate or invalid prompt argument name"
            );
        }
    }
    let mut resource_uris = BTreeSet::new();
    for resource in resources {
        inspection.charge(&Value::String(resource.uri.clone()))?;
        ensure!(
            resource_uris.insert(resource.uri.as_str()),
            "duplicate discovered resource URI"
        );
        if let Some(meta) = resource.meta.as_ref() {
            inspection.charge(&serde_json::to_value(meta)?)?;
            inspection.value_work(&serde_json::to_value(meta)?, 0)?;
            if let Some(value) = meta.get(veoveo_mcp_apps_extension::UI_META_KEY) {
                adopted::<veoveo_mcp_apps_extension::ResourceUiMeta>(
                    value,
                    &mut inspection,
                    "resources/ui",
                )?;
            }
            if let Some(value) =
                meta.get(veoveo_gateway_contract::APP_RESOURCE_DEPENDENCIES_META_KEY)
            {
                adopted::<Vec<veoveo_gateway_contract::AppResourceDependency>>(
                    value,
                    &mut inspection,
                    "resources/appDependencies",
                )?;
            }
            if let Some(value) = meta.get(veoveo_mcp_apps_extension::AGENT_MESSAGE_TARGETS_META_KEY)
            {
                let targets: Vec<String> = serde_json::from_value(value.clone())?;
                ensure!(
                    veoveo_mcp_apps_extension::AgentMessageTargets::new(targets).is_some(),
                    "invalid adopted agent-message targets"
                );
                inspection.charge(value)?;
            }
        }
    }
    let mut addresses = BTreeSet::new();
    for template in templates {
        inspection.charge(&Value::String(template.uri_template.clone()))?;
        ensure!(
            addresses.insert(template.uri_template.as_str()),
            "duplicate discovered resource template"
        );
        if let Some(meta) = &template.meta {
            inspection.charge(&serde_json::to_value(meta)?)?;
            inspection.value_work(&serde_json::to_value(meta)?, 0)?;
        }
        if let Some(value) = template
            .meta
            .as_ref()
            .and_then(|meta| meta.get(veoveo_mcp_knowledge_extension::EXTENSION_ID))
        {
            adopted::<veoveo_mcp_knowledge_extension::CollectionDescriptor>(
                value,
                &mut inspection,
                "templates/knowledge",
            )?;
        }
        // The foundational maintained URI-template admission owns parsing. Its
        // declared variable names are D2 names, not camelCase DTO fields.
        let uri = veoveo_types::ResourceTemplateUri::new(&template.uri_template)?;
        for variable in uri.variables() {
            inspection.charge(&Value::String(variable.to_owned()))?;
            ensure!(
                controlled_name(variable),
                "template variable violates D2 grammar"
            );
        }
    }
    if let Some(extensions) = extensions {
        inspection.charge(&serde_json::to_value(extensions)?)?;
        inspection.value_work(&serde_json::to_value(extensions)?, 0)?;
        if let Some(value) = extensions.get(veoveo_mcp_apps_extension::EXTENSION_ID) {
            adopted::<veoveo_mcp_apps_extension::AppExtensionCapability>(
                &Value::Object(value.clone()),
                &mut inspection,
                "capabilities/apps",
            )?;
        }
        if let Some(value) = extensions.get(veoveo_mcp_knowledge_extension::EXTENSION_ID) {
            ensure!(
                value.is_empty(),
                "unsupported adopted knowledge capability settings"
            );
            inspection.tick()?;
        }
    }
    for body in evidence.bodies {
        inspection.owner(body)?;
    }
    Ok(inspection)
}

// These are the actual adopted owners, rather than raw recursive DTO guesses.
fn adopted<T: schemars::JsonSchema + serde::de::DeserializeOwned>(
    value: &Value,
    inspection: &mut NamingInspection,
    location: &str,
) -> Result<()> {
    inspection.charge(value)?;
    inspection.value_work(value, 0)?;
    serde_json::from_value::<T>(value.clone())?;
    inspection.tick()?;
    let schema = schemars::SchemaGenerator::default().into_root_schema_for::<T>();
    inspection.schema(location, &schema, None, SchemaEvidenceOrigin::Remote)?;
    let validator = jsonschema::validator_for(schema.as_value())?;
    inspection.tick()?;
    ensure!(
        validator.is_valid(value),
        "adopted metadata does not satisfy owner schema"
    );
    inspection.tick()?;
    Ok(())
}

/// Turn the finite inspection into its report check. Empty or missing owner
/// evidence is an incomplete selection, never successful naming qualification.
pub fn check_discovery(
    complete: bool,
    tools: &[Tool],
    resources: &[Resource],
    templates: &[ResourceTemplate],
    prompts: &[Prompt],
    extensions: Option<&rmcp::model::ExtensionCapabilities>,
    evidence: &NamingEvidence<'_>,
) -> crate::CheckResult {
    check_discovery_since(
        std::time::Instant::now(),
        complete,
        tools,
        resources,
        templates,
        prompts,
        extensions,
        evidence,
    )
}

pub(crate) fn check_discovery_since(
    started: std::time::Instant,
    complete: bool,
    tools: &[Tool],
    resources: &[Resource],
    templates: &[ResourceTemplate],
    prompts: &[Prompt],
    extensions: Option<&rmcp::model::ExtensionCapabilities>,
    evidence: &NamingEvidence<'_>,
) -> crate::CheckResult {
    use crate::{CheckResult, CheckStatus};
    let mut result = CheckResult {
        requirement_id: "VV-MCP-NAMING-001".into(),
        status: CheckStatus::Incomplete,
        summary: "C33 requires complete discovery and selected owner observations".into(),
        evidence: None,
    };
    if !complete || !evidence.has_required_observations() {
        return result;
    }
    match inspect_discovery_since(
        started, tools, resources, templates, prompts, extensions, evidence,
    ) {
        Ok(inspection) if !inspection.roots().is_empty() => {
            result.status = CheckStatus::Passed;
            result.summary = "C33 naming inspection completed; owner and exception declarations require source review".into();
            result.evidence = Some(serde_json::json!({
                "requirement":"C33", "mode":crate::requirements::VerificationMode::Mixed,
                "outcome":crate::requirements::RequirementOutcome::ReviewRequired,
                "roots":inspection.roots(),
            }));
        }
        Ok(_) => result.summary = "C33 received no schema roots to inspect".into(),
        Err(error) => {
            result.status = CheckStatus::Failed;
            result.summary = format!("C33 naming inspection failed: {error}");
        }
    }
    result
}
