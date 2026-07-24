use std::sync::Arc;

use chrono::Utc;
use thiserror::Error;
use tokio::sync::Mutex;
use veoveo_task_runtime::TaskOwner;

use crate::{
    adapter::{Adapter, AdapterError},
    contract::{
        LiveStreamCapability, LiveStreamConnection, LiveStreamEndpoint, LiveStreamLifecycle,
        LiveStreamState, SessionId, StreamId,
    },
    uris,
};

struct OwnedLiveStream {
    owner: TaskOwner,
    state: LiveStreamState,
}

pub(super) struct LiveStreamService {
    adapter: Arc<Adapter>,
    endpoint: LiveStreamEndpoint,
    stream: Mutex<Option<OwnedLiveStream>>,
}

impl LiveStreamService {
    pub(super) fn new(adapter: Arc<Adapter>, endpoint: LiveStreamEndpoint) -> Self {
        Self {
            adapter,
            endpoint,
            stream: Mutex::new(None),
        }
    }

    pub(super) async fn open(
        &self,
        owner: TaskOwner,
        session_id: SessionId,
        stream_id: StreamId,
        capability: &LiveStreamCapability,
    ) -> Result<LiveStreamConnection, LiveStreamError> {
        let mut current = self.stream.lock().await;
        self.expire_locked(&mut current).await;
        if current.is_some() {
            return Err(LiveStreamError::AlreadyLeased);
        }
        let lease = self
            .adapter
            .open_live_stream(&session_id, &stream_id)
            .await?;
        if lease.stream_id != stream_id {
            let _ = self.adapter.close_live_stream(&lease.stream_id).await;
            return Err(LiveStreamError::AdapterIdentityMismatch);
        }
        let now = Utc::now();
        let resource_uri = uris::stream(&session_id, &lease.stream_id);
        let state = LiveStreamState {
            stream_id,
            session_id,
            lifecycle: capability.lifecycle.clone(),
            source: capability.source.clone(),
            codec: capability.codec.clone(),
            hardware_encoder: capability.hardware_encoder.clone(),
            width: capability.width,
            height: capability.height,
            fps: capability.fps,
            connected_viewers: capability.connected_viewers,
            resource_uri,
            created_at: now,
            expires_at: lease.expires_at,
        };
        current.replace(OwnedLiveStream {
            owner,
            state: state.clone(),
        });
        Ok(LiveStreamConnection {
            stream: state,
            endpoint: self.endpoint.clone(),
            access_token: lease.access_token,
        })
    }

    pub(super) async fn renew(
        &self,
        owner: &TaskOwner,
        session_id: &SessionId,
        stream_id: &StreamId,
        capability: &LiveStreamCapability,
    ) -> Result<LiveStreamConnection, LiveStreamError> {
        let mut current = self.stream.lock().await;
        self.expire_locked(&mut current).await;
        let owned = require_owned(current.as_mut(), owner, session_id, stream_id)?;
        let lease = self.adapter.renew_live_stream(stream_id).await?;
        if &lease.stream_id != stream_id {
            return Err(LiveStreamError::AdapterIdentityMismatch);
        }
        project_capability(&mut owned.state, capability);
        owned.state.expires_at = lease.expires_at;
        Ok(LiveStreamConnection {
            stream: owned.state.clone(),
            endpoint: self.endpoint.clone(),
            access_token: lease.access_token,
        })
    }

    pub(super) async fn close(
        &self,
        owner: &TaskOwner,
        session_id: &SessionId,
        stream_id: &StreamId,
    ) -> Result<LiveStreamState, LiveStreamError> {
        let mut current = self.stream.lock().await;
        self.expire_locked(&mut current).await;
        let owned = require_owned(current.as_mut(), owner, session_id, stream_id)?;
        self.adapter.close_live_stream(stream_id).await?;
        let mut closed = owned.state.clone();
        closed.lifecycle = LiveStreamLifecycle::Closed;
        closed.connected_viewers = 0;
        current.take();
        Ok(closed)
    }

    pub(super) async fn list(
        &self,
        owner: &TaskOwner,
        capability: &LiveStreamCapability,
    ) -> Vec<LiveStreamState> {
        let mut current = self.stream.lock().await;
        self.expire_locked(&mut current).await;
        current
            .as_mut()
            .filter(|stream| &stream.owner == owner)
            .map(|stream| {
                project_capability(&mut stream.state, capability);
                stream.state.clone()
            })
            .into_iter()
            .collect()
    }

    pub(super) async fn get(
        &self,
        owner: &TaskOwner,
        stream_id: &StreamId,
        capability: &LiveStreamCapability,
    ) -> Option<LiveStreamState> {
        let mut current = self.stream.lock().await;
        self.expire_locked(&mut current).await;
        current
            .as_mut()
            .filter(|stream| &stream.owner == owner && &stream.state.stream_id == stream_id)
            .map(|stream| {
                project_capability(&mut stream.state, capability);
                stream.state.clone()
            })
    }

    async fn expire_locked(&self, current: &mut Option<OwnedLiveStream>) {
        let expired_id = current
            .as_ref()
            .filter(|stream| stream.state.expires_at <= Utc::now())
            .map(|stream| stream.state.stream_id.clone());
        if let Some(stream_id) = expired_id {
            if let Err(error) = self.adapter.close_live_stream(&stream_id).await {
                tracing::warn!(%error, %stream_id, "expired UAV live-stream adapter lease cleanup failed");
            }
            current.take();
        }
    }
}

fn require_owned<'a>(
    current: Option<&'a mut OwnedLiveStream>,
    owner: &TaskOwner,
    session_id: &SessionId,
    stream_id: &StreamId,
) -> Result<&'a mut OwnedLiveStream, LiveStreamError> {
    current
        .filter(|stream| {
            &stream.owner == owner
                && &stream.state.session_id == session_id
                && &stream.state.stream_id == stream_id
        })
        .ok_or(LiveStreamError::NotFound)
}

fn project_capability(state: &mut LiveStreamState, capability: &LiveStreamCapability) {
    state.lifecycle = capability.lifecycle.clone();
    state.connected_viewers = capability.connected_viewers;
}

#[derive(Debug, Error)]
pub(super) enum LiveStreamError {
    #[error("the follow-camera live stream is already leased")]
    AlreadyLeased,
    #[error("live stream not found")]
    NotFound,
    #[error("simulator adapter returned a different live-stream identity")]
    AdapterIdentityMismatch,
    #[error(transparent)]
    Adapter(#[from] AdapterError),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use veoveo_mcp_contract::{
        AccessSubject, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId,
        TenantId, WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
    };
    use veoveo_task_runtime::{PrincipalKind, TaskOwner};

    use super::*;
    use crate::adapter::FakeAdapter;

    fn owner(principal_key: &str) -> TaskOwner {
        let principal = PrincipalId::new(principal_key).unwrap();
        TaskOwner {
            principal_key: principal_key.to_owned(),
            principal_kind: PrincipalKind::User,
            issuer: "issuer".to_owned(),
            subject: principal_key.to_owned(),
            profile: "operator".to_owned(),
            tenant_key: Some("tenant".to_owned()),
            data_labels: BTreeSet::new(),
            authority: InvocationAuthority {
                work_context: WorkContextId::new("flight").unwrap(),
                tenant: TenantId::new("tenant").unwrap(),
                membership: WorkContextMembershipLevel::Owner,
                policy_revision: PolicyVersion::new("r1").unwrap(),
                output_policy: WorkContextOutputPolicy {
                    owner: AccessSubject::Principal(principal.clone()),
                    initial_grants: Vec::new(),
                    classification: None,
                    data_labels: BTreeSet::new(),
                },
                provenance: InvocationProvenance::Direct {
                    initiator: principal,
                },
            },
        }
    }

    #[tokio::test]
    async fn lease_is_single_viewer_and_exact_owner_scoped() {
        let state = super::super::service::fake_state().unwrap();
        let session_id = state.session_id.clone();
        let capability = state.live_stream.clone();
        let adapter = Arc::new(Adapter::Fake(Arc::new(Mutex::new(FakeAdapter::new(state)))));
        let service = LiveStreamService::new(
            adapter,
            LiveStreamEndpoint {
                signaling_server: "127.0.0.1".to_owned(),
                signaling_port: 49101,
                signaling_path: "/webrtc".to_owned(),
                media_server: "127.0.0.1".to_owned(),
                media_port: 47998,
                force_wss: false,
            },
        );
        let first_owner = owner("pilot-a");
        let other_owner = owner("pilot-b");
        let stream_id = StreamId::new("stream-one").unwrap();
        let opened = service
            .open(
                first_owner.clone(),
                session_id.clone(),
                stream_id.clone(),
                &capability,
            )
            .await
            .unwrap();
        assert_eq!(opened.access_token.expose(), "fake-stream-one-access");
        assert!(matches!(
            service
                .open(
                    other_owner.clone(),
                    session_id.clone(),
                    StreamId::new("stream-two").unwrap(),
                    &capability,
                )
                .await,
            Err(LiveStreamError::AlreadyLeased)
        ));
        assert!(
            service
                .get(&other_owner, &stream_id, &capability)
                .await
                .is_none()
        );
        assert!(matches!(
            service
                .renew(&other_owner, &session_id, &stream_id, &capability)
                .await,
            Err(LiveStreamError::NotFound)
        ));
        service
            .close(&first_owner, &session_id, &stream_id)
            .await
            .unwrap();
    }
}
