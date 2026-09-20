//! Contentless catalog invalidation. Durable heads recover dropped LIVE hints.
use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::{
    Router,
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use futures::StreamExt;
use tokio::sync::broadcast;
use veoveo_mcp_contract::{GatewayAction as Action, agent_management::CatalogWake};
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::{OutboxEventRecord, PlatformTable, RecordId};

use super::{AgentManagementState, authority};
use crate::stream_limits::Limits;

#[derive(Clone)]
struct Events {
    agents: AgentManagementState,
    wakes: broadcast::Sender<Option<RecordId>>,
    limits: Arc<Limits>,
}

pub(super) fn router(agents: AgentManagementState) -> Router<AgentManagementState> {
    let (wakes, _) = broadcast::channel(128);
    let events = Events {
        agents,
        wakes,
        limits: Limits::new(64),
    };
    let shared = events.clone();
    tokio::spawn(async move {
        loop {
            let source = tokio::select! { _ = shared.agents.stop.cancelled() => return, source = shared.agents.store().live::<OutboxEventRecord>(PlatformTable::OutboxEvent) => source };
            if let Ok(mut source) = source {
                loop {
                    tokio::select! {
                        _ = shared.agents.stop.cancelled() => return,
                        hint = source.next() => match hint {
                            Some(Ok(hint)) if hint.data.aggregate_type == "agent_definition" => { let _ = shared.wakes.send(hint.data.tenant); },
                            Some(Ok(_)) => {},
                            _ => break,
                        }
                    }
                }
            }
            let _ = shared.wakes.send(None);
            tokio::select! { _ = shared.agents.stop.cancelled() => return, _ = tokio::time::sleep(Duration::from_secs(1)) => {} }
        }
    });
    Router::new()
        .route("/admin/{profile}/agent-events", get(stream))
        .with_state(events)
}

async fn stream(
    State(state): State<Events>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let Ok(profile_id) = veoveo_mcp_contract::GatewayProfileId::new(&profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let action = if authority::allowed(
        &state.agents.catalog.current(),
        &profile_id,
        &subject,
        Action::AgentDefinitionsRead,
    ) {
        Action::AgentDefinitionsRead
    } else {
        Action::AgentDefinitionsUse
    };
    let actor = match authority::admit(&state.agents, profile, subject, action).await {
        Ok(actor) => actor,
        Err(error) => return error.into_response(),
    };
    let Some(slot) = state.limits.acquire(actor.subject.principal.id.as_str()) else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let mut hints = state.wakes.subscribe();
    let first = match state
        .agents
        .store()
        .agent_catalog_head(&actor.authority)
        .await
    {
        Ok(head) => head,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let stream = async_stream::stream! {
        let _slot = slot;
        let mut head = first;
        yield Ok::<_, Infallible>(change(head));
        let mut tick = tokio::time::interval(Duration::from_secs(15));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tick.tick().await;
        let lifetime = tokio::time::sleep(Duration::from_secs(300));
        tokio::pin!(lifetime);
        loop {
            tokio::select! {
                _ = state.agents.stop.cancelled() => break,
                _ = &mut lifetime => break,
                _ = tick.tick() => {},
                hint = hints.recv() => match hint {
                    Ok(Some(tenant)) if tenant != actor.authority.tenant => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                    _ => {},
                }
            }
            if !Arc::ptr_eq(&actor.catalog, &state.agents.catalog.current()) { break; }
            if authority::live_session(&state.agents, &actor.profile, &actor.subject).await.is_err() { break; }
            match state.agents.store().agent_catalog_head(&actor.authority).await {
                Ok(next) if next != head => { head = next; yield Ok(change(head)); },
                Ok(_) => {},
                Err(_) => { yield Ok(Event::default().event("expired").data("{}")); break; },
            }
        }
    };
    let mut response = Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(10)))
        .into_response();
    response
        .headers_mut()
        .insert("x-accel-buffering", "no".parse().unwrap());
    response
}

fn change(sequence: i64) -> Event {
    Event::default()
        .event("change")
        .id(sequence.to_string())
        .retry(Duration::from_secs(2))
        .json_data(CatalogWake { sequence })
        .expect("bounded wake")
}
