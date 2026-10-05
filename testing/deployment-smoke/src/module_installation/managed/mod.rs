//! One real managed kernel, observed across installation credential recovery.
use veoveo_agent_runtime::persistence::AgentRepository;
mod configuration;
mod observations;
mod provision;
#[cfg(test)]
mod tests;

use super::{fixture::Fixture, process};
use anyhow::{Context, Result, ensure};
pub(super) use configuration::Configuration;
use futures::{StreamExt, stream::BoxStream};
use serde::Serialize;
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use surrealdb::{Notification, types::Uuid as LiveId};
use veoveo_agent_runtime::persistence::AgentRecord;
use veoveo_agent_runtime::persistence::instances::{
    ManagedAgentDesired, ManagedAgentLimits, ManagedAgentMutation, ManagedAgentPhase,
};
use veoveo_platform_store::PlatformStore;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Recovery {
    manager_image: String,
    kernel_image: String,
    agent_namespace: String,
    before: observations::Ready,
    after: observations::Ready,
    foreground_retirement: bool,
    old_pod_deleted: bool,
    old_lease_drained: bool,
    replay_kept_replacement: bool,
    stopped_without_episodes: bool,
    resource_creations: Vec<ResourceCreation>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceCreation {
    resource_uid: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
}
pub(super) struct Managed {
    runtime: Arc<tokio::runtime::Runtime>,
    store: PlatformStore,
    provisioned: provision::Provisioned,
    live: AgentLive,
    forward: process::Background,
    pods: observations::PodWatch,
    deployments: observations::PodWatch,
    history: Vec<AgentRecord>,
    before: Option<observations::Ready>,
    after: Option<observations::Ready>,
    recovery: Option<Recovery>,
}
// The SDK stream's Drop spawns a KILL task. This owner retains its runtime
// through stream destruction, including errors before Managed is constructed.
struct AgentLive {
    store: PlatformStore,
    stream: Option<BoxStream<'static, Result<Notification<AgentRecord>, surrealdb::Error>>>,
    id: Option<LiveId>,
    runtime: Arc<tokio::runtime::Runtime>,
}
impl AgentLive {
    fn start(runtime: Arc<tokio::runtime::Runtime>, store: PlatformStore) -> Result<Self> {
        let (id, stream) = database(&runtime, async {
            let mut response = store
                .client()
                .query(include_str!("queries/live_select_from_agent.surql"))
                .await?
                .check()?;
            let id: Option<LiveId> = response.take(0)?;
            let id = id.context("fixture LIVE identity")?;
            let stream = match response.stream::<Notification<AgentRecord>>(0) {
                Ok(stream) => stream.boxed(),
                Err(error) => {
                    let cleanup = async {
                        store
                            .client()
                            .query(include_str!("queries/kill_query.surql"))
                            .bind(("query", id))
                            .await?
                            .check()?;
                        Ok::<_, anyhow::Error>(())
                    }
                    .await;
                    return Err(setup_failure(error.into(), cleanup));
                }
            };
            Ok::<_, anyhow::Error>((id, stream))
        })?;
        Ok(Self {
            store,
            stream: Some(stream),
            id: Some(id),
            runtime,
        })
    }
    fn finish_setup<T>(&mut self, result: Result<T>) -> Result<T> {
        result.map_err(|error| setup_failure(error, self.close()))
    }
    fn close(&mut self) -> Result<()> {
        let result = if let Some(id) = self.id.take() {
            database(&self.runtime, async {
                self.store
                    .client()
                    .query(include_str!("queries/kill_query.surql"))
                    .bind(("query", id))
                    .await?
                    .check()?;
                Ok::<_, anyhow::Error>(())
            })
        } else {
            Ok(())
        };
        // Destruction must also run here when explicit KILL failed or timed out.
        // The SDK has no public close method to disable its Drop cleanup.
        let _entered = self.runtime.enter();
        drop(self.stream.take());
        result
    }
}
impl Drop for AgentLive {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
fn setup_failure(error: anyhow::Error, cleanup: Result<()>) -> anyhow::Error {
    match cleanup {
        Ok(()) => error,
        Err(cleanup) => error.context(format!("owned LIVE cleanup also failed: {cleanup:#}")),
    }
}

impl Managed {
    pub fn start(fixture: &Fixture) -> Result<Self> {
        let mut forward = process::Background::start(
            fixture.kubectl().args([
                "port-forward",
                "service/surrealdb",
                "--address=127.0.0.1",
                "0:8000",
                "--request-timeout=0",
            ]),
            1800,
            "managed Store port-forward",
        )?;
        let started = Instant::now();
        let mut output = Vec::new();
        let port = loop {
            output.extend(forward.read()?);
            if let Some(line) = std::str::from_utf8(&output)?.lines().find(|line| {
                line.starts_with("Forwarding from 127.0.0.1:") && line.ends_with(" -> 8000")
            }) {
                break line
                    .strip_prefix("Forwarding from 127.0.0.1:")
                    .unwrap()
                    .strip_suffix(" -> 8000")
                    .unwrap()
                    .parse::<u16>()?;
            }
            ensure!(
                started.elapsed() < Duration::from_secs(30),
                "owned database forward did not bind in 30 seconds"
            );
            thread::sleep(Duration::from_millis(50));
        };
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()?,
        );
        let store = database(
            &runtime,
            PlatformStore::connect(fixture.store_config(&format!("ws://127.0.0.1:{port}"))?),
        )
        .map_err(|_| anyhow::anyhow!("privileged fixture Store connection failed"))?;
        let mut live = AgentLive::start(Arc::clone(&runtime), store.clone())?;
        // From this point every error must retain the physical connection until
        // the registered LIVE ID is killed; setup failure uses the same owner.
        let prepared = (|| {
            let provisioned = database(&runtime, async {
                ensure!(
                    observations::zero_episodes(&store).await?.is_empty(),
                    "fixture had agent runtimes before launch"
                );
                provision::provision(&store, &fixture.managed_config).await
            })?;
            let resources = &provisioned.instance.resources;
            let pods =
                observations::PodWatch::start(fixture, &resources.namespace, &resources.workload)?;
            let deployments = observations::PodWatch::start_resource(
                fixture,
                &resources.namespace,
                &resources.workload,
                observations::WorkloadKind::Deployments,
            )?;
            Ok::<_, anyhow::Error>((provisioned, pods, deployments))
        })();
        let (provisioned, pods, deployments) = live.finish_setup(prepared)?;
        Ok(Self {
            runtime,
            store,
            provisioned,
            live,
            forward,
            pods,
            deployments,
            history: Vec::new(),
            before: None,
            after: None,
            recovery: None,
        })
    }
    fn observe(&mut self) -> Result<Vec<AgentRecord>> {
        self.forward.read()?;
        database(&self.runtime, async {
            // This is delivery from the owner-scoped native LIVE stream. Timeout
            // means there is no additional event now, never that a lease drained.
            while let Ok(next) = tokio::time::timeout(
                Duration::from_millis(5),
                self.live
                    .stream
                    .as_mut()
                    .context("fixture Agent LIVE already closed; drain is unknown")?
                    .next(),
            )
            .await
            {
                let record = next
                    .context("fixture Agent LIVE ended; drain is unknown")??
                    .data;
                ensure!(
                    record.agent_key == "recovery"
                        && record.next_episode_sequence == 1
                        && record.last_episode.is_none(),
                    "fixture LIVE observed an unrelated runtime or an episode"
                );
                self.history.push(record);
                ensure!(
                    self.history.len() <= 4096,
                    "fixture LIVE exceeded 4096 observations"
                );
            }
            observations::zero_episodes(&self.store).await
        })
    }
    fn await_ready(&mut self, fixture: &Fixture, initial: bool) -> Result<observations::Ready> {
        let started = Instant::now();
        let mut owner = None;
        let mut fence = None;
        let mut renewals = Vec::new();
        let mut last_expiry = None;
        loop {
            let instance = database(
                &self.runtime,
                AgentRepository::new(self.store.clone())
                    .managed_agent(&self.provisioned.authority, "recovery"),
            )?;
            let agents = self.observe()?;
            self.pods.advance()?;
            self.deployments.advance()?;
            ensure!(
                started.elapsed() < Duration::from_secs(180),
                "managed Ready observation deadline exceeded"
            );
            if let Some((agent, pod)) = select_readiness(&instance, &agents, &self.pods.pods)? {
                let lease_owner = agent
                    .lease_owner
                    .clone()
                    .context("managed Ready lease owner absent")?;
                let expiry = agent
                    .lease_expires_at
                    .context("managed Ready lease expiry absent")?;
                if owner.is_none() {
                    owner = Some(lease_owner.clone());
                    fence = Some(agent.fence);
                    last_expiry = Some(expiry);
                }
                ensure!(
                    owner.as_ref() == Some(&lease_owner) && fence == Some(agent.fence),
                    "kernel lease ownership changed while qualifying renewals"
                );
                if last_expiry.is_some_and(|previous| expiry > previous) {
                    renewals.push(expiry);
                    last_expiry = Some(expiry);
                }
                if renewals.len() >= 2 {
                    let resources = &instance.resources;
                    let deployment = observations::object(
                        fixture,
                        &resources.namespace,
                        "deployment",
                        &resources.workload,
                    )?;
                    ensure!(
                        deployment.metadata.deletion_timestamp.is_none(),
                        "ready Deployment is retiring"
                    );
                    let revision = pod.database_credential_revision()?.to_owned();
                    ensure!(
                        revision
                            == if initial {
                                "fixture-runtime-1"
                            } else {
                                "fixture-runtime-2"
                            },
                        "managed Deployment uses another credential revision"
                    );
                    let secret = observations::object(
                        fixture,
                        &resources.namespace,
                        "secret",
                        &resources.credential_secret,
                    )?;
                    let claim = observations::object(
                        fixture,
                        &resources.namespace,
                        "pvc",
                        &resources.volume_claim,
                    )?;
                    let registration = database(
                        &self.runtime,
                        AgentRepository::new(self.store.clone())
                            .managed_agent_registration(&instance.identity.client_id),
                    )?
                    .context("managed OAuth registration absent")?;
                    ensure!(
                        registration.enabled
                            && registration.instance.id == instance.id
                            && registration.instance.identity == instance.identity,
                        "managed OAuth registration differs from active instance"
                    );
                    let snapshot = observations::Ready {
                        registration: instance.id.clone(),
                        revision: registration.revision.id.clone(),
                        identity: instance.identity.clone(),
                        instance_generation: instance.generation,
                        active_generation: instance.active_generation,
                        runtime_id: agent.id.clone(),
                        deployment_uid: deployment.metadata.uid,
                        pod_uid: pod.metadata.uid,
                        signing_secret_uid: secret.metadata.uid,
                        public_jwk: instance.public_key.context("managed public JWK absent")?,
                        pvc_uid: claim.metadata.uid,
                        content_sha256: observations::witness(
                            fixture,
                            &resources.namespace,
                            &pod.metadata.name,
                            initial,
                        )?,
                        lease_owner,
                        lease_fence: agent.fence,
                        renewals,
                        database_credential_revision: revision,
                        kernel_image_id: pod
                            .status
                            .container_statuses
                            .first()
                            .context("kernel image runtime status absent")?
                            .image_id
                            .clone(),
                    };
                    ensure!(
                        !snapshot.kernel_image_id.is_empty(),
                        "kernel runtime image identity absent"
                    );
                    if initial {
                        self.before = Some(snapshot.clone());
                    } else {
                        self.after = Some(snapshot.clone());
                    }
                    return Ok(snapshot);
                }
            }
            ensure!(
                started.elapsed() < Duration::from_secs(180),
                "managed Ready and two lease renewals did not settle in 180 seconds"
            );
            thread::sleep(Duration::from_millis(100));
        }
    }
    pub fn observe_recovery(&mut self, fixture: &Fixture) -> Result<()> {
        let before = self
            .before
            .clone()
            .context("initial managed Ready missing")?;
        let started = Instant::now();
        let mut lease_drained = false;
        let mut foreground = false;
        loop {
            self.observe()?;
            let mut prior_old = None;
            for record in &self.history {
                if record.fence == before.lease_fence {
                    if record.lease_owner.is_none() {
                        lease_drained = true;
                    }
                    prior_old = Some(record);
                } else if record.fence > before.lease_fence
                    && record.lease_owner.as_ref() != Some(&before.lease_owner)
                {
                    // Both records belong to the ordered native Agent LIVE.
                    // Compare only the lease domain's own admitted time values.
                    let old = prior_old
                        .context("new fence appeared without the old lease observation")?;
                    ensure!(
                        old.lease_owner.is_none()
                            || old
                                .lease_expires_at
                                .is_some_and(|expiry| expiry <= record.updated_at),
                        "new fence preceded old lease release/expiry"
                    );
                    lease_drained = true;
                }
            }
            self.pods.advance()?;
            self.deployments.advance()?;
            foreground |= self.deployments.retired.contains(&before.deployment_uid);
            let pod_deleted = self.pods.deleted.contains(&before.pod_uid);
            if foreground
                && pod_deleted
                && lease_drained
                && self
                    .pods
                    .pods
                    .values()
                    .any(|p| p.metadata.uid != before.pod_uid && p.ready())
            {
                break;
            }
            ensure!(
                started.elapsed() < Duration::from_secs(180),
                "credential recovery did not establish foreground/Pod/lease drain in 180 seconds"
            );
            thread::sleep(Duration::from_millis(100));
        }
        let after = self.await_ready(fixture, false)?;
        ensure!(
            before.registration == after.registration
                && before.revision == after.revision
                && before.identity == after.identity
                && before.instance_generation == after.instance_generation
                && before.active_generation == after.active_generation
                && before.runtime_id == after.runtime_id,
            "credential recovery changed registration or managed generation"
        );
        ensure!(
            before.deployment_uid != after.deployment_uid
                && before.pod_uid != after.pod_uid
                && before.lease_fence < after.lease_fence
                && before.lease_owner != after.lease_owner,
            "credential recovery reused old workload or lease identity"
        );
        ensure!(
            before.signing_secret_uid == after.signing_secret_uid
                && before.public_jwk == after.public_jwk
                && before.pvc_uid == after.pvc_uid
                && before.content_sha256 == after.content_sha256,
            "credential recovery changed signing identity or retained storage"
        );
        let resource_creations = self
            .deployments
            .added
            .iter()
            .chain(self.pods.added.iter())
            .map(|metadata| ResourceCreation {
                resource_uid: metadata.uid,
                created_at: metadata.creation_timestamp,
            })
            .collect();
        self.recovery = Some(Recovery {
            manager_image: fixture.managed_config.manager_image.reference(),
            kernel_image: fixture.managed_config.template.workload.image.clone(),
            agent_namespace: fixture.managed_config.namespace.clone(),
            before,
            after,
            foreground_retirement: foreground,
            old_pod_deleted: true,
            old_lease_drained: lease_drained,
            replay_kept_replacement: false,
            stopped_without_episodes: false,
            resource_creations,
        });
        Ok(())
    }
    pub fn replay(&mut self, fixture: &Fixture) -> Result<()> {
        let before = self
            .after
            .as_ref()
            .context("replacement Ready missing")?
            .clone();
        // Observe for more than two renewal intervals: replay cannot settle merely
        // because reconciliation has not read the unchanged installation yet.
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(25) {
            let agents = self.observe()?;
            self.pods.advance()?;
            self.deployments.advance()?;
            ensure!(
                self.pods.pods.len() == 1 && self.pods.pods.contains_key(&before.pod_uid),
                "installation replay replaced the kernel Pod"
            );
            ensure!(
                agents.len() == 1
                    && agents[0].fence == before.lease_fence
                    && agents[0].lease_owner.as_ref() == Some(&before.lease_owner),
                "installation replay replaced kernel lease"
            );
            ensure!(
                observations::object(
                    fixture,
                    &fixture.managed_config.namespace,
                    "deployment",
                    &self.provisioned.instance.resources.workload
                )?
                .metadata
                .uid == before.deployment_uid,
                "installation replay replaced Deployment"
            );
            thread::sleep(Duration::from_millis(100));
        }
        self.recovery
            .as_mut()
            .context("recovery result missing")?
            .replay_kept_replacement = true;
        Ok(())
    }
    pub fn stop(&mut self) -> Result<Recovery> {
        let generation = self
            .after
            .as_ref()
            .context("replacement Ready missing")?
            .instance_generation;
        database(
            &self.runtime,
            AgentRepository::new(self.store.clone()).mutate_managed_agent(
                &self.provisioned.authority,
                "recovery",
                uuid::Uuid::now_v7(),
                Some(generation),
                ManagedAgentMutation::State {
                    desired: ManagedAgentDesired::Archived,
                },
                ManagedAgentLimits {
                    instances: 1,
                    storage_gib: 1,
                },
            ),
        )?;
        let started = Instant::now();
        loop {
            let agents = self.observe()?;
            self.pods.advance()?;
            self.deployments.advance()?;
            let instance = database(
                &self.runtime,
                AgentRepository::new(self.store.clone())
                    .managed_agent(&self.provisioned.authority, "recovery"),
            )?;
            if instance.observed == ManagedAgentPhase::Archived
                && self.pods.pods.is_empty()
                && self.deployments.pods.is_empty()
                && agents.iter().all(|a| a.lease_owner.is_none())
            {
                break;
            }
            ensure!(
                started.elapsed() < Duration::from_secs(180),
                "managed stop did not establish workload and lease drain in 180 seconds"
            );
            thread::sleep(Duration::from_millis(100));
        }
        self.final_zero_episodes()?;
        let mut result = self.recovery.take().context("recovery result missing")?;
        result.stopped_without_episodes = true;
        Ok(result)
    }
    pub fn final_zero_episodes(&mut self) -> Result<()> {
        self.observe()?;
        Ok(())
    }
    pub fn close_live(&mut self) -> Result<()> {
        self.live.close()
    }
}

// A Ready write and independent DB/watch deliveries can be observed in different
// samples. Missing healthy observations wait; contradictory observations fail.
fn select_readiness<'a>(
    instance: &veoveo_agent_runtime::persistence::instances::ManagedAgentInstance,
    agents: &'a [AgentRecord],
    pods: &'a std::collections::BTreeMap<uuid::Uuid, observations::Pod>,
) -> Result<Option<(&'a AgentRecord, &'a observations::Pod)>> {
    ensure!(
        instance.observed != ManagedAgentPhase::Failed,
        "managed kernel provisioning failed"
    );
    ensure!(agents.len() <= 1, "managed Ready has duplicate runtimes");
    ensure!(pods.len() <= 1, "managed Ready has extra owned Pods");
    if instance.observed != ManagedAgentPhase::Ready {
        return Ok(None);
    }
    let Some(agent) = agents.first() else {
        return Ok(None);
    };
    ensure!(
        agent.agent_key == instance.key
            && agent.tenant == instance.tenant
            && agent.work_context == instance.work_context,
        "managed Ready runtime identity mismatch"
    );
    let Some(ready) = &agent.managed_ready else {
        return Ok(None);
    };
    ensure!(
        ready.generation == instance.generation
            && instance.active_generation == instance.generation,
        "managed Ready generation mismatch"
    );
    let Some(pod) = pods.get(&ready.pod_uid) else {
        return Ok(None);
    };
    ensure!(
        pod.metadata.uid == ready.pod_uid,
        "managed Ready Pod identity mismatch"
    );
    Ok(pod.ready().then_some((agent, pod)))
}

pub(super) fn start(fixture: &mut Fixture) -> Result<()> {
    fixture.managed = Some(Managed::start(fixture)?);
    with_managed(fixture, |managed, fixture| {
        managed.await_ready(fixture, true).map(|_| ())
    })
}
pub(super) fn recover(fixture: &mut Fixture) -> Result<()> {
    with_managed(fixture, Managed::observe_recovery)
}
pub(super) fn replay_and_stop(fixture: &mut Fixture) -> Result<Recovery> {
    with_managed(fixture, |managed, fixture| {
        managed.replay(fixture)?;
        managed.stop()
    })
}
fn with_managed<T>(
    fixture: &mut Fixture,
    action: impl FnOnce(&mut Managed, &Fixture) -> Result<T>,
) -> Result<T> {
    let mut owned = fixture
        .managed
        .take()
        .context("managed fixture not started")?;
    let result = action(&mut owned, fixture);
    fixture.managed = Some(owned);
    result
}

// Fixture-local transport budget also covers SDK connection and reconnect waits.
fn database<T, E>(
    runtime: &tokio::runtime::Runtime,
    operation: impl std::future::Future<Output = Result<T, E>>,
) -> Result<T>
where
    E: Into<anyhow::Error>,
{
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(30), operation)
            .await
            .map_err(|_| anyhow::anyhow!("fixture database operation exceeded 30 seconds"))?
            .map_err(Into::into)
    })
}
