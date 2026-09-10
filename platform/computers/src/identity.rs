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
    if owner_key(stored)? != owner_key(caller)?
        || !stored.data_labels.is_subset(&caller.data_labels)
        || !stored
            .authority
            .output_policy
            .data_labels
            .iter()
            .all(|label| caller.data_labels.contains(label.as_str()))
    {
        return Err(ComputerError::NotFound);
    }
    Ok(())
}

pub(crate) fn can_mutate(caller: &TaskOwner) -> Result<()> {
    owner_key(caller)?;
    permits(caller, caller).map_err(|_| ComputerError::Forbidden)?;
    if !caller
        .authority
        .membership
        .allows(veoveo_mcp_contract::WorkContextMembershipLevel::Contributor)
    {
        return Err(ComputerError::Forbidden);
    }
    Ok(())
}

pub(crate) fn quota_key(owner: &TaskOwner) -> Result<String> {
    // Profile and Work Context isolate access, but cannot multiply an owner's quota.
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
