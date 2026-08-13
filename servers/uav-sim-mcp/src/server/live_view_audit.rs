use std::{collections::BTreeMap, sync::Arc};

use chrono::Utc;
use serde_json::Value;
use veoveo_mcp_contract::{GatewayInternalIdentity, LiveViewId, LiveViewState};
use veoveo_platform_store::{
    AuditEventId, AuditEventRecord, AuditOutcome, OpenObject, PlatformStore,
    deterministic_principal_id, deterministic_tenant_id,
};

#[derive(Clone)]
pub(super) struct LiveViewAudit {
    store: PlatformStore,
}

impl LiveViewAudit {
    pub(super) fn new(store: PlatformStore) -> Arc<Self> {
        Arc::new(Self { store })
    }

    pub(super) async fn append(
        &self,
        identity: &GatewayInternalIdentity,
        live_view_id: Option<&LiveViewId>,
        action: &'static str,
        outcome: AuditOutcome,
        details: BTreeMap<String, Value>,
    ) -> anyhow::Result<()> {
        let tenant_key = identity.authority.tenant.as_str();
        let tenant = deterministic_tenant_id(tenant_key)?.record_id();
        let actor = deterministic_principal_id(tenant_key, identity.actor.id.as_str())?.record_id();
        let resource_id = live_view_id.map(ToString::to_string);
        self.store
            .append_live_view_audit(AuditEventRecord {
                id: AuditEventId::new().record_id(),
                tenant: Some(tenant),
                actor: Some(actor),
                action: action.to_owned(),
                resource_type: "simulator_live_view".to_owned(),
                resource_id: resource_id.clone(),
                outcome,
                request_id: None,
                trace_id: None,
                source_ip: None,
                details: OpenObject::new(details),
                occurred_at: Utc::now(),
                search_text: format!(
                    "simulator_live_view {action} {}",
                    resource_id.as_deref().unwrap_or("unallocated")
                ),
            })
            .await?;
        Ok(())
    }

    pub(super) async fn append_lease(
        &self,
        lease: &LiveViewState,
        action: &'static str,
        outcome: AuditOutcome,
        mut details: BTreeMap<String, Value>,
    ) -> anyhow::Result<()> {
        let tenant_key = lease.owner.tenant.as_str();
        details.insert(
            "session_id".to_owned(),
            Value::String(lease.session_id.to_string()),
        );
        details.insert(
            "camera_id".to_owned(),
            Value::String(lease.camera_id.to_string()),
        );
        details.insert(
            "work_context".to_owned(),
            Value::String(lease.owner.work_context.to_string()),
        );
        self.store
            .append_live_view_audit(AuditEventRecord {
                id: AuditEventId::new().record_id(),
                tenant: Some(deterministic_tenant_id(tenant_key)?.record_id()),
                actor: Some(
                    deterministic_principal_id(tenant_key, lease.viewer_actor.as_str())?
                        .record_id(),
                ),
                action: action.to_owned(),
                resource_type: "simulator_live_view".to_owned(),
                resource_id: Some(lease.live_view_id.to_string()),
                outcome,
                request_id: None,
                trace_id: None,
                source_ip: None,
                details: OpenObject::new(details),
                occurred_at: Utc::now(),
                search_text: format!("simulator_live_view {action} {}", lease.live_view_id),
            })
            .await?;
        Ok(())
    }
}
