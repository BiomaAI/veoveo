//! LIVE is a contentless latency hint. Every browser wake is based on a fresh,
//! authorized durable head. Database reconciliation never queries a provider.
use std::{collections::BTreeMap, convert::Infallible, sync::Arc, time::Duration};

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
use chrono::Utc;
use futures::StreamExt;
use parking_lot::Mutex;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, broadcast};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use veoveo_mcp_contract::{AuthorizationServerId, GatewayProfileId, workspace::ChatWake};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalogHandle, GatewayState};
use veoveo_platform_store::{PlatformStore, RecordId, WorkspaceChatId};

use super::{WorkspaceState, authority, fault};

const RECONCILE: Duration = Duration::from_secs(15);
const MAX_LIFETIME: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
struct EventState {
    workspace: WorkspaceState,
    gateway: GatewayState,
    catalog: GatewayCatalogHandle,
    wake: broadcast::Sender<RecordId>,
    limits: Arc<Limits>,
    stop: CancellationToken,
}

struct Limits {
    total: Arc<Semaphore>,
    people: Mutex<BTreeMap<String, usize>>,
}
struct Slot {
    _permit: OwnedSemaphorePermit,
    limits: Arc<Limits>,
    person: String,
}
impl Limits {
    fn acquire(self: &Arc<Self>, person: &str) -> Option<Slot> {
        let permit = self.total.clone().try_acquire_owned().ok()?;
        let mut people = self.people.lock();
        let count = people.entry(person.to_owned()).or_default();
        if *count >= 4 {
            return None;
        }
        *count += 1;
        Some(Slot {
            _permit: permit,
            limits: self.clone(),
            person: person.to_owned(),
        })
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        let mut people = self.limits.people.lock();
        if let Some(count) = people.get_mut(&self.person) {
            *count -= 1;
            if *count == 0 {
                people.remove(&self.person);
            }
        }
    }
}

pub(crate) fn router(
    store: PlatformStore,
    gateway: GatewayState,
    catalog: GatewayCatalogHandle,
    stop: CancellationToken,
) -> Router {
    let (wake, _) = broadcast::channel(256);
    spawn_hub(store.clone(), wake.clone(), stop.clone());
    Router::new()
        .route("/workspace-api/{profile}/chats/{chat}/events", get(events))
        .with_state(EventState {
            workspace: WorkspaceState { store },
            gateway,
            catalog,
            wake,
            limits: Arc::new(Limits {
                total: Arc::new(Semaphore::new(128)),
                people: Mutex::new(BTreeMap::new()),
            }),
            stop,
        })
}

fn spawn_hub(store: PlatformStore, wake: broadcast::Sender<RecordId>, stop: CancellationToken) {
    tokio::spawn(async move {
        loop {
            let source = tokio::select! { _ = stop.cancelled() => return, result = store.workspace_wakes() => result };
            if let Ok(mut source) = source {
                loop {
                    tokio::select! {
                        _ = stop.cancelled() => return,
                        event = source.next() => match event {
                            Some(Ok(event)) => { let _ = wake.send(event.data.chat); }
                            _ => break,
                        }
                    }
                }
            }
            tokio::select! { _ = stop.cancelled() => return, _ = tokio::time::sleep(Duration::from_secs(1)) => {} }
        }
    });
}

async fn events(
    State(state): State<EventState>,
    Path((profile, chat)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let Ok(profile) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = state.catalog.current();
    let Some(config) = catalog.profile(&profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let authorization_server = config.authorization_server.clone();
    let Some(slot) = state.limits.acquire(subject.principal.id.as_str()) else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let chat = WorkspaceChatId::from_uuid(chat);
    // Subscribe before the first durable read; a racing commit cannot disappear.
    let mut wakes = state.wake.subscribe();
    let first = match current_head(
        &state.workspace,
        &state.gateway,
        &profile,
        &authorization_server,
        &subject,
        chat,
    )
    .await
    {
        Ok(head) => head,
        Err(status) => return status.into_response(),
    };
    let ttl = (subject.access_token.expires_at - Utc::now())
        .to_std()
        .unwrap_or_default()
        .min(MAX_LIFETIME);
    let deadline = tokio::time::Instant::now() + ttl;
    let stream = async_stream::stream! {
        let _slot = slot;
        let mut head = first;
        yield Ok::<_, Infallible>(change(head));
        let mut reconcile = tokio::time::interval(RECONCILE);
        reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        reconcile.tick().await;
        loop {
            tokio::select! {
                _ = state.stop.cancelled() => break,
                _ = tokio::time::sleep_until(deadline) => break,
                _ = reconcile.tick() => {},
                hint = wakes.recv() => match hint {
                    Ok(id) if id != chat.record_id() => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                    _ => {}, // Lag also requires authoritative reconciliation.
                }
            }
            // Changed installation policy requires a fresh HTTP admission.
            if !Arc::ptr_eq(&catalog, &state.catalog.current()) { break; }
            match current_head(&state.workspace, &state.gateway, &profile, &authorization_server, &subject, chat).await {
                Ok(current) if current != head => { head = current; yield Ok(change(head)); }
                Ok(_) => {},
                Err(_) => { yield Ok(Event::default().event("expired").data("{}")); break; }
            }
        }
    };
    let mut response = Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(10)))
        .into_response();
    response
        .headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
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
        .json_data(ChatWake { sequence })
        .expect("integer wake is serializable")
}

async fn current_head(
    workspace: &WorkspaceState,
    gateway: &GatewayState,
    profile: &GatewayProfileId,
    authorization_server: &AuthorizationServerId,
    subject: &AuthenticatedSubject,
    chat: WorkspaceChatId,
) -> Result<i64, StatusCode> {
    tokio::time::timeout(Duration::from_secs(5), async {
        if subject.access_token.expires_at <= Utc::now()
            || !gateway
                .access_token_session_valid(
                    profile,
                    authorization_server,
                    &subject.access_token,
                    &subject.principal,
                )
                .await
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        {
            return Err(StatusCode::UNAUTHORIZED);
        }
        if let Some(jwt) = &subject.access_token.jwt_id
            && gateway
                .jwt_revocation(profile, &subject.access_token.issuer, jwt, Utc::now())
                .await
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
                .is_some()
        {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let actor = authority::admit(workspace, subject).await?;
        workspace
            .store
            .workspace_head(&actor, chat)
            .await
            .map_err(fault)
    })
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn durable_heads_and_live_hints_recheck_membership_and_token_expiry() {
        let db = crate::test_store::TestDb::new().await;
        super::super::tests::setup(&db.a).await;
        let workspace = WorkspaceState {
            store: db.a.clone(),
        };
        let gateway = GatewayState::new(db.b.clone());
        let mut subject = super::super::tests::subject("Alice");
        let profile = GatewayProfileId::new("operator").unwrap();
        let server = AuthorizationServerId::new("test").unwrap();
        let chat = WorkspaceChatId::from_uuid(Uuid::now_v7());
        let actor = authority::admit(&workspace, &subject).await.unwrap();
        let mut live = db.b.workspace_wakes().await.unwrap();
        db.a.create_workspace_chat(&actor, chat, "Live chat")
            .await
            .unwrap();
        let notification = tokio::time::timeout(Duration::from_secs(3), live.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(notification.data.chat, chat.record_id());
        // Missing bound families fail closed, even when the signed token lives.
        assert_eq!(
            current_head(&workspace, &gateway, &profile, &server, &subject, chat).await,
            Err(StatusCode::UNAUTHORIZED)
        );
        subject.access_token.session_family = None;
        assert_eq!(
            current_head(&workspace, &gateway, &profile, &server, &subject, chat)
                .await
                .unwrap(),
            1
        );
        db.b.client()
            .query("UPDATE ONLY $person SET enabled = false;")
            .bind(("person", actor.principal.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            current_head(&workspace, &gateway, &profile, &server, &subject, chat).await,
            Err(StatusCode::FORBIDDEN)
        );
        subject.access_token.expires_at = Utc::now() - chrono::TimeDelta::seconds(1);
        assert_eq!(
            current_head(&workspace, &gateway, &profile, &server, &subject, chat).await,
            Err(StatusCode::UNAUTHORIZED)
        );
    }
    #[test]
    fn slow_consumers_and_many_tabs_have_bounded_capacity() {
        let limits = Arc::new(Limits {
            total: Arc::new(Semaphore::new(5)),
            people: Mutex::new(BTreeMap::new()),
        });
        let tabs: Vec<_> = (0..4).map(|_| limits.acquire("Alice").unwrap()).collect();
        assert!(limits.acquire("Alice").is_none());
        let bob = limits.acquire("Bob").unwrap();
        assert!(limits.acquire("Eve").is_none());
        drop(tabs);
        drop(bob);
        assert!(limits.people.lock().is_empty());
        assert_eq!(limits.total.available_permits(), 5);
    }
}
