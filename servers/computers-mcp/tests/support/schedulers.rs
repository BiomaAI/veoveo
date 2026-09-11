//! Owned continuous worker execution for native qualification.
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub struct Schedulers {
    shutdown: CancellationToken,
    jobs: Vec<tokio::task::JoinHandle<()>>,
}
impl Schedulers {
    pub fn maintenance(
        workers: impl IntoIterator<Item = Arc<veoveo_computers_mcp::MaintenanceWorker>>,
    ) -> Self {
        let shutdown = CancellationToken::new();
        let jobs = workers
            .into_iter()
            .map(|worker| tokio::spawn(worker.run(shutdown.clone())))
            .collect();
        Self { shutdown, jobs }
    }
    pub fn start(
        workers: impl IntoIterator<Item = Arc<veoveo_computers_mcp::CommandWorker>>,
    ) -> Self {
        let shutdown = CancellationToken::new();
        let jobs = workers
            .into_iter()
            .map(|worker| tokio::spawn(worker.run(shutdown.clone())))
            .collect();
        Self { shutdown, jobs }
    }
    pub async fn stop(mut self) {
        self.shutdown.cancel();
        for mut job in self.jobs.drain(..) {
            match tokio::time::timeout(Duration::from_secs(5), &mut job).await {
                Ok(result) => result.expect("native Computer scheduler panicked"),
                Err(_) => {
                    job.abort();
                    let _ = job.await;
                    panic!("native Computer scheduler did not stop");
                }
            }
        }
    }
}
impl Drop for Schedulers {
    fn drop(&mut self) {
        self.shutdown.cancel();
        for job in &self.jobs {
            job.abort();
        }
    }
}
