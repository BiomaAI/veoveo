use std::{sync::Arc, time::Duration};

use chrono::{DateTime, TimeDelta, Utc};
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::SubscriptionHub;
use veoveo_platform_store::AuditOutcome;

use crate::{
    contract::{
        PoseSourceState, ReconciliationFailureCode, ReconciliationPhase, SessionLifecycle,
        SimulationViewSession,
    },
    durability::SimulationViewRepository,
    runtime::RuntimeClients,
    state::SimulationViewService,
    uris,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct ReconcilerConfig {
    pub interval: Duration,
    pub authorization_renewal_lead: Duration,
    pub retry_max: Duration,
}

pub(crate) fn spawn_reconciler(
    service: Arc<SimulationViewService>,
    runtimes: Arc<RuntimeClients>,
    repository: Arc<SimulationViewRepository>,
    subscriptions: Arc<SubscriptionHub>,
    config: ReconcilerConfig,
    cancellation: CancellationToken,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(config.interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => break,
                _ = interval.tick() => {
                    for session in service.reconciliation_sessions() {
                        let session_id = session.session_id.clone();
                        let _operation = service.operation_guard(&session_id).await;
                        let session = match service.get_session(&session.owner, &session_id) {
                            Ok(session) => session,
                            Err(error) => {
                                tracing::warn!(error = ?error, %session_id, "Simulation View session disappeared before reconciliation");
                                continue;
                            }
                        };
                        if let Err(error) = reconcile_session(
                            &service,
                            &runtimes,
                            &repository,
                            config,
                            session,
                        ).await {
                            tracing::warn!(error = ?error, %session_id, "Simulation View desired-state reconciliation failed");
                        }
                        for uri in [
                            uris::session(&session_id),
                            uris::pose_source(&session_id),
                            uris::reconciliation(&session_id),
                            uris::cameras(&session_id),
                            uris::streams(&session_id),
                        ] {
                            subscriptions.notify_resource_updated(uri).await;
                        }
                    }
                }
            }
        }
    });
}

async fn reconcile_session(
    service: &SimulationViewService,
    runtimes: &RuntimeClients,
    repository: &SimulationViewRepository,
    config: ReconcilerConfig,
    mut session: SimulationViewSession,
) -> anyhow::Result<()> {
    let now = Utc::now();
    let session_id = session.session_id.clone();
    if let Err(error) = repository
        .audit(
            &session.owner,
            session_id.as_str(),
            "reconciliation_started",
            AuditOutcome::Allowed,
            ReconciliationAudit::from_session(&session, ReconciliationPhase::Pending, None),
        )
        .await
    {
        fail(
            service,
            &session_id,
            config,
            "audit",
            ReconciliationFailureCode::AuditUnavailable,
            "reconciliation audit is unavailable",
        );
        if let Err(persist_error) = repository.persist(service, &session_id).await {
            tracing::warn!(
                error = ?persist_error,
                %session_id,
                "failed to retain blocked Simulation View reconciliation state"
            );
        }
        return Err(error);
    }

    if session.lifecycle == SessionLifecycle::Closed {
        if let Some(source) = session.pose_source.as_ref()
            && let Err(error) = runtimes.revoke_pose(&session, source).await
        {
            return failed_runtime(
                service,
                repository,
                &session,
                config,
                "pose_ingress",
                ReconciliationFailureCode::PoseIngressUnavailable,
                "closed-session pose revocation is not yet realized",
                error,
            )
            .await;
        }
        if let Err(error) = runtimes.close_session(&session_id).await {
            return failed_runtime(
                service,
                repository,
                &session,
                config,
                "renderer_session",
                ReconciliationFailureCode::RendererUnavailable,
                "closed-session cleanup failed",
                error,
            )
            .await;
        }
        return finish(service, repository, config, &session, now).await;
    }

    service.mark_reconciliation_phase(&session_id, ReconciliationPhase::RendererSession, None);
    if let Err(error) = runtimes.create_session(&session).await {
        return failed_runtime(
            service,
            repository,
            &session,
            config,
            "renderer_session",
            ReconciliationFailureCode::RendererUnavailable,
            "renderer session is unavailable",
            error,
        )
        .await;
    }

    if session.scene.is_some() {
        service.mark_reconciliation_phase(&session_id, ReconciliationPhase::Scene, None);
        if let Err(error) = runtimes.bind_scene(&session).await {
            return failed_runtime(
                service,
                repository,
                &session,
                config,
                "scene",
                ReconciliationFailureCode::SceneUnavailable,
                "governed scene is unavailable to the renderer",
                error,
            )
            .await;
        }
    }

    if let Some(mut source) = session.pose_source.clone() {
        service.mark_reconciliation_phase(
            &session_id,
            ReconciliationPhase::PoseAuthorization,
            None,
        );
        if source.revoked {
            if let Err(error) = runtimes.revoke_pose(&session, &source).await {
                return failed_runtime(
                    service,
                    repository,
                    &session,
                    config,
                    "pose_ingress",
                    ReconciliationFailureCode::PoseIngressUnavailable,
                    "pose producer revocation is not yet realized",
                    error,
                )
                .await;
            }
        } else {
            let renewal_at = authorization_renewal_at(&source, config, now);
            if renewal_at <= now {
                source = service.renew_pose_authorization(&session_id, now)?;
                repository
                    .persist(service, &session_id)
                    .await
                    .inspect_err(|_error| {
                        fail(
                            service,
                            &session_id,
                            config,
                            "store",
                            ReconciliationFailureCode::StoreUnavailable,
                            "renewed authorization could not be committed",
                        );
                    })?;
                session = service.get_session(&session.owner, &session_id)?;
                if let Err(error) = repository
                    .audit(
                        &session.owner,
                        session_id.as_str(),
                        "pose_authorization_renewed",
                        AuditOutcome::Allowed,
                        ReconciliationAudit::from_session(
                            &session,
                            ReconciliationPhase::PoseAuthorization,
                            None,
                        ),
                    )
                    .await
                {
                    fail(
                        service,
                        &session_id,
                        config,
                        "audit",
                        ReconciliationFailureCode::AuditUnavailable,
                        "pose authorization renewal audit is unavailable",
                    );
                    if let Err(persist_error) = repository.persist(service, &session_id).await {
                        tracing::warn!(
                            error = ?persist_error,
                            %session_id,
                            "failed to retain blocked Simulation View reconciliation state"
                        );
                    }
                    return Err(error);
                }
            }
            if let Err(error) = runtimes.bind_pose(&session, &source).await {
                let code = if source.expires_at <= Utc::now() {
                    ReconciliationFailureCode::PoseAuthorizationExpired
                } else {
                    ReconciliationFailureCode::PoseIngressUnavailable
                };
                return failed_runtime(
                    service,
                    repository,
                    &session,
                    config,
                    "pose_ingress",
                    code,
                    if code == ReconciliationFailureCode::PoseAuthorizationExpired {
                        "pose producer authorization expired"
                    } else {
                        "pose authorization is unavailable to the private runtimes"
                    },
                    error,
                )
                .await;
            }
            let next = authorization_renewal_at(&source, config, Utc::now());
            service.schedule_pose_renewal(&session_id, next);
        }
    }

    service.mark_reconciliation_phase(&session_id, ReconciliationPhase::Cameras, None);
    for (camera, render_slot) in service.reconciliation_cameras(&session_id) {
        if let Err(error) = runtimes.upsert_camera(&camera, render_slot).await {
            return failed_runtime(
                service,
                repository,
                &session,
                config,
                "camera",
                ReconciliationFailureCode::CameraRejected,
                "logical camera could not be realized",
                error,
            )
            .await;
        }
    }

    service.mark_reconciliation_phase(&session_id, ReconciliationPhase::Streams, None);
    for stream in service.reconciliation_streams(&session_id) {
        let render_slot = service.render_slot(&stream.camera_id)?;
        if let Err(error) = runtimes.open_stream(&stream, render_slot).await {
            return failed_runtime(
                service,
                repository,
                &session,
                config,
                "stream",
                ReconciliationFailureCode::StreamUnavailable,
                "requested stream could not be realized",
                error,
            )
            .await;
        }
    }

    finish(service, repository, config, &session, Utc::now()).await
}

fn authorization_renewal_at(
    source: &PoseSourceState,
    config: ReconcilerConfig,
    now: DateTime<Utc>,
) -> DateTime<Utc> {
    let lifetime_lead = Duration::from_secs((source.authorization_lifetime_seconds / 3).max(1));
    let lead = config.authorization_renewal_lead.min(lifetime_lead);
    let lead = TimeDelta::from_std(lead).unwrap_or_else(|_| TimeDelta::seconds(1));
    source.expires_at.checked_sub_signed(lead).unwrap_or(now)
}

async fn finish(
    service: &SimulationViewService,
    repository: &SimulationViewRepository,
    config: ReconcilerConfig,
    session: &SimulationViewSession,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    service.mark_reconciliation_healthy(&session.session_id, now);
    if let Err(error) = repository.persist(service, &session.session_id).await {
        fail(
            service,
            &session.session_id,
            config,
            "store",
            ReconciliationFailureCode::StoreUnavailable,
            "realized reconciliation state could not be committed",
        );
        return Err(error);
    }
    let current = service.get_session(&session.owner, &session.session_id)?;
    if let Err(error) = repository
        .audit(
            &session.owner,
            session.session_id.as_str(),
            "reconciliation_succeeded",
            AuditOutcome::Allowed,
            ReconciliationAudit::from_session(&current, current.reconciliation.phase, None),
        )
        .await
    {
        fail(
            service,
            &session.session_id,
            config,
            "audit",
            ReconciliationFailureCode::AuditUnavailable,
            "successful reconciliation could not be audited",
        );
        if let Err(persist_error) = repository.persist(service, &session.session_id).await {
            tracing::warn!(
                error = ?persist_error,
                session_id = %session.session_id,
                "failed to retain blocked Simulation View reconciliation state"
            );
        }
        return Err(error);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn failed_runtime(
    service: &SimulationViewService,
    repository: &SimulationViewRepository,
    session: &SimulationViewSession,
    config: ReconcilerConfig,
    dependency: &str,
    code: ReconciliationFailureCode,
    diagnostic: &str,
    error: anyhow::Error,
) -> anyhow::Result<()> {
    tracing::warn!(error = ?error, %dependency, ?code, "Simulation View runtime dependency is not reconciled");
    fail(
        service,
        &session.session_id,
        config,
        dependency,
        code,
        diagnostic,
    );
    repository.persist(service, &session.session_id).await?;
    repository
        .audit(
            &session.owner,
            session.session_id.as_str(),
            "reconciliation_failed",
            AuditOutcome::Failed,
            ReconciliationAudit::from_session(session, ReconciliationPhase::Blocked, Some(code)),
        )
        .await?;
    anyhow::bail!("Simulation View reconciliation blocked at {dependency}: {diagnostic}")
}

fn fail(
    service: &SimulationViewService,
    session_id: &veoveo_mcp_contract::LiveSessionId,
    config: ReconcilerConfig,
    dependency: &str,
    code: ReconciliationFailureCode,
    diagnostic: &str,
) {
    let retry = config.retry_max.min(config.interval.saturating_mul(2));
    let retry = TimeDelta::from_std(retry).unwrap_or_else(|_| TimeDelta::seconds(10));
    service.mark_reconciliation_failed(
        session_id,
        dependency,
        code,
        diagnostic,
        Utc::now() + retry,
    );
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReconciliationAudit {
    service_identity: &'static str,
    work_context: String,
    policy_revision: String,
    desired_revision: u64,
    realized_revision: u64,
    phase: ReconciliationPhase,
    epoch_id: String,
    producer_id: Option<String>,
    producer_spiffe_id: Option<String>,
    authorization_revision: Option<u64>,
    authorization_expires_at: Option<DateTime<Utc>>,
    scene_revision: Option<String>,
    failure_code: Option<ReconciliationFailureCode>,
}

impl ReconciliationAudit {
    fn from_session(
        session: &SimulationViewSession,
        phase: ReconciliationPhase,
        failure_code: Option<ReconciliationFailureCode>,
    ) -> Self {
        Self {
            service_identity: "simulation-view-mcp",
            work_context: session.owner.work_context.as_str().to_owned(),
            policy_revision: session.owner.policy_revision.as_str().to_owned(),
            desired_revision: session.reconciliation.desired_revision,
            realized_revision: session.reconciliation.realized_revision,
            phase,
            epoch_id: session.epoch_id.to_string(),
            producer_id: session
                .pose_source
                .as_ref()
                .map(|source| source.producer_id.to_string()),
            producer_spiffe_id: session
                .pose_source
                .as_ref()
                .map(|source| source.spiffe_id.clone()),
            authorization_revision: session
                .pose_source
                .as_ref()
                .map(|source| source.authorization_revision)
                .or_else(|| {
                    (session.reconciliation.authorization_revision > 0)
                        .then_some(session.reconciliation.authorization_revision)
                }),
            authorization_expires_at: session.pose_source.as_ref().map(|source| source.expires_at),
            scene_revision: session
                .scene
                .as_ref()
                .map(|scene| scene.digest.as_str().to_owned()),
            failure_code,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{PoseSourceState, ProducerId};

    #[test]
    fn shortened_lifetimes_use_a_fractional_renewal_lead() {
        let now = Utc::now();
        let source = PoseSourceState {
            producer_id: ProducerId::new("fixture").unwrap(),
            spiffe_id: "spiffe://example.test/fixture".to_owned(),
            authorization_revision: 1,
            authorization_lifetime_seconds: 30,
            authorized_at: now,
            expires_at: now + TimeDelta::seconds(30),
            revoked: false,
            last_sequence: None,
            last_snapshot_at: None,
            stale: true,
        };
        let renewal = authorization_renewal_at(
            &source,
            ReconcilerConfig {
                interval: Duration::from_secs(1),
                authorization_renewal_lead: Duration::from_secs(300),
                retry_max: Duration::from_secs(10),
            },
            now,
        );
        assert_eq!(renewal, now + TimeDelta::seconds(20));
    }
}
