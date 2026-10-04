use std::time::Instant;
use veoveo_mcp_contract::audit::AdministrativeOperation;

use axum::{
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use veoveo_mcp_contract::{GatewayAction, GatewayProfile, PolicyTarget, ServerSlug, TaskExposure};
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_task_runtime::{TaskError, TaskOwner, TaskRuntime};
use veoveo_types::TaskId;

use crate::{
    admin::admin_profile_id,
    audit::{
        AdminAuthorizationRequest, AdminOperationAuditRecord, AdminOperationFailure,
        AdminOperationStatus, authorize_admin_target_request, internal_error_response,
        record_admin_target_operation_audit,
    },
    runtime::AdminState,
};

pub(crate) async fn cancel_task(
    State(state): State<AdminState>,
    AxumPath((profile, server, task_id)): AxumPath<(String, String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(task_id) = task_id.parse::<TaskId>() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if task_id.as_uuid().get_version_num() != 7 {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(server_slug) = ServerSlug::new(server) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let target = PolicyTarget::PlatformTask {
        server: server_slug.clone(),
        task_id,
    };
    let (catalog, profile, subject) = match authorize_admin_target_request(
        &state,
        &profile_id,
        subject,
        AdminAuthorizationRequest {
            audit_target: None,
            action: GatewayAction::TasksCancel,
            target: target.clone(),
            operation: AdministrativeOperation::TaskCancel,
            started_at,
        },
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };

    let exposed = catalog
        .profile_server(&profile_id, &server_slug)
        .is_some_and(|(_, exposure, server)| {
            exposure.tasks == TaskExposure::Enabled && server.capabilities.tasks
        });
    if !exposed {
        if let Err(error) = record_task_result(
            &state,
            &profile,
            &subject,
            target,
            started_at,
            AdminOperationStatus::Rejected,
            Some(AdminOperationFailure::TaskRoute),
        )
        .await
        {
            return internal_error_response(error);
        }
        return StatusCode::NOT_FOUND.into_response();
    }

    let task_runtime = TaskRuntime::new(
        state.control_store.platform_store().clone(),
        server_slug.to_string(),
        "gateway-admin",
    );
    let owner = TaskOwner {
        principal_key: subject.principal.id.to_string(),
        principal_kind: match subject.principal.kind {
            veoveo_mcp_contract::PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            veoveo_mcp_contract::PrincipalKind::Service => {
                veoveo_task_runtime::PrincipalKind::Service
            }
        },
        issuer: subject.principal.issuer.to_string(),
        subject: subject.principal.subject.to_string(),
        profile: profile_id.to_string(),
        tenant_key: subject.principal.tenant.as_ref().map(ToString::to_string),
        data_labels: subject
            .principal
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
        authority: subject.authority.clone(),
    };
    let query = match task_runtime.for_owner(&owner).in_work_context() {
        Ok(query) => query,
        Err(error) => return internal_error_response(error),
    };
    match query.cancel(task_id).await {
        Ok(_) => {}
        Err(TaskError::NotFound(_)) => {
            if let Err(error) = record_task_result(
                &state,
                &profile,
                &subject,
                target,
                started_at,
                AdminOperationStatus::Rejected,
                Some(AdminOperationFailure::TaskOwnership),
            )
            .await
            {
                return internal_error_response(error);
            }
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(error) => {
            return audited_task_failure(
                &state,
                &profile,
                &subject,
                target,
                started_at,
                AdminOperationFailure::CancelTask,
                error,
            )
            .await;
        }
    };
    if let Err(error) = record_task_result(
        &state,
        &profile,
        &subject,
        target,
        started_at,
        AdminOperationStatus::Succeeded,
        None,
    )
    .await
    {
        return internal_error_response(error);
    }
    StatusCode::NO_CONTENT.into_response()
}

#[allow(clippy::too_many_arguments)]
async fn record_task_result(
    state: &AdminState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    target: PolicyTarget,
    started_at: Instant,
    status: AdminOperationStatus,
    failure: Option<AdminOperationFailure>,
) -> anyhow::Result<()> {
    record_admin_target_operation_audit(
        state,
        profile,
        subject,
        target,
        AdminOperationAuditRecord {
            audit_target: None,
            action: GatewayAction::TasksCancel,
            operation: AdministrativeOperation::TaskCancel,
            started_at,
            status,
            failure,
        },
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn audited_task_failure(
    state: &AdminState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    target: PolicyTarget,
    started_at: Instant,
    failure: AdminOperationFailure,

    error: impl std::fmt::Display,
) -> Response {
    tracing::error!(failure = ?failure, "gateway task cancellation failed: {error}");
    if let Err(audit_error) = record_task_result(
        state,
        profile,
        subject,
        target,
        started_at,
        AdminOperationStatus::Failed,
        Some(failure),
    )
    .await
    {
        return internal_error_response(audit_error);
    }
    StatusCode::BAD_GATEWAY.into_response()
}
