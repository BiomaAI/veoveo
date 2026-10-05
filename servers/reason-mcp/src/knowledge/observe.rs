//! One process observer wakes caller-admitted SQL snapshots without disclosing rows.
use crate::contract::{AnalysisId, ReasonTaskKind};
use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use veoveo_platform_store::{
    ArtifactReadScope, PlatformStore, PlatformTable, RecordId, ResourceInvalidation, task_record_id,
};
use veoveo_types::{Sha256Digest, TaskTypeDefinition};

pub struct FindingSnapshot {
    pub fingerprint: Sha256Digest,
    pub present: bool,
    pub deadline: Option<DateTime<Utc>>,
}

pub async fn snapshot(
    store: &PlatformStore,
    scope: &ArtifactReadScope,
    analysis: Option<AnalysisId>,
) -> Result<FindingSnapshot> {
    let mut response = scope
        .bind(
            store
                .client()
                .query(include_str!("../../queries/knowledge/observe.surql")),
        )
        .bind(("reason_server", RecordId::new("mcp_server", "reason")))
        .bind((
            "reason_task_type",
            ReasonTaskKind::AnalyzeRecording.name().to_string(),
        ))
        .bind(("analysis", analysis.map(|id| task_record_id(id.task_id()))))
        .await?
        .check()?;
    let (digest, present, deadline): (String, bool, Option<DateTime<Utc>>) = response
        .take::<Option<_>>(response.num_statements() - 1)?
        .ok_or_else(|| anyhow::anyhow!("finding snapshot returned no value"))?;
    Ok(FindingSnapshot {
        fingerprint: format!("sha256:{digest}").parse()?,
        present,
        deadline,
    })
}

pub struct FindingChanges {
    changes: tokio::sync::watch::Sender<Option<ResourceInvalidation>>,
    worker: tokio::task::JoinHandle<()>,
}
impl FindingChanges {
    pub fn new(store: PlatformStore) -> Self {
        let (changes, _) = tokio::sync::watch::channel(None);
        let sender = changes.clone();
        let worker = tokio::spawn(async move {
            use PlatformTable::*;
            let mut source = store.resource_changes(vec![
                Task.into(),
                ArtifactOccurrence.into(),
                ArtifactGrant.into(),
                veoveo_modules::ObservationTable::new(
                    veoveo_modules::TableName::new("reason_analysis").expect("Reason lookup table"),
                    veoveo_modules::ObservationReplay::Changefeed(
                        veoveo_modules::ChangefeedRetention::from_days(1)
                            .expect("Reason lookup retention"),
                    ),
                ),
            ]);
            while let Some(value) = source.next().await {
                sender.send_replace(Some(value));
            }
        });
        Self { changes, worker }
    }
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<Option<ResourceInvalidation>> {
        self.changes.subscribe()
    }
}
impl Drop for FindingChanges {
    fn drop(&mut self) {
        self.worker.abort();
    }
}
