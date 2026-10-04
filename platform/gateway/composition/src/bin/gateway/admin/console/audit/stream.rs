use super::*;
use axum::{
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use base64::Engine;
use futures::StreamExt;
use model::{StreamCursor, StreamParameters};
use std::convert::Infallible;

fn checkpoint(cursor: &StreamCursor) -> Event {
    let id = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(cursor).expect("typed audit cursor"));
    Event::default().event("checkpoint").id(id).data("{}")
}
fn invalidate() -> Event {
    Event::default().event("invalidate").data("{}")
}
fn reset() -> Event {
    Event::default().event("reset").data("{}")
}

pub(crate) async fn stream_audit(
    State(state): State<AdminState>,
    Path(profile): Path<GatewayProfileId>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(parameters): Query<StreamParameters>,
    headers: HeaderMap,
) -> Response {
    let partition = parameters.partition.0;
    let scope = match admit_view(&state, &profile, &subject, &partition, parameters.view).await {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let requested = match headers.get("last-event-id") {
        None => None,
        Some(raw) => {
            let parsed = raw
                .to_str()
                .ok()
                .filter(|s| s.len() <= 4096)
                .and_then(|s| {
                    base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .decode(s)
                        .ok()
                })
                .and_then(|bytes| serde_json::from_slice::<StreamCursor>(&bytes).ok());
            match parsed {
                Some(cursor) if cursor.partition == partition => Some(cursor),
                _ => return StatusCode::BAD_REQUEST.into_response(),
            }
        }
    };
    let Some(slot) = state.console_stream.acquire(subject.principal.id.as_str()) else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let store = state.control_store.platform_store().clone();
    let subscriptions = tokio::time::timeout(READ_DEADLINE, async {
        Ok::<_, veoveo_platform_store::StoreError>((
            store.audit_live(&scope, &partition).await?,
            store.audit_blocks_live(&scope, &partition).await?,
        ))
    })
    .await;
    let (mut records, mut blocks) = match subscriptions {
        Ok(Ok(value)) => value,
        _ => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    // Register both watches before the initial read. Unsealed records invalidate
    // immediately; partition block sequences own resume progress in commit order.
    let head =
        match tokio::time::timeout(READ_DEADLINE, store.audit_partition_checkpoint(&partition))
            .await
        {
            Ok(Ok(head)) => head.map(|head| head.sequence),
            _ => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        };
    let mut catalog = state.catalog.subscribe();
    let lifetime = (subject.access_token.expires_at - Utc::now())
        .to_std()
        .unwrap_or_default()
        .min(Duration::from_secs(900));
    let deadline = tokio::time::Instant::now() + lifetime;
    let stream = async_stream::stream! {
        let _slot = slot;
        let mut cursor = requested.unwrap_or(StreamCursor { partition: partition.clone(), sequence: head });
        yield Ok::<_, Infallible>(Event::default().retry(Duration::from_secs(2)));
        yield Ok(invalidate());
        if cursor.sequence > head { cursor.sequence = head; yield Ok(reset()); }
        let mut wake_replay = true;
        loop {
            if admit_view(&state, &profile, &subject, &partition, parameters.view).await.is_err() { break; }
            if wake_replay {
                let mut caught_up = false;
                // Large/expired resume ranges reset the view; they cannot create
                // an unbounded replay or expose another partition's feed.
                for _ in 0..64 {
                    let page = match tokio::time::timeout(READ_DEADLINE, store.audit_blocks(&scope, &partition, cursor.sequence, 16)).await {
                        Ok(Ok(page)) => page, _ => { yield Ok(reset()); return; }
                    };
                    if page.is_empty() { caught_up = true; break; }
                    for block in page {
                        let expected = cursor.sequence.map_or(1, |sequence| sequence.get() + 1);
                        if block.head.sequence.get() != expected { yield Ok(reset()); }
                        cursor.sequence = Some(block.head.sequence);
                    }
                    yield Ok(invalidate());
                    yield Ok(checkpoint(&cursor));
                    if tokio::time::Instant::now() >= deadline { return; }
                }
                if !caught_up { yield Ok(reset()); return; }
                yield Ok(checkpoint(&cursor));
            }
            wake_replay = false;
            tokio::select! {
                () = tokio::time::sleep_until(deadline) => break,
                _ = catalog.changed() => break,
                item = records.next() => match item {
                    Some(Ok(_)) => {
                        if admit_view(&state, &profile, &subject, &partition, parameters.view).await.is_err() { break; }
                        yield Ok(invalidate());
                    }
                    _ => { yield Ok(reset()); break; }
                },
                item = blocks.next() => match item {
                    Some(Ok(())) => wake_replay = true,
                    _ => { yield Ok(reset()); break; }
                },
            }
        }
    };
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(10))),
    )
        .into_response()
}
