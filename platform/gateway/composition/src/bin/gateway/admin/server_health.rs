//! Per-server health for operators and tools.
//!
//! The gateway's background prober checks each registered server's internal
//! health URL. This endpoint returns its latest result to an authorized
//! administrator, the same cache the Console snapshot reads.
use veoveo_gateway_contract::GatewayAction;

use std::time::Instant;

use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_mcp_contract::{ServerSlug, audit::AdministrativeOperation};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayServerHealth, GatewayServerHealthState};

use super::admin_profile_id;
use crate::{audit::authorize_admin_request, runtime::AdminState};

/// Health of every registered server.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerHealthReport {
    servers: Vec<ServerHealthEntry>,
    module_bindings: Vec<veoveo_mcp_gateway::http::ModuleBindingSnapshot>,
}

/// One server's latest probe. Both fields are null until the first probe after
/// the gateway starts.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerHealthEntry {
    server: ServerSlug,
    state: Option<GatewayServerHealthState>,
    checked_at: Option<DateTime<Utc>>,
}

pub(crate) async fn read_server_health(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (catalog, _profile, _subject) = match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        GatewayAction::AdminRead,
        AdministrativeOperation::ServerHealth,
        started_at,
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    let probed = state.server_health.snapshot();
    let servers = catalog
        .control_plane()
        .servers
        .iter()
        .map(|server| server.slug.clone());
    let mut report = report(servers, |slug| probed.get(slug));
    report.module_bindings = state.module_bindings.as_ref().clone();
    Json(report).into_response()
}

fn report<'a>(
    servers: impl Iterator<Item = ServerSlug>,
    probed: impl Fn(&ServerSlug) -> Option<&'a GatewayServerHealth>,
) -> ServerHealthReport {
    let mut servers = servers
        .map(|server| {
            let health = probed(&server);
            ServerHealthEntry {
                state: health.map(|health| health.state),
                checked_at: health.map(|health| health.checked_at),
                server,
            }
        })
        .collect::<Vec<_>>();
    servers.sort_by(|left, right| left.server.cmp(&right.server));
    ServerHealthReport {
        servers,
        module_bindings: vec![],
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn unprobed_servers_report_no_state_instead_of_offline() {
        let checked_at = Utc::now();
        let probed = BTreeMap::from([(
            ServerSlug::parse("frames").unwrap(),
            GatewayServerHealth {
                state: GatewayServerHealthState::Healthy,
                checked_at,
            },
        )]);
        let report = report(
            ["uav-sim", "frames"]
                .into_iter()
                .map(|slug| ServerSlug::parse(slug).unwrap()),
            |slug| probed.get(slug),
        );
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["servers"][0]["server"], "frames");
        assert_eq!(json["servers"][0]["state"], "healthy");
        assert_eq!(json["servers"][1]["server"], "uav-sim");
        assert!(json["servers"][1]["state"].is_null());
        assert!(json["servers"][1]["checkedAt"].is_null());
    }
    #[test]
    fn module_binding_snapshot_does_not_invent_backend_probe_health() {
        use veoveo_mcp_gateway::http::{ModuleBindingSnapshot, ModuleBindingState};
        let mut report = report(std::iter::empty(), |_| None);
        report.module_bindings = vec![ModuleBindingSnapshot {
            module: veoveo_modules::ModuleName::new("extension").unwrap(),
            state: ModuleBindingState::Unbound,
            required: false,
        }];
        let json = serde_json::to_value(report).unwrap();
        assert_eq!(json["servers"], serde_json::json!([]));
        assert_eq!(json["moduleBindings"][0]["state"], "unbound");
        assert_eq!(json["moduleBindings"][0]["required"], false);
    }
}
