use std::{collections::BTreeMap, future::Future};
use uuid::Uuid;
use veoveo_computers::{Operation, api::Action};
use veoveo_computers_runtime::{AllocationConfig, Binding, DevelopmentTemplate, HomeAllocator};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PreflightError {
    #[error("retained storage configuration or binding is invalid")]
    Configuration,
    #[error("retained storage is unavailable")]
    Unavailable,
}

/// Production retained storage adapter. This grants no dispatch authority; the
/// domain independently checks current policy under its durable Task lease.
#[derive(Clone)]
pub struct RetainedHomes {
    provider_id: Uuid,
    allocators: BTreeMap<String, HomeAllocator>,
}
impl RetainedHomes {
    pub(crate) fn provider_id(&self) -> Uuid {
        self.provider_id
    }
    pub(crate) fn allocator(&self, fingerprint: &str) -> Result<&HomeAllocator, PreflightError> {
        self.allocators
            .get(fingerprint)
            .ok_or(PreflightError::Configuration)
    }
    /// Each admitted template must agree with the allocator's selected capacity.
    pub async fn ready(&self) -> Result<(), PreflightError> {
        use futures::{StreamExt, TryStreamExt};
        futures::stream::iter(self.allocators.values().cloned().collect::<Vec<_>>())
            .map(|allocator| async move { allocator.ready().await })
            .buffer_unordered(8)
            .try_collect::<Vec<_>>()
            .await
            .map(|_| ())
            .map_err(|_| PreflightError::Unavailable)
    }
    pub async fn new(
        provider_id: Uuid,
        config: AllocationConfig,
        templates: &[DevelopmentTemplate],
    ) -> Result<Self, PreflightError> {
        if provider_id.is_nil() || templates.is_empty() || templates.len() > 64 {
            return Err(PreflightError::Configuration);
        }
        let mut allocators = BTreeMap::new();
        for template in templates {
            let home = template
                .persistent_home()
                .ok_or(PreflightError::Configuration)?;
            let fingerprint = template.fingerprint();
            let allocator = HomeAllocator::new(
                config.clone(),
                provider_id,
                fingerprint.clone(),
                u64::from(home.capacity_mib()) * 1024 * 1024,
            )
            .await
            .map_err(|_| PreflightError::Configuration)?;
            if allocators.insert(fingerprint, allocator).is_some() {
                return Err(PreflightError::Configuration);
            }
        }
        Ok(Self {
            provider_id,
            allocators,
        })
    }
}
impl Preflight for RetainedHomes {
    async fn prepare_home(
        &self,
        operation: &Operation,
        binding: &Binding,
        template: &DevelopmentTemplate,
    ) -> Result<(), PreflightError> {
        let fingerprint = template.fingerprint();
        if operation.provider_instance_id != self.provider_id
            || operation.computer_id != binding.computer_id()
            || operation.template_fingerprint != fingerprint
            || binding.template_fingerprint() != fingerprint
        {
            return Err(PreflightError::Configuration);
        }
        let allocator = self
            .allocators
            .get(&fingerprint)
            .ok_or(PreflightError::Configuration)?;
        match operation.action {
            Action::Create => allocator.prepare(binding).await,
            Action::Start => allocator.restore(binding).await,
            Action::Stop => return Err(PreflightError::Configuration),
        }
        .map_err(|_| PreflightError::Unavailable)
    }
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
