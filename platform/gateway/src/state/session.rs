//! Current browser-family binding for signed access tokens. Never cache positive results.
use anyhow::{Context, Result};
use chrono::Utc;
use veoveo_mcp_contract::{AccessTokenSubject, AuthorizationServerId, GatewayProfileId, Principal};
use veoveo_platform_store::{GatewayRefreshFamilyRecord, gateway_refresh_family_record_id};

use super::GatewayState;
use veoveo_policy::session::{SessionFamilyAuthority, SessionRequest};

impl GatewayState {
    /// Unbound service/exchange tokens use their existing token authority. They do
    /// not acquire a browser session or permission for session-bound renewal.
    pub async fn access_token_session_valid(
        &self,
        profile: &GatewayProfileId,
        authorization_server: &AuthorizationServerId,
        token: &AccessTokenSubject,
        principal: &Principal,
    ) -> Result<bool> {
        let Some(family_id) = &token.session_family else {
            return Ok(true);
        };
        let family_uuid = family_id.as_str().parse()?;
        let family: Option<GatewayRefreshFamilyRecord> =
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                self.platform
                    .client()
                    .select(gateway_refresh_family_record_id(family_uuid))
                    .await
            })
            .await
            .context("access-token session read deadline")?
            .context("failed to read access-token session family")?;
        let Some(family) = family else {
            return Ok(false);
        };
        if family.id != gateway_refresh_family_record_id(family_uuid) {
            return Ok(false);
        }
        Ok(
            SessionFamilyAuthority::from_stored(&family)?.allows(SessionRequest {
                profile,
                authorization_server,
                token,
                principal,
                now: Utc::now(),
            }),
        )
    }
}
