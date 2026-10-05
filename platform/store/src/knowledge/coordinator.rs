use super::*;
use crate::PlatformStore;

/// One process identity. It cannot be substituted for an index generation ID.
#[derive(Clone, Copy, Debug)]
pub struct CoordinatorId(uuid::Uuid);
impl Default for CoordinatorId {
    fn default() -> Self {
        Self::new()
    }
}
impl CoordinatorId {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
}

/// Store-issued write authority. Every index mutation checks its current epoch.
#[derive(Clone, Debug)]
pub struct CoordinatorLease {
    tenant: TenantId,
    owner: CoordinatorId,
    epoch: i64,
    mutations: std::sync::Arc<tokio::sync::Mutex<()>>,
}
impl CoordinatorLease {
    // Serialize this worker's renewal and index transactions. Store epochs still
    // enforce ownership across independent processes and connections.
    pub(super) async fn mutation(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.mutations.lock().await
    }
    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }
    pub(super) fn bind<'a, C: surrealdb::Connection>(
        &self,
        query: surrealdb::method::Query<'a, C>,
    ) -> surrealdb::method::Query<'a, C> {
        query
            .bind((
                "coordinator",
                RecordId::new("knowledge_coordinator", self.tenant.as_str()),
            ))
            .bind(("owner", surrealdb::types::Uuid::from(self.owner.0)))
            .bind(("owner_epoch", self.epoch))
    }
    pub(super) fn check_tenant(&self, tenant: &TenantId) -> Result<(), StoreError> {
        if tenant == &self.tenant {
            Ok(())
        } else {
            Err(StoreError::Knowledge(
                "coordinator belongs to another tenant",
            ))
        }
    }
}
pub(super) fn sync_record(generation: GenerationId, collection: &CollectionId) -> RecordId {
    RecordId::new(
        "knowledge_sync",
        Array::from(vec![generation.to_string(), collection.to_string()]),
    )
}
impl PlatformStore {
    pub async fn claim_knowledge_coordinator(
        &self,
        tenant: &TenantId,
        owner: CoordinatorId,
    ) -> Result<Option<CoordinatorLease>, StoreError> {
        let candidate = CoordinatorLease {
            tenant: tenant.clone(),
            owner,
            epoch: 0,
            mutations: Default::default(),
        };
        let mut response = candidate
            .bind(
                self.client()
                    .query(include_str!("../queries/knowledge/claim.surql")),
            )
            .await?
            .knowledge_check()?;
        let epoch: Option<i64> = response.take(response.num_statements() - 2)?;
        Ok(epoch.map(|epoch| CoordinatorLease { epoch, ..candidate }))
    }
    pub async fn renew_knowledge_coordinator(
        &self,
        lease: &CoordinatorLease,
    ) -> Result<(), StoreError> {
        let _mutation = lease.mutation().await;
        lease
            .bind(self.client().query(include_str!(
                "../queries/knowledge/coordinator/renew_knowledge_coordinator.surql"
            )))
            .await?
            .knowledge_check()?;
        Ok(())
    }
    /// A departing worker cannot expire its successor's lease.
    pub async fn release_knowledge_coordinator(
        &self,
        lease: &CoordinatorLease,
    ) -> Result<(), StoreError> {
        let _mutation = lease.mutation().await;
        lease
            .bind(self.client().query(include_str!(
                "../queries/knowledge/coordinator/release_knowledge_coordinator.surql"
            )))
            .await?
            .knowledge_check()?;
        Ok(())
    }
}
