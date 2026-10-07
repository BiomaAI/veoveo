use super::*;
pub(super) use veoveo_mcp_conformance::client::{Client, TaskCapability};
pub(super) async fn connect(args: &Args, capability: TaskCapability) -> Result<Client> {
    veoveo_mcp_conformance::client::connect(
        &veoveo_mcp_conformance::client::ConnectionOptions {
            url: args.url.clone(),
            bearer_token: args.bearer_token.clone(),
        },
        capability,
    )
    .await
}
pub(super) fn bearer_token_from_args(args: &Args) -> Result<Option<String>> {
    Ok(args.bearer_token.clone())
}
