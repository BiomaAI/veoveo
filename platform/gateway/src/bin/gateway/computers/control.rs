use super::{
    ComputersState, Fault,
    authority::authorize,
    exact_origin,
    routes::{Operation, Route},
};
use axum::{
    body::{Body, to_bytes},
    extract::{Extension, MatchedPath, Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;
use uuid::Uuid;
use veoveo_computers_contract as api;
use veoveo_mcp_gateway::AuthenticatedSubject;

const REQUEST_LIMIT: usize = 64 * 1024;
const RESPONSE_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Page {
    after: Option<Uuid>,
}

pub(super) async fn proxy(
    State(state): State<ComputersState>,
    Extension(subject): Extension<AuthenticatedSubject>,
    route: Result<Path<Route>, axum::extract::rejection::PathRejection>,
    matched: MatchedPath,
    page: Result<Query<Page>, axum::extract::rejection::QueryRejection>,
    request: Request,
) -> Result<Response, Fault> {
    let Path(route) = route.map_err(|_| Fault::invalid())?;
    let Query(page) = page.map_err(|_| Fault::invalid())?;
    let operation = Operation::from_route(
        matched.as_str(),
        request.method(),
        route.id,
        route.operation_id,
        route.grant_id,
        route.pairing_id,
    )?;
    if (operation != Operation::List && request.uri().query().is_some())
        || page.after.is_some_and(|id| id.is_nil())
    {
        return Err(Fault::invalid());
    }
    tokio::time::timeout(
        Duration::from_secs(30),
        forward(state, subject, route, operation, page, request),
    )
    .await
    .map_err(|_| Fault::unavailable())?
}

async fn forward(
    state: ComputersState,
    subject: AuthenticatedSubject,
    route: Route,
    operation: Operation,
    page: Page,
    request: Request,
) -> Result<Response, Fault> {
    let origin = if operation.is_attachment() {
        Some(exact_origin(request.headers(), &state.origin)?)
    } else {
        None
    };
    if operation.requires_json() {
        require_json(request.headers())?;
    }
    let method = request.method().clone();
    let bytes = to_bytes(request.into_body(), REQUEST_LIMIT)
        .await
        .map_err(|_| Fault(StatusCode::PAYLOAD_TOO_LARGE, api::ErrorCode::InvalidInput))?;
    let body = match operation {
        Operation::Create => normalize::<api::CreateInput>(&bytes),
        Operation::Start(_) => normalize::<api::StartInput>(&bytes),
        Operation::Stop(_) => normalize::<api::StopInput>(&bytes),
        Operation::UpdateTemplate(computer) => super::maintenance::input(&bytes, computer),
        Operation::ResumeUpdate {
            computer,
            operation,
        } => super::maintenance::resume_input(&bytes, computer, operation),
        Operation::Ticket(_) => normalize::<api::TerminalTicketInput>(&bytes),
        Operation::RevokeAccess { .. } => normalize::<api::RevokeAccessBody>(&bytes),
        Operation::RevokeAutomation { .. } => normalize::<api::RevokeAutomationGrantBody>(&bytes),
        Operation::GrantAutomation(computer) => {
            let input: api::IssueAutomationGrantInput =
                serde_json::from_slice(&bytes).map_err(|_| Fault::invalid())?;
            if input.computer_id != computer {
                return Err(Fault::invalid());
            }
            serde_json::to_vec(&input).map_err(|_| ())
        }
        Operation::Pairing(_) => {
            let input: api::CliPairingInput =
                serde_json::from_slice(&bytes).map_err(|_| Fault::invalid())?;
            if !input.is_valid() {
                return Err(Fault::invalid());
            }
            serde_json::to_vec(&input).map_err(|_| ())
        }
        Operation::ConfirmPairing { .. } => normalize::<api::CliPairingConfirmBody>(&bytes),
        Operation::List
        | Operation::Read(_)
        | Operation::Receipt { .. }
        | Operation::Maintenance(_)
        | Operation::MaintenanceReceipt { .. }
        | Operation::Access(_)
        | Operation::Automation(_)
        | Operation::AutomationGrant { .. }
            if bytes.is_empty() =>
        {
            Ok(Vec::new())
        }
        _ => Err(()),
    }
    .map_err(|_| Fault::invalid())?;
    let mut admitted = authorize(&state, &route, operation, subject).await?;
    if let Some(after) = page.after {
        admitted
            .url
            .query_pairs_mut()
            .append_pair("after", &after.to_string());
    }
    let client = state
        .upstream
        .client(&admitted.catalog, &admitted.manifest)
        .await
        .map_err(|_| Fault::unavailable())?;
    let mut upstream = client
        .request(method, admitted.url)
        .header(header::AUTHORIZATION, admitted.authorization)
        .body(body);
    if operation.requires_json() {
        upstream = upstream.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(origin) = origin {
        upstream = upstream.header(header::ORIGIN, origin);
    }
    let response = upstream.send().await.map_err(|_| Fault::unavailable())?;
    let status = response.status();
    if status.is_redirection() {
        return Err(Fault::unavailable());
    }
    let bytes = to_bytes(Body::from_stream(response.bytes_stream()), RESPONSE_LIMIT)
        .await
        .map_err(|_| Fault::unavailable())?;
    let body = if status.is_success() {
        match operation {
            Operation::List if status == StatusCode::OK => {
                normalize::<api::ComputerSnapshot>(&bytes)
            }
            Operation::Read(_) if status == StatusCode::OK => {
                normalize::<api::ComputerView>(&bytes)
            }
            Operation::Maintenance(computer) if status == StatusCode::OK => {
                super::maintenance::state(&bytes, computer)
            }
            Operation::MaintenanceReceipt {
                computer,
                operation,
            } if status == StatusCode::OK => {
                super::maintenance::receipt(&bytes, computer, Some(operation))
            }
            Operation::UpdateTemplate(computer)
                if matches!(status, StatusCode::OK | StatusCode::ACCEPTED) =>
            {
                super::maintenance::receipt(&bytes, computer, None)
            }
            Operation::ResumeUpdate {
                computer,
                operation,
            } if matches!(status, StatusCode::OK | StatusCode::ACCEPTED) => {
                super::maintenance::receipt(&bytes, computer, Some(operation))
            }
            Operation::Access(computer) if status == StatusCode::OK => {
                access_grants(&bytes, computer)
            }
            Operation::Automation(computer) if status == StatusCode::OK => {
                let value: api::AutomationGrantCollection =
                    serde_json::from_slice(&bytes).map_err(|_| Fault::unavailable())?;
                if value.computer_id != computer
                    || value.grants.iter().any(|g| g.computer_id != computer)
                {
                    return Err(Fault::unavailable());
                }
                serde_json::to_vec(&value).map_err(|_| ())
            }
            Operation::GrantAutomation(computer) if status == StatusCode::OK => {
                automation_result(&bytes, computer, None, false)
            }
            Operation::AutomationGrant { computer, grant } if status == StatusCode::OK => {
                automation_result(&bytes, computer, Some(grant), false)
            }
            Operation::RevokeAutomation { computer, grant } if status == StatusCode::OK => {
                automation_result(&bytes, computer, Some(grant), true)
            }
            Operation::RevokeAccess { computer, grant } if status == StatusCode::OK => {
                access_revocation(&bytes, computer, grant)
            }
            Operation::Receipt {
                computer,
                operation,
            } if status == StatusCode::OK => read_receipt(&bytes, computer, operation),
            Operation::Ticket(id) if status == StatusCode::CREATED => {
                rewrite_ticket(&bytes, id, route.profile.as_str())
            }
            Operation::Pairing(computer) if status == StatusCode::CREATED => {
                pairing_challenge(&bytes, computer)
            }
            Operation::ConfirmPairing { computer, pairing } if status == StatusCode::CREATED => {
                pairing_result(&bytes, computer, pairing)
            }
            Operation::Create | Operation::Start(_) | Operation::Stop(_)
                if matches!(status, StatusCode::OK | StatusCode::ACCEPTED) =>
            {
                normalize::<api::OperationReceipt>(&bytes)
            }
            _ => Err(()),
        }
    } else {
        normalize::<api::ApiError>(&bytes)
    }
    .map_err(|_| Fault::unavailable())?;
    Ok((
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response())
}

fn automation_result(
    bytes: &[u8],
    computer: Uuid,
    grant: Option<Uuid>,
    revoked: bool,
) -> Result<Vec<u8>, ()> {
    let result: api::AutomationGrantResult = serde_json::from_slice(bytes).map_err(|_| ())?;
    if result.grant.computer_id != computer
        || result.grant.grant_id.is_nil()
        || grant.is_some_and(|grant| result.grant.grant_id != grant)
        || (revoked && result.grant.revoked_at.is_none())
        || result.result_uri != api::automation_grant_uri(computer, result.grant.grant_id)
    {
        return Err(());
    }
    serde_json::to_vec(&result).map_err(|_| ())
}

fn pairing_challenge(bytes: &[u8], computer: Uuid) -> Result<Vec<u8>, ()> {
    let value: api::CliPairingChallenge = serde_json::from_slice(bytes).map_err(|_| ())?;
    if value.computer_id != computer || value.pairing_id.is_nil() {
        return Err(());
    }
    serde_json::to_vec(&value).map_err(|_| ())
}
fn pairing_result(bytes: &[u8], computer: Uuid, pairing: Uuid) -> Result<Vec<u8>, ()> {
    let value: api::CliPairingResult = serde_json::from_slice(bytes).map_err(|_| ())?;
    if value.computer_id != computer
        || value.pairing_id != pairing
        || value.grant_id.is_nil()
        || value.callback_port < 1024
        || value.token.expose_secret().len() != 107
    {
        return Err(());
    }
    serde_json::to_vec(&value).map_err(|_| ())
}

fn read_receipt(bytes: &[u8], computer: Uuid, operation: Uuid) -> Result<Vec<u8>, ()> {
    let receipt: api::OperationReceipt = serde_json::from_slice(bytes).map_err(|_| ())?;
    if receipt.computer_id != computer || receipt.task_id != operation {
        return Err(());
    }
    serde_json::to_vec(&receipt).map_err(|_| ())
}
fn access_grants(bytes: &[u8], computer: Uuid) -> Result<Vec<u8>, ()> {
    let grants: api::AccessGrantCollection = serde_json::from_slice(bytes).map_err(|_| ())?;
    let mut ids = std::collections::BTreeSet::new();
    if grants.computer_id != computer
        || grants.grants.len() > 128
        || grants
            .grants
            .iter()
            .any(|grant| grant.grant_id.is_nil() || !ids.insert(grant.grant_id))
    {
        return Err(());
    }
    serde_json::to_vec(&grants).map_err(|_| ())
}
fn access_revocation(bytes: &[u8], computer: Uuid, grant: Uuid) -> Result<Vec<u8>, ()> {
    let receipt: api::AccessRevocation = serde_json::from_slice(bytes).map_err(|_| ())?;
    if receipt.computer_id != computer || receipt.grant_id != grant || !receipt.revoked {
        return Err(());
    }
    serde_json::to_vec(&receipt).map_err(|_| ())
}

fn require_json(headers: &HeaderMap) -> Result<(), Fault> {
    let mut values = headers.get_all(header::CONTENT_TYPE).iter();
    match (values.next(), values.next()) {
        (Some(value), None)
            if value
                .to_str()
                .ok()
                .and_then(|s| s.split(';').next())
                .is_some_and(|s| s.trim().eq_ignore_ascii_case("application/json")) =>
        {
            Ok(())
        }
        _ => Err(Fault::invalid()),
    }
}
fn normalize<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<Vec<u8>, ()> {
    serde_json::from_slice::<T>(bytes)
        .and_then(|body| serde_json::to_vec(&body))
        .map_err(|_| ())
}
fn rewrite_ticket(bytes: &[u8], id: Uuid, profile: &str) -> Result<Vec<u8>, ()> {
    let mut ticket: api::TerminalTicket = serde_json::from_slice(bytes).map_err(|_| ())?;
    if ticket.computer_id != id {
        return Err(());
    }
    ticket.endpoint = format!("/computers/{profile}/{id}/terminal");
    serde_json::to_vec(&ticket).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_input_does_not_reach_admission() {
        for body in [br#"{"requestId":"00000000-0000-4000-8000-000000000001","owner":"foreign"}"#.as_slice(), br#"{"requestId":"00000000-0000-4000-8000-000000000001","requestId":"00000000-0000-4000-8000-000000000002"}"#, b"null"] {
            assert!(normalize::<api::CreateInput>(body).is_err());
        }
    }
    #[test]
    fn ticket_destination_is_owned_by_gateway_and_bound_to_computer() {
        let id = Uuid::new_v4();
        let bytes = serde_json::to_vec(&api::TerminalTicket {
            computer_id: id,
            token: api::TerminalToken::new("fixture".into()),
            expires_at: chrono::Utc::now(),
            endpoint: "https://foreign.invalid/?token=do-not-forward".into(),
        })
        .unwrap();
        let ticket: api::TerminalTicket =
            serde_json::from_slice(&rewrite_ticket(&bytes, id, "operator").unwrap()).unwrap();
        assert_eq!(
            ticket.endpoint,
            format!("/computers/operator/{id}/terminal")
        );
        assert!(rewrite_ticket(&bytes, Uuid::new_v4(), "operator").is_err());
    }
}
