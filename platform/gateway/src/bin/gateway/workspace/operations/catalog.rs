//! Bounded native discovery and notification-driven admission of required tools.
use axum::http::StatusCode;
use rmcp::model::{PaginatedRequestParams, ServerNotification, SubscriptionFilter, Tool};
use std::time::Duration;
use veoveo_mcp_contract::{GatewayDiscoveryDegradation, GatewayDiscoverySurface, GatewayToolName};

use super::{mcp_error, native::NativeClient};

/// A partial isolated catalog is provisional until its native list-change stream
/// settles the required servers. An empty selection requests the complete authoring
/// picker, whose callers have not chosen a required subset yet. Open the stream
/// first to retain early changes.
pub(super) async fn required_tools(
    client: &NativeClient,
    required: &[GatewayToolName],
) -> Result<Vec<Tool>, StatusCode> {
    let reactive = client.peer().peer_info().is_some_and(|info| {
        info.capabilities
            .tools
            .as_ref()
            .is_some_and(|tools| tools.list_changed == Some(true))
    });
    if !reactive {
        return catalog_tools(client, Some(required)).await;
    }
    tokio::time::timeout(Duration::from_secs(8), async {
        let mut filter = SubscriptionFilter::new();
        filter.tools_list_changed = Some(true);
        let mut changes = client
            .peer()
            .listen(filter.clone())
            .await
            .map_err(mcp_error)?;
        if changes.acknowledged() != &filter {
            return Err(StatusCode::BAD_GATEWAY);
        }
        let result = loop {
            match catalog_tools(client, Some(required)).await {
                Err(StatusCode::SERVICE_UNAVAILABLE) => {}
                result => break result,
            }
            loop {
                match changes.next().await {
                    Ok(Some(ServerNotification::ToolListChangedNotification(_))) => break,
                    Ok(Some(_)) => {}
                    Ok(None) | Err(_) => return Err(StatusCode::SERVICE_UNAVAILABLE),
                }
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(1), changes.cancel()).await;
        result
    })
    .await
    .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
}

pub(super) async fn tools(client: &NativeClient) -> Result<Vec<rmcp::model::Tool>, StatusCode> {
    catalog_tools(client, None).await
}

async fn catalog_tools(
    client: &NativeClient,
    required: Option<&[GatewayToolName]>,
) -> Result<Vec<rmcp::model::Tool>, StatusCode> {
    let mut result = vec![];
    let mut cursor = None;
    for _ in 0..16 {
        let page = tokio::time::timeout(
            Duration::from_secs(8),
            client.peer().list_tools(
                cursor.map(|cursor| PaginatedRequestParams::default().with_cursor(Some(cursor))),
            ),
        )
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
        .map_err(mcp_error)?;
        let degradation = GatewayDiscoveryDegradation::from_meta(page.meta.as_ref())
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
        if required.is_some_and(|required| {
            degradation.failures.iter().any(|failure| {
                failure.surface == GatewayDiscoverySurface::Tools
                    && (required.is_empty()
                        || required.iter().any(|tool| {
                            tool.as_str()
                                .split_once("__")
                                .is_none_or(|(server, _)| server == failure.server.as_str())
                        }))
            })
        }) {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        result.extend(page.tools);
        if result.len() > 512 {
            return Err(StatusCode::BAD_GATEWAY);
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(result);
        }
    }
    Err(StatusCode::BAD_GATEWAY)
}
