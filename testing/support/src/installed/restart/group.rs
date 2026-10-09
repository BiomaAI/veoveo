//! One selected Pod, one watch and one fenced mutation for a container group.
use super::{
    DeploymentRestart, DrainProfile, DrainReceipt, SelectedDrainIdentity, SelectedDrainTarget,
    drain,
};
use crate::lifecycle::owner::{self, CleanupKind};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Clone)]
pub struct SelectedDrainGroup {
    members: Vec<SelectedDrainTarget>,
    images: Vec<String>,
    replica_set: String,
    replica_set_uid: Uuid,
    replica_set_version: String,
    progress: Arc<Mutex<DrainGroupProgress>>,
    attempted: Arc<std::sync::atomic::AtomicBool>,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DrainGroupState {
    Admitted,
    Dispatched,
    Draining,
    Replaced,
    Unqualified,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerExit {
    pub container: String,
    pub exit_code: i32,
    pub finished_at: DateTime<Utc>,
    pub deletion_timestamp: DateTime<Utc>,
    pub container_instance_sha256: String,
    pub restart_count: u32,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrainGroupProgress {
    pub selected: Vec<SelectedDrainIdentity>,
    pub replica_set_uid: Uuid,
    pub replica_set_resource_version: String,
    pub state: DrainGroupState,
    pub exits: Vec<ContainerExit>,
    pub watch_closed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacementContainerIdentity {
    pub container: String,
    pub container_instance_sha256: String,
    pub image_sha256: String,
    pub restart_count: u32,
    pub ready: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrainGroupReceipt {
    pub exits: Vec<DrainReceipt>,
    pub replacement_pod: String,
    pub replacement_pod_uid: Uuid,
    pub replacement_pod_resource_version: String,
    pub replacement_replica_set_uid: Uuid,
    pub replacement_replica_set_resource_version: String,
    pub replacement_generation: u64,
    pub containers: Vec<ReplacementContainerIdentity>,
}
impl SelectedDrainGroup {
    /// Partial qualified exits survive errors and cancellation; never imply group success.
    pub fn progress(&self) -> DrainGroupProgress {
        self.progress.lock().expect("group progress").clone()
    }
    pub fn pod_name(&self) -> &str {
        &self.members[0].pod
    }
    fn mark(&self, state: DrainGroupState) {
        self.progress.lock().expect("group progress").state = state;
    }
}
fn profiles(profiles: &[DrainProfile]) -> Result<()> {
    ensure!(
        (2..=8).contains(&profiles.len()),
        "coordinated drain requires two to eight selected containers"
    );
    let names: BTreeSet<_> = profiles.iter().map(|p| &p.container).collect();
    ensure!(
        names.len() == profiles.len(),
        "coordinated drain container profiles must be unique"
    );
    Ok(())
}
fn runtime_identity(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_whitespace),
        "selected runtime identity is absent or invalid"
    );
    Ok(())
}
fn controller(pod: &drain::Pod) -> Result<&drain::Owner> {
    let owners: Vec<_> = pod
        .metadata
        .owner_references
        .iter()
        .filter(|o| o.kind == "ReplicaSet" && o.controller == Some(true))
        .collect();
    ensure!(
        owners.len() == 1,
        "selected Pod requires one controlling ReplicaSet"
    );
    Ok(owners[0])
}
impl DeploymentRestart {
    pub async fn select_drain_group(
        &self,
        pod_name: &str,
        requested: Vec<DrainProfile>,
    ) -> Result<SelectedDrainGroup> {
        profiles(&requested)?;
        let (first, pod) = self
            .select_drain_snapshot(pod_name, requested[0].clone())
            .await?;
        let owner = controller(&pod)?;
        let rs = self.group_replica_set(&owner.name).await?;
        self.require_group_replica_set(&rs, owner.uid, first.deployment_uid)?;
        drain::version(&rs.metadata.resource_version)?;
        let mut members = Vec::new();
        let mut images = Vec::new();
        for profile in requested {
            let mut member = first.clone();
            member.grace = pod.admit(&self.namespace, &profile)?;
            ensure!(
                pod.spec
                    .containers
                    .iter()
                    .filter(|c| c.name == profile.container)
                    .count()
                    == 1,
                "selected container specification is ambiguous"
            );
            let status = pod.selected_status(&profile)?;
            runtime_identity(&status.container_id)?;
            runtime_identity(&status.image_id)?;
            member.container_id = status.container_id.clone();
            member.restart_count = status.restart_count;
            member.profile = profile;
            images.push(status.image_id.clone());
            members.push(member);
        }
        let progress = DrainGroupProgress {
            selected: members.iter().map(SelectedDrainTarget::identity).collect(),
            replica_set_uid: owner.uid,
            replica_set_resource_version: rs.metadata.resource_version.clone(),
            state: DrainGroupState::Admitted,
            exits: vec![],
            watch_closed: false,
        };
        Ok(SelectedDrainGroup {
            members,
            images,
            replica_set: owner.name.clone(),
            replica_set_uid: owner.uid,
            replica_set_version: rs.metadata.resource_version,
            progress: Arc::new(Mutex::new(progress)),
            attempted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }
    async fn group_replica_set(&self, name: &str) -> Result<drain::ReplicaSet> {
        drain::name(name)?;
        serde_json::from_slice(
            &self
                .command(&["get", "replicaset", name, "-o", "json"])
                .await?,
        )
        .map_err(|_| anyhow::anyhow!("invalid selected ReplicaSet response"))
    }
    fn require_group_replica_set(
        &self,
        rs: &drain::ReplicaSet,
        uid: Uuid,
        deployment: Uuid,
    ) -> Result<()> {
        drain::version(&rs.metadata.resource_version)?;
        ensure!(
            rs.metadata.uid == uid
                && !uid.is_nil()
                && rs.metadata.deletion_timestamp.is_none()
                && rs.metadata.namespace.as_deref() == Some(&self.namespace)
                && rs
                    .metadata
                    .owner_references
                    .iter()
                    .filter(|o| o.controller == Some(true))
                    .count()
                    == 1
                && rs
                    .metadata
                    .owner_references
                    .iter()
                    .any(|o| o.kind == "Deployment"
                        && o.controller == Some(true)
                        && o.uid == deployment
                        && o.name == self.deployment),
            "selected ReplicaSet owner or identity changed"
        );
        Ok(())
    }
    async fn group_namespace(&self, selected: &SelectedDrainTarget) -> Result<()> {
        let namespace: drain::Namespace = serde_json::from_slice(
            &self
                .command(&["get", "namespace", &self.namespace, "-o", "json"])
                .await?,
        )
        .map_err(|_| anyhow::anyhow!("invalid namespace fence response"))?;
        ensure!(
            namespace.metadata.uid == selected.namespace_uid
                && namespace.metadata.deletion_timestamp.is_none(),
            "selected namespace changed"
        );
        Ok(())
    }
    /// Fence later retained-state reads against this group's observed replacement.
    /// Resource versions may advance; selected process identities must agree.
    pub async fn verify_drain_group_replacement(
        &self,
        selected: &SelectedDrainGroup,
        receipt: &DrainGroupReceipt,
    ) -> Result<()> {
        let first = &selected.members[0];
        ensure!(
            first.namespace == self.namespace && first.deployment == self.deployment,
            "coordinated drain target belongs to another workload"
        );
        let generation = first
            .generation
            .checked_add(1)
            .context("Deployment generation overflow")?;
        ensure!(
            receipt.replacement_generation == generation,
            "replacement receipt generation differs"
        );
        drain::name(&receipt.replacement_pod)?;
        drain::version(&receipt.replacement_pod_resource_version)?;
        drain::version(&receipt.replacement_replica_set_resource_version)?;
        self.group_namespace(first).await?;
        require_generation(&self.deployment().await?, first, generation)?;
        self.require_public_route().await?;
        let pod: drain::Pod = serde_json::from_slice(
            &self
                .command(&["get", "pod", &receipt.replacement_pod, "-o", "json"])
                .await?,
        )
        .map_err(|_| anyhow::anyhow!("invalid replacement Pod fence response"))?;
        verify_replacement_receipt(selected, receipt, &pod)?;
        let owner = controller(&pod)?;
        let rs = self.group_replica_set(&owner.name).await?;
        self.require_group_replica_set(&rs, owner.uid, first.deployment_uid)?;
        self.group_namespace(first).await?;
        require_generation(&self.deployment().await?, first, generation)
    }
    /// Requires an active shared owner; failures retain progress and never replay the patch.
    pub async fn restart_with_drain_group(
        &self,
        selected: &SelectedDrainGroup,
    ) -> Result<DrainGroupReceipt> {
        let first = &selected.members[0];
        ensure!(
            first.namespace == self.namespace && first.deployment == self.deployment,
            "coordinated drain target belongs to another workload"
        );
        ensure!(
            !selected
                .attempted
                .swap(true, std::sync::atomic::Ordering::AcqRel),
            "coordinated restart was already attempted; reconcile without replay"
        );
        let (watch, registration) = OwnedWatch::register(selected.clone())?;
        let limit = selected
            .members
            .iter()
            .map(|s| s.profile.deadline)
            .max()
            .unwrap();
        let result = tokio::time::timeout(limit + Duration::from_secs(75), async {
            self.group_namespace(first).await?;
            let rs = self.group_replica_set(&selected.replica_set).await?;
            self.require_group_replica_set(&rs, selected.replica_set_uid, first.deployment_uid)?;
            ensure!(
                rs.metadata.resource_version == selected.replica_set_version,
                "selected ReplicaSet changed before restart"
            );
            let mut slot = watch.slot.lock().await;
            slot.watch = Some(drain::PodWatch::start(
                &self.context,
                &self.namespace,
                &first.pod,
            )?);
            drop(slot);
            let mut observation = GroupObservation::new(selected.clone());
            observation.initial(
                tokio::time::timeout(Duration::from_secs(10), watch.next())
                    .await
                    .context("coordinated Pod watch admission exceeded ten seconds")??,
            )?;
            let deadline = tokio::time::Instant::now() + limit;
            tokio::time::timeout_at(deadline, async {
                let at = chrono::SubsecRound::trunc_subsecs(Utc::now(), 0);
                observation.dispatched(at);
                self.fenced_patch(first).await?; // Exactly one UID/resourceVersion-fenced mutation.
                loop {
                    if observation.observe(watch.next().await?)? {
                        break;
                    }
                }
                Ok::<_, anyhow::Error>(())
            })
            .await
            .context("coordinated container exits exceeded drain deadline")??;
            let resource = format!("deployment/{}", self.deployment);
            self.command(&["rollout", "status", &resource, "--timeout=55s"])
                .await?;
            let after = self.deployment().await?;
            let generation = first
                .generation
                .checked_add(1)
                .context("Deployment generation overflow")?;
            require_generation(&after, first, generation)?;
            let pods: GroupPodList = serde_json::from_slice(
                &self
                    .command(&[
                        "get",
                        "pods",
                        "--selector",
                        &format!("app.kubernetes.io/component={}", self.component),
                        "-o",
                        "json",
                    ])
                    .await?,
            )
            .map_err(|_| anyhow::anyhow!("invalid replacement Pod response"))?;
            let candidates: Vec<_> = pods
                .items
                .iter()
                .filter(|p| p.metadata.deletion_timestamp.is_none())
                .collect();
            ensure!(
                candidates.len() == 1,
                "coordinated replacement requires exactly one current Pod"
            );
            let pod = candidates[0];
            let containers = replacement(selected, pod)?;
            let owner = controller(pod)?;
            let rs = self.group_replica_set(&owner.name).await?;
            self.require_group_replica_set(&rs, owner.uid, first.deployment_uid)?;
            self.group_namespace(first).await?;
            self.require_public_route().await?;
            let final_deployment = self.deployment().await?;
            require_generation(&final_deployment, first, generation)?;
            Ok(DrainGroupReceipt {
                exits: observation.receipts(generation)?,
                replacement_pod: pod.metadata.name.clone(),
                replacement_pod_uid: pod.metadata.uid,
                replacement_pod_resource_version: pod.metadata.resource_version.clone(),
                replacement_replica_set_uid: owner.uid,
                replacement_replica_set_resource_version: rs.metadata.resource_version,
                replacement_generation: generation,
                containers,
            })
        })
        .await
        .context("coordinated restart exceeded lifecycle deadline")
        .and_then(|r| r);
        let close = watch.close().await;
        if result.is_err() || close.is_err() {
            selected.mark(DrainGroupState::Unqualified);
        }
        close?;
        registration.settled()?;
        if result.is_ok() {
            selected.mark(DrainGroupState::Replaced);
        }
        result
    }
}
fn require_generation(
    deployment: &super::Deployment,
    selected: &SelectedDrainTarget,
    generation: u64,
) -> Result<()> {
    deployment.require_ready()?;
    ensure!(
        deployment.metadata.uid == selected.deployment_uid.to_string()
            && deployment.metadata.generation == generation
            && deployment.spec.replicas == 1,
        "coordinated replacement Deployment changed or is not ready"
    );
    Ok(())
}
fn verify_replacement_receipt(
    selected: &SelectedDrainGroup,
    receipt: &DrainGroupReceipt,
    pod: &drain::Pod,
) -> Result<()> {
    ensure!(
        pod.metadata.name == receipt.replacement_pod
            && pod.metadata.uid == receipt.replacement_pod_uid
            && controller(pod)?.uid == receipt.replacement_replica_set_uid,
        "current replacement Pod or ReplicaSet differs from receipt"
    );
    ensure!(
        replacement(selected, pod)? == receipt.containers,
        "current replacement container identity differs from receipt"
    );
    Ok(())
}
#[derive(serde::Deserialize)]
struct GroupPodList {
    items: Vec<drain::Pod>,
}
fn replacement(
    selected: &SelectedDrainGroup,
    pod: &drain::Pod,
) -> Result<Vec<ReplacementContainerIdentity>> {
    let first = &selected.members[0];
    ensure!(
        pod.metadata.uid != first.pod_uid && controller(pod)?.uid != selected.replica_set_uid,
        "replacement Pod or ReplicaSet identity did not change"
    );
    selected
        .members
        .iter()
        .zip(&selected.images)
        .map(|(member, image)| {
            pod.admit(&member.namespace, &member.profile)?;
            let status = pod.selected_status(&member.profile)?;
            runtime_identity(&status.container_id)?;
            runtime_identity(&status.image_id)?;
            ensure!(
                status.container_id != member.container_id && &status.image_id == image,
                "replacement container instance or admitted image differs"
            );
            Ok(ReplacementContainerIdentity {
                container: member.profile.container.clone(),
                container_instance_sha256: drain::instance_digest(&status.container_id),
                image_sha256: drain::instance_digest(&status.image_id),
                restart_count: status.restart_count,
                ready: status.ready,
            })
        })
        .collect()
}
struct GroupObservation {
    selected: SelectedDrainGroup,
    members: Vec<drain::DrainObservation>,
    dispatched: Option<DateTime<Utc>>,
}
impl GroupObservation {
    fn new(selected: SelectedDrainGroup) -> Self {
        Self {
            members: selected
                .members
                .iter()
                .cloned()
                .map(drain::DrainObservation::new)
                .collect(),
            selected,
            dispatched: None,
        }
    }
    fn initial(&self, event: drain::WatchEvent) -> Result<()> {
        let drain::WatchEvent::Added(pod) = event else {
            anyhow::bail!("coordinated watch requires initial selected Pod");
        };
        ensure!(
            controller(&pod)?.uid == self.selected.replica_set_uid
                && controller(&pod)?.name == self.selected.replica_set,
            "selected Pod ReplicaSet changed"
        );
        for (index, member) in self.members.iter().enumerate() {
            member.initial_pod(&pod)?;
            ensure!(
                pod.selected_status(&self.selected.members[index].profile)?
                    .image_id
                    == self.selected.images[index],
                "selected container image changed"
            );
        }
        Ok(())
    }
    fn dispatched(&mut self, at: DateTime<Utc>) {
        self.dispatched = Some(at);
        for member in &mut self.members {
            member.dispatched(at);
        }
        self.selected.mark(DrainGroupState::Dispatched);
    }
    fn observe(&mut self, event: drain::WatchEvent) -> Result<bool> {
        let at = self
            .dispatched
            .context("coordinated observation precedes dispatch")?;
        let deleted = matches!(&event, drain::WatchEvent::Deleted(_));
        let pod = match event {
            drain::WatchEvent::Modified(p) | drain::WatchEvent::Deleted(p) => p,
            _ => anyhow::bail!("coordinated watch gap or unexpected event"),
        };
        ensure!(
            controller(&pod)?.uid == self.selected.replica_set_uid
                && controller(&pod)?.name == self.selected.replica_set,
            "old Pod ReplicaSet changed"
        );
        let mut complete = true;
        let mut first_error = None;
        for (index, member) in self.members.iter_mut().enumerate() {
            // A received full snapshot can qualify another member even when an
            // earlier member fails. Do not discard those independent exit facts.
            let observed = (|| {
                ensure!(
                    pod.selected_status(&self.selected.members[index].profile)?
                        .image_id
                        == self.selected.images[index],
                    "selected container image changed"
                );
                member.observe_pod(&pod, deleted, at)
            })();
            match observed {
                Ok(qualified) => complete &= qualified,
                Err(error) => {
                    complete = false;
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
            if let Some((exit, deletion)) = member.qualified() {
                let selected = &self.selected.members[index];
                let fact = ContainerExit {
                    container: selected.profile.container.clone(),
                    exit_code: exit.exit_code,
                    finished_at: exit.finished_at,
                    deletion_timestamp: deletion,
                    container_instance_sha256: drain::instance_digest(&selected.container_id),
                    restart_count: selected.restart_count,
                };
                let mut progress = self.selected.progress.lock().expect("group progress");
                if let Some(previous) = progress
                    .exits
                    .iter_mut()
                    .find(|p| p.container == fact.container)
                {
                    *previous = fact;
                } else {
                    progress.exits.push(fact);
                }
            }
        }
        if let Some(error) = first_error {
            self.selected.mark(DrainGroupState::Unqualified);
            return Err(error);
        }
        self.selected.mark(DrainGroupState::Draining);
        Ok(complete)
    }
    fn receipts(self, generation: u64) -> Result<Vec<DrainReceipt>> {
        self.members
            .into_iter()
            .map(|m| m.receipt(generation))
            .collect()
    }
}

type CloseFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
struct WatchSlot {
    watch: Option<drain::PodWatch>,
    closing: Option<CloseFuture>,
    deadline: Option<Instant>,
    failed: bool,
    closed: bool,
}
struct OwnedWatch {
    slot: tokio::sync::Mutex<WatchSlot>,
    selected: SelectedDrainGroup,
}
impl OwnedWatch {
    fn register(selected: SelectedDrainGroup) -> Result<(Arc<Self>, owner::CleanupRegistration)> {
        let watch = Arc::new(Self {
            slot: tokio::sync::Mutex::new(WatchSlot {
                watch: None,
                closing: None,
                deadline: None,
                failed: false,
                closed: false,
            }),
            selected,
        });
        let retained = Arc::clone(&watch);
        let registration = owner::register_cleanup(
            CleanupKind::Remote,
            "selected_pod_group_watch",
            "coordinated_drain",
            move || async move {
                retained.selected.mark(DrainGroupState::Unqualified);
                retained.close().await
            },
        )?;
        Ok((watch, registration))
    }
    async fn next(&self) -> Result<drain::WatchEvent> {
        self.slot
            .lock()
            .await
            .watch
            .as_mut()
            .context("coordinated watch unavailable")?
            .next()
            .await
    }
    async fn close(&self) -> Result<()> {
        let end = owner::cleanup_deadline()?;
        let mut slot = self.slot.lock().await;
        ensure!(!slot.failed, "original coordinated watch cleanup failed");
        if slot.closed {
            return Ok(());
        }
        let deadline = slot.deadline.map_or(end, |d| d.min(end));
        slot.deadline = Some(deadline);
        if slot.closing.is_none()
            && let Some(watch) = slot.watch.take()
        {
            slot.closing = Some(Box::pin(async move { watch.close_until(deadline).await }));
        }
        let passed = if let Some(future) = slot.closing.as_mut() {
            matches!(
                tokio::time::timeout_at(deadline.into(), future).await,
                Ok(Ok(()))
            )
        } else {
            true
        };
        slot.closing = None;
        slot.closed = passed;
        slot.failed = !passed;
        self.selected
            .progress
            .lock()
            .expect("group progress")
            .watch_closed = passed;
        ensure!(
            passed,
            "original coordinated watch cleanup failed or exceeded owner deadline"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests;
