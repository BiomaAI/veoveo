use std::{
    sync::{Arc, Mutex, TryLockError},
    time::{Duration, Instant},
};

use crate::{PoseBinding, PoseError, PoseLimits, PoseSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishDisposition {
    Accepted,
    DroppedStale,
    DroppedBusy,
}

#[derive(Debug)]
struct AcceptedSnapshot {
    snapshot: Arc<PoseSnapshot>,
    accepted_at: Instant,
}

#[derive(Debug)]
pub struct LatestPoseStore {
    binding: Mutex<PoseBinding>,
    latest: Mutex<Option<AcceptedSnapshot>>,
    limits: PoseLimits,
}

impl LatestPoseStore {
    pub fn new(binding: PoseBinding, limits: PoseLimits) -> Result<Self, PoseError> {
        binding.frame_revision.validate()?;
        if limits.max_entities == 0
            || limits.max_message_bytes == 0
            || limits.max_cadence_hz == 0
            || limits.stale_after.is_zero()
        {
            return Err(PoseError::SharedSlot(
                "pose limits must be positive".to_owned(),
            ));
        }
        Ok(Self {
            binding: Mutex::new(binding),
            latest: Mutex::new(None),
            limits,
        })
    }

    pub fn publish(&self, snapshot: PoseSnapshot) -> Result<PublishDisposition, PoseError> {
        snapshot.validate(&self.limits)?;
        let binding = self.binding.lock().expect("pose binding lock poisoned");
        validate_binding(&binding, &snapshot)?;
        drop(binding);

        let mut latest = match self.latest.try_lock() {
            Ok(latest) => latest,
            Err(TryLockError::WouldBlock) => return Ok(PublishDisposition::DroppedBusy),
            Err(TryLockError::Poisoned(_)) => panic!("latest pose lock poisoned"),
        };
        if let Some(previous) = latest.as_ref() {
            if snapshot.sequence <= previous.snapshot.sequence {
                return Ok(PublishDisposition::DroppedStale);
            }
            let minimum_period_ns = 1_000_000_000_i64 / i64::from(self.limits.max_cadence_hz);
            let elapsed_ns =
                snapshot.simulation_timestamp_ns - previous.snapshot.simulation_timestamp_ns;
            if elapsed_ns < minimum_period_ns {
                return Err(PoseError::CadenceExceeded);
            }
        }
        *latest = Some(AcceptedSnapshot {
            snapshot: Arc::new(snapshot),
            accepted_at: Instant::now(),
        });
        Ok(PublishDisposition::Accepted)
    }

    pub fn latest(&self) -> Option<Arc<PoseSnapshot>> {
        self.latest
            .lock()
            .expect("latest pose lock poisoned")
            .as_ref()
            .map(|accepted| accepted.snapshot.clone())
    }

    pub fn age(&self) -> Option<Duration> {
        self.latest
            .lock()
            .expect("latest pose lock poisoned")
            .as_ref()
            .map(|accepted| accepted.accepted_at.elapsed())
    }

    pub fn is_stale(&self) -> bool {
        self.age().is_none_or(|age| age > self.limits.stale_after)
    }

    pub fn reset_epoch(&self, binding: PoseBinding) -> Result<(), PoseError> {
        binding.frame_revision.validate()?;
        let mut current = self.binding.lock().expect("pose binding lock poisoned");
        if binding.session_id != current.session_id {
            return Err(PoseError::BindingMismatch {
                field: "session_id",
            });
        }
        *current = binding;
        *self.latest.lock().expect("latest pose lock poisoned") = None;
        Ok(())
    }
}

fn validate_binding(binding: &PoseBinding, snapshot: &PoseSnapshot) -> Result<(), PoseError> {
    if snapshot.session_id != binding.session_id {
        return Err(PoseError::BindingMismatch {
            field: "session_id",
        });
    }
    if snapshot.epoch_id != binding.epoch_id {
        return Err(PoseError::BindingMismatch { field: "epoch_id" });
    }
    if snapshot.frame_revision != binding.frame_revision {
        return Err(PoseError::BindingMismatch {
            field: "frame_revision",
        });
    }
    if snapshot.entity_table_revision != binding.entity_table_revision
        || snapshot.entity_table_digest != binding.entity_table_digest
    {
        return Err(PoseError::BindingMismatch {
            field: "entity_table",
        });
    }
    Ok(())
}
