use std::{collections::BTreeMap, fmt};

use serde_json::Value;
use veoveo_mcp_contract::{LiveStreamProductLifecycle, LiveStreamProductState, LiveViewId};

use super::{LiveViewError, LiveViewService, LiveViewStateStore, active};
use crate::contract::SimulationState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProductReconciliationFailure {
    RuntimeStateUnavailable,
    CapacitySlotNotUnique,
    AssignmentIdentityMissing,
    AssignmentIdentityMismatch,
    ExactReleaseFailed,
    ExactReleaseNotObserved,
}

impl ProductReconciliationFailure {
    fn as_str(self) -> &'static str {
        match self {
            Self::RuntimeStateUnavailable => "runtime_state_unavailable",
            Self::CapacitySlotNotUnique => "capacity_slot_not_unique",
            Self::AssignmentIdentityMissing => "assignment_identity_missing",
            Self::AssignmentIdentityMismatch => "assignment_identity_mismatch",
            Self::ExactReleaseFailed => "exact_release_failed",
            Self::ExactReleaseNotObserved => "exact_release_not_observed",
        }
    }
}

impl fmt::Display for ProductReconciliationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("slot {capacity_slot}: {failure}")]
pub(in crate::server) struct ProductReconciliationError {
    capacity_slot: u16,
    expected_live_view_id: Option<LiveViewId>,
    observed_live_view_id: Option<LiveViewId>,
    failure: ProductReconciliationFailure,
}

impl ProductReconciliationError {
    fn new(
        capacity_slot: u16,
        expected_live_view_id: Option<LiveViewId>,
        observed_live_view_id: Option<LiveViewId>,
        failure: ProductReconciliationFailure,
    ) -> Self {
        tracing::error!(
            capacity_slot,
            expected_live_view_id = expected_live_view_id.as_ref().map(ToString::to_string),
            observed_live_view_id = observed_live_view_id.as_ref().map(ToString::to_string),
            failure = failure.as_str(),
            "native live-view product reconciliation failed"
        );
        Self {
            capacity_slot,
            expected_live_view_id,
            observed_live_view_id,
            failure,
        }
    }

    pub(super) fn audit_details(&self) -> BTreeMap<String, Value> {
        let mut details = BTreeMap::from([
            (
                "cleanup_capacity_slot".to_owned(),
                Value::from(self.capacity_slot),
            ),
            (
                "cleanup_failure".to_owned(),
                Value::String(self.failure.as_str().to_owned()),
            ),
        ]);
        if let Some(live_view_id) = &self.expected_live_view_id {
            details.insert(
                "cleanup_live_view_id".to_owned(),
                Value::String(live_view_id.to_string()),
            );
        }
        if let Some(live_view_id) = &self.observed_live_view_id {
            details.insert(
                "cleanup_observed_live_view_id".to_owned(),
                Value::String(live_view_id.to_string()),
            );
        }
        details
    }
}

impl LiveViewService {
    pub(in crate::server) async fn reconcile_untracked_products(
        &self,
    ) -> Result<(), LiveViewError> {
        let state = self.state.lock().await;
        let simulation = self
            .adapter
            .state()
            .await
            .map_err(|error| LiveViewError::Runtime(error.to_string()))?;
        self.reclaim_untracked_products_locked(simulation, &state)
            .await?;
        Ok(())
    }

    pub(super) async fn reclaim_untracked_products_locked(
        &self,
        simulation: SimulationState,
        state: &LiveViewStateStore,
    ) -> Result<SimulationState, LiveViewError> {
        ensure_unique_capacity_slots(&simulation)?;
        let tracked = state
            .leases
            .values()
            .filter(|lease| active(&lease.state))
            .map(|lease| (lease.state.capacity_slot, lease.state.live_view_id.clone()))
            .collect::<Vec<_>>();
        let orphans = simulation
            .stream_products
            .iter()
            .filter(|product| {
                !inactive_product(product)
                    && !tracked.iter().any(|(capacity_slot, live_view_id)| {
                        product.capacity_slot == *capacity_slot
                            && product.live_view_id.as_ref() == Some(live_view_id)
                    })
            })
            .cloned()
            .collect::<Vec<_>>();

        for product in &orphans {
            let Some(live_view_id) = product.live_view_id.as_ref() else {
                return Err(reconciliation_error(
                    product.capacity_slot,
                    None,
                    None,
                    ProductReconciliationFailure::AssignmentIdentityMissing,
                ));
            };
            self.reconcile_exact_product(product.capacity_slot, live_view_id)
                .await?;
            tracing::info!(
                capacity_slot = product.capacity_slot,
                %live_view_id,
                "reclaimed untracked native live-view product"
            );
        }

        if let Some(last) = orphans.last() {
            let refreshed = self
                .runtime_state_for_cleanup(last.capacity_slot, last.live_view_id.clone())
                .await?;
            ensure_unique_capacity_slots(&refreshed)?;
            Ok(refreshed)
        } else {
            Ok(simulation)
        }
    }

    pub(super) async fn reconcile_exact_product(
        &self,
        capacity_slot: u16,
        live_view_id: &LiveViewId,
    ) -> Result<(), LiveViewError> {
        let before = self
            .runtime_state_for_cleanup(capacity_slot, Some(live_view_id.clone()))
            .await?;
        let product = product_for_cleanup(&before, capacity_slot, Some(live_view_id.clone()))?;
        if inactive_product(product) {
            return Ok(());
        }
        if product.live_view_id.as_ref() != Some(live_view_id) {
            return Err(reconciliation_error(
                capacity_slot,
                Some(live_view_id.clone()),
                product.live_view_id.clone(),
                ProductReconciliationFailure::AssignmentIdentityMismatch,
            ));
        }

        let release_failed = self
            .adapter
            .release_live_product(capacity_slot, live_view_id)
            .await
            .is_err();
        let after = self
            .runtime_state_for_cleanup(capacity_slot, Some(live_view_id.clone()))
            .await?;
        let product = product_for_cleanup(&after, capacity_slot, Some(live_view_id.clone()))?;
        if inactive_product(product) {
            return Ok(());
        }
        if product.live_view_id.as_ref() != Some(live_view_id) {
            return Err(reconciliation_error(
                capacity_slot,
                Some(live_view_id.clone()),
                product.live_view_id.clone(),
                ProductReconciliationFailure::AssignmentIdentityMismatch,
            ));
        }
        Err(reconciliation_error(
            capacity_slot,
            Some(live_view_id.clone()),
            product.live_view_id.clone(),
            if release_failed {
                ProductReconciliationFailure::ExactReleaseFailed
            } else {
                ProductReconciliationFailure::ExactReleaseNotObserved
            },
        ))
    }

    pub(super) async fn release_product(
        &self,
        capacity_slot: u16,
        live_view_id: &LiveViewId,
    ) -> Result<(), LiveViewError> {
        self.reconcile_exact_product(capacity_slot, live_view_id)
            .await
    }

    async fn runtime_state_for_cleanup(
        &self,
        capacity_slot: u16,
        live_view_id: Option<LiveViewId>,
    ) -> Result<SimulationState, LiveViewError> {
        self.adapter.state().await.map_err(|_| {
            reconciliation_error(
                capacity_slot,
                live_view_id,
                None,
                ProductReconciliationFailure::RuntimeStateUnavailable,
            )
        })
    }
}

pub(super) fn ensure_unique_capacity_slots(
    simulation: &SimulationState,
) -> Result<(), LiveViewError> {
    let mut slots = simulation
        .stream_products
        .iter()
        .map(|product| product.capacity_slot)
        .collect::<Vec<_>>();
    slots.sort_unstable();
    if let Some(duplicate) = slots.windows(2).find(|slots| slots[0] == slots[1]) {
        return Err(reconciliation_error(
            duplicate[0],
            None,
            None,
            ProductReconciliationFailure::CapacitySlotNotUnique,
        ));
    }
    Ok(())
}

fn product_for_cleanup(
    simulation: &SimulationState,
    capacity_slot: u16,
    live_view_id: Option<LiveViewId>,
) -> Result<&LiveStreamProductState, LiveViewError> {
    let mut matches = simulation
        .stream_products
        .iter()
        .filter(|product| product.capacity_slot == capacity_slot);
    let product = matches.next();
    if product.is_none() || matches.next().is_some() {
        return Err(reconciliation_error(
            capacity_slot,
            live_view_id,
            None,
            ProductReconciliationFailure::CapacitySlotNotUnique,
        ));
    }
    Ok(product.expect("slot uniqueness was checked"))
}

pub(super) fn inactive_product(product: &LiveStreamProductState) -> bool {
    product.lifecycle == LiveStreamProductLifecycle::Inactive
        && product.camera_id.is_none()
        && product.live_view_id.is_none()
        && product.active_viewer_leases == 0
        && product.connected_viewers == 0
        && product.nvenc_sessions == 0
}

fn reconciliation_error(
    capacity_slot: u16,
    expected_live_view_id: Option<LiveViewId>,
    observed_live_view_id: Option<LiveViewId>,
    failure: ProductReconciliationFailure,
) -> LiveViewError {
    LiveViewError::Reconciliation(ProductReconciliationError::new(
        capacity_slot,
        expected_live_view_id,
        observed_live_view_id,
        failure,
    ))
}
