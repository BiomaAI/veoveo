use std::{collections::BTreeMap, time::Duration};

use chrono::{DateTime, Utc};
use futures::future::join_all;
use serde::Serialize;
use veoveo_mcp_contract::{ServerManifest, ServerSlug};

use crate::GatewayCatalog;

use super::upstream_http::build_upstream_http_client;

const SERVER_HEALTH_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayServerHealthState {
    Healthy,
    Degraded,
    Offline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayServerHealth {
    pub state: GatewayServerHealthState,
    pub checked_at: DateTime<Utc>,
}

pub async fn probe_gateway_server_health(
    catalog: &GatewayCatalog,
) -> BTreeMap<ServerSlug, GatewayServerHealth> {
    join_all(
        catalog
            .control_plane()
            .servers
            .iter()
            .map(|server| probe_server(catalog, server)),
    )
    .await
    .into_iter()
    .collect()
}

async fn probe_server(
    catalog: &GatewayCatalog,
    server: &ServerManifest,
) -> (ServerSlug, GatewayServerHealth) {
    let checked_at = Utc::now();
    let state = match build_upstream_http_client(catalog, server).await {
        Ok(client) => match tokio::time::timeout(
            SERVER_HEALTH_TIMEOUT,
            client.head(server.upstream.url.as_str()).send(),
        )
        .await
        {
            Ok(Ok(response)) => classify_status(response.status()),
            Ok(Err(error)) => {
                tracing::debug!(server = %server.slug, %error, "gateway upstream health probe failed");
                GatewayServerHealthState::Offline
            }
            Err(_) => GatewayServerHealthState::Offline,
        },
        Err(error) => {
            tracing::warn!(server = %server.slug, %error, "gateway upstream health probe configuration failed");
            GatewayServerHealthState::Degraded
        }
    };
    (
        server.slug.clone(),
        GatewayServerHealth { state, checked_at },
    )
}

fn classify_status(status: reqwest::StatusCode) -> GatewayServerHealthState {
    if status.is_success()
        || (status.is_client_error()
            && status != reqwest::StatusCode::NOT_FOUND
            && status != reqwest::StatusCode::GONE)
    {
        GatewayServerHealthState::Healthy
    } else {
        GatewayServerHealthState::Degraded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_reachable_mcp_responses_without_hiding_bad_routes() {
        assert_eq!(
            classify_status(reqwest::StatusCode::NO_CONTENT),
            GatewayServerHealthState::Healthy
        );
        assert_eq!(
            classify_status(reqwest::StatusCode::UNAUTHORIZED),
            GatewayServerHealthState::Healthy
        );
        assert_eq!(
            classify_status(reqwest::StatusCode::METHOD_NOT_ALLOWED),
            GatewayServerHealthState::Healthy
        );
        assert_eq!(
            classify_status(reqwest::StatusCode::NOT_FOUND),
            GatewayServerHealthState::Degraded
        );
        assert_eq!(
            classify_status(reqwest::StatusCode::BAD_GATEWAY),
            GatewayServerHealthState::Degraded
        );
    }
}
