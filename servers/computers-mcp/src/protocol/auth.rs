use rmcp::{ErrorData, RoleServer, service::RequestContext};
use veoveo_computers::ComputerActor;
use veoveo_mcp_contract::GatewayInternalIdentity;

pub fn identity(
    context: &RequestContext<RoleServer>,
) -> Result<GatewayInternalIdentity, ErrorData> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|p| p.extensions.get::<GatewayInternalIdentity>())
        .cloned()
        .ok_or_else(|| ErrorData::invalid_request("verified gateway identity required", None))
}
pub fn actor(context: &RequestContext<RoleServer>) -> Result<ComputerActor, ErrorData> {
    ComputerActor::from_verified(&identity(context)?).map_err(|_| forbidden())
}
pub fn forbidden() -> ErrorData {
    ErrorData::invalid_request("Computer access is not authorized", None)
}
pub fn unavailable() -> ErrorData {
    ErrorData::internal_error("Computers state is unavailable", None)
}
