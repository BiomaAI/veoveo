use std::{collections::HashMap, path::PathBuf};

use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use sha2::{Digest, Sha256};
use veoveo_duckdb_mcp::state::{DatabaseOwner, TaskOwner};
use veoveo_mcp_contract::{DuckDbDatabaseId, GatewayInternalIdentity, PlaneCaller};

use super::app_state::AppState;

pub(super) type TaskOwnerMap = HashMap<String, TaskOwner>;

pub(super) fn internal_identity(
    context: &RequestContext<RoleServer>,
) -> Result<GatewayInternalIdentity, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| McpError::invalid_request("authenticated HTTP context missing", None))?;
    parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))
}

/// Build the [`PlaneCaller`] for artifact-plane calls: the verified identity plus
/// the raw bearer to forward. Group memberships come from the signed identity
/// via Principal::group_memberships() (bare membership = Read).
pub(super) fn internal_caller(
    context: &RequestContext<RoleServer>,
) -> Result<PlaneCaller, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| McpError::invalid_request("authenticated HTTP context missing", None))?;
    let identity = parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))?;
    let bearer = parts
        .extensions
        .get::<super::internal_auth::ForwardedBearer>()
        .map(|b| b.0.clone())
        .ok_or_else(|| McpError::invalid_request("forwarded bearer missing", None))?;
    Ok(caller_from(identity, bearer))
}

/// Assemble a [`PlaneCaller`] from a verified identity and its raw bearer.
pub(super) fn caller_from(identity: GatewayInternalIdentity, bearer: String) -> PlaneCaller {
    let memberships = identity.principal.group_memberships();
    PlaneCaller {
        bearer_token: bearer,
        identity,
        memberships,
    }
}

pub(super) fn task_owner_from_identity(
    task_id: &str,
    identity: &GatewayInternalIdentity,
) -> TaskOwner {
    TaskOwner {
        task_id: task_id.to_string(),
        principal_id: identity.principal.id.clone(),
        profile: identity.profile.clone(),
        tenant: identity.principal.tenant.clone(),
        data_labels: identity.principal.data_labels.clone(),
    }
}

pub(super) async fn optional_task_owner(
    state: &AppState,
    task_id: &str,
) -> Result<Option<TaskOwner>, McpError> {
    if let Some(owner) = state.task_owners.read().await.get(task_id).cloned() {
        return Ok(Some(owner));
    }
    let Some(owner) = state
        .durable
        .task_owner(task_id)
        .map_err(|err| McpError::internal_error(err.to_string(), None))?
    else {
        return Ok(None);
    };
    state
        .task_owners
        .write()
        .await
        .insert(task_id.to_string(), owner.clone());
    Ok(Some(owner))
}

pub(super) async fn require_task_owner(
    state: &AppState,
    context: &RequestContext<RoleServer>,
    task_id: &str,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    let owner = optional_task_owner(state, task_id)
        .await?
        .ok_or_else(|| McpError::invalid_request("task ownership record missing", None))?;
    if task_owner_allows(&owner, &identity) {
        Ok(identity)
    } else {
        Err(McpError::invalid_request(
            "duckdb task policy denied request",
            None,
        ))
    }
}

pub(super) fn task_owner_allows(owner: &TaskOwner, identity: &GatewayInternalIdentity) -> bool {
    owner.principal_id == identity.principal.id
        && owner.profile == identity.profile
        && owner.tenant == identity.principal.tenant
        && owner.data_labels.is_subset(&identity.principal.data_labels)
}

/// Writers must be the owning principal. Readers may also be tenant peers:
/// same tenant, and the database's data labels covered by the reader's.
pub(super) fn database_readable(owner: &DatabaseOwner, identity: &GatewayInternalIdentity) -> bool {
    if database_writable(owner, identity) {
        return true;
    }
    match (&owner.tenant, &identity.principal.tenant) {
        (Some(db_tenant), Some(reader_tenant)) => {
            db_tenant == reader_tenant
                && owner.data_labels.is_subset(&identity.principal.data_labels)
        }
        _ => false,
    }
}

pub(super) fn database_writable(owner: &DatabaseOwner, identity: &GatewayInternalIdentity) -> bool {
    owner.principal_id == identity.principal.id
        && owner.tenant == identity.principal.tenant
        && owner.data_labels.is_subset(&identity.principal.data_labels)
}

/// Databases live under an owner-scoped directory so equal db ids from
/// different principals never collide on the filesystem.
pub(super) fn database_file_path(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    db_id: &DuckDbDatabaseId,
) -> PathBuf {
    let principal_digest = hex::encode(Sha256::digest(identity.principal.id.as_str().as_bytes()));
    state
        .dirs
        .database_dir
        .join(&principal_digest[..16])
        .join(format!("{db_id}.duckdb"))
}

/// Resolve a database for reading: prefer the caller's own database, then a
/// single tenant-visible one. Ambiguity is an error rather than a guess.
pub(super) fn resolve_readable_database(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    db_id: &DuckDbDatabaseId,
) -> Result<DatabaseOwner, McpError> {
    let owners = state
        .durable
        .database_owners(db_id)
        .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    if let Some(own) = owners
        .iter()
        .find(|owner| database_writable(owner, identity))
    {
        return Ok(own.clone());
    }
    let visible: Vec<&DatabaseOwner> = owners
        .iter()
        .filter(|owner| database_readable(owner, identity))
        .collect();
    match visible.as_slice() {
        [] => Err(McpError::invalid_params(
            format!("unknown database `{db_id}`"),
            None,
        )),
        [only] => Ok((*only).clone()),
        _ => Err(McpError::invalid_params(
            format!(
                "database id `{db_id}` is ambiguous within the tenant; ask the owner to export a snapshot instead"
            ),
            None,
        )),
    }
}

/// Resolve a database for writing, optionally creating the caller-owned
/// record. Only the owning principal ever writes.
pub(super) fn resolve_writable_database(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    db_id: &DuckDbDatabaseId,
    create_if_missing: bool,
) -> Result<(DatabaseOwner, bool), McpError> {
    let owners = state
        .durable
        .database_owners(db_id)
        .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    if let Some(own) = owners
        .iter()
        .find(|owner| owner.principal_id == identity.principal.id)
    {
        if !database_writable(own, identity) {
            return Err(McpError::invalid_request(
                "duckdb database policy denied write",
                None,
            ));
        }
        return Ok((own.clone(), false));
    }
    if !create_if_missing {
        return Err(McpError::invalid_params(
            format!("unknown database `{db_id}`; pass create_if_missing to create it"),
            None,
        ));
    }
    let file_path = database_file_path(state, identity, db_id);
    let owner = DatabaseOwner {
        db_id: db_id.clone(),
        principal_id: identity.principal.id.clone(),
        profile: identity.profile.clone(),
        tenant: identity.principal.tenant.clone(),
        data_labels: identity.principal.data_labels.clone(),
        file_path: file_path.to_string_lossy().into_owned(),
    };
    Ok((owner, true))
}
