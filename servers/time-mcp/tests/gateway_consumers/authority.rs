//! Isolated installed authority acquisition, guarded activation and retained epochs.
use super::*;
use anyhow::Context;
use rmcp::{
    model::{GetMeta, ServerNotification, SubscriptionFilter},
    service::Subscription,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Mutex;
use veoveo_testing_support::{SmokeMcpClient, lifecycle::owner};
use veoveo_time_mcp::{
    ActivateReleaseRequest, ActiveAuthoritySelection, AdminError, AdminErrorCode, AdminPage,
    AuthorityDatasetKind, AuthorityRelease, AuthorityReleaseId, AuthorityReleaseState,
    AuthoritySourceDigest, CreateAcquisitionRequest, CreateAcquisitionRequestValue,
    CreateSourceRequest, TimeAcquisition, TimeAcquisitionId, TimeAcquisitionPhase,
    TimeAcquisitionStatus, TimeWriteGuard, UpsertMissionEpochRequest,
    UpsertMissionEpochRequestValue,
};
use veoveo_types::ResourceAddress;
#[path = "authority/admin.rs"]
mod admin;
#[path = "authority/input.rs"]
mod input;
#[cfg(test)]
#[path = "authority/tests.rs"]
mod tests;
#[path = "authority/workflow.rs"]
mod workflow;
use input::Input;

#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Phase {
    Admitted,
    Connected,
    InitialObserved,
    Acquisition,
    Activation,
    ConcurrentActivation,
    StaleRefusal,
    EpochRebound,
    Passed,
    Failed,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    schema: &'static str,
    phase: Phase,
    isolation: input::Isolation,
    initial_authority: EffectiveTimeAuthority,
    initial_epoch: MissionEpoch,
    http: Vec<admin::Observation>,
    active_snapshots: Vec<Vec<ActiveAuthoritySelection>>,
    delivered_changes: Vec<EffectiveTimeAuthority>,
    subscription_id: Option<rmcp::model::RequestId>,
    subscription_filter: Option<SubscriptionFilter>,
    initial_subscription_authority: Option<EffectiveTimeAuthority>,
    trace: trace::Trace,
    listener_closed: bool,
    caller_closed: bool,
    failed: bool,
    failure_sha256: Option<veoveo_types::Sha256Digest>,
    remaining_gates: [&'static str; 3],
}
struct Journal {
    evidence: Evidence,
    file: fs::File,
}
impl Journal {
    fn persist(&mut self) -> Result<()> {
        serde_json::to_writer(&mut self.file, &self.evidence)?;
        self.file.write_all(b"\n")?;
        self.file.sync_all()?;
        Ok(())
    }
    fn phase(&mut self, phase: Phase) -> Result<()> {
        self.evidence.phase = phase;
        self.persist()
    }
}
#[derive(Default)]
struct Handles {
    caller: Option<SmokeMcpClient>,
    listener: Option<Subscription>,
    caller_close: super::cleanup::CloseState,
    listener_close: super::cleanup::CloseState,
}
impl Handles {
    async fn close(&mut self) -> Result<()> {
        let end = owner::cleanup_deadline()?;
        let listener = self
            .listener_close
            .close(
                &mut self.listener,
                |mut listener| async move {
                    listener
                        .cancel()
                        .await
                        .map_err(|_| anyhow::anyhow!("authority listener close failed"))
                },
                end,
            )
            .await;
        let caller = self
            .caller_close
            .close(
                &mut self.caller,
                |caller| async move {
                    caller
                        .cancel()
                        .await
                        .map_err(|_| anyhow::anyhow!("authority caller close failed"))
                },
                end,
            )
            .await;
        listener?;
        caller
    }
}
fn hash(bytes: &[u8]) -> veoveo_types::Sha256Digest {
    veoveo_types::Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
#[tokio::test]
#[ignore = "requires a dedicated isolated installation, verified OAuth reader/admin identities, admitted immutable HTTPS authorities and independent epoch expectations; writes retained isolated state"]
async fn isolated_authority_acquisition_activation_and_retained_epoch_through_gateway() -> Result<()>
{
    let result = run().await;
    ensure!(
        result.is_ok(),
        "Time isolated authority qualification failed; inspect private journal and ownership lease"
    );
    Ok(())
}
async fn run() -> Result<()> {
    let path = PathBuf::from(
        std::env::var_os("VEOVEO_TIME_AUTHORITY_INPUT")
            .context("set private authority fixture path")?,
    );
    let input: Input =
        veoveo_testing_support::final_tasks::public_caller::read_private_input(&path)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(900);
    let target = input.installation.validate()?;
    input.admit(&target)?;
    let journal = Arc::new(Mutex::new(Journal {
        file: open_receipt(&input.installation.output)?,
        evidence: Evidence {
            schema: "veoveo.ai/time-authority-acceptance/v1",
            phase: Phase::Admitted,
            isolation: input.isolation.clone(),
            initial_authority: input.initial_authority.clone(),
            initial_epoch: input.epoch.clone(),
            http: vec![],
            active_snapshots: vec![],
            delivered_changes: vec![],
            subscription_id: None,
            subscription_filter: None,
            initial_subscription_authority: None,
            trace: trace::Trace::default(),
            listener_closed: true,
            caller_closed: true,
            failed: false,
            failure_sha256: None,
            remaining_gates: [
                "isolated_state_retirement_owned_by_ops",
                "unfinished_acquisition_cancellation_or_recovery",
                "authority_restart_replica_and_coordinated_upgrade",
            ],
        },
    }));
    journal.lock().await.persist()?;
    let admin = admin::Admin::new(&input, journal.clone(), deadline)?;
    let handles = Arc::new(Mutex::new(Handles::default()));
    let result = owner::run(async {
        let captured = handles.clone();
        let retained_admin = admin.clone();
        owner::register_cleanup(
            owner::CleanupKind::Remote,
            "Time authority SDK handles",
            &uuid::Uuid::now_v7().to_string(),
            move || async move {
                let flushed = retained_admin.flush().await;
                let closed = captured.lock().await.close().await;
                flushed?;
                closed
            },
        )?;
        let mut handles = handles.lock().await;
        tokio::time::timeout_at(deadline, async {
            handles.caller = Some(input.installation.task_caller().await?);
            {
                let mut journal = journal.lock().await;
                journal.evidence.caller_closed = false;
                journal.persist()?;
            }
            journal.lock().await.phase(Phase::Connected)?;
            workflow::exercise(&input, &admin, &mut handles, &journal).await
        })
        .await
        .context("authority original operation deadline")?
    })
    .await;
    let flushed = admin.flush().await;
    let handles = handles.lock().await;
    let mut journal = journal.lock().await;
    journal.evidence.listener_closed =
        handles.listener.is_none() && handles.listener_close.closed();
    journal.evidence.caller_closed = handles.caller.is_none() && handles.caller_close.closed();
    journal.evidence.failed = result.is_err()
        || flushed.is_err()
        || !journal.evidence.listener_closed
        || !journal.evidence.caller_closed;
    if let Err(error) = &result {
        journal.evidence.trace.interrupt(error);
    }
    journal.evidence.failure_sha256 = result
        .as_ref()
        .err()
        .or(flushed.as_ref().err())
        .map(|error| hash(error.to_string().as_bytes()));
    let phase = if journal.evidence.failed {
        Phase::Failed
    } else {
        Phase::Passed
    };
    journal.phase(phase)?;
    ensure!(
        !journal.evidence.failed,
        "authority qualification or owned cleanup failed"
    );
    Ok(())
}
