//! Actual SDK slots and their consuming close futures survive operation drop.
use super::*;
use receipt::Close;
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::sync::Mutex;
use veoveo_testing_support::lifecycle::owner::{self, CleanupKind};
type Closing = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
#[derive(Default)]
pub(super) struct Handles {
    pub caller: Option<rmcp::service::RunningService<RoleClient, ClientConfig>>,
    pub listener: Option<rmcp::service::Subscription>,
    caller_close: Option<Closing>,
    listener_close: Option<Closing>,
    caller_state: Option<Close>,
    listener_state: Option<Close>,
    deadline: Option<tokio::time::Instant>,
}
impl Handles {
    #[cfg(test)]
    pub fn retain_control_close(
        &mut self,
        count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        failed: bool,
    ) {
        let caller = self.caller.take().expect("control actual caller");
        self.caller_state = Some(Close::Pending);
        self.caller_close = Some(Box::pin(async move {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(2500)).await;
            caller.cancel().await?;
            ensure!(!failed, "controlled missing acknowledgement");
            Ok(())
        }));
    }
    #[cfg(test)]
    pub async fn refuses_consumed_failed_control(&mut self) -> Result<()> {
        ensure!(
            self.caller_state == Some(Close::Failed) && self.caller_close.is_none(),
            "control did not retain failed consumed close"
        );
        ensure!(
            finish(
                &mut self.caller_close,
                &mut self.caller_state,
                tokio::time::Instant::now() + Duration::from_secs(1)
            )
            .await
            .is_err(),
            "empty retry erased actual failed close"
        );
        ensure!(
            self.caller_state == Some(Close::Failed),
            "failed close state changed"
        );
        Ok(())
    }
    pub fn opened_caller(&mut self, journal: &receipt::Journal) -> Result<()> {
        self.caller_state = Some(Close::Open);
        self.record(journal)
    }
    pub fn opened_listener(&mut self, journal: &receipt::Journal) -> Result<()> {
        self.listener_state = Some(Close::Open);
        self.record(journal)
    }
    fn record(&self, journal: &receipt::Journal) -> Result<()> {
        journal.close(
            self.caller_state.unwrap_or(Close::Absent),
            self.listener_state.unwrap_or(Close::Absent),
        )
    }
    pub async fn close(&mut self, journal: &receipt::Journal) -> Result<()> {
        let end = tokio::time::Instant::from_std(owner::cleanup_deadline()?);
        let end = *self.deadline.get_or_insert(end);
        let mut intent = Ok(());
        if self.listener_close.is_none() && self.listener.is_some() {
            self.listener_state = Some(Close::Pending);
            intent = self.record(journal);
            let mut listener = self.listener.take().unwrap();
            self.listener_close = Some(Box::pin(async move {
                listener
                    .cancel()
                    .await
                    .map_err(|_| anyhow::anyhow!("Knowledge listener close failed"))
            }));
        }
        let listener = finish(&mut self.listener_close, &mut self.listener_state, end).await;
        let listener_record = self.record(journal);
        if self.caller_close.is_none() && self.caller.is_some() {
            self.caller_state = Some(Close::Pending);
            let recorded = self.record(journal);
            if intent.is_ok() {
                intent = recorded;
            }
            let caller = self.caller.take().unwrap();
            self.caller_close = Some(Box::pin(async move {
                caller
                    .cancel()
                    .await
                    .map(|_| ())
                    .map_err(|_| anyhow::anyhow!("Knowledge caller close failed"))
            }));
        }
        let caller = finish(&mut self.caller_close, &mut self.caller_state, end).await;
        let recorded = self.record(journal);
        listener?;
        caller?;
        intent?;
        listener_record?;
        recorded?;
        ensure!(
            matches!(self.caller_state, None | Some(Close::Closed)),
            "Knowledge caller close unproven"
        );
        Ok(())
    }
}
async fn finish(
    pending: &mut Option<Closing>,
    state: &mut Option<Close>,
    end: tokio::time::Instant,
) -> Result<()> {
    let Some(future) = pending.as_mut() else {
        ensure!(
            !matches!(state, Some(Close::Open | Close::Pending | Close::Failed)),
            "Knowledge prior close unproven"
        );
        return Ok(());
    };
    match tokio::time::timeout_at(end, future).await {
        Ok(result) => {
            *state = Some(if result.is_ok() {
                Close::Closed
            } else {
                Close::Failed
            });
            pending.take();
            result
        }
        Err(_) => anyhow::bail!("Knowledge close deadline; original future retained"),
    }
}
pub(super) fn register(journal: &receipt::Journal) -> Result<Arc<Mutex<Handles>>> {
    let handles = Arc::new(Mutex::new(Handles::default()));
    let retained = handles.clone();
    let journal = journal.clone();
    owner::register_cleanup(
        CleanupKind::Remote,
        "Knowledge installed SDK",
        &uuid::Uuid::now_v7().to_string(),
        move || async move {
            let mut handles = retained.lock().await;
            let before = journal.cleanup(
                handles.caller_state.unwrap_or(Close::Absent),
                handles.listener_state.unwrap_or(Close::Absent),
            );
            let result = handles.close(&journal).await;
            let recorded = journal.cleanup(
                handles.caller_state.unwrap_or(Close::Absent),
                handles.listener_state.unwrap_or(Close::Absent),
            );
            result?;
            before?;
            recorded
        },
    )?;
    Ok(handles)
}
