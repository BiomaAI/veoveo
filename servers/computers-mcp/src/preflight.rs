use std::future::Future;
use veoveo_computers::Operation;
use veoveo_computers_runtime::{Binding, DevelopmentTemplate};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PreflightError {
    #[error("retained storage is unavailable")]
    Unavailable,
}

/// The installation supplies retained storage. Current action policy is always
/// enforced by the domain at dispatch; implementations cannot override it.
pub trait Preflight: Send + Sync + 'static {
    fn prepare_home(
        &self,
        operation: &Operation,
        binding: &Binding,
        template: &DevelopmentTemplate,
    ) -> impl Future<Output = Result<(), PreflightError>> + Send;
}
