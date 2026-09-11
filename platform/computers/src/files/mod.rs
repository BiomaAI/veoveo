//! Governed file work shares Computer identity and the exclusive execution slot.
mod admission;
mod artifact_access;
mod authority;
mod model;
mod tasks;
pub use artifact_access::{
    FILE_PREPARATION_SECONDS, FILE_PUBLICATION_SECONDS, FileCapabilityRequest,
};
pub use authority::FileTransferAuthority;
pub use model::FileOperation;

fn record(id: uuid::Uuid) -> surrealdb::types::RecordId {
    surrealdb::types::RecordId::new("computer_file_transfer", surrealdb::types::Uuid::from(id))
}
fn object(value: &impl serde::Serialize) -> crate::Result<veoveo_platform_store::OpenObject> {
    serde_json::from_value(
        serde_json::to_value(value).map_err(|_| crate::ComputerError::Unavailable)?,
    )
    .map_err(|_| crate::ComputerError::Unavailable)
}
fn actor_key(actor: &crate::AcceptedAuthority) -> crate::Result<String> {
    crate::identity::digest(&(
        "veoveo.computer.file-actor.v1",
        crate::identity::owner_key(&actor.task_owner())?,
        &actor.request_context.principal.id,
        &actor.request_context.access_token.oauth_client_id,
    ))
}

pub(crate) fn target() -> veoveo_mcp_contract::PolicyTarget {
    veoveo_mcp_contract::PolicyTarget::Tool {
        server: veoveo_mcp_contract::ServerSlug::new("computers").expect("static server"),
        tool: veoveo_mcp_contract::LocalToolName::new("transfer_file").expect("static tool"),
    }
}
