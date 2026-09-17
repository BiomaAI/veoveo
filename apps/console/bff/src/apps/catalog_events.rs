//! Catalog notifications drive updates. Bounded native discovery repairs missed
//! notifications and requests routed to a different gateway replica.
use super::*;
use std::pin::Pin;
use tokio::{
    sync::broadcast,
    time::{Interval, Sleep},
};

const RECONCILE: Duration = Duration::from_secs(30);

struct CatalogFeed {
    client: SharedMcpClient,
    receiver: broadcast::Receiver<u64>,
    deadline: Pin<Box<Sleep>>,
    initial: Option<Arc<McpAppCatalog>>,
    reconcile: Interval,
    previous: Option<String>,
}

impl CatalogFeed {
    async fn next(&mut self) -> Option<Event> {
        let catalog = if let Some(catalog) = self.initial.take() {
            catalog
        } else {
            tokio::select! {
                biased;
                _ = &mut self.deadline => return None,
                _ = self.client.resource_source_lost() => return None,
                _ = self.reconcile.tick() => {},
                update = self.receiver.recv() => match update {
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {},
                    Err(broadcast::error::RecvError::Closed) => return None,
                },
            }
            let result = tokio::select! {
                biased;
                _ = &mut self.deadline => return None,
                _ = self.client.resource_source_lost() => return None,
                result = self.client.app_catalog() => result,
            };
            match result {
                Ok(catalog) => catalog,
                Err(error) => {
                    tracing::warn!(%error, "MCP App catalog stream will reconnect");
                    return None;
                }
            }
        };
        let data = serde_json::to_string(&AppCatalog {
            apps: app_descriptors(&catalog),
            degradations: catalog.degradation().failures.clone(),
        })
        .expect("App catalog serializes");
        if self.previous.as_ref() == Some(&data) {
            return Some(Event::default().comment("catalog current"));
        }
        self.previous = Some(data.clone());
        Some(Event::default().event("catalog").data(data))
    }
}

pub(crate) async fn app_catalog_events(
    State(state): State<AppState>,
    request_headers: HeaderMap,
) -> Response {
    let subscription = with_apps_session(&state, &request_headers, |mcp| async move {
        // Subscribe before reading: discovery completion cannot race admission.
        let receiver = mcp.catalog_updates();
        let catalog = mcp.app_catalog().await?;
        Ok::<_, rmcp::ServiceError>((receiver, catalog))
    })
    .await;
    let AppsSessionOutcome {
        client,
        response_headers,
        access_expires_at,
        result,
        ..
    } = match subscription {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    let (receiver, catalog) = match result {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "MCP App catalog event stream failed");
            return with_session_headers(StatusCode::BAD_GATEWAY.into_response(), response_headers);
        }
    };
    let remaining = access_expires_at
        .saturating_sub(5)
        .saturating_sub(chrono::Utc::now().timestamp())
        .max(1);
    let mut reconcile =
        tokio::time::interval_at(tokio::time::Instant::now() + RECONCILE, RECONCILE);
    reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let feed = CatalogFeed {
        client,
        receiver,
        deadline: Box::pin(tokio::time::sleep(Duration::from_secs(
            remaining.unsigned_abs(),
        ))),
        initial: Some(catalog),
        reconcile,
        previous: None,
    };
    let stream = futures::stream::unfold(feed, |mut feed| async move {
        feed.next()
            .await
            .map(|event| (Ok::<_, Infallible>(event), feed))
    });
    let mut response = Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(10)))
        .into_response();
    response.headers_mut().extend(response_headers);
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert("x-accel-buffering", HeaderValue::from_static("no"));
    response
}
