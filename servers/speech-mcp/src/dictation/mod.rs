//! Ephemeral human drafts; audio and text never enter a chat or durable Task here.
mod session;

use crate::{
    process::WorkerProcess,
    worker::{WorkerConnection, WorkerEvent, WorkerRequest},
};
use anyhow::{Result, ensure};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{Mutex, Semaphore, mpsc, oneshot, watch};
use uuid::Uuid;
use veoveo_mcp_contract::{GatewayInternalIdentity, PrincipalKind, WorkContextMembershipLevel};
use veoveo_speech_contract::{
    dictation::{DictationSnapshot, DictationStatus, StartDictation},
    transcript::MAX_DICTATION_SECONDS,
};

pub struct Dictations {
    sessions: Mutex<HashMap<Uuid, Arc<Session>>>,
    slots: Arc<Semaphore>,
    worker: Arc<WorkerProcess>,
}

struct Session {
    owner: GatewayInternalIdentity,
    sample_rate: u32,
    commands: mpsc::Sender<Command>,
    snapshot: watch::Receiver<DictationSnapshot>,
}
enum Command {
    Chunk {
        sequence: u32,
        bytes: Vec<u8>,
        reply: oneshot::Sender<Result<()>>,
    },
    Finish {
        reply: oneshot::Sender<Result<()>>,
    },
    Cancel {
        reply: oneshot::Sender<Result<()>>,
    },
}

fn human(identity: &GatewayInternalIdentity) -> Result<()> {
    ensure!(
        identity.actor.kind == PrincipalKind::User
            && identity
                .authority
                .membership
                .allows(WorkContextMembershipLevel::Contributor)
            && identity
                .request_context
                .as_ref()
                .is_some_and(|context| context.access_token.session_family.is_some()),
        "dictation requires an active human browser session"
    );
    Ok(())
}

fn same_owner(owner: &GatewayInternalIdentity, caller: &GatewayInternalIdentity) -> bool {
    owner.actor.id == caller.actor.id
        && owner.actor.issuer == caller.actor.issuer
        && owner.actor.subject == caller.actor.subject
        && owner.profile == caller.profile
        && owner.authority.work_context == caller.authority.work_context
        && owner.authority.tenant == caller.authority.tenant
        && owner
            .request_context
            .as_ref()
            .map(|c| &c.access_token.session_family)
            == caller
                .request_context
                .as_ref()
                .map(|c| &c.access_token.session_family)
}

impl Dictations {
    pub fn new(worker: Arc<WorkerProcess>, capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            slots: Arc::new(Semaphore::new(capacity)),
            worker,
        })
    }

    pub async fn start(
        self: &Arc<Self>,
        caller: GatewayInternalIdentity,
        request: StartDictation,
    ) -> Result<DictationSnapshot> {
        human(&caller)?;
        ensure!(
            !request.id.is_nil() && (8000..=48000).contains(&request.sample_rate),
            "invalid dictation parameters"
        );
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(&request.id) {
            ensure!(
                same_owner(&session.owner, &caller) && session.sample_rate == request.sample_rate,
                "unknown dictation"
            );
            return Ok(session.snapshot.borrow().clone());
        }
        ensure!(sessions.len() < 64, "dictation receipts at capacity");
        ensure!(
            !sessions
                .values()
                .any(|session| same_owner(&session.owner, &caller)
                    && !session.snapshot.borrow().status.terminal()),
            "a dictation is already active"
        );
        let slot = self.slots.clone().try_acquire_owned()?;
        let mut connection = WorkerConnection::connect(
            self.worker.socket(),
            &WorkerRequest::Live {
                sample_rate: request.sample_rate,
                max_duration_seconds: MAX_DICTATION_SECONDS,
            },
        )
        .await?;
        ensure!(
            matches!(
                tokio::time::timeout(Duration::from_secs(5), connection.event()).await??,
                WorkerEvent::Accepted
            ),
            "dictation capacity unavailable"
        );
        let snapshot = DictationSnapshot {
            id: request.id,
            result_uri: format!("speech://dictation/{}", request.id),
            status: DictationStatus::Listening,
            next_sequence: 0,
            max_duration_seconds: MAX_DICTATION_SECONDS,
            transcript: None,
        };
        let (updates, receiver) = watch::channel(snapshot.clone());
        let (commands, input) = mpsc::channel(2);
        sessions.insert(
            request.id,
            Arc::new(Session {
                owner: caller,
                sample_rate: request.sample_rate,
                commands,
                snapshot: receiver,
            }),
        );
        tokio::spawn(session::run(
            connection,
            input,
            updates,
            request.sample_rate,
            slot,
        ));
        // Bounded receipt lifetime. The session loop independently closes idle audio
        // after ten seconds; this timer performs no inference or provider query.
        let registry = Arc::downgrade(self);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(u64::from(MAX_DICTATION_SECONDS) + 45)).await;
            if let Some(registry) = registry.upgrade() {
                registry.sessions.lock().await.remove(&request.id);
            }
        });
        Ok(snapshot)
    }

    async fn authorized(&self, caller: &GatewayInternalIdentity, id: Uuid) -> Result<Arc<Session>> {
        human(caller)?;
        let sessions = self.sessions.lock().await;
        let session = sessions
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("dictation expired"))?;
        ensure!(same_owner(&session.owner, caller), "unknown dictation");
        Ok(session.clone())
    }

    pub async fn read(
        &self,
        caller: &GatewayInternalIdentity,
        id: Uuid,
    ) -> Result<DictationSnapshot> {
        Ok(self.authorized(caller, id).await?.snapshot.borrow().clone())
    }

    pub async fn chunk(
        &self,
        caller: &GatewayInternalIdentity,
        id: Uuid,
        sequence: u32,
        bytes: Vec<u8>,
    ) -> Result<DictationSnapshot> {
        let session = self.authorized(caller, id).await?;
        let (reply, received) = oneshot::channel();
        tokio::time::timeout(Duration::from_secs(5), async {
            session
                .commands
                .send(Command::Chunk {
                    sequence,
                    bytes,
                    reply,
                })
                .await?;
            received.await?
        })
        .await??;
        Ok(session.snapshot.borrow().clone())
    }

    pub async fn finish(
        &self,
        caller: &GatewayInternalIdentity,
        id: Uuid,
        cancel: bool,
    ) -> Result<DictationSnapshot> {
        let session = self.authorized(caller, id).await?;
        if session.snapshot.borrow().status.terminal() {
            return Ok(session.snapshot.borrow().clone());
        }
        let (reply, received) = oneshot::channel();
        tokio::time::timeout(Duration::from_secs(5), async {
            session
                .commands
                .send(if cancel {
                    Command::Cancel { reply }
                } else {
                    Command::Finish { reply }
                })
                .await?;
            received.await?
        })
        .await??;
        let mut updates = session.snapshot.clone();
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let snapshot = updates.borrow_and_update().clone();
                if snapshot.status.terminal() {
                    return Ok(snapshot);
                }
                updates.changed().await?;
            }
        })
        .await?
    }
}
