//! Installed public authoring and one existing Manager's finite lifecycle.
use anyhow::{Context, Result, anyhow, ensure};
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::Duration,
};
use tokio::time::Instant;
use uuid::Uuid;
use veoveo_agent_runtime::contract::authoring as wire;
use veoveo_deploy_contract::InstallationTarget;
use veoveo_testing_support::{
    final_tasks::public_caller::{PrivateCallerJournal, read_private_input},
    lifecycle::owner::{self, CleanupKind},
};
use veoveo_types::{ResourceUri, Sha256Digest, WorkContextId};
#[path = "agent_manager/input.rs"]
mod input;
#[path = "agent_manager/journey.rs"]
mod journey;
fn digest(bytes: &[u8]) -> Sha256Digest {
    use sha2::{Digest, Sha256};
    Sha256Digest::from_hex(hex::encode(Sha256::digest(bytes))).expect("SHA256")
}
pub(super) async fn run(installation: &Path, fixture: &Path, output: &Path) -> Result<()> {
    let input: input::Input = read_private_input(fixture)?;
    let target = InstallationTarget::load(installation)
        .map_err(|_| anyhow!("installation target admission failed"))?;
    input.validate(&target)?;
    let journal = Arc::new(PrivateCallerJournal::create(output)?);
    journal.append(&journey::Record::Prepared {
        schema: "veoveo.ai/agent-manager-journey/v1",
        input: &input,
    })?;
    let deadline = Instant::now() + Duration::from_secs(input.timeout_seconds);
    let operation_end = deadline - Duration::from_secs(30);
    let header =
        veoveo_testing_support::installed::knowledge::bearer_header(&input.caller_token_file)
            .map_err(|_| anyhow!("private admin bearer admission failed"))?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .default_headers(reqwest::header::HeaderMap::from_iter([(
            reqwest::header::AUTHORIZATION,
            header,
        )]))
        .build()
        .map_err(|_| anyhow!("public HTTP client initialization failed"))?;
    let mut base = target.public_base_url.clone();
    base.path_segments_mut()
        .map_err(|_| anyhow!("invalid public origin"))?
        .clear()
        .extend(["admin", "admin", ""]);
    let state = Arc::new(tokio::sync::Mutex::new(journey::Journey::new(
        input,
        client,
        base,
        journal.clone(),
        deadline,
    )));
    let retained = state.clone();
    let registration = owner::register_cleanup(
        CleanupKind::Remote,
        "agent-manager-journey",
        &output.to_string_lossy(),
        move || async move { retained.lock().await.finish().await },
    )?;
    let result = tokio::time::timeout_at(operation_end, async {
        let mut state = state.lock().await;
        state.input.admit(&target).await?;
        state.execute().await?;
        state.input.admit(&target).await
    })
    .await
    .map_err(|_| {
        anyhow!("Agent Manager operation deadline; retained identities require reconciliation")
    })
    .and_then(|r| r);
    let cleanup = state.lock().await.finish().await;
    if cleanup.is_ok() {
        registration.settled()?;
    }
    journal.append(&journey::Record::Outcome {
        operation_passed: result.is_ok(),
        cleanup_passed: cleanup.is_ok(),
        physical_drain_proven: false,
        retained_storage: true,
    })?;
    result?;
    cleanup?;
    Ok(())
}
