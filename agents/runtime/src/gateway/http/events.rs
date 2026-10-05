//! Contentless native invalidation with revisions of current authorized views.
use crate::contract::AgentAction as Action;
use crate::persistence::AgentRepository;
use std::{convert::Infallible, sync::Arc, time::Duration};

use crate::contract::authoring::CatalogWake;
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
use tokio::sync::watch;
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::{ChangefeedConsumerId, ChangefeedDelivery, PlatformTable};
use veoveo_types::Sha256Digest;

use super::{AgentManagementState, authority};
use veoveo_mcp_gateway::http::stream_limits::Limits;

#[derive(Clone)]
struct Events {
    agents: AgentManagementState,
    wakes: watch::Receiver<bool>,
    limits: Arc<Limits>,
}

pub(super) fn router(agents: AgentManagementState) -> anyhow::Result<Router<AgentManagementState>> {
    let (wake_tx, wakes) = watch::channel(true);
    let events = Events {
        agents,
        wakes,
        limits: Limits::new(64),
    };
    let shared = events.clone();
    events.agents.scope.spawn(async move {
        let replica = std::env::var("HOSTNAME").unwrap_or_else(|_| "local".into());
        let consumer = ChangefeedConsumerId::new(format!("gateway-agent-events/{replica}"))
            .expect("gateway replica must be a valid consumer identity");
        let store = shared.agents.store();
        let cursor = store
            .changefeed_checkpoint(&consumer)
            .await
            .unwrap_or_default();
        let mut source = store.observe_changes(
            vec![
                veoveo_modules::ObservationTable::from(
                    crate::AgentObservationTable::AgentDefinition,
                ),
                veoveo_modules::ObservationTable::from(crate::AgentObservationTable::ManagedAgent),
                veoveo_modules::ObservationTable::from(PlatformTable::WorkContext),
                veoveo_modules::ObservationTable::from(PlatformTable::Principal),
                veoveo_modules::ObservationTable::from(PlatformTable::Tenant),
                veoveo_modules::ObservationTable::from(PlatformTable::GatewayRefreshFamily),
                veoveo_modules::ObservationTable::from(PlatformTable::GatewayJwtRevocation),
            ],
            cursor,
        );
        loop {
            let delivery = tokio::select! {
                _ = shared.agents.stop.cancelled() => return,
                delivery = source.next() => delivery,
            };
            match delivery {
                Some(Ok(delivery)) => {
                    let changed = match &delivery {
                        ChangefeedDelivery::Reconcile { .. } => true,
                        ChangefeedDelivery::Changes { entries, .. } => !entries.is_empty(),
                    };
                    if changed {
                        wake_tx.send_replace(true);
                    }
                    if let Err(error) = store.checkpoint_changes(&consumer, delivery.cursor()).await
                    {
                        tracing::warn!(%error, "agent event checkpoint unavailable");
                    }
                }
                Some(Err(error)) => {
                    tracing::warn!(%error, "agent event source disconnected");
                    wake_tx.send_replace(false);
                }
                None => {
                    return;
                }
            }
        }
    })?;
    Ok(Router::new()
        .route("/admin/{profile}/agent-events", get(stream))
        .with_state(events))
}

async fn stream(
    State(state): State<Events>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let Ok(profile_id) = veoveo_mcp_contract::GatewayProfileId::parse(&profile) else {
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
    let mut hints = state.wakes.clone();
    if hints.has_changed().is_err() || !*hints.borrow_and_update() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let mut catalog = state.agents.catalog.subscribe();
    let first = match AgentRepository::new(state.agents.store().clone())
        .agent_management_revision(&actor.authority)
        .await
    {
        Ok(head) => head,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let stream = async_stream::stream! {
        let _slot = slot;
        let mut head = first;
        yield Ok::<_, Infallible>(change(&head));
        let remaining = (actor.subject.access_token.expires_at - chrono::Utc::now())
            .to_std().unwrap_or(Duration::ZERO).min(Duration::from_secs(300));
        let lifetime = tokio::time::sleep(remaining);
        tokio::pin!(lifetime);
        loop {
            tokio::select! {
                _ = state.agents.stop.cancelled() => break,
                _ = &mut lifetime => break,
                _ = catalog.changed() => break,
                hint = hints.changed() => {
                    if hint.is_err() || !*hints.borrow_and_update() { break; }
                }
            }
            if !Arc::ptr_eq(&actor.catalog, &state.agents.catalog.current()) { break; }
            if authority::live_session(&state.agents, &actor.profile, &actor.subject).await.is_err() { break; }
            match AgentRepository::new(state.agents.store().clone()).agent_management_revision(&actor.authority).await {
                Ok(next) if next != head => { head = next; yield Ok(change(&head)); },
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

fn change(revision: &Sha256Digest) -> Event {
    Event::default()
        .event("change")
        .id(revision)
        .retry(Duration::from_secs(2))
        .json_data(CatalogWake {
            revision: revision.clone(),
        })
        .expect("bounded wake")
}
