use std::{collections::BTreeMap, time::Instant};

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{TimeDelta, Utc};
use veoveo_mcp_contract::{
    CanonicalTaskId, GatewayAction, GatewayProfile, GatewayProfileId, PolicyTarget, ServerSlug,
    TaskExposure, TenantId,
};
use veoveo_mcp_gateway::{AuthenticatedSubject, FinalTaskClient};
use veoveo_mcp_task_extension::{AcknowledgeTaskResult, ProtocolTaskId};
use veoveo_platform_store::TaskRecord;
use veoveo_task_runtime::TaskSnapshot;

use crate::{
    admin::admin_profile_id,
    audit::{
        AdminAuthorizationRequest, AdminOperationAuditRecord, AdminOperationFailure,
        AdminOperationStatus, authorize_admin_target_request, internal_error_response,
        record_admin_target_operation_audit,
    },
    runtime::AdminState,
};

const INTERNAL_TASK_TOKEN_TTL_SECONDS: i64 = 60;
const ADMIN_TASK_CANCEL_METHOD: &str = "admin/tasks/cancel";
const ADMIN_TASK_CANCEL_RESULT_METHOD: &str = "admin/tasks/cancel/result";

pub(crate) async fn cancel_task(
    State(state): State<AdminState>,
    AxumPath((profile, task_id)): AxumPath<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(task_id) = task_id.parse::<ProtocolTaskId>() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let snapshot = match load_task(&state, task_id).await {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };
    let Ok(server_slug) = ServerSlug::new(snapshot.server.clone()) else {
        return internal_error_response("canonical task has an invalid server identity");
    };
    let canonical_task_id = match CanonicalTaskId::new(task_id.to_string()) {
        Ok(task_id) => task_id,
        Err(error) => return internal_error_response(error),
    };
    let target = PolicyTarget::Task {
        server: server_slug.clone(),
        task_id: canonical_task_id,
    };
    let metadata = BTreeMap::from([
        ("operation".to_owned(), "cancel_task".to_owned()),
        ("task_id".to_owned(), task_id.to_string()),
        ("server".to_owned(), server_slug.to_string()),
    ]);
    let (catalog, profile, subject) = match authorize_admin_target_request(
        &state,
        &profile_id,
        subject,
        AdminAuthorizationRequest {
            action: GatewayAction::TasksCancel,
            target: target.clone(),
            method: ADMIN_TASK_CANCEL_METHOD,
            metadata: metadata.clone(),
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
            metadata,
        )
        .await
        {
            return internal_error_response(error);
        }
        return StatusCode::NOT_FOUND.into_response();
    }

    let labels = subject
        .principal
        .data_labels
        .iter()
        .map(ToString::to_string)
        .collect();
    if !snapshot.owner.allows(
        subject.principal.id.as_str(),
        &snapshot.owner.profile,
        subject.principal.tenant.as_ref().map(TenantId::as_str),
        &labels,
    ) {
        if let Err(error) = record_task_result(
            &state,
            &profile,
            &subject,
            target,
            started_at,
            AdminOperationStatus::Rejected,
            Some(AdminOperationFailure::TaskOwnership),
            metadata,
        )
        .await
        {
            return internal_error_response(error);
        }
        return StatusCode::NOT_FOUND.into_response();
    }

    let owner_profile = match GatewayProfileId::new(snapshot.owner.profile) {
        Ok(profile) => profile,
        Err(error) => {
            return audited_task_failure(
                &state,
                &profile,
                &subject,
                target,
                started_at,
                AdminOperationFailure::TaskRoute,
                metadata,
                error,
            )
            .await;
        }
    };
    let expires_at = std::cmp::min(
        subject.access_token.expires_at,
        Utc::now() + TimeDelta::seconds(INTERNAL_TASK_TOKEN_TTL_SECONDS),
    );
    let internal_token = match state.internal_token_issuer.issue(
        owner_profile,
        server_slug.clone(),
        subject.actor.clone(),
        subject.authority.clone(),
        expires_at,
    ) {
        Ok(token) => token,
        Err(error) => {
            return audited_task_failure(
                &state,
                &profile,
                &subject,
                target,
                started_at,
                AdminOperationFailure::IssueInternalToken,
                metadata,
                error,
            )
            .await;
        }
    };
    let Some(server) = catalog.server(&server_slug).cloned() else {
        return audited_task_failure(
            &state,
            &profile,
            &subject,
            target,
            started_at,
            AdminOperationFailure::TaskRoute,
            metadata,
            "canonical task server is no longer configured",
        )
        .await;
    };
    let client =
        match FinalTaskClient::for_server(&catalog, &server, internal_token.bearer_token).await {
            Ok(client) => client,
            Err(error) => {
                return audited_task_failure(
                    &state,
                    &profile,
                    &subject,
                    target,
                    started_at,
                    AdminOperationFailure::ConnectFinalTaskExtension,
                    metadata,
                    error,
                )
                .await;
            }
        };
    let result = match client.cancel(task_id).await {
        Ok(result) => result,
        Err(error) => {
            return audited_task_failure(
                &state,
                &profile,
                &subject,
                target,
                started_at,
                AdminOperationFailure::CancelTask,
                metadata,
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
        metadata,
    )
    .await
    {
        return internal_error_response(error);
    }
    Json::<AcknowledgeTaskResult>(result).into_response()
}

async fn load_task(
    state: &AdminState,
    task_id: ProtocolTaskId,
) -> anyhow::Result<Option<TaskSnapshot>> {
    let mut response = state
        .control_store
        .platform_store()
        .client()
        .query("SELECT * FROM ONLY $task;")
        .bind(("task", task_id.task_id().record_id()))
        .await?
        .check()?;
    let record: Option<TaskRecord> = response.take(0)?;
    record
        .map(TaskSnapshot::try_from)
        .transpose()
        .map_err(Into::into)
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
    metadata: BTreeMap<String, String>,
) -> anyhow::Result<()> {
    record_admin_target_operation_audit(
        state,
        profile,
        subject,
        target,
        AdminOperationAuditRecord {
            action: GatewayAction::TasksCancel,
            method: ADMIN_TASK_CANCEL_RESULT_METHOD,
            started_at,
            status,
            failure,
            metadata,
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
    metadata: BTreeMap<String, String>,
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
        metadata,
    )
    .await
    {
        return internal_error_response(audit_error);
    }
    StatusCode::BAD_GATEWAY.into_response()
}
