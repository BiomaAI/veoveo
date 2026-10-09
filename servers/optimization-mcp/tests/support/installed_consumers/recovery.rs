//! One explicitly selected control/executor replacement over retained GPU products.
use super::{
    cleanup::{Cleanup, Journal},
    fixture::{Input, RecoveryInput},
    reads,
};
use anyhow::{Result, ensure};
use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use veoveo_optimization_mcp::contract::OptimizationResource;
use veoveo_testing_support::{
    installed::{
        knowledge as installed,
        restart::{DeploymentRestart, DrainGroupProgress, DrainGroupReceipt, SelectedDrainGroup},
    },
    lifecycle::owner::{self, CleanupKind},
};
use veoveo_types::{ResourceAddress, Sha256Digest};

#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Failure {
    Connection,
    BeforeReads,
    Selection,
    Restart,
    AfterReads,
    ReplacementFence,
    Deadline,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Stage {
    BeforeConnection,
    BeforeReads,
    Selecting,
    Restarting,
    AfterReads,
    ReplacementFence,
    Passed,
    Failed,
}
#[derive(Serialize)]
#[serde(
    tag = "phase",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum Record<'a> {
    CoordinatedIntent {
        schema: &'static str,
        fixture_sha256: Sha256Digest,
        visible_solves: usize,
        denied_solves: usize,
        selection: &'a RecoveryInput,
        remaining_gates: [&'static str; 5],
    },
    RestartIntent {
        admitted: DrainGroupProgress,
        maximum_attempts: u8,
    },
    RestartObserved {
        progress: DrainGroupProgress,
        receipt: &'a DrainGroupReceipt,
    },
    CoordinatedProgress {
        stage: Stage,
        progress: Option<DrainGroupProgress>,
        owner_cancelled: bool,
    },
    CoordinatedFinished {
        passed: bool,
        failure: Option<Failure>,
    },
}
struct Progress {
    current: Mutex<(Stage, Option<SelectedDrainGroup>)>,
    journal: Journal,
}
impl Progress {
    fn register(journal: Journal) -> Result<Arc<Self>> {
        let state = Arc::new(Self {
            current: Mutex::new((Stage::BeforeConnection, None)),
            journal,
        });
        let retained = Arc::clone(&state);
        // Registered before connections. The shared group registers its actual watch
        // later, so its cleanup runs before this final partial-observation flush.
        owner::register_cleanup(
            CleanupKind::Remote,
            "optimization_coordinated_replacement",
            "retained_progress_journal",
            move || async move { retained.record() },
        )?;
        Ok(state)
    }
    fn stage(&self, stage: Stage) {
        self.current.lock().expect("replacement progress").0 = stage;
    }
    fn selected(&self, selected: SelectedDrainGroup) {
        self.current.lock().expect("replacement progress").1 = Some(selected);
    }
    fn record(&self) -> Result<()> {
        let state = self.current.lock().expect("replacement progress");
        self.journal.append(&Record::CoordinatedProgress {
            stage: state.0,
            progress: state.1.as_ref().map(SelectedDrainGroup::progress),
            owner_cancelled: owner::check_effect().is_err(),
        })
    }
}

#[tokio::test]
#[ignore = "requires private real-GPU retained corpus, stable admitted primary/alternate OAuth callers, selected Optimization Pod, Kubernetes read/watch/patch authority and one coordinated control/NVIDIA executor replacement; no solves are generated"]
async fn installed_optimization_coordinated_replacement_consumers() -> Result<()> {
    owner::run(async {
        let mut input = Input::load().map_err(|_| anyhow::anyhow!("Optimization private input admission failed"))?;
        let recovery_input = input.coordinated_replacement.as_ref().ok_or_else(|| anyhow::anyhow!("declare coordinatedReplacement for the separately selected replacement case"))?;
        let recovery = recovery_input.admit()?;
        let recovery_input = recovery_input.clone();
        let (corpus, target) = input.admit().map_err(|_| anyhow::anyhow!("Optimization installed corpus admission failed"))?;
        let journal = Journal::create(&input.installation.output).map_err(|_| anyhow::anyhow!("Optimization private report admission failed"))?;
        use sha2::{Digest, Sha256};
        journal.append(&Record::CoordinatedIntent {schema:"veoveo.ai/optimization-installed-consumers/v1",
            fixture_sha256:Sha256Digest::from_bytes(Sha256::digest(serde_json::to_vec(&corpus)?).into()),
            visible_solves:corpus.visible.len(), denied_solves:corpus.denied.len(), selection:&recovery_input,
            remaining_gates:["unfinished_task_claim_recovery", "cross_replica_claim_and_subscriptions", "mid_run_label_revocation", "fresh_gpu_execution_qualification", "task_creation_cancellation_delivery_a05"]})?;
        let (cleanup, registration) = Cleanup::register(journal.clone())?;
        let progress = Progress::register(journal.clone())?;
        let result = tokio::time::timeout(Duration::from_secs(660), async {
            owner::check_effect().map_err(|_| Failure::Connection)?;
            let mut admitted = cleanup.admission().await;
            let primary = tokio::time::timeout(Duration::from_secs(30), input.installation.task_caller()).await
                .map_err(|_| Failure::Connection)?.map_err(|_| Failure::Connection)?;
            let primary_peer = primary.peer().clone();
            admitted.retain(0, async move {primary.cancel().await});
            owner::check_effect().map_err(|_| Failure::Connection)?;
            let alternate = tokio::time::timeout(Duration::from_secs(30), installed::connect(&input.installation.endpoint, &input.alternate_token_file)).await
                .map_err(|_| Failure::Connection)?.map_err(|_| Failure::Connection)?;
            let alternate_peer = alternate.peer().clone();
            admitted.retain(1, async move {alternate.cancel().await?; Ok(())});
            drop(admitted);
            progress.stage(Stage::BeforeReads);
            tokio::time::timeout(Duration::from_secs(240), reads::exercise(&primary_peer, &alternate_peer, &corpus, &journal)).await
                .map_err(|_| Failure::BeforeReads)?.map_err(|_| Failure::BeforeReads)?;
            progress.stage(Stage::Selecting);
            let restart = DeploymentRestart::new(&target, "optimization-mcp", "optimization-mcp", primary_peer.clone(), OptimizationResource::Contract.to_uri().map_err(|_| Failure::Selection)?)
                .map_err(|_| Failure::Selection)?;
            let selected = restart.select_drain_group(&recovery.pod, recovery.profiles).await.map_err(|_| Failure::Selection)?;
            progress.selected(selected.clone());
            owner::check_effect().map_err(|_| Failure::Restart)?;
            // This synchronous fsynced intent precedes the sole restart call,
            // including shared native watch acquisition and the fenced patch.
            journal.append(&Record::RestartIntent {admitted:selected.progress(), maximum_attempts:1}).map_err(|_| Failure::Restart)?;
            progress.stage(Stage::Restarting);
            let receipt = restart.restart_with_drain_group(&selected).await.map_err(|_| Failure::Restart)?;
            journal.append(&Record::RestartObserved {progress:selected.progress(), receipt:&receipt}).map_err(|_| Failure::Restart)?;
            progress.stage(Stage::AfterReads);
            tokio::time::timeout(Duration::from_secs(240), reads::exercise(&primary_peer, &alternate_peer, &corpus, &journal)).await
                .map_err(|_| Failure::AfterReads)?.map_err(|_| Failure::AfterReads)?;
            progress.stage(Stage::ReplacementFence);
            restart.verify_drain_group_replacement(&selected, &receipt).await.map_err(|_| Failure::ReplacementFence)?;
            Ok::<_, Failure>(())
        }).await;
        let failure = match result {Ok(Ok(())) => None, Ok(Err(f)) => Some(f), Err(_) => Some(Failure::Deadline)};
        progress.stage(if failure.is_none() {Stage::Passed} else {Stage::Failed});
        cleanup.finish(failure.is_none());
        let recorded = progress.record();
        let finished = journal.append(&Record::CoordinatedFinished {passed:failure.is_none(), failure});
        let close = cleanup.close().await;
        let facts = cleanup.record().await;
        recorded?; finished?; facts?; close?; registration.settled()?;
        ensure!(matches!(result, Ok(Ok(()))), "Optimization coordinated replacement consumers failed; inspect private report");
        Ok(())
    }).await.map_err(|_| anyhow::anyhow!("Optimization coordinated replacement operation failed; inspect private report and ownership cleanup"))
}
