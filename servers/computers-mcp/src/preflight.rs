use std::{future::Future, time::Duration};
use tokio::time::Instant;
use veoveo_computers::Operation;
use veoveo_computers_runtime::{Binding, DevelopmentTemplate, MAX_AUTHORITY_STALENESS};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PreflightError {
    #[error("current action authority denies dispatch")]
    Denied,
    #[error("current authority or retained storage is unavailable")]
    Unavailable,
}

/// Internal evidence from an authoritative read, scoped to one operation. Capture
/// `checked_at` before that read, and cap lifetime by its policy/grant expiry.
pub struct DispatchPermit {
    operation: uuid::Uuid,
    deadline: Instant,
}
impl DispatchPermit {
    pub fn issue(
        operation: uuid::Uuid,
        checked_at: Instant,
        valid_for: Duration,
    ) -> Result<Self, PreflightError> {
        if operation.is_nil()
            || checked_at > Instant::now()
            || valid_for.is_zero()
            || valid_for > MAX_AUTHORITY_STALENESS
        {
            return Err(PreflightError::Unavailable);
        }
        let permit = Self {
            operation,
            deadline: checked_at + valid_for,
        };
        permit.check(operation)?;
        Ok(permit)
    }
    pub(crate) fn check(&self, operation: uuid::Uuid) -> Result<(), PreflightError> {
        if self.operation != operation || self.deadline <= Instant::now() {
            return Err(PreflightError::Unavailable);
        }
        Ok(())
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
}

/// Installation implementations must supply both boundaries. There is no default
/// allow policy or implicit empty home. Fixtures implement an explicitly local gate.
pub trait Preflight: Send + Sync + 'static {
    fn prepare_home(
        &self,
        operation: &Operation,
        binding: &Binding,
        template: &DevelopmentTemplate,
    ) -> impl Future<Output = Result<(), PreflightError>> + Send;
    fn authorize(
        &self,
        operation: &Operation,
    ) -> impl Future<Output = Result<DispatchPermit, PreflightError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permission_is_operation_specific_and_read_latency_consumes_the_bound() {
        let id = uuid::Uuid::now_v7();
        let now = Instant::now();
        let permit = DispatchPermit::issue(id, now, Duration::from_secs(30)).unwrap();
        assert!(permit.check(uuid::Uuid::now_v7()).is_err());
        assert!(
            DispatchPermit::issue(id, now - Duration::from_secs(31), Duration::from_secs(30))
                .is_err()
        );
        assert!(
            DispatchPermit::issue(id, now + Duration::from_secs(1), Duration::from_secs(30))
                .is_err()
        );
        assert!(DispatchPermit::issue(id, now, Duration::from_secs(31)).is_err());
    }
}
