//! Ephemeral human drafts; audio and text never enter a chat or durable Task here.
mod audit;
mod session;
use veoveo_audit::AuditWriter;
use veoveo_mcp_contract::audit::{AuditContext, AuditReason};

use crate::{
    process::WorkerProcess,
    worker::{WorkerConnection, WorkerEvent, WorkerRequest},
};
use anyhow::{Result, ensure};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{Mutex, Semaphore, mpsc, oneshot, watch};
use veoveo_mcp_contract::{GatewayInternalIdentity, PrincipalKind};
use veoveo_speech_contract::DictationSessionId;
use veoveo_speech_contract::{
    dictation::{DictationSnapshot, StartDictation},
    transcript::MAX_DICTATION_SECONDS,
};
use veoveo_types::WorkContextMembershipLevel;

pub struct Dictations {
    sessions: Mutex<HashMap<DictationSessionId, Arc<Session>>>,
    slots: Arc<Semaphore>,
    worker: Arc<WorkerProcess>,
    audit: AuditWriter,
    stop: tokio_util::sync::CancellationToken,
    workers: tokio_util::task::TaskTracker,
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
        audit: AuditContext,
        reply: oneshot::Sender<Result<()>>,
    },
    Cancel {
        audit: AuditContext,
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
        Rejection(AuditReason::PolicyDenied)
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
    pub fn new(worker: Arc<WorkerProcess>, capacity: usize, audit: AuditWriter) -> Arc<Self> {
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            slots: Arc::new(Semaphore::new(capacity)),
            worker,
            audit,
            stop: tokio_util::sync::CancellationToken::new(),
            workers: tokio_util::task::TaskTracker::new(),
        })
    }

    pub async fn start(
        self: &Arc<Self>,
        caller: GatewayInternalIdentity,
        request: StartDictation,
    ) -> Result<DictationSnapshot> {
        let context = caller.clone();
        let id = request.id;
        let result = self.start_session(caller, request).await;
        if let Err(error) = &result
            && let Some(reason) = error.downcast_ref::<Rejection>()
        {
            audit::denial(&self.audit, &context, id, reason.0).await?;
        }
        result
    }

    async fn start_session(
        self: &Arc<Self>,
        caller: GatewayInternalIdentity,
        request: StartDictation,
    ) -> Result<DictationSnapshot> {
        human(&caller)?;
        ensure!(
            (8000..=48000).contains(&request.sample_rate),
            Rejection(AuditReason::InvalidRequest)
        );
        let mut sessions = self.sessions.lock().await;
        ensure!(
            !self.stop.is_cancelled(),
            Rejection(AuditReason::Unavailable)
        );
        if let Some(session) = sessions.get(&request.id) {
            ensure!(
                same_owner(&session.owner, &caller) && session.sample_rate == request.sample_rate,
                Rejection(AuditReason::NotFound)
            );
            return Ok(session.snapshot.borrow().clone());
        }
        ensure!(sessions.len() < 64, Rejection(AuditReason::QuotaExceeded));
        ensure!(
            !sessions
                .values()
                .any(|session| same_owner(&session.owner, &caller)
                    && !session.snapshot.borrow().status.terminal()),
            Rejection(AuditReason::Conflict)
        );
        let slot = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Rejection(AuditReason::QuotaExceeded))?;
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
        let session_audit =
            audit::SessionAudit::open(self.audit.clone(), &caller, request.id, request.sample_rate)
                .await?;
        let snapshot = DictationSnapshot::new(request.id);
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
        self.workers.spawn(session::run(
            connection,
            input,
            updates,
            request.sample_rate,
            slot,
            session_audit,
            self.stop.child_token(),
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

    pub async fn shutdown(&self) -> Result<()> {
        // Admission holds this lock until its session task is registered.
        let mut sessions = self.sessions.lock().await;
        self.stop.cancel();
        sessions.clear();
        self.workers.close();
        drop(sessions);
        tokio::time::timeout(Duration::from_secs(20), self.workers.wait()).await?;
        Ok(())
    }

    async fn authorized(
        &self,
        caller: &GatewayInternalIdentity,
        id: DictationSessionId,
    ) -> Result<Arc<Session>> {
        let result = async {
            human(caller)?;
            let sessions = self.sessions.lock().await;
            let session = sessions.get(&id).ok_or(Rejection(AuditReason::NotFound))?;
            ensure!(
                same_owner(&session.owner, caller),
                Rejection(AuditReason::NotFound)
            );
            Ok(session.clone())
        }
        .await;
        if let Err(error) = &result {
            let reason = error
                .downcast_ref::<Rejection>()
                .map_or(AuditReason::PolicyDenied, |r| r.0);
            audit::denial(&self.audit, caller, id, reason).await?;
        }
        result
    }

    pub async fn read(
        &self,
        caller: &GatewayInternalIdentity,
        id: DictationSessionId,
    ) -> Result<DictationSnapshot> {
        Ok(self.authorized(caller, id).await?.snapshot.borrow().clone())
    }

    pub async fn chunk(
        &self,
        caller: &GatewayInternalIdentity,
        id: DictationSessionId,
        sequence: u32,
        bytes: Vec<u8>,
    ) -> Result<DictationSnapshot> {
        let session = self.authorized(caller, id).await?;
        let (reply, received) = oneshot::channel();
        let result = tokio::time::timeout(Duration::from_secs(5), async {
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
        .await?;
        if let Err(error) = &result
            && let Some(reason) = error.downcast_ref::<Rejection>()
        {
            audit::denial(&self.audit, caller, id, reason.0).await?;
        }
        result?;
        Ok(session.snapshot.borrow().clone())
    }

    pub async fn finish(
        &self,
        caller: &GatewayInternalIdentity,
        id: DictationSessionId,
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
                    Command::Cancel {
                        reply,
                        audit: caller.audit_context()?,
                    }
                } else {
                    Command::Finish {
                        reply,
                        audit: caller.audit_context()?,
                    }
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

#[derive(Debug)]
struct Rejection(AuditReason);
impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "dictation request rejected: {:?}", self.0)
    }
}
impl std::error::Error for Rejection {}
