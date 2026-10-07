//! Composition-owned direct-hosted fixture identity; issuance uses runtime adapter.
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use veoveo_mcp_contract::fixture_client::{InternalFixtureIdentity, issue_internal_fixture_token};
use veoveo_types::{GatewayProfileId, ServerSlug, TokenSubject};
use veoveo_types::{ScopeName, TenantId, WorkContextId};

pub fn fixture_identity(server: ServerSlug) -> Result<InternalFixtureIdentity> {
    Ok(InternalFixtureIdentity {
        signing_key_id: veoveo_mcp_contract::DEFAULT_GATEWAY_INTERNAL_SIGNING_KEY_ID.to_owned(),
        server,
        profile: GatewayProfileId::parse("operator")?,
        work_context: WorkContextId::parse("conformance")?,
        tenant: TenantId::parse("local")?,
        subject: TokenSubject::parse("conformance")?,
        scopes: vec![ScopeName::parse("operator:use")?],
    })
}
pub fn fixture_bearer(identity: InternalFixtureIdentity) -> Result<String> {
    issue_internal_fixture_token(
        identity,
        STANDARD.decode(super::INTERNAL_SIGNING_KEY_DER_B64)?,
    )
}
