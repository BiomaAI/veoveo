//! Versioned persistence adapter. Live policy consumes only the current target model.
use anyhow::{Result, anyhow, bail};
use veoveo_mcp_contract::{AuditEvent, GatewayAction, PolicyTarget};
use veoveo_platform_store::OpenObject;

mod v1;

const FORMAT_KEY: &str = "policy_event_format";
const CURRENT_FORMAT: &str = "veoveo.ai/gateway-policy-event/v2";

pub(super) fn mark_current(details: OpenObject) -> OpenObject {
    let mut fields = details.as_map().clone();
    fields.insert(FORMAT_KEY.into(), CURRENT_FORMAT.into());
    OpenObject::new(fields)
}

pub(super) fn decode(details: &OpenObject) -> Result<AuditEvent> {
    let fields = details.as_map();
    let legacy = match fields.get(FORMAT_KEY) {
        None => true, // The deployed, unversioned envelope is persistence version 1.
        Some(serde_json::Value::String(version)) if version == CURRENT_FORMAT => false,
        Some(_) => bail!("unsupported gateway policy audit format; upgrade the gateway reader"),
    };
    let value = fields
        .get("event")
        .ok_or_else(|| anyhow!("gateway policy audit event is missing"))?;
    let event = if legacy {
        serde_json::from_value::<v1::Event>(value.clone())
            .map_err(|_| anyhow!("invalid gateway policy audit v1 event; inspect the retained record with compatible tooling"))?
            .into_current()?
    } else {
        serde_json::from_value::<AuditEvent>(value.clone())
            .map_err(|_| anyhow!("invalid gateway policy audit v2 event; inspect the retained record with compatible tooling"))?
    };
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
