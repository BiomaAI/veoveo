use super::*;

mod sdk;

#[tokio::test]
async fn recovery_expired_close_never_polls_late_ready_original() -> Result<()> {
    let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = polls.clone();
    let mut slot: Option<Closing> = Some(Box::pin(async move {
        observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }));
    let mut failed = false;
    let end = tokio::time::Instant::now();
    assert!(finish_original(&mut slot, end, &mut failed).await.is_err());
    assert!(finish_original(&mut slot, end, &mut failed).await.is_err());
    assert!(slot.is_some() && failed);
    assert_eq!(polls.load(std::sync::atomic::Ordering::SeqCst), 0);
    Ok(())
}
