use super::fixture::Fixture;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Metadata {
    pub generation: i64,
    #[serde(default)]
    annotations: Annotations,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Annotations {
    #[serde(
        rename = "veoveo.io/cancellation-phase",
        skip_serializing_if = "Option::is_none"
    )]
    phase: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Resource {
    pub metadata: Metadata,
    #[serde(default)]
    pub status: Status,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Status {
    observed_generation: Option<i64>,
    artifact: Option<Artifact>,
    last_applied_revision: Option<String>,
    #[serde(default)]
    conditions: Vec<Condition>,
    #[serde(default)]
    ready_replicas: u64,
    #[serde(default)]
    updated_replicas: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct Artifact {
    revision: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Condition {
    #[serde(rename = "type")]
    kind: String,
    status: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    message: String,
    observed_generation: Option<i64>,
}

impl Resource {
    fn condition(&self, kind: &str, status: &str) -> bool {
        self.status.conditions.iter().any(|condition| {
            condition.kind == kind
                && condition.status == status
                && condition.observed_generation == Some(self.metadata.generation)
        })
    }

    fn ready(&self) -> bool {
        self.status.observed_generation == Some(self.metadata.generation)
            && self.condition("Ready", "True")
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Snapshot {
    observed_at_unix_millis: u128,
    source: Resource,
    root: Resource,
    pub release: Resource,
    workload: Resource,
}

impl Snapshot {
    pub fn read(fixture: &Fixture<'_>) -> Result<Self> {
        Ok(Self {
            observed_at_unix_millis: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
            source: fixture.read("ocirepository", "configuration")?,
            root: fixture.read("kustomization", "configuration")?,
            release: fixture.read("helmrelease", "witness")?,
            workload: fixture.read("deployment", "witness")?,
        })
    }

    fn source_at(&self, digest: &str) -> bool {
        self.source.ready()
            && self
                .source
                .status
                .artifact
                .as_ref()
                .is_some_and(|artifact| artifact.revision.ends_with(digest))
    }

    fn workload_phase(&self, phase: &str) -> bool {
        self.workload
            .metadata
            .annotations
            .phase
            .as_ref()
            .is_some_and(|value| value == phase)
            && self.workload.status.observed_generation == Some(self.workload.metadata.generation)
    }

    pub fn ready(&self, digest: &str, phase: &str) -> bool {
        self.source_at(digest)
            && self.root.ready()
            && self.release.ready()
            && self
                .root
                .status
                .last_applied_revision
                .as_ref()
                .is_some_and(|revision| revision.ends_with(digest))
            && self.workload_phase(phase)
            && self.workload.status.ready_replicas == 1
            && self.workload.status.updated_replicas == 1
    }

    pub fn checking_broken(&self, digest: &str) -> bool {
        self.source_at(digest)
            && self.root.condition("Reconciling", "True")
            && self.root.condition("Ready", "Unknown")
            && self.root.status.conditions.iter().any(|condition| {
                condition.kind == "Reconciling" && condition.message.contains(digest)
            })
            && self.release.condition("Reconciling", "True")
            && self.release.condition("Ready", "Unknown")
            && self.release.status.conditions.iter().any(|condition| {
                condition.kind == "Reconciling" && condition.message.contains("upgrade")
            })
            && self.workload_phase("broken")
            && self.workload.status.ready_replicas == 0
            && self.workload.status.updated_replicas == 1
    }
}

pub(super) fn wait(
    fixture: &Fixture<'_>,
    timeout: Duration,
    predicate: impl Fn(&Snapshot) -> bool,
) -> Result<Snapshot> {
    wait_stable(fixture, timeout, Duration::ZERO, predicate)
}

pub(super) fn wait_stable(
    fixture: &Fixture<'_>,
    timeout: Duration,
    stable_for: Duration,
    predicate: impl Fn(&Snapshot) -> bool,
) -> Result<Snapshot> {
    let start = Instant::now();
    let mut matched_since = None;
    loop {
        let state = fixture.snapshot()?;
        if predicate(&state) {
            let matched = matched_since.get_or_insert_with(Instant::now);
            if matched.elapsed() >= stable_for {
                return Ok(state);
            }
        } else {
            matched_since = None;
        }
        if start.elapsed() >= timeout {
            bail!(
                "fixture did not reach its expected state within {} s: {}",
                timeout.as_secs(),
                serde_json::to_string(&state)?
            );
        }
        thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_requires_current_resource_and_condition_generation() {
        let mut resource: Resource = serde_json::from_str(r#"{"metadata":{"generation":2},"status":{"observedGeneration":1,"conditions":[{"type":"Ready","status":"True","observedGeneration":1}]}}"#).unwrap();
        assert!(!resource.ready());
        resource.status.observed_generation = Some(2);
        assert!(!resource.ready());
        resource.status.conditions[0].observed_generation = Some(2);
        assert!(resource.ready());
    }
}
