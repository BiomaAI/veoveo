use rmcp::{ErrorData, RoleServer, service::RequestContext};
use veoveo_computers::ComputerActor;
use veoveo_mcp_contract::hosting::gateway_identity;

pub fn actor(context: &RequestContext<RoleServer>) -> Result<ComputerActor, ErrorData> {
    ComputerActor::from_verified(&gateway_identity(context)?).map_err(|_| forbidden())
}
pub fn forbidden() -> ErrorData {
    ErrorData::invalid_request("You don't have permission to use this Computer.", None)
}
pub fn unavailable() -> ErrorData {
    ErrorData::internal_error(
        "Computers is temporarily unavailable. Try again shortly.",
        None,
    )
}
