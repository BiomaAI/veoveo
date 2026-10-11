//! Owned Store observation; signal maps never authorize a token.
use super::*;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use veoveo_platform_store::ChangefeedCursor;
impl LiveViewService {
    pub(super) async fn ensure_observation(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.stop.is_cancelled(), "live-view observation stopped");
        self.observation.get_or_try_init(|| async {
            // Each caller has an Arc at construction; task captures only owned projections.
            let mut changes = self.grants.store.observe_changes(vec![crate::UavObservationTable::UavLiveView], ChangefeedCursor::initial());
            tokio::time::timeout(Duration::from_secs(15), changes.next()).await?
                .ok_or_else(||anyhow::anyhow!("live-view observation ended before establishment"))??;
            self.grants.prune().await?;
            let state = self.state.clone();
            let grants = self.grants.clone();
            let issuer = self.issuer;
            let stop = self.stop.clone();
            self.workers.spawn(async move {
                loop {
                    tokio::select! {
                        _ = stop.cancelled() => break,
                        next = changes.next() => {
                            if !apply_delivery(&state, &grants, &issuer, &stop, next).await { break; }

                        }
                    }
                }
            });
            Ok::<_,anyhow::Error>(())
        }).await?;
        anyhow::ensure!(!self.stop.is_cancelled(), "live-view observation failed");
        Ok(())
    }
    pub(in crate::server) async fn closed(&self) {
        self.stop.cancelled().await;
    }
    pub(super) async fn fail_closed(&self) {
        self.stop.cancel();
        let mut local = self.state.lock().await;
        for session in local.sessions.values_mut() {
            close_session(session);
        }
    }
    pub(super) fn grant(
        &self,
        session: &ViewerSession,
        closed_at: Option<DateTime<Utc>>,
    ) -> super::super::live_view_store::Grant {
        super::super::live_view_store::Grant {
            state: super::super::live_view_store::Document(session.state.clone()),
            issuer: self.issuer.as_uuid(),
            token_hash: session.token_hash.to_vec(),
            generation: session.generation as i64,
            expires_at: session.state.expires_at,
            closed_at,
        }
    }
}

// Read and apply under the same lock used by issuance/renewal commits. The narrow
// post-read seam permits a deterministic test of that serialization order.
pub(super) async fn reconcile_current(
    state: &Mutex<LiveViewStateStore>,
    grants: &super::super::live_view_store::LiveViewStore,
    issuer: &super::super::live_view_store::LiveViewIssuerId,
    after_snapshot: impl std::future::Future<Output = ()>,
) -> anyhow::Result<()> {
    let mut local = state.lock().await;
    let rows = grants.issuer_grants(issuer).await?;
    after_snapshot.await;
    for session in local.sessions.values_mut() {
        let current = rows
            .iter()
            .find(|row| row.state.0.live_view_id == session.state.live_view_id);
        match current {
            Some(row) if active(&row.state.0) && active(&session.state) => {
                let _ = session
                    .events
                    .send_replace(signal_generation(&row.state.0, row.generation as u64));
            }
            _ => close_session(session),
        }
    }
    Ok(())
}

pub(super) async fn apply_delivery(
    state: &Mutex<LiveViewStateStore>,
    grants: &super::super::live_view_store::LiveViewStore,
    issuer: &super::super::live_view_store::LiveViewIssuerId,
    stop: &CancellationToken,
    next: Option<
        Result<veoveo_platform_store::ChangefeedDelivery, veoveo_platform_store::StoreError>,
    >,
) -> bool {
    let result = async {
        match next.ok_or_else(|| anyhow::anyhow!("live-view observation ended"))?? {
            veoveo_platform_store::ChangefeedDelivery::Reconcile { .. } => {
                anyhow::bail!("live-view observation source was reestablished");
            }
            veoveo_platform_store::ChangefeedDelivery::Changes { .. } => {}
        }
        reconcile_current(state, grants, issuer, std::future::ready(())).await
    }
    .await;
    if result.is_err() {
        stop.cancel();
        let mut local = state.lock().await;
        for session in local.sessions.values_mut() {
            close_session(session);
        }
        tracing::error!("live-view Store observation failed; admissions stopped");
        return false;
    }
    true
}
