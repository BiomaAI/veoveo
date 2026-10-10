use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

#[tokio::test(flavor = "current_thread")]
async fn calculation_keeps_current_thread_async_service_responsive() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let stop = CancellationToken::new();
        let mut owned = Calculation::new(&stop);
        let (started, entered) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let job = owned.run(move |_| {
            started.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(CallToolResult::success(vec![]))
        });
        tokio::pin!(job);
        tokio::select! {
            _ = entered => (),
            result = &mut job => panic!("calculation ended before async service: {result:?}"),
        }
        // This async operation must run on the same single-thread runtime while
        // the actual blocking calculation remains in flight.
        tokio::spawn(async move {
            release.send(()).unwrap();
        })
        .await
        .unwrap();
        assert!(job.await.is_ok());
    })
    .await
    .expect("CPU calculation blocked async service");
}

#[tokio::test(flavor = "current_thread")]
async fn interrupted_calculation_retains_original_job_until_stop_and_drain() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let stop = CancellationToken::new();
        let mut owned = Calculation::new(&stop);
        let finished = Arc::new(AtomicBool::new(false));
        let completed = finished.clone();
        let (started, entered) = tokio::sync::oneshot::channel();
        {
            let job = owned.run(move |stop| {
                started.send(()).unwrap();
                while !stop.is_cancelled() {
                    std::hint::spin_loop();
                }
                completed.store(true, Ordering::SeqCst);
                Err(crate::engine::CalculationStopped.into())
            });
            tokio::pin!(job);
            tokio::select! {
                _ = entered => (),
                _ = &mut job => panic!("calculation ended before stop"),
            }
            assert!(!finished.load(Ordering::SeqCst));
            // Drop the interrupted async waiter, as heartbeat failure does.
        }
        assert!(owned.job.is_some());
        owned.stop_and_drain().await;
        assert!(finished.load(Ordering::SeqCst));
        assert!(owned.job.is_none());
    })
    .await
    .expect("original CPU job did not drain");
}
