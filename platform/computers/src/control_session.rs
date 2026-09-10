//! A browser-bound control request cannot outlive logout. Accepted background
//! execution retains its independent current-action checks in current_authority.
use crate::{ComputerError, ComputersStore, Result, authority_snapshot::AuthoritySnapshot};
use veoveo_platform_store::{GatewayRefreshFamilyRecord, gateway_refresh_family_record_id};
use veoveo_policy::{
    PolicyCatalogView,
    session::{SessionFamilyAuthority, SessionRequest},
};
impl ComputersStore {
    pub(crate) async fn check_control_session(
        &self,
        snapshot: &AuthoritySnapshot,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
        let accepted = &snapshot.accepted;
        let token = &accepted.request_context.access_token;
        let Some(family_id) = &token.session_family else {
            return Ok(None);
        };
        let id = family_id
            .as_str()
            .parse()
            .map_err(|_| ComputerError::Forbidden)?;
        let expected = gateway_refresh_family_record_id(id);
        let family: Option<GatewayRefreshFamilyRecord> = self
            .platform
            .client()
            .select(expected.clone())
            .await
            .map_err(|_| ComputerError::Unavailable)?;
        let family = family
            .filter(|f| f.id == expected)
            .ok_or(ComputerError::Forbidden)?;
        let profile = snapshot
            .catalog
            .profile(&accepted.profile)
            .ok_or(ComputerError::Forbidden)?;
        if !SessionFamilyAuthority::from_stored(&family)
            .map_err(|_| ComputerError::Forbidden)?
            .allows(SessionRequest {
                profile: &accepted.profile,
                authorization_server: &profile.authorization_server,
                token,
                principal: &accepted.request_context.principal,
                now: chrono::Utc::now(),
            })
        {
            return Err(ComputerError::Forbidden);
        }
        Ok(Some(family.expires_at))
    }
}
