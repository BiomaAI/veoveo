//! One bounded invalidation stream backed by the existing auth-scoped MCP pool.
use super::fault;
use crate::{
    AppState, api,
    mcp_client::{ResourceSubscriptionError, SharedMcpClient},
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
};
use serde::Deserialize;
use std::{convert::Infallible, time::Duration};
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use veoveo_computers_contract::{COMPUTERS_URI, ComputerEvent, ComputerEventKind};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EventsInput {}

pub(super) async fn events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_): Json<EventsInput>,
) -> Response {
    let session = match api::upstream_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let response_headers = match api::response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return fault(status),
    };
    let id = Uuid::now_v7();
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let client = state
            .mcp
            .client(
                &state.config,
                &session.session.access_token,
                session.session.access_expires_at,
                &session.session.csrf_token,
            )
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let subscription = client
            .subscribe_resource(id, COMPUTERS_URI.into())
            .await
            .map_err(|error| match error {
                ResourceSubscriptionError::Capacity { .. } => StatusCode::TOO_MANY_REQUESTS,
                ResourceSubscriptionError::NotAdmitted => StatusCode::FORBIDDEN,
                _ => StatusCode::SERVICE_UNAVAILABLE,
            })?;
        Ok::<_, StatusCode>((client, subscription.receiver))
    })
    .await;
    let mut response = match result {
        Ok(Ok((client, receiver))) => stream(
            client,
            id,
            receiver,
            session.session.access_expires_at,
            state.computers.stop.clone(),
        ),
        Ok(Err(status)) => fault(status),
        Err(_) => fault(StatusCode::SERVICE_UNAVAILABLE),
    };
    response.headers_mut().extend(response_headers);
    response
}

fn stream(
    client: SharedMcpClient,
    id: Uuid,
    receiver: broadcast::Receiver<String>,
    expires: i64,
    shutdown: CancellationToken,
) -> Response {
    let remaining = expires
        .saturating_sub(5)
        .saturating_sub(chrono::Utc::now().timestamp())
        .max(0);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(remaining.unsigned_abs());
    let cancel = shutdown.child_token();
    let drop_guard = cancel.clone().drop_guard();
    let (sender, receiver_body) = mpsc::channel(1);
    tokio::spawn(async move {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {},
            _ = client.resource_source_lost() => {},
            _ = tokio::time::sleep_until(deadline) => {},
            _ = forward(receiver, sender) => {},
        }
        // Cleanup also runs when the browser stops reading; it is outside the body pump.
        let _ = tokio::time::timeout(Duration::from_secs(5), client.unsubscribe_resource(id)).await;
    });
    let body = futures::stream::unfold(
        (receiver_body, drop_guard),
        |(mut receiver, guard)| async move {
            receiver
                .recv()
                .await
                .map(|event| (Ok::<_, Infallible>(event), (receiver, guard)))
        },
    );
    let mut response = Sse::new(body)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(10)))
        .into_response();
    response.headers_mut().insert(
        "x-accel-buffering",
        axum::http::HeaderValue::from_static("no"),
    );
    response
}

async fn forward(mut receiver: broadcast::Receiver<String>, sender: mpsc::Sender<Event>) {
    let prefix = format!("{COMPUTERS_URI}/");
    // Subscription precedes this baseline wake; the browser then reads current HTTP state.
    loop {
        let event = Event::default()
            .event("computer")
            .json_data(ComputerEvent {
                kind: ComputerEventKind::SnapshotChanged,
                computer_id: None,
            })
            .expect("closed event");
        if sender.send(event).await.is_err() {
            return;
        }
        loop {
            match receiver.recv().await {
                Ok(uri) if uri == COMPUTERS_URI || uri.starts_with(&prefix) => {
                    break;
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => break,
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn feed_starts_with_a_baseline_and_filters_other_domains() {
        let (updates, receiver) = broadcast::channel(8);
        let (sender, mut body) = mpsc::channel(1);
        let task = tokio::spawn(forward(receiver, sender));
        let event = tokio::time::timeout(Duration::from_secs(1), body.recv())
            .await
            .unwrap()
            .unwrap();
        let response =
            Sse::new(futures::stream::iter([Ok::<_, Infallible>(event)])).into_response();
        let bytes = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("event: computer"));
        assert!(text.contains("\"kind\":\"snapshot_changed\""));
        assert!(!text.contains("token"));
        updates.send("artifact://artifacts/foreign".into()).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), body.recv())
                .await
                .is_err()
        );
        updates.send(COMPUTERS_URI.into()).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), body.recv())
                .await
                .unwrap()
                .is_some()
        );
        drop(updates);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
    }
    #[tokio::test]
    async fn lag_and_a_blocked_consumer_keep_the_output_queue_bounded() {
        let (updates, receiver) = broadcast::channel(2);
        let (sender, mut body) = mpsc::channel(1);
        let task = tokio::spawn(forward(receiver, sender));
        for _ in 0..32 {
            updates.send(COMPUTERS_URI.into()).unwrap();
        }
        assert!(
            tokio::time::timeout(Duration::from_secs(1), body.recv())
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            tokio::time::timeout(Duration::from_secs(1), body.recv())
                .await
                .unwrap()
                .is_some()
        );
        drop(body);
        drop(updates);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
    }
}
