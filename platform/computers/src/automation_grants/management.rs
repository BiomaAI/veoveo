//! Bounded registration and permission hints share issuance's current profile boundary.
use super::authority::{require_permission, require_tool};
use crate::{
    ComputerActor, ComputerError, ComputersStore, Result,
    api::{AutomationClientChoice, AutomationPermission},
    authority_snapshot::AuthoritySnapshot,
};
use std::collections::BTreeSet;
use uuid::Uuid;
use veoveo_mcp_contract::{InvocationMode, OAuthClientRegistration};
use veoveo_policy::PolicyCatalogView;

#[derive(Default)]
pub(super) struct GrantManagement {
    pub can_grant: bool,
    pub can_revoke: bool,
    pub permissions: BTreeSet<AutomationPermission>,
    pub clients: Vec<AutomationClientChoice>,
    pub truncated: bool,
}

pub(super) fn admitted_client(
    snapshot: &AuthoritySnapshot,
    client: &OAuthClientRegistration,
) -> bool {
    snapshot
        .catalog
        .profile(&snapshot.accepted.profile)
        .is_some_and(|profile| {
            client.authorization_server == profile.authorization_server
                && client
                    .allowed_resources
                    .contains(&profile.protected_resource)
                && client
                    .tenant
                    .as_ref()
                    .is_none_or(|tenant| tenant == &snapshot.accepted.invocation.tenant)
        })
}

pub(super) fn service_principal_id(
    snapshot: &AuthoritySnapshot,
    client: &OAuthClientRegistration,
) -> Result<Option<String>> {
    if client.invocation_mode != InvocationMode::Automated {
        return Ok(None);
    }
    let server = snapshot
        .catalog
        .control_plane()
        .authorization_servers
        .iter()
        .find(|server| server.id == client.authorization_server)
        .ok_or(ComputerError::Unavailable)?;
    Ok(Some(format!("{}#{}", server.issuer, client.id)))
}

impl ComputersStore {
    /// Current UI hints. Every mutation independently rechecks its authority.
    pub(super) async fn automation_management(
        &self,
        actor: &ComputerActor,
        computer: Uuid,
    ) -> Result<GrantManagement> {
        actor.check_admission()?;
        if actor.accepted().actor.id != actor.accepted().request_context.principal.id {
            return Ok(GrantManagement::default());
        }
        let snapshot = self.read_authority(actor.accepted()).await?;
        self.check_control_session(&snapshot).await?;
        let mut hints = GrantManagement {
            can_grant: require_tool(&snapshot, "grant_automation").is_ok(),
            can_revoke: require_tool(&snapshot, "revoke_automation").is_ok(),
            ..Default::default()
        };
        if hints.can_grant {
            hints.permissions = [
                AutomationPermission::Read,
                AutomationPermission::Execute,
                AutomationPermission::Start,
                AutomationPermission::Stop,
            ]
            .into_iter()
            .filter(|permission| require_permission(&snapshot, computer, *permission).is_ok())
            .collect();
            let catalog = snapshot.catalog.control_plane();
            let mut clients: Vec<_> = catalog
                .oauth_clients
                .iter()
                .filter(|client| admitted_client(&snapshot, client))
                .collect();
            clients.sort_by(|a, b| a.id.cmp(&b.id));
            hints.truncated = clients.len() > 128;
            clients.truncate(128);
            for client in clients {
                hints.clients.push(AutomationClientChoice {
                    oauth_client_id: client.id.to_string(),
                    display_name: client
                        .display_name
                        .clone()
                        .unwrap_or_else(|| client.id.to_string()),
                    service_principal_id: service_principal_id(&snapshot, client)?,
                });
            }
            hints.can_grant = !hints.permissions.is_empty();
        }
        snapshot.check_fresh()?;
        actor.check_admission()?;
        Ok(hints)
    }
}
