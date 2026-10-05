use crate::{ComputerError, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use veoveo_task_runtime::TaskOwner;

pub(crate) fn digest(value: &impl Serialize) -> Result<String> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(value).map_err(|_| ComputerError::InvalidInput)?,
    )))
}

pub(crate) fn owner_key(owner: &TaskOwner) -> Result<String> {
    if owner.authority.tenant.as_str() != owner.tenant_key()
        || [
            &owner.principal_key,
            &owner.issuer,
            &owner.subject,
            &owner.profile,
        ]
        .into_iter()
        .any(|s| s.is_empty() || s.len() > 2048 || s.chars().any(char::is_control))
    {
        return Err(ComputerError::InvalidInput);
    }
    digest(&(
        "veoveo.computer.owner.v1",
        owner.tenant_key(),
        &owner.principal_key,
        owner.principal_kind,
        &owner.issuer,
        &owner.subject,
        &owner.profile,
        &owner.authority.work_context,
    ))
}

pub(crate) fn permits(stored: &TaskOwner, caller: &TaskOwner) -> Result<()> {
    if !same_resource_owner(stored, caller)?
        || !stored.data_labels.is_subset(&caller.data_labels)
        || !stored
            .authority
            .output_policy
            .data_labels
            .iter()
            .all(|label| caller.data_labels.contains(label.as_str()))
        || stored
            .authority
            .output_policy
            .classification
            .as_ref()
            .is_some_and(|label| !caller.data_labels.contains(label.as_str()))
    {
        return Err(ComputerError::NotFound);
    }
    Ok(())
}

/// Profiles govern current actions, while retained resources belong to a
/// principal in one Work Context. The original storage/encryption key stays intact.
pub(crate) fn same_resource_owner(stored: &TaskOwner, caller: &TaskOwner) -> Result<bool> {
    owner_key(stored)?;
    owner_key(caller)?;
    Ok(stored.tenant_key() == caller.tenant_key()
        && stored.principal_key == caller.principal_key
        && stored.principal_kind == caller.principal_kind
        && stored.issuer == caller.issuer
        && stored.subject == caller.subject
        && stored.authority.work_context == caller.authority.work_context)
}

/// A grant keeps the retained Computer binding while its accepted actor may use
/// another profile. Parsing an authority envelope alone does not verify a parent.
pub(crate) fn verify_retained_owner(
    stored: &TaskOwner,
    retained_key: &str,
    accepted: &TaskOwner,
) -> Result<()> {
    if owner_key(stored)? != retained_key || !same_resource_owner(stored, accepted)? {
        return Err(ComputerError::Unavailable);
    }
    Ok(())
}

pub(crate) fn can_mutate(caller: &TaskOwner) -> Result<()> {
    owner_key(caller)?;
    permits(caller, caller).map_err(|_| ComputerError::Forbidden)?;
    if !caller
        .authority
        .membership
        .allows(veoveo_types::WorkContextMembershipLevel::Contributor)
    {
        return Err(ComputerError::Forbidden);
    }
    Ok(())
}

pub(crate) fn quota_key(owner: &TaskOwner) -> Result<String> {
    // Client profiles and Work Contexts cannot multiply an owner's quota.
    owner_key(owner)?;
    digest(&(
        "veoveo.computer.quota.v1",
        owner.tenant_key(),
        &owner.principal_key,
        owner.principal_kind,
        &owner.issuer,
        &owner.subject,
    ))
}

/// Admit the same typed owner snapshot used by the shared Task producer.
pub(crate) fn stored_owner(
    owner: &veoveo_task_runtime::TaskOwner,
) -> crate::Result<veoveo_platform_store::TaskOwnerRecord> {
    veoveo_platform_store::TaskOwnerRecord::try_from(owner)
        .map_err(|_| crate::ComputerError::InvalidInput)
}

/// Preserve the native tenant selected by the shared Task admission producer.
pub(crate) fn task_tenant(
    owner: &veoveo_task_runtime::TaskOwner,
) -> crate::Result<veoveo_platform_store::RecordId> {
    owner_key(owner)?;
    veoveo_platform_store::deterministic_tenant_id(owner.tenant_key())
        .map(|tenant| tenant.record_id())
        .map_err(|_| crate::ComputerError::InvalidInput)
}

pub(crate) fn task_output(value: Option<&serde_json::Value>) -> Result<surrealdb::types::Value> {
    use surrealdb::types::SurrealValue;
    let Some(value) = value else {
        return Ok(surrealdb::types::Value::None);
    };
    if !value.is_object() {
        return Err(ComputerError::InvalidInput);
    }
    let surrealdb::types::Value::Object(mut envelope) =
        veoveo_platform_store::TaskResultRecord::new(value.clone()).into_value()
    else {
        unreachable!()
    };
    envelope
        .remove("payload")
        .ok_or(ComputerError::InvalidInput)
}
