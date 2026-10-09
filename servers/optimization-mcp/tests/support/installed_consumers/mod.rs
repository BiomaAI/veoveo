//! Opt-in read-only installed acceptance in the maintained reads target.
//! Completed products must already have been produced on real NVIDIA hardware.
mod artifact_refusal;
mod cleanup;
mod controls;
mod fixture;
mod reads;

use anyhow::{Result, ensure};
use cleanup::{Cleanup, Journal};
use fixture::Input;
use serde::Serialize;
use std::time::Duration;
use veoveo_testing_support::{installed::knowledge as installed, lifecycle::owner};

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Failure {
    Connection,
    ConsumerChecks,
    Deadline,
}
#[derive(Serialize)]
#[serde(
    tag = "phase",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum Record {
    Intent {
        schema: &'static str,
        visible_solves: usize,
        denied_solves: usize,
        fixture_sha256: veoveo_types::Sha256Digest,
        remaining_gates: [&'static str; 6],
    },
    Finished {
        passed: bool,
        failure: Option<Failure>,
    },
}

#[tokio::test]
#[ignore = "requires explicit private installed Optimization corpus produced on real GPUs, stable isolated caller/context and alternate denied caller; no solves or mutations are generated"]
async fn installed_optimization_read_consumers() -> Result<()> {
    owner::run(async {
        let mut input = Input::load().map_err(|_| anyhow::anyhow!("Optimization private input admission failed"))?;
        let corpus = input.admit().map_err(|_| anyhow::anyhow!("Optimization installed corpus admission failed"))?;
        let journal = Journal::create(&input.installation.output).map_err(|_| anyhow::anyhow!("Optimization private report admission failed"))?;
        use sha2::{Digest, Sha256};
        journal.append(&Record::Intent {schema:"veoveo.ai/optimization-installed-consumers/v1",
            visible_solves:corpus.visible.len(), denied_solves:corpus.denied.len(),
            fixture_sha256:veoveo_types::Sha256Digest::from_bytes(Sha256::digest(serde_json::to_vec(&corpus)?).into()),
            remaining_gates:["coordinated_control_executor_replacement_f42", "unfinished_task_claim_recovery",
                "cross_replica_claim_and_subscriptions", "mid_run_label_revocation", "fresh_gpu_readiness_qualification", "task_creation_cancellation_delivery_a05"]})?;
        let (cleanup, registration) = Cleanup::register(journal.clone())?;
        let result = tokio::time::timeout(Duration::from_secs(240), async {
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
            reads::exercise(&primary_peer, &alternate_peer, &corpus, &journal).await.map_err(|_| Failure::ConsumerChecks)
        }).await;
        let failure = match result {Ok(Ok(())) => None, Ok(Err(f)) => Some(f), Err(_) => Some(Failure::Deadline)};
        cleanup.finish(failure.is_none());
        // These retained handles and journal also belong to the outer owner callback.
        let record = journal.append(&Record::Finished {passed:failure.is_none(), failure});
        let close = cleanup.close().await;
        let facts = cleanup.record().await;
        record?; facts?; close?; registration.settled()?;
        ensure!(matches!(result, Ok(Ok(()))), "Optimization installed consumers failed; inspect private report");
        Ok(())
    }).await.map_err(|_| anyhow::anyhow!("Optimization installed read operation failed; inspect private report and ownership cleanup"))
}
