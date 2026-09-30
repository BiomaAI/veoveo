//! The activated revision and each changed Work Context share one commit.
use anyhow::Result;
use std::collections::BTreeMap;
use veoveo_audit_contract::{
    AccountActivity, AuditContext, AuditDetail, AuditOutcome, AuditReason, AuditTarget,
};
use veoveo_mcp_contract::WorkContextDefinition;
use veoveo_platform_store::audit::AuditTransactionWrite;

pub(super) fn changes(
    before: &[WorkContextDefinition],
    after: &[WorkContextDefinition],
    context: &AuditContext,
) -> Result<AuditTransactionWrite> {
    let before = before
        .iter()
        .map(|value| ((&value.tenant, &value.id), value))
        .collect::<BTreeMap<_, _>>();
    let after = after
        .iter()
        .map(|value| ((&value.tenant, &value.id), value))
        .collect::<BTreeMap<_, _>>();
    let mut records = Vec::new();
    for (key, value) in &after {
        let activity = match before.get(key) {
            None => AccountActivity::Create,
            Some(old) if *old == *value => continue,
            Some(_) => AccountActivity::Update,
        };
        records.push(context.draft(
            AuditTarget::WorkContext {
                tenant: value.tenant.clone(),
                context: value.id.clone(),
            },
            AuditDetail::AccountChange { activity },
            AuditOutcome::Succeeded,
            AuditReason::Accepted,
        )?);
    }
    for (key, value) in &before {
        if !after.contains_key(key) {
            records.push(context.draft(
                AuditTarget::WorkContext {
                    tenant: value.tenant.clone(),
                    context: value.id.clone(),
                },
                AuditDetail::AccountChange {
                    activity: AccountActivity::Delete,
                },
                AuditOutcome::Succeeded,
                AuditReason::Accepted,
            )?);
        }
    }
    Ok(AuditTransactionWrite::batch(records)?)
}
