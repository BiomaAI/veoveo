//! Read-only physical routing fences for a completed Task handoff.
//! The operator owns every Deployment and Service mutation.
mod model;
#[cfg(test)]
mod tests;
mod watch;

use anyhow::{Result, ensure};
pub use model::{
    HandoffFixture, HandoffObservation, HandoffPhase, HandoffReceipt, HandoffResourceVersion,
};
use model::{Inventory, Object};
use std::{collections::BTreeMap, time::Duration};
use veoveo_deploy_contract::InstallationTarget;
pub use watch::Kind as HandoffObjectKind;

pub struct HandoffObserver {
    target: InstallationTarget,
    fixture: HandoffFixture,
    inventories: BTreeMap<watch::Kind, Inventory>,
    watches: Vec<watch::Watch>,
    receipt: HandoffReceipt,
    cleanup_read: Option<watch::CleanupRead>,
}
impl HandoffObserver {
    pub fn new(target: &InstallationTarget, fixture: HandoffFixture) -> Result<Self> {
        target.validate()?;
        fixture.validate(target)?;
        Ok(Self {
            target: target.clone(),
            fixture,
            inventories: BTreeMap::new(),
            watches: vec![],
            receipt: HandoffReceipt::default(),
            cleanup_read: None,
        })
    }
    pub fn receipt(&self) -> &HandoffReceipt {
        &self.receipt
    }
    pub async fn initialize(&mut self) -> Result<()> {
        watch::admit_storage_identity(&self.target, &self.fixture).await?;
        for &kind in watch::Kind::ALL {
            let inventory = watch::inventory(&self.target, &self.fixture, kind).await?;
            // Retain the actual native watch before any subsequent await.
            self.watches.push(watch::Watch::start(
                &self.target,
                &self.fixture,
                kind,
                &inventory.resource_version,
            )?);
            self.inventories.insert(kind, inventory);
        }
        self.check(HandoffPhase::OriginalOnly)?;
        self.receipt.original_admitted = true;
        Ok(())
    }
    pub fn check(&mut self, phase: HandoffPhase) -> Result<()> {
        ensure!(
            model::check(&self.fixture, &self.inventories, &mut self.receipt, phase)?,
            "handoff phase not established"
        );
        Ok(())
    }
    async fn next(&mut self) -> Result<()> {
        ensure!(self.watches.len() == 5, "handoff watches are not admitted");
        let (a, rest) = self.watches.split_at_mut(1);
        let (b, rest) = rest.split_at_mut(1);
        let (c, rest) = rest.split_at_mut(1);
        let (d, e) = rest.split_at_mut(1);
        let (kind, event) = tokio::select! {
            event = a[0].next() => (a[0].kind, event?),
            event = b[0].next() => (b[0].kind, event?),
            event = c[0].next() => (c[0].kind, event?),
            event = d[0].next() => (d[0].kind, event?),
            event = e[0].next() => (e[0].kind, event?),
        };
        let inventory = self.inventories.get_mut(&kind).expect("admitted inventory");
        match event.kind.as_str() {
            "ADDED" | "MODIFIED" => {
                let object: Object = serde_json::from_value(event.object)
                    .map_err(|_| anyhow::anyhow!("invalid handoff object"))?;
                if kind == watch::Kind::Pods {
                    model::observe_exit(&self.fixture, &object, &mut self.receipt)?;
                    model::observe_marker(&mut self.receipt, &object)?;
                }
                inventory.objects.insert(object.metadata.uid, object);
            }
            "DELETED" => {
                let object: Object = serde_json::from_value(event.object)
                    .map_err(|_| anyhow::anyhow!("invalid handoff deletion"))?;
                if kind == watch::Kind::Pods {
                    model::observe_exit(&self.fixture, &object, &mut self.receipt)?;
                }
                inventory.objects.remove(&object.metadata.uid);
            }
            "BOOKMARK" => (),
            _ => anyhow::bail!("handoff watch gap or unexpected event"),
        }
        self.receipt.watch_events += 1;
        Ok(())
    }
    pub async fn wait(
        &mut self,
        phase: HandoffPhase,
        deadline: tokio::time::Instant,
    ) -> Result<()> {
        tokio::time::timeout_at(deadline, async {
            loop {
                if model::check(&self.fixture, &self.inventories, &mut self.receipt, phase)? {
                    return Ok(());
                }
                self.next().await?;
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("handoff phase deadline"))?
    }
    /// Keep the physical fence active while the owner performs fresh SDK calls.
    pub async fn protect<T>(
        &mut self,
        phase: HandoffPhase,
        work: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        self.check(phase)?;
        if phase == HandoffPhase::ReplacementOnly {
            self.receipt.assertion_versions =
                model::versions(&self.fixture, &self.inventories, &self.receipt)?;
        }
        tokio::pin!(work);
        loop {
            tokio::select! {
                biased;
                event = self.next() => { event?; self.check(phase)?; }
                result = &mut work => {
                    let result = result?;
                    // Fresh API inventories also fence the final domain observation.
                    watch::admit_storage_identity(&self.target,&self.fixture).await?;
                    for &kind in watch::Kind::ALL {
                        let inventory = watch::inventory(&self.target, &self.fixture, kind).await?;
                        self.inventories.insert(kind, inventory);
                    }
                    self.check(phase)?;
                    return Ok(result);
                }
            }
        }
    }
    /// The operator must CAS-patch only this annotation on the selected B Pod.
    /// Consume its marker through the original Pod watch before relaxing A absence.
    pub fn request_barrier(&mut self, nonce: uuid::Uuid) -> Result<()> {
        self.check(HandoffPhase::ReplacementOnly)?;
        ensure!(
            !nonce.is_nil() && self.receipt.barrier_nonce.is_none(),
            "handoff barrier must be fresh and requested once"
        );
        self.receipt.barrier_nonce = Some(nonce);
        Ok(())
    }
    pub async fn finish_barrier(&mut self, deadline: tokio::time::Instant) -> Result<()> {
        ensure!(
            self.receipt.barrier_nonce.is_some(),
            "handoff barrier was not requested"
        );
        tokio::time::timeout_at(deadline, async {
            while !self.receipt.barrier_observed {
                self.next().await?;
                self.check(HandoffPhase::ReplacementOnly)?;
            }
            watch::admit_storage_identity(&self.target, &self.fixture).await?;
            for &kind in watch::Kind::ALL {
                self.inventories.insert(
                    kind,
                    watch::inventory(&self.target, &self.fixture, kind).await?,
                );
            }
            self.check(HandoffPhase::ReplacementOnly)?;
            self.receipt.final_versions =
                model::versions(&self.fixture, &self.inventories, &self.receipt)?;
            ensure!(
                self.receipt.final_versions == self.receipt.assertion_versions,
                "handoff routing/configuration changed during assertions"
            );
            Ok(())
        })
        .await
        .map_err(|_| anyhow::anyhow!("handoff barrier original deadline"))?
    }
    pub async fn original_route_for_cleanup(&mut self, end: std::time::Instant) -> Result<bool> {
        ensure!(
            self.cleanup_read.is_none(),
            "prior cleanup inventory remains unresolved"
        );
        for &kind in watch::Kind::ALL {
            self.cleanup_read = Some(watch::cleanup_inventory(
                &self.target,
                &self.fixture,
                kind,
                end,
            )?);
            let inventory = self.cleanup_read.as_mut().unwrap().finish().await?;
            self.cleanup_read.take();
            self.inventories.insert(kind, inventory);
        }
        model::check(
            &self.fixture,
            &self.inventories,
            &mut self.receipt,
            HandoffPhase::OriginalRouting,
        )
    }
    pub async fn close_until(&mut self, deadline: std::time::Instant) -> Result<()> {
        let mut failed = false;
        if let Some(read) = &mut self.cleanup_read {
            if read.drain(deadline).await.is_err() {
                failed = true;
            } else {
                self.cleanup_read.take();
            }
        }
        for watch in &mut self.watches {
            if watch.close_until(deadline).await.is_err() {
                failed = true;
            }
        }
        self.receipt.watches_closed = !failed;
        ensure!(!failed, "handoff native watch cleanup unresolved");
        Ok(())
    }
}
