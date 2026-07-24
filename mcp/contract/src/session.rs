//! Bounded in-memory ownership for canonical Streamable HTTP sessions.

use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, Weak},
    task::{Context, Poll},
    time::{Duration, Instant},
};

use futures::Stream;
use parking_lot::Mutex;
use rmcp::{
    model::{ClientJsonRpcMessage, ServerJsonRpcMessage},
    transport::streamable_http_server::{
        RestoreOutcome, SessionId, SessionManager,
        session::{
            ServerSseMessage,
            local::{LocalSessionManager, LocalSessionManagerError},
        },
    },
};

/// Grace period retained after a session has no live HTTP stream or request.
///
/// This keeps Streamable HTTP reconnection possible across a brief network
/// interruption while bounding state left by a client that disappears without
/// sending the protocol DELETE.
pub const MCP_SESSION_DISCONNECT_GRACE: Duration = Duration::from_secs(60);

const MCP_SESSION_REAP_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct SessionActivity {
    last_activity: Instant,
    active_streams: usize,
}

impl SessionActivity {
    fn new() -> Self {
        Self {
            last_activity: Instant::now(),
            active_streams: 0,
        }
    }
}

#[derive(Debug)]
struct BoundedSessionInner {
    local: LocalSessionManager,
    activity: Mutex<HashMap<SessionId, SessionActivity>>,
    disconnect_grace: Duration,
}

/// Stateful local MCP sessions with bounded cleanup for vanished clients.
///
/// Explicit MCP DELETE remains the immediate cleanup path. A live GET/SSE or
/// request response stream keeps its session active. Once all streams vanish,
/// ordinary requests may still reconnect or continue the session during the
/// disconnect grace period. The reaper then closes the local transport, which
/// drops the server handler and lets it close any owned upstream sessions.
#[derive(Clone, Debug)]
pub struct BoundedLocalSessionManager {
    inner: Arc<BoundedSessionInner>,
}

impl BoundedLocalSessionManager {
    fn with_timing(disconnect_grace: Duration, reap_interval: Duration) -> Self {
        let inner = Arc::new(BoundedSessionInner {
            local: LocalSessionManager::default(),
            activity: Mutex::new(HashMap::new()),
            disconnect_grace,
        });
        spawn_session_reaper(Arc::downgrade(&inner), reap_interval);
        Self { inner }
    }

    fn touch(&self, id: &SessionId) -> Result<(), LocalSessionManagerError> {
        let mut activity = self.inner.activity.lock();
        let session = activity
            .get_mut(id)
            .ok_or_else(|| LocalSessionManagerError::SessionNotFound(id.clone()))?;
        session.last_activity = Instant::now();
        Ok(())
    }

    fn begin_stream(&self, id: &SessionId) -> Result<SessionStreamGuard, LocalSessionManagerError> {
        let mut activity = self.inner.activity.lock();
        let session = activity
            .get_mut(id)
            .ok_or_else(|| LocalSessionManagerError::SessionNotFound(id.clone()))?;
        session.last_activity = Instant::now();
        session.active_streams += 1;
        Ok(SessionStreamGuard {
            inner: Arc::downgrade(&self.inner),
            session_id: id.clone(),
        })
    }
}

impl Default for BoundedLocalSessionManager {
    fn default() -> Self {
        Self::with_timing(MCP_SESSION_DISCONNECT_GRACE, MCP_SESSION_REAP_INTERVAL)
    }
}

/// Constructs the canonical stateful session manager for Veoveo HTTP servers.
pub fn canonical_session_manager() -> Arc<BoundedLocalSessionManager> {
    Arc::new(BoundedLocalSessionManager::default())
}

impl SessionManager for BoundedLocalSessionManager {
    type Error = LocalSessionManagerError;
    type Transport = <LocalSessionManager as SessionManager>::Transport;

    async fn create_session(&self) -> Result<(SessionId, Self::Transport), Self::Error> {
        let (id, transport) = self.inner.local.create_session().await?;
        self.inner
            .activity
            .lock()
            .insert(id.clone(), SessionActivity::new());
        Ok((id, transport))
    }

    async fn initialize_session(
        &self,
        id: &SessionId,
        message: ClientJsonRpcMessage,
    ) -> Result<ServerJsonRpcMessage, Self::Error> {
        self.touch(id)?;
        self.inner.local.initialize_session(id, message).await
    }

    async fn close_session(&self, id: &SessionId) -> Result<(), Self::Error> {
        self.inner.activity.lock().remove(id);
        self.inner.local.close_session(id).await
    }

    async fn has_session(&self, id: &SessionId) -> Result<bool, Self::Error> {
        if !self.inner.activity.lock().contains_key(id) {
            return Ok(false);
        }
        self.inner.local.has_session(id).await
    }

    async fn create_stream(
        &self,
        id: &SessionId,
        message: ClientJsonRpcMessage,
    ) -> Result<impl Stream<Item = ServerSseMessage> + Send + Sync + 'static, Self::Error> {
        let guard = self.begin_stream(id)?;
        let stream = self.inner.local.create_stream(id, message).await?;
        Ok(TrackedSessionStream::new(stream, guard))
    }

    async fn accept_message(
        &self,
        id: &SessionId,
        message: ClientJsonRpcMessage,
    ) -> Result<(), Self::Error> {
        self.touch(id)?;
        self.inner.local.accept_message(id, message).await
    }

    async fn create_standalone_stream(
        &self,
        id: &SessionId,
    ) -> Result<impl Stream<Item = ServerSseMessage> + Send + Sync + 'static, Self::Error> {
        let guard = self.begin_stream(id)?;
        let stream = self.inner.local.create_standalone_stream(id).await?;
        Ok(TrackedSessionStream::new(stream, guard))
    }

    async fn resume(
        &self,
        id: &SessionId,
        last_event_id: String,
    ) -> Result<impl Stream<Item = ServerSseMessage> + Send + Sync + 'static, Self::Error> {
        let guard = self.begin_stream(id)?;
        let stream = self.inner.local.resume(id, last_event_id).await?;
        Ok(TrackedSessionStream::new(stream, guard))
    }

    async fn restore_session(
        &self,
        _id: SessionId,
    ) -> Result<RestoreOutcome<Self::Transport>, Self::Error> {
        Ok(RestoreOutcome::NotSupported)
    }
}

fn spawn_session_reaper(inner: Weak<BoundedSessionInner>, reap_interval: Duration) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(reap_interval).await;
            let Some(inner) = inner.upgrade() else {
                return;
            };
            let expired = {
                let now = Instant::now();
                let mut activity = inner.activity.lock();
                let expired = activity
                    .iter()
                    .filter_map(|(id, session)| {
                        (session.active_streams == 0
                            && now.duration_since(session.last_activity) >= inner.disconnect_grace)
                            .then(|| id.clone())
                    })
                    .collect::<Vec<_>>();
                for id in &expired {
                    activity.remove(id);
                }
                expired
            };
            for id in expired {
                if let Err(error) = inner.local.close_session(&id).await {
                    tracing::warn!(session_id = %id, %error, "failed to reap abandoned MCP session");
                } else {
                    tracing::debug!(session_id = %id, "reaped abandoned MCP session");
                }
            }
        }
    });
}

struct SessionStreamGuard {
    inner: Weak<BoundedSessionInner>,
    session_id: SessionId,
}

impl Drop for SessionStreamGuard {
    fn drop(&mut self) {
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        let mut activity = inner.activity.lock();
        let Some(session) = activity.get_mut(&self.session_id) else {
            return;
        };
        session.active_streams = session.active_streams.saturating_sub(1);
        session.last_activity = Instant::now();
    }
}

struct TrackedSessionStream<S> {
    inner: Pin<Box<S>>,
    _guard: SessionStreamGuard,
}

impl<S> TrackedSessionStream<S> {
    fn new(inner: S, guard: SessionStreamGuard) -> Self {
        Self {
            inner: Box::pin(inner),
            _guard: guard,
        }
    }
}

impl<S: Stream> Stream for TrackedSessionStream<S> {
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().inner.as_mut().poll_next(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reaps_a_session_abandoned_without_delete() {
        let manager = BoundedLocalSessionManager::with_timing(
            Duration::from_millis(20),
            Duration::from_millis(5),
        );
        let (session_id, _transport) = manager.create_session().await.unwrap();

        tokio::time::sleep(Duration::from_millis(60)).await;

        assert!(!manager.has_session(&session_id).await.unwrap());
    }

    #[tokio::test]
    async fn live_stream_defers_reaping_until_disconnect_grace_elapses() {
        let manager = BoundedLocalSessionManager::with_timing(
            Duration::from_millis(20),
            Duration::from_millis(5),
        );
        let (session_id, _transport) = manager.create_session().await.unwrap();
        let stream_guard = manager.begin_stream(&session_id).unwrap();

        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(manager.has_session(&session_id).await.unwrap());

        drop(stream_guard);
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(!manager.has_session(&session_id).await.unwrap());
    }

    #[tokio::test]
    async fn explicit_close_remains_immediate_and_idempotent() {
        let manager =
            BoundedLocalSessionManager::with_timing(Duration::from_secs(1), Duration::from_secs(1));
        let (session_id, _transport) = manager.create_session().await.unwrap();

        manager.close_session(&session_id).await.unwrap();
        manager.close_session(&session_id).await.unwrap();

        assert!(!manager.has_session(&session_id).await.unwrap());
    }
}
