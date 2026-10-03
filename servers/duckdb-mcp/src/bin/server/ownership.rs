use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use chrono::{TimeDelta, Utc};
use rmcp::ErrorData as McpError;
use sha2::{Digest, Sha256};
use veoveo_duckdb_mcp::contract::DuckDbDatabaseId;
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalIdentity, GatewayProfileId, JwtId, Principal,
    PrincipalKind, ServerSlug, TokenIssuer, TokenSubject,
};
use veoveo_types::{PrincipalId, TenantId};

use super::app_state::AppState;

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
                .map(veoveo_types::DataLabelId::new)
                .collect::<Result<_, _>>()
                .map_err(|error| error.to_string())?,
            assurances: BTreeSet::new(),
            authenticated_at: None,
        },
        authority: owner.authority.clone(),
        request_context: None,
        jwt_id: JwtId::new(uuid::Uuid::now_v7().to_string()).map_err(|error| error.to_string())?,
        issued_at: now,
        not_before: now,
        expires_at: now + TimeDelta::hours(1),
    })
}

fn owner_storage_key(identity: &GatewayInternalIdentity) -> String {
    // Serialize the optional tenant as null or a string, without a sentinel that
    // can collide with an admitted tenant name. Structured fields also preserve
    // their boundaries independently of the identity providers' string values.
    let canonical = serde_json::to_vec(&(
        &identity.actor.issuer,
        &identity.actor.subject,
        &identity.actor.id,
        &identity.actor.tenant,
        &identity.profile,
    ))
    .expect("closed owner identity fields serialize");
    let digest = hex::encode(Sha256::digest(canonical));
    digest[..32].to_owned()
}

fn owner_directory(database_root: &Path, identity: &GatewayInternalIdentity) -> PathBuf {
    database_root.join(owner_storage_key(identity))
}

pub(super) fn database_file_path(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    db_id: &DuckDbDatabaseId,
) -> PathBuf {
    owner_directory(&state.dirs.database_dir, identity).join(format!("{db_id}.duckdb"))
}

pub(super) async fn database_page_for_identity(
    database_root: &Path,
    identity: &GatewayInternalIdentity,
    cursor: Option<&veoveo_duckdb_mcp::contract::DuckDbDatabaseCursor>,
) -> Result<veoveo_duckdb_mcp::contract::DuckDbDatabasePage, McpError> {
    let directory = owner_directory(database_root, identity);
    let cursor = cursor.cloned();
    tokio::task::spawn_blocking(move || {
        veoveo_duckdb_mcp::catalog::database_page(&directory, cursor.as_ref())
    })
    .await
    .map_err(|_| McpError::internal_error("database catalog worker failed", None))?
    .map_err(|_| McpError::internal_error("reading owner database catalog failed", None))
}

pub(super) fn resolve_readable_database(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    db_id: &DuckDbDatabaseId,
) -> Result<PathBuf, McpError> {
    let path = database_file_path(state, identity, db_id);
    if path.is_file() {
        Ok(path)
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
) -> Result<(PathBuf, bool), McpError> {
    let path = database_file_path(state, identity, db_id);
    let exists = path.is_file();
    if !exists && !create_if_missing {
        return Err(McpError::invalid_params(
            format!("unknown database `{db_id}`; pass create_if_missing to create it"),
            None,
        ));
    }
    Ok((path, !exists))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::identity;

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

    #[tokio::test]
    async fn catalog_pages_reselect_the_current_identity_directory() {
        let root = tempfile::tempdir().unwrap();
        let original = identity("default", "user-a");
        let directory = owner_directory(root.path(), &original);
        std::fs::create_dir(&directory).unwrap();
        for n in 0..101 {
            std::fs::write(
                directory.join(format!("db_{n:03}.duckdb")),
                b"not a database",
            )
            .unwrap();
        }
        let first = database_page_for_identity(root.path(), &original, None)
            .await
            .unwrap();
        assert_eq!(first.items().len(), 100);
        let cursor = first.next_cursor().unwrap();

        let mut identities = Vec::new();
        let mut changed = original.clone();
        changed.actor.tenant = None;
        identities.push(changed);
        let mut changed = original.clone();
        changed.actor.tenant = Some(TenantId::new("installation").unwrap());
        identities.push(changed);
        let mut changed = original.clone();
        changed.actor.issuer = TokenIssuer::new("https://other.example.test").unwrap();
        identities.push(changed);
        let mut changed = original.clone();
        changed.actor.subject = TokenSubject::new("another-subject").unwrap();
        identities.push(changed);
        let mut changed = original.clone();
        changed.actor.id = PrincipalId::new("another-principal").unwrap();
        identities.push(changed);
        let mut changed = original.clone();
        changed.profile = GatewayProfileId::new("research").unwrap();
        identities.push(changed);
        let mut selected_directories = BTreeSet::from([directory]);
        for (n, identity) in identities.iter().enumerate() {
            let directory = owner_directory(root.path(), identity);
            assert!(selected_directories.insert(directory.clone()));
            std::fs::create_dir(&directory).unwrap();
            let name = format!("separate_{n}");
            std::fs::write(directory.join(format!("{name}.duckdb")), []).unwrap();
            let page = database_page_for_identity(root.path(), identity, Some(cursor))
                .await
                .unwrap();
            assert_eq!(page.items().len(), 1);
            assert_eq!(page.items()[0].id().as_str(), name);
            assert!(page.next_cursor().is_none());
        }
        let recovered = identity_from_runtime(&runtime_owner(&original)).unwrap();
        let last = database_page_for_identity(root.path(), &recovered, Some(cursor))
            .await
            .unwrap();
        assert_eq!(last.items()[0].id().as_str(), "db_100");
        assert!(last.next_cursor().is_none());
    }
}
