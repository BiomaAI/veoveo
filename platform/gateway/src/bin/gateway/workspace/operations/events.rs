//! One bounded native subscription watches a tab's visible Task IDs. Events are
//! contentless wakes; current tasks/get remains the detail/correctness path.
use super::*;
use axum::response::{
    Sse,
    sse::{Event, KeepAlive},
};
use rmcp::model::SubscriptionFilter;
use std::convert::Infallible;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Watch {
    ids: String,
}

pub(super) async fn watch(
    State(state): State<OperationState>,
    Path(raw_profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(watch): Query<Watch>,
    headers: HeaderMap,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let profile = profile(raw_profile)?;
    let ids = ids(&watch.ids)?;
    let slot = state
        .watches
        .acquire(subject.principal.id.as_str())
        .ok_or(StatusCode::TOO_MANY_REQUESTS)?;
    let catalog = state.catalog.current();
    let authority = state.authority(&subject, &profile).await?;
    let mut tasks = vec![];
    for id in ids {
        let operation = state
            .workspace
            .store
            .workspace_operation(&authority, WorkspaceOperationId::from_uuid(id))
            .await
            .map_err(fault)?;
        if operation.profile != profile.as_str() {
            return Err(StatusCode::NOT_FOUND);
        }
        tasks.push(task_id(&operation)?);
    }
    tasks.sort();
    tasks.dedup();
    let client = state
        .native
        .connect(&profile, &native::bearer(&headers)?)
        .await?;
    let filter = SubscriptionFilter::builder().task_ids(tasks).build();
    let mut listener =
        tokio::time::timeout(Duration::from_secs(8), client.peer().listen(filter.clone()))
            .await
            .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
            .map_err(mcp_error)?;
    if listener.acknowledged() != &filter {
        return Err(StatusCode::FORBIDDEN);
    }
    let deadline = tokio::time::Instant::now()
        + (subject.access_token.expires_at - chrono::Utc::now())
            .to_std()
            .unwrap_or_default()
            .min(Duration::from_secs(55));
    let stream = async_stream::stream! {
        let _slot = slot;
        yield Ok(Event::default().event("change").data("{}").retry(Duration::from_secs(2)));
        let mut authority_check = tokio::time::interval(Duration::from_secs(5));
        authority_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = state.stop.cancelled() => break,
                _ = tokio::time::sleep_until(deadline) => break,
                _ = authority_check.tick() => {
                    if !Arc::ptr_eq(&catalog, &state.catalog.current()) || state.authority(&subject, &profile).await.is_err() {
                        yield Ok(Event::default().event("expired").data("{}")); break;
                    }
                },
                notification = listener.next() => match notification {
                    Ok(Some(_)) => {
                        if state.authority(&subject, &profile).await.is_err() {
                            yield Ok(Event::default().event("expired").data("{}")); break;
                        }
                        yield Ok(Event::default().event("change").data("{}"));
                    }
                    _ => break,
                }
            }
        }
        let _ = tokio::time::timeout(Duration::from_secs(2), listener.cancel()).await;
        client.close().await;
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(10))))
}

fn ids(value: &str) -> Result<Vec<Uuid>, StatusCode> {
    if value.len() > 32 * 37 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let ids = value
        .split(',')
        .map(Uuid::parse_str)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    if ids.is_empty() || ids.len() > 32 {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(ids)
}
