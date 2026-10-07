use super::*;

pub(super) use veoveo_mcp_conformance::client::{Client, TaskCapability};

pub(super) async fn connect(args: &Args, capability: TaskCapability) -> Result<Client> {
    veoveo_mcp_conformance::client::connect(
        &veoveo_mcp_conformance::client::ConnectionOptions {
            url: args.url.clone(),
            bearer_token: bearer_token_from_args(args)?,
        },
        capability,
    )
    .await
}
pub(super) fn bearer_token_from_args(args: &Args) -> Result<Option<String>> {
    if let Some(token) = &args.bearer_token {
        Ok(Some(token.clone()))
    } else if let Some(private_key_der_b64) = &args.internal_signing_key_der_b64 {
        Ok(Some(issue_internal_conformance_token(
            args,
            private_key_der_b64,
        )?))
    } else {
        Ok(None)
    }
}

fn issue_internal_conformance_token(args: &Args, private_key_der_b64: &str) -> Result<String> {
    veoveo_mcp_contract::fixture_client::issue_internal_fixture_token(
        veoveo_mcp_contract::fixture_client::InternalFixtureIdentity {
            signing_key_id: args.internal_signing_key_id.clone(),
            server: ServerSlug::parse(args.internal_server.clone())?,
            profile: GatewayProfileId::parse(args.internal_profile.clone())?,
            work_context: WorkContextId::parse(args.internal_work_context.clone())?,
            tenant: TenantId::parse(args.internal_tenant.clone())?,
            subject: TokenSubject::parse(args.internal_principal_subject.clone())?,
            scopes: args
                .internal_scopes
                .iter()
                .map(|scope| ScopeName::parse(scope.clone()))
                .collect::<Result<_, _>>()?,
        },
        BASE64_STANDARD.decode(private_key_der_b64.trim())?,
    )
}
