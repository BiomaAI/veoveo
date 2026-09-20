//! Claim-fenced reconciliation; no database transaction spans a Kubernetes call.
use anyhow::{Context, Result, ensure};
use chrono::Utc;
use std::sync::Arc;
use veoveo_platform_store::{PlatformStore, agent_management::instances::*};

use crate::{
    config::Config,
    credentials,
    kubernetes::{self, GENERATION, Kubernetes, Resource, owned},
    kubernetes_types::*,
    resources,
};

#[derive(Clone)]
pub struct Manager {
    pub store: PlatformStore,
    pub kube: Kubernetes,
    pub config: Arc<Config>,
}

impl Manager {
    pub async fn process(&self, operation: ManagedAgentOperation) {
        let owner = uuid::Uuid::now_v7();
        let claim = match self
            .store
            .claim_managed_agent_operation(operation.id, owner)
            .await
        {
            Ok(Some(operation)) => operation.claim(owner).expect("claimed by this worker"),
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(%error, "managed operation claim unavailable");
                return;
            }
        };
        if let Err(error) = self.reconcile(&claim).await {
            if kubernetes::retryable(&error) {
                tracing::warn!(generation = claim.generation, %error, "managed operation will recover from inventory");
            } else {
                let message: String = error.to_string().chars().take(480).collect();
                if let Err(observation) = self
                    .store
                    .observe_managed_agent(&claim, ManagedAgentPhase::Failed, Some(message))
                    .await
                {
                    tracing::warn!(%observation, "managed failure observation lost its claim");
                }
            }
        }
        let _ = self.store.release_managed_agent_claim(&claim).await;
    }

    async fn reconcile(&self, claim: &ManagedAgentClaim) -> Result<()> {
        // Advance immediately through completed phases; yielding a pending phase
        // releases the claim for watch-driven retry and other instances.
        let mut verified_config: Option<Vec<ConfigItem>> = None;
        for _ in 0..8 {
            let snapshot = self.store.managed_agent_reconciliation(claim).await?;
            let instance = &snapshot.instance;
            ensure!(
                instance.resources.namespace == self.config.namespace,
                "instance namespace is not owned by this manager"
            );
            if instance.desired != ManagedAgentDesired::Running {
                return self.negative(claim, &snapshot).await;
            }
            let execution = self.config.execution(&snapshot);
            if !snapshot.enabled || execution.is_err() {
                self.scale_down(claim, instance).await?;
                if !self.drained(&snapshot).await? {
                    return Ok(());
                }
                anyhow::bail!(
                    "instance authority or approved configuration changed; review and retry"
                );
            }
            let (template, model) = execution?;
            let items = match &verified_config {
                Some(items) => items.clone(),
                None => {
                    let config = self
                        .kube
                        .get::<ConfigMap>(Resource::ConfigMaps, &template.workload.config_map)
                        .await?
                        .context("approved template ConfigMap is missing")?;
                    let items = resources::configuration_items(&config, template)?;
                    verified_config = Some(items.clone());
                    items
                }
            };
            match instance.observed {
                ManagedAgentPhase::Queued => {
                    self.observe(claim, ManagedAgentPhase::Credentials).await?;
                }
                ManagedAgentPhase::Credentials => {
                    credentials::ensure_credentials(&self.kube, &self.store, claim, instance)
                        .await?;
                    self.observe(claim, ManagedAgentPhase::Storage).await?;
                }
                ManagedAgentPhase::Storage => {
                    let pvc = match self
                        .kube
                        .get::<Pvc>(Resource::Claims, &instance.resources.volume_claim)
                        .await?
                    {
                        Some(pvc) => pvc,
                        None => {
                            ensure!(
                                instance.active_generation == 0,
                                "retained memory is missing; operator storage recovery is required"
                            );
                            self.store.renew_managed_agent_claim(claim).await?;
                            self.kube
                                .create::<_, Pvc>(
                                    Resource::Claims,
                                    &resources::volume_claim(instance, template),
                                )
                                .await?
                        }
                    };
                    owned(&pvc.metadata, &instance.resources.workload)?;
                    ensure!(
                        pvc.spec.storage_class_name == template.workload.storage_class
                            && pvc.spec.resources.requests.storage
                                == format!("{}Gi", instance.resources.storage_gib)
                            && pvc.spec.access_modes == ["ReadWriteOnce"],
                        "retained volume differs from the approved template"
                    );
                    ensure!(
                        pvc.status.phase != "Lost" && pvc.metadata.deletion_timestamp.is_none(),
                        "retained memory requires operator recovery"
                    );
                    // WaitForFirstConsumer classes bind only after scheduling.
                    self.observe(claim, ManagedAgentPhase::Draining).await?;
                }
                ManagedAgentPhase::Draining => {
                    self.scale_down(claim, instance).await?;
                    if !self.drained(&snapshot).await? {
                        return Ok(());
                    }
                    self.observe(claim, ManagedAgentPhase::Workload).await?;
                }
                ManagedAgentPhase::Workload => {
                    if (Utc::now() - instance.updated_at).num_seconds() > 600 {
                        self.scale_down(claim, instance).await?;
                        anyhow::bail!(
                            "kernel was not ready within 10 minutes; inspect scheduling, image availability, template and credentials, then retry"
                        );
                    }
                    let mut desired =
                        resources::deployment(&self.config, &snapshot, template, model, items)?;
                    match self
                        .kube
                        .get::<Deployment>(Resource::Deployments, &instance.resources.workload)
                        .await?
                    {
                        None => {
                            self.store.renew_managed_agent_claim(claim).await?;
                            self.kube
                                .create::<_, Deployment>(Resource::Deployments, &desired)
                                .await?;
                            return Ok(());
                        }
                        Some(existing) => {
                            owned(&existing.metadata, &instance.resources.workload)?;
                            if existing.metadata.annotations.get(GENERATION)
                                != Some(&instance.generation.to_string())
                                || existing.spec.replicas != 1
                            {
                                ensure!(
                                    existing.metadata.deletion_timestamp.is_none(),
                                    "prior workload is still being deleted"
                                );
                                desired.metadata.resource_version =
                                    existing.metadata.resource_version;
                                desired.metadata.uid = existing.metadata.uid;
                                self.store.renew_managed_agent_claim(claim).await?;
                                self.kube
                                    .replace::<_, Deployment>(
                                        Resource::Deployments,
                                        &instance.resources.workload,
                                        &desired,
                                    )
                                    .await?;
                                return Ok(());
                            }
                        }
                    }
                    if self.ready(&snapshot).await? {
                        self.observe(claim, ManagedAgentPhase::Ready).await?;
                    }
                    return Ok(());
                }
                ManagedAgentPhase::Ready
                    if self
                        .kube
                        .get::<Deployment>(Resource::Deployments, &instance.resources.workload)
                        .await?
                        .is_none()
                        && self.drained(&snapshot).await? =>
                {
                    credentials::ensure_credentials(&self.kube, &self.store, claim, instance)
                        .await?;
                    let pvc = self
                        .kube
                        .get::<Pvc>(Resource::Claims, &instance.resources.volume_claim)
                        .await?
                        .context("retained memory is missing; operator recovery is required")?;
                    owned(&pvc.metadata, &instance.resources.workload)?;
                    self.observe(claim, ManagedAgentPhase::Workload).await?;
                }
                _ => return Ok(()),
            }
        }
        Ok(())
    }

    async fn observe(&self, claim: &ManagedAgentClaim, phase: ManagedAgentPhase) -> Result<()> {
        self.store.observe_managed_agent(claim, phase, None).await?;
        Ok(())
    }

    async fn scale_down(
        &self,
        claim: &ManagedAgentClaim,
        instance: &ManagedAgentInstance,
    ) -> Result<()> {
        if let Some(mut deployment) = self
            .kube
            .get::<Deployment>(Resource::Deployments, &instance.resources.workload)
            .await?
        {
            owned(&deployment.metadata, &instance.resources.workload)?;
            owned_generation(&deployment.metadata, instance.generation)?;
            if deployment.spec.replicas > 0
                || deployment.metadata.annotations.get(GENERATION)
                    != Some(&instance.generation.to_string())
            {
                deployment.spec.replicas = 0;
                deployment
                    .metadata
                    .annotations
                    .insert(GENERATION.into(), instance.generation.to_string());
                self.store.renew_managed_agent_claim(claim).await?;
                self.kube
                    .replace::<_, Deployment>(
                        Resource::Deployments,
                        &instance.resources.workload,
                        &deployment,
                    )
                    .await?;
            }
        }
        Ok(())
    }

    async fn drained(&self, snapshot: &ManagedAgentReconciliation) -> Result<bool> {
        Ok(!snapshot.runtime.as_ref().is_some_and(|runtime| {
            runtime
                .lease_expires_at
                .is_some_and(|until| until > Utc::now())
        }) && self
            .kube
            .pods(Some(&snapshot.instance.resources.workload))
            .await?
            .items
            .is_empty())
    }

    async fn ready(&self, snapshot: &ManagedAgentReconciliation) -> Result<bool> {
        let instance = &snapshot.instance;
        let Some(runtime) = &snapshot.runtime else {
            return Ok(false);
        };
        let Some(readiness) = &runtime.managed_ready else {
            return Ok(false);
        };
        if readiness.generation != instance.generation
            || !runtime
                .lease_expires_at
                .is_some_and(|until| until > Utc::now())
        {
            return Ok(false);
        }
        let pods = self.kube.pods(Some(&instance.resources.workload)).await?;
        let ready = pods.items.iter().any(|pod| {
            pod.ready()
                && pod.metadata.uid.as_deref() == Some(readiness.pod_uid.to_string().as_str())
                && pod.metadata.annotations.get(GENERATION)
                    == Some(&instance.generation.to_string())
        });
        let pvc = self
            .kube
            .get::<Pvc>(Resource::Claims, &instance.resources.volume_claim)
            .await?
            .context("retained memory is missing")?;
        owned(&pvc.metadata, &instance.resources.workload)?;
        Ok(ready && pvc.status.phase == "Bound")
    }

    async fn negative(
        &self,
        claim: &ManagedAgentClaim,
        snapshot: &ManagedAgentReconciliation,
    ) -> Result<()> {
        let instance = &snapshot.instance;
        if instance.observed == ManagedAgentPhase::Paused {
            if !snapshot.enabled {
                self.scale_down(claim, instance).await?;
            }
            return Ok(());
        }
        let phase = match instance.observed {
            ManagedAgentPhase::Queued => Some(ManagedAgentPhase::Credentials),
            ManagedAgentPhase::Credentials => Some(ManagedAgentPhase::Storage),
            ManagedAgentPhase::Storage => Some(ManagedAgentPhase::Draining),
            _ => None,
        };
        if let Some(phase) = phase {
            self.observe(claim, phase).await?;
            return Ok(());
        }
        if instance.desired == ManagedAgentDesired::Paused {
            let lease_alive = snapshot
                .runtime
                .as_ref()
                .is_some_and(|r| r.lease_expires_at.is_some_and(|until| until > Utc::now()));
            if !snapshot.episode_running || !lease_alive {
                self.observe(claim, ManagedAgentPhase::Paused).await?;
            }
            return Ok(());
        }
        self.scale_down(claim, instance).await?;
        if !self.drained(snapshot).await? {
            return Ok(());
        }
        if let Some(deployment) = self
            .kube
            .get::<Deployment>(Resource::Deployments, &instance.resources.workload)
            .await?
        {
            owned(&deployment.metadata, &instance.resources.workload)?;
            ensure!(
                owned_generation(&deployment.metadata, instance.generation)? == instance.generation,
                "cleanup requires the current workload generation"
            );
            self.store.renew_managed_agent_claim(claim).await?;
            self.kube
                .delete(Resource::Deployments, &deployment.metadata)
                .await?;
            return Ok(());
        }
        if let Some(mut secret) = self
            .kube
            .get::<Secret>(Resource::Secrets, &instance.resources.credential_secret)
            .await?
        {
            owned(&secret.metadata, &instance.resources.workload)?;
            owned_generation(&secret.metadata, instance.generation)?;
            let public = credentials::public_key(&secret)?;
            ensure!(
                instance
                    .public_key
                    .as_ref()
                    .is_none_or(|registered| registered == &public),
                "credential differs from the registered key; operator recovery is required"
            );
            if secret.metadata.annotations.get(GENERATION) != Some(&instance.generation.to_string())
            {
                secret
                    .metadata
                    .annotations
                    .insert(GENERATION.into(), instance.generation.to_string());
                self.store.renew_managed_agent_claim(claim).await?;
                secret = self
                    .kube
                    .replace(
                        Resource::Secrets,
                        &instance.resources.credential_secret,
                        &secret,
                    )
                    .await?;
            }
            self.store.renew_managed_agent_claim(claim).await?;
            self.kube
                .delete(Resource::Secrets, &secret.metadata)
                .await?;
        }
        self.observe(claim, ManagedAgentPhase::Archived).await
    }
}

fn owned_generation(metadata: &Metadata, current: i64) -> Result<i64> {
    let generation = metadata
        .annotations
        .get(GENERATION)
        .and_then(|value| value.parse::<i64>().ok())
        .context("owned resource has no valid generation")?;
    ensure!(
        (1..=current).contains(&generation),
        "resource belongs to a newer or invalid generation"
    );
    Ok(generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_cannot_claim_a_future_or_unknown_generation() {
        let mut metadata = Metadata::default();
        assert!(owned_generation(&metadata, 2).is_err());
        metadata.annotations.insert(GENERATION.into(), "3".into());
        assert!(owned_generation(&metadata, 2).is_err());
        metadata.annotations.insert(GENERATION.into(), "1".into());
        assert_eq!(owned_generation(&metadata, 2).unwrap(), 1);
    }
}
