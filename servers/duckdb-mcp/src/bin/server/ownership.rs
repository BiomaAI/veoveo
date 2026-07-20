use std::{collections::BTreeSet, path::PathBuf};

use chrono::{TimeDelta, Utc};
use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use sha2::{Digest, Sha256};
use veoveo_duckdb_mcp::{
    contract::DuckDbDatabaseId,
    state::{DatabaseOwner, TaskOwner},
};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalIdentity, GatewayProfileId, JwtId, PlaneCaller,
    Principal, PrincipalId, PrincipalKind, ServerSlug, TenantId, TokenIssuer, TokenSubject,
};

use super::app_state::AppState;

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
        .map(|bearer| bearer.0.clone())
        .ok_or_else(|| McpError::invalid_request("forwarded bearer missing", None))?;
    Ok(caller_from(identity, bearer))
}

pub(super) fn caller_from(identity: GatewayInternalIdentity, bearer: String) -> PlaneCaller {
    let memberships = identity.actor.group_memberships();
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
        task_id: task_id.to_owned(),
        principal_id: identity.actor.id.clone(),
        profile: identity.profile.clone(),
        tenant: identity.actor.tenant.clone(),
        data_labels: identity.actor.data_labels.clone(),
    }
}

pub(super) fn task_owner_from_runtime(
    task_id: &str,
    owner: &veoveo_task_runtime::TaskOwner,
) -> Result<TaskOwner, String> {
    Ok(TaskOwner {
        task_id: task_id.to_owned(),
        principal_id: PrincipalId::new(owner.principal_key.clone())
            .map_err(|error| error.to_string())?,
        profile: GatewayProfileId::new(owner.profile.clone()).map_err(|error| error.to_string())?,
        tenant: owner
            .tenant_key
            .clone()
            .map(TenantId::new)
            .transpose()
            .map_err(|error| error.to_string())?,
        data_labels: owner
            .data_labels
            .iter()
            .cloned()
            .map(veoveo_mcp_contract::DataLabelId::new)
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())?,
    })
}

pub(super) fn runtime_owner(identity: &GatewayInternalIdentity) -> veoveo_task_runtime::TaskOwner {
    veoveo_task_runtime::TaskOwner {
        principal_key: identity.actor.id.to_string(),
        principal_kind: match identity.actor.kind {
            PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            PrincipalKind::Service => veoveo_task_runtime::PrincipalKind::Service,
        },
        issuer: identity.actor.issuer.to_string(),
        subject: identity.actor.subject.to_string(),
        profile: identity.profile.to_string(),
        tenant_key: identity.actor.tenant.as_ref().map(ToString::to_string),
        data_labels: identity
            .actor
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
        authority: identity.authority.clone(),
    }
}

pub(super) fn identity_from_runtime(
    owner: &veoveo_task_runtime::TaskOwner,
) -> Result<GatewayInternalIdentity, String> {
    let now = Utc::now();
    Ok(GatewayInternalIdentity {
        issuer: TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)
            .map_err(|error| error.to_string())?,
        profile: GatewayProfileId::new(owner.profile.clone()).map_err(|error| error.to_string())?,
        server: ServerSlug::new("duckdb").map_err(|error| error.to_string())?,
        actor: Principal {
            id: PrincipalId::new(owner.principal_key.clone()).map_err(|error| error.to_string())?,
            kind: match owner.principal_kind {
                veoveo_task_runtime::PrincipalKind::User => PrincipalKind::User,
                veoveo_task_runtime::PrincipalKind::Service => PrincipalKind::Service,
            },
            issuer: TokenIssuer::new(owner.issuer.clone()).map_err(|error| error.to_string())?,
            subject: TokenSubject::new(owner.subject.clone()).map_err(|error| error.to_string())?,
            tenant: owner
                .tenant_key
                .clone()
                .map(TenantId::new)
                .transpose()
                .map_err(|error| error.to_string())?,
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::new(),
            data_labels: owner
                .data_labels
                .iter()
                .cloned()
                .map(veoveo_mcp_contract::DataLabelId::new)
                .collect::<Result<_, _>>()
                .map_err(|error| error.to_string())?,
            assurances: BTreeSet::new(),
            authenticated_at: None,
        },
        authority: owner.authority.clone(),
        jwt_id: JwtId::new(uuid::Uuid::now_v7().to_string()).map_err(|error| error.to_string())?,
        issued_at: now,
        not_before: now,
        expires_at: now + TimeDelta::hours(1),
    })
}

pub(super) async fn optional_task_owner(
    state: &AppState,
    task_id: &str,
) -> Result<Option<TaskOwner>, McpError> {
    state
        .tasks
        .owner(task_id)
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))?
        .as_ref()
        .map(|owner| task_owner_from_runtime(task_id, owner))
        .transpose()
        .map_err(|error| McpError::internal_error(error, None))
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
    owner.principal_id == identity.actor.id
        && owner.profile == identity.profile
        && owner.tenant == identity.actor.tenant
        && owner.data_labels.is_subset(&identity.actor.data_labels)
}

fn owner_storage_key(identity: &GatewayInternalIdentity) -> String {
    let canonical = format!(
        "{}\0{}\0{}\0{}\0{}",
        identity.actor.issuer,
        identity.actor.subject,
        identity.actor.id,
        identity
            .actor
            .tenant
            .as_ref()
            .map(TenantId::as_str)
            .unwrap_or("installation"),
        identity.profile,
    );
    let digest = hex::encode(Sha256::digest(canonical.as_bytes()));
    digest[..32].to_owned()
}

fn owner_directory(state: &AppState, identity: &GatewayInternalIdentity) -> PathBuf {
    state.dirs.database_dir.join(owner_storage_key(identity))
}

pub(super) fn database_file_path(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    db_id: &DuckDbDatabaseId,
) -> PathBuf {
    owner_directory(state, identity).join(format!("{db_id}.duckdb"))
}

fn derived_database_owner(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    db_id: DuckDbDatabaseId,
) -> DatabaseOwner {
    let file_path = database_file_path(state, identity, &db_id);
    DatabaseOwner {
        db_id,
        principal_id: identity.actor.id.clone(),
        profile: identity.profile.clone(),
        tenant: identity.actor.tenant.clone(),
        data_labels: identity.actor.data_labels.clone(),
        file_path: file_path.to_string_lossy().into_owned(),
    }
}

pub(super) fn databases_for_identity(
    state: &AppState,
    identity: &GatewayInternalIdentity,
) -> Result<Vec<DatabaseOwner>, McpError> {
    let directory = owner_directory(state, identity);
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(McpError::internal_error(error.to_string(), None)),
    };
    let mut databases = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| McpError::internal_error(error.to_string(), None))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("duckdb") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let Ok(db_id) = DuckDbDatabaseId::new(stem) else {
            continue;
        };
        databases.push(derived_database_owner(state, identity, db_id));
    }
    databases.sort_by(|left, right| left.db_id.as_str().cmp(right.db_id.as_str()));
    Ok(databases)
}

pub(super) fn resolve_readable_database(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    db_id: &DuckDbDatabaseId,
) -> Result<DatabaseOwner, McpError> {
    let owner = derived_database_owner(state, identity, db_id.clone());
    if PathBuf::from(&owner.file_path).is_file() {
        Ok(owner)
    } else {
        Err(McpError::invalid_params(
            format!("unknown database `{db_id}`"),
            None,
        ))
    }
}

pub(super) fn resolve_writable_database(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    db_id: &DuckDbDatabaseId,
    create_if_missing: bool,
) -> Result<(DatabaseOwner, bool), McpError> {
    let owner = derived_database_owner(state, identity, db_id.clone());
    let exists = PathBuf::from(&owner.file_path).is_file();
    if !exists && !create_if_missing {
        return Err(McpError::invalid_params(
            format!("unknown database `{db_id}`; pass create_if_missing to create it"),
            None,
        ));
    }
    Ok((owner, !exists))
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_mcp_contract::{
        AccessSubject, DataLabelId, GroupId, InvocationAuthority, InvocationProvenance,
        PolicyVersion, PrincipalAssurance, RoleId, ScopeName, TokenSubject, WorkContextId,
        WorkContextMembershipLevel, WorkContextOutputPolicy,
    };

    fn identity(profile: &str, subject: &str) -> GatewayInternalIdentity {
        let now = Utc::now();
        let issuer = TokenIssuer::new("https://idp.example.test").unwrap();
        let actor = Principal {
            id: PrincipalId::new(format!("principal-{subject}")).unwrap(),
            kind: PrincipalKind::User,
            issuer,
            subject: TokenSubject::new(subject).unwrap(),
            tenant: Some(TenantId::new("tenant-a").unwrap()),
            groups: BTreeSet::<GroupId>::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::<RoleId>::new(),
            scopes: BTreeSet::<ScopeName>::new(),
            data_labels: BTreeSet::<DataLabelId>::new(),
            assurances: BTreeSet::<PrincipalAssurance>::new(),
            authenticated_at: Some(now),
        };
        GatewayInternalIdentity {
            issuer: TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(),
            profile: GatewayProfileId::new(profile).unwrap(),
            server: ServerSlug::new("duckdb").unwrap(),
            actor: actor.clone(),
            authority: InvocationAuthority {
                work_context: WorkContextId::new("mission").unwrap(),
                tenant: TenantId::new("tenant-a").unwrap(),
                membership: WorkContextMembershipLevel::Owner,
                policy_revision: PolicyVersion::new("r1").unwrap(),
                output_policy: WorkContextOutputPolicy {
                    owner: AccessSubject::Principal(actor.id.clone()),
                    initial_grants: Vec::new(),
                    classification: None,
                    data_labels: BTreeSet::new(),
                },
                provenance: InvocationProvenance::Direct {
                    initiator: actor.id,
                },
            },
            jwt_id: JwtId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
            issued_at: now,
            not_before: now,
            expires_at: now + TimeDelta::minutes(5),
        }
    }

    #[test]
    fn owner_storage_key_is_canonical_and_profile_scoped() {
        let default = identity("default", "user-a");
        assert_eq!(owner_storage_key(&default), owner_storage_key(&default));
        assert_ne!(
            owner_storage_key(&default),
            owner_storage_key(&identity("research", "user-a"))
        );
        assert_ne!(
            owner_storage_key(&default),
            owner_storage_key(&identity("default", "user-b"))
        );
    }

    #[test]
    fn recovered_identity_resolves_the_same_workspace() {
        let original = identity("default", "user-a");
        let recovered = identity_from_runtime(&runtime_owner(&original)).unwrap();
        assert_eq!(owner_storage_key(&original), owner_storage_key(&recovered));
    }
}
