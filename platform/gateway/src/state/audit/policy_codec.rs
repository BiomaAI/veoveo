//! Current policy-event persistence and action/target validation.
use anyhow::{Result, anyhow, bail};
use veoveo_mcp_contract::{AuditEvent, GatewayAction, PolicyTarget};
use veoveo_platform_store::OpenObject;

const FORMAT_KEY: &str = "policy_event_format";
const CURRENT_FORMAT: &str = "veoveo.ai/gateway-policy-event/v2";

pub(super) fn mark_current(details: OpenObject) -> OpenObject {
    let mut fields = details.as_map().clone();
    fields.insert(FORMAT_KEY.into(), CURRENT_FORMAT.into());
    OpenObject::new(fields)
}

pub(super) fn decode(details: &OpenObject) -> Result<AuditEvent> {
    let fields = details.as_map();
    if !matches!(fields.get(FORMAT_KEY), Some(serde_json::Value::String(version)) if version == CURRENT_FORMAT)
    {
        bail!("gateway policy audit record requires the current format");
    }
    let value = fields
        .get("event")
        .ok_or_else(|| anyhow!("gateway policy audit event is missing"))?;
    let event = serde_json::from_value::<AuditEvent>(value.clone())
        .map_err(|_| anyhow!("gateway policy audit event does not satisfy the current contract"))?;
    validate(&event)?;
    Ok(event)
}

pub(super) fn validate(event: &AuditEvent) -> Result<()> {
    if event.action != event.decision.action || event.target != event.decision.target {
        bail!("gateway policy audit event and decision disagree on action or target");
    }
    let template_action = matches!(
        event.action,
        GatewayAction::CompletionComplete | GatewayAction::ResourcesTemplatesList
    );
    match &event.target {
        PolicyTarget::ResourceTemplate { .. } if !template_action => bail!(
            "gateway policy audit resource-template target requires completion or template discovery"
        ),
        PolicyTarget::Resource { .. }
        | PolicyTarget::Artifact { .. }
        | PolicyTarget::Usage { .. }
            if template_action =>
        {
            bail!(
                "gateway policy audit completion or template discovery requires a typed template target"
            )
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
