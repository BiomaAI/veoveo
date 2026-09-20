//! Current managed model authority; this preflight never invokes a model.
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use veoveo_mcp_contract::{GatewayProfileId, SecretPurpose, agent_management as wire};
use veoveo_mcp_gateway::AuthenticatedSubject;

use super::{AgentManagementState, Fault, authority, projection};

pub(super) async fn check(
    State(state): State<AgentManagementState>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<wire::ManagedDispatch>,
) -> Result<StatusCode, Fault> {
    let forbidden = || Fault::status(StatusCode::FORBIDDEN);
    let catalog = state.catalog.current();
    let profile = GatewayProfileId::new(profile).map_err(|_| forbidden())?;
    let profile = catalog.profile(&profile).ok_or_else(forbidden)?;
    authority::live_session(&state, profile, &subject).await?;
    let binding = subject
        .access_token
        .managed_agent
        .as_ref()
        .ok_or_else(forbidden)?;
    if request.generation != binding.generation
        || request.epoch < 1
        || request.epoch != binding.epoch
    {
        return Err(forbidden());
    }
    let effective = state
        .gateway
        .effective_oauth_client(&catalog, &subject.access_token.oauth_client_id)
        .await
        .map_err(|_| forbidden())?
        .ok_or_else(forbidden)?;
    let managed = effective.managed.ok_or_else(forbidden)?;
    if managed.instance.key != binding.instance.as_str()
        || managed.instance.identity.profile != profile.id.as_str()
        || managed.context_key != subject.authority.work_context.as_str()
        || !state
            .store()
            .managed_agent_kernel_dispatch(
                managed.instance.id,
                request.generation,
                request.epoch,
                request.lease_owner,
                request.lease_fence,
            )
            .await?
    {
        return Err(forbidden());
    }
    let content = projection::public_content(managed.revision.content)?;
    let model = state
        .models
        .iter()
        .find(|model| {
            model.id == content.model.id
                && model.revision() == content.model.revision
                && model.permits(&subject.principal, &subject.authority.work_context)
                && model.admits(&content.budgets)
        })
        .ok_or_else(forbidden)?;
    if !catalog
        .secret_reference(&model.api_key)
        .is_some_and(|reference| reference.purpose == SecretPurpose::ProviderApiKey)
    {
        return Err(forbidden());
    }
    Ok(StatusCode::NO_CONTENT)
}
