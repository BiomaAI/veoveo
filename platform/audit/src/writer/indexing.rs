use super::*;

/// Closed windows survive process loss. Any replica can finish them, including
/// windows left by a previous deployment. Open windows persist across shutdown.
pub(super) async fn indexing_windows(
    store: PlatformStore,
    mut stop: watch::Receiver<bool>,
) -> Result<(), AuditShutdownError> {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => return Ok(()),
            _ = interval.tick() => {}
        }
        loop {
            let result = tokio::time::timeout(
                Duration::from_secs(30),
                store.close_audit_indexing_windows(),
            )
            .await;
            match result {
                Ok(Ok(32)) if !*stop.borrow() => tokio::task::yield_now().await,
                Ok(Ok(_)) => break,
                _ => {
                    tracing::error!("indexing audit finalization failed; stopping audit admission");
                    return Err(AuditShutdownError::Worker);
                }
            }
        }
        // Receipt retention exceeds the one-hour first-commit admission bound.
        // Removing a receipt can therefore never turn an expired retry into a read.
        let cleanup_started = tokio::time::Instant::now();
        loop {
            let prune = tokio::time::timeout(
                Duration::from_secs(15),
                store.prune_audit_indexing_receipts(),
            )
            .await;
            match prune {
                Ok(Ok(1024))
                    if cleanup_started.elapsed() < Duration::from_secs(2) && !*stop.borrow() =>
                {
                    tokio::task::yield_now().await;
                }
                Ok(Ok(_)) => break,
                _ => {
                    tracing::error!(
                        "indexing audit receipt cleanup failed; stopping audit admission"
                    );
                    return Err(AuditShutdownError::Worker);
                }
            }
        }
    }
}
