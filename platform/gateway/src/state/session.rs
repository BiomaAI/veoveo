//! Current browser-family binding for signed access tokens. Never cache positive results.
use anyhow::{Context, Result};
use chrono::Utc;
use veoveo_mcp_contract::{AccessTokenSubject, AuthorizationServerId, GatewayProfileId, Principal};
use veoveo_platform_store::{GatewayRefreshFamilyRecord, gateway_refresh_family_record_id};

use super::{GatewayState, refresh_tokens::grant_from_family};

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
        if family.revoked_at.is_some()
            || family.expires_at <= Utc::now()
            || family.principal_id != principal.id.as_str()
            || family.tenant.as_deref() != principal.tenant.as_ref().map(|id| id.as_str())
        {
            return Ok(false);
        }
        let grant = grant_from_family(family)?;
        Ok(grant.family_id == *family_id
            && grant.authorization_server == *authorization_server
            && grant.profile == *profile
            && grant.oauth_client_id == token.oauth_client_id
            && grant.work_context == token.work_context
            && grant.principal.id == principal.id
            && grant.principal.kind == principal.kind
            // The family retains the original IdP principal. Access-token verification
            // records the gateway token issuer; principal_id preserves original identity.
            && token.issuer == principal.issuer
            && token.subject == principal.subject
            && grant.principal.subject == principal.subject
            && grant.principal.tenant == principal.tenant
            && token.scopes.is_subset(&grant.scopes))
    }
}
