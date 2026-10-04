//! Deployed Time source checks use two explicitly selected disposable events.
use anyhow::{Context, Result, ensure};
use rmcp::{Peer, RoleClient};
use serde::Deserialize;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use veoveo_gateway_contract::GatewayToolName;
use veoveo_mcp_conformance::{knowledge_probes::*, *};
use veoveo_time_mcp::{
    CancelTemporalEventRequest, TemporalEvent, TemporalEventId, TemporalEventState,
    TimeKnowledgeCollection, TimeResource,
};
use veoveo_types::ResourceAddress;

#[path = "../../../testing/installed/knowledge.rs"]
mod installed;
#[path = "../../../testing/installed/restart.rs"]
mod restart;
#[path = "../../../testing/installed/tools.rs"]
mod tools;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    installation: installed::InstalledSource,
    events: [TemporalEventId; 2],
}

struct Events {
    caller: Peer<RoleClient>,
    ids: [TemporalEventId; 2],
    changes: AtomicUsize,
    restart: restart::DeploymentRestart,
}

impl Events {
    async fn event(&self, id: &TemporalEventId) -> Result<TemporalEvent> {
        let event: TemporalEvent =
            installed::read(&self.caller, &TimeResource::Event(id.clone()).to_uri()?).await?;
        ensure!(
            &event.event_id == id && event.name.starts_with("Source conformance "),
            "Time mutation requires a selected source-conformance fixture"
        );
        Ok(event)
    }

    async fn cancel(&self, event: TemporalEvent) -> Result<()> {
        let next = event.record_version.checked_next()?;
        let result: TemporalEvent = tools::call(
            &self.caller,
            GatewayToolName::from_parts(&"time".parse()?, &"cancel_temporal_event".parse()?)?,
            &CancelTemporalEventRequest {
                event_id: event.event_id.clone(),
                expected_record_version: event.record_version,
            },
        )
        .await?;
        ensure!(
            result.event_id == event.event_id
                && result.state == TemporalEventState::Cancelled
                && result.record_version == next,
            "event cancellation receipt disagrees"
        );
        Ok(())
    }

    async fn cleanup(&self) -> Result<()> {
        let (first, second) = tokio::join!(
            self.cleanup_event(&self.ids[0]),
            self.cleanup_event(&self.ids[1])
        );
        first?;
        second
    }

    async fn cleanup_event(&self, id: &TemporalEventId) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(30), async {
            let event = self.event(id).await?;
            if event.state != TemporalEventState::Cancelled {
                self.cancel(event).await?;
            }
            ensure!(
                self.event(id).await?.state == TemporalEventState::Cancelled,
                "fixture event is still active after cleanup"
            );
            Ok(())
        })
        .await
        .context("Time fixture cleanup exceeded thirty seconds")?
    }
}

impl KnowledgeChangeDriver for Events {
    fn mutate(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(async move {
            let index = self.changes.fetch_add(1, Ordering::SeqCst);
            let id = self
                .ids
                .get(index)
                .context("Time fixture mutation count exceeded")?;
            let event = self.event(id).await?;
            ensure!(
                event.state == TemporalEventState::Scheduled,
                "fixture event is no longer scheduled"
            );
            self.cancel(event).await
        })
    }
    fn restart(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(self.restart.restart())
    }
}

#[tokio::test]
#[ignore = "requires deployed Time with populated collections, two future fixture events, a private caller token and Kubernetes access"]
async fn events_conform_through_gateway_across_cancellation_and_restart() -> Result<()> {
    run().await
}

async fn run() -> Result<()> {
    let input: Input = installed::input()?;
    let target = input.installation.validate()?;
    ensure!(
        input.events[0] != input.events[1],
        "Time requires two distinct fixture events"
    );
    let caller = input.installation.caller().await?;
    let driver = Events {
        caller: caller.peer().clone(),
        ids: input.events,
        changes: AtomicUsize::new(0),
        restart: restart::DeploymentRestart::new(
            &target,
            &input.installation.deployment,
            "time-mcp",
            caller.peer().clone(),
            veoveo_mcp_contract::ServerResourceUris::new("time".parse()?).contract_uri(),
        )?,
    };
    for id in &driver.ids {
        ensure!(
            driver.event(id).await?.state == TemporalEventState::Scheduled,
            "Time fixture must start scheduled"
        );
    }
    let source = KnowledgeSourceTarget::new(
        input.installation.endpoint.as_str().parse()?,
        "time".parse()?,
        ["time".parse()?].into(),
        KnowledgeRoute::Gateway,
    )?;
    let probes = KnowledgeProbes {
        changes: vec![KnowledgeChangeProbe::updates(
            TimeKnowledgeCollection::Events
                .descriptor()
                .collection()
                .clone(),
            [
                TimeResource::Event(driver.ids[0].clone()).to_uri()?,
                TimeResource::Event(driver.ids[1].clone()).to_uri()?,
            ],
            &driver,
        )],
        searches: vec![],
    };
    let result =
        run_knowledge_source_conformance(&source, &input.installation.credentials()?, &probes)
            .await;
    let reported = result.and_then(|report| input.installation.report(&report));
    let cleanup = driver.cleanup().await;
    let closed = installed::close(caller).await;
    cleanup?;
    closed?;
    reported
}
