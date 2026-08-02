use std::sync::Arc;

#[cfg(test)]
use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{LiveCameraHealth, LiveViewLifecycle, LiveViewOwner};
use veoveo_platform_store::{
    AuditEventId, AuditEventRecord, AuditOutcome, OpenObject, PlatformStore,
    SimulationViewStateDraft, deterministic_tenant_id,
};

use crate::{
    contract::{ReconciliationStatus, SessionLifecycle},
    state::{DurableSimulationViewState, SimulationViewService},
};

#[derive(Clone, Debug)]
pub(crate) struct SimulationViewRepository {
    store: PlatformStore,
}

impl SimulationViewRepository {
    pub fn new(store: PlatformStore) -> Arc<Self> {
        Arc::new(Self { store })
    }

    pub async fn ready(&self) -> bool {
        self.store.client().health().await.is_ok()
    }

    pub async fn restore(&self, service: &SimulationViewService) -> Result<usize> {
        let records = self
            .store
            .simulation_view_states()
            .await
            .context("read durable Simulation View desired state")?;
        let mut restored = 0;
        for record in records {
            let mut desired: DurableSimulationViewState = serde_json::from_value(
                serde_json::Value::Object(record.snapshot.into_map().into_iter().collect()),
            )
            .context("decode durable Simulation View desired state")?;
            desired.session.reconciliation = serde_json::from_value(serde_json::Value::Object(
                record.reconciliation.into_map().into_iter().collect(),
            ))
            .context("decode durable Simulation View reconciliation status")?;
            service
                .restore_durable_state(desired)
                .context("restore durable Simulation View desired state")?;
            restored += 1;
        }
        Ok(restored)
    }

    pub async fn persist(
        &self,
        service: &SimulationViewService,
        session_id: &veoveo_mcp_contract::LiveSessionId,
    ) -> Result<()> {
        let mut desired = service
            .durable_state(session_id)
            .context("snapshot Simulation View desired state")?;
        let reconciliation = desired.session.reconciliation.clone();
        normalize_desired(&mut desired);
        let snapshot_value = serde_json::to_value(&desired)?;
        let snapshot_bytes = serde_json::to_vec(&snapshot_value)?;
        let snapshot_digest = hex_digest(&snapshot_bytes);
        let snapshot = object(snapshot_value, "Simulation View desired state")?;
        let reconciliation_object = object(
            serde_json::to_value(&reconciliation)?,
            "Simulation View reconciliation status",
        )?;
        let source = desired.session.pose_source.as_ref();
        self.store
            .commit_simulation_view_state(SimulationViewStateDraft {
                tenant_key: desired.session.owner.tenant.as_str().to_owned(),
                owner_key: serde_json::to_string(&desired.session.owner.subject)?,
                work_context_key: desired.session.owner.work_context.as_str().to_owned(),
                policy_revision: desired.session.owner.policy_revision.as_str().to_owned(),
                session_id: desired.session.session_id.as_str().to_owned(),
                epoch_id: desired.session.epoch_id.as_str().to_owned(),
                desired_revision: reconciliation.desired_revision,
                realized_revision: reconciliation.realized_revision,
                authorization_revision: reconciliation.authorization_revision,
                revoked: source.is_some_and(|value| value.revoked),
                authorization_expires_at: source.map(|value| value.expires_at),
                snapshot_digest,
                snapshot,
                reconciliation: reconciliation_object,
                updated_at: Utc::now(),
            })
            .await
            .context("commit Simulation View desired state")?;
        Ok(())
    }

    pub async fn audit(
        &self,
        owner: &LiveViewOwner,
        session_id: &str,
        action: &str,
        outcome: AuditOutcome,
        details: impl Serialize,
    ) -> Result<()> {
        let tenant = deterministic_tenant_id(owner.tenant.as_str())?.record_id();
        let occurred_at = Utc::now();
        self.store
            .append_simulation_view_audit(AuditEventRecord {
                id: AuditEventId::new().record_id(),
                tenant: Some(tenant),
                actor: None,
                action: action.to_owned(),
                resource_type: "simulation_view_reconciliation".to_owned(),
                resource_id: Some(session_id.to_owned()),
                outcome,
                request_id: None,
                trace_id: None,
                source_ip: None,
                details: object(serde_json::to_value(details)?, "audit details")?,
                occurred_at,
                search_text: format!("simulation_view_reconciliation {action} {session_id}"),
            })
            .await
            .context("append Simulation View reconciliation audit")
    }
}

fn normalize_desired(desired: &mut DurableSimulationViewState) {
    let desired_revision = desired.session.reconciliation.desired_revision;
    desired.session.reconciliation = ReconciliationStatus::pending(desired_revision);
    if desired.session.lifecycle != SessionLifecycle::Closed {
        desired.session.lifecycle = if desired.session.scene.is_some() {
            SessionLifecycle::SceneBound
        } else {
            SessionLifecycle::Created
        };
    }
    if let Some(source) = desired.session.pose_source.as_mut() {
        source.last_sequence = None;
        source.last_snapshot_at = None;
        source.stale = true;
    }
    for durable in &mut desired.cameras {
        durable.camera.health = LiveCameraHealth::Warming;
        durable.camera.last_pose_sequence = None;
        durable.camera.last_frame_at = None;
    }
    for lease in &mut desired.leases {
        lease.state.connected_viewers = 0;
        if !matches!(
            lease.state.lifecycle,
            LiveViewLifecycle::Closed | LiveViewLifecycle::Failed
        ) {
            lease.state.lifecycle = LiveViewLifecycle::Ready;
            lease.state.camera_health = LiveCameraHealth::Warming;
            lease.state.last_frame_at = None;
        }
    }
}

fn object(value: serde_json::Value, label: &'static str) -> Result<OpenObject> {
    match value {
        serde_json::Value::Object(values) => Ok(OpenObject::new(values.into_iter().collect())),
        _ => anyhow::bail!("{label} must serialize as an object"),
    }
}

fn hex_digest(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_stable_and_lowercase_hex() {
        assert_eq!(
            hex_digest(b"managed-session"),
            "6bc26d8cb052e27e7ac4b4bb616b3be4582500a0331b7b1a626424ec6efcafac"
        );
    }

    #[test]
    fn open_objects_reject_non_objects() {
        assert!(object(serde_json::json!([]), "fixture").is_err());
        assert_eq!(
            object(serde_json::json!({"healthy": true}), "fixture")
                .unwrap()
                .as_map(),
            &BTreeMap::from([("healthy".to_owned(), serde_json::json!(true))])
        );
    }
}
