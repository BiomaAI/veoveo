use super::*;
use crate::PlatformStore;
use veoveo_knowledge_contract::CollectionRegistration;

/// A complete enumeration belongs to one source epoch and coordinator. A late
/// traversal cannot publish coverage after a newer invalidation.
#[derive(Clone, Debug)]
pub struct CollectionSyncTicket {
    lease: CoordinatorLease,
    registration: CollectionRegistration,
    generation: GenerationId,
    epoch: i64,
}
impl PlatformStore {
    pub async fn knowledge_collection_sync(
        &self,
        lease: &CoordinatorLease,
        registration: &CollectionRegistration,
        generation: GenerationId,
    ) -> Result<CollectionSyncTicket, StoreError> {
        self.collection_sync(lease, registration, generation, false)
            .await
    }

    pub async fn invalidate_knowledge_collection(
        &self,
        lease: &CoordinatorLease,
        registration: &CollectionRegistration,
        generation: GenerationId,
    ) -> Result<CollectionSyncTicket, StoreError> {
        self.collection_sync(lease, registration, generation, true)
            .await
    }

    async fn collection_sync(
        &self,
        lease: &CoordinatorLease,
        registration: &CollectionRegistration,
        generation: GenerationId,
        invalidate: bool,
    ) -> Result<CollectionSyncTicket, StoreError> {
        let _mutation = lease.mutation().await;
        lease.check_tenant(&registration.tenant)?;
        let sql = if invalidate {
            include_str!("../queries/knowledge/invalidate.surql")
        } else {
            include_str!("../queries/knowledge/collections/collection_sync.surql")
        };
        let collection = registration.descriptor.collection();
        let mut response = lease
            .bind(self.client().query(sql))
            .bind(("tenant", registration.tenant.to_string()))
            .bind(("generation", generation_record(generation)))
            .bind((
                "collection",
                collection_record(&registration.tenant, collection),
            ))
            .bind(("approval", registration.revision().to_string()))
            .bind(("sync", sync_record(generation, collection)))
            .bind(("coverage", coverage_record(generation, collection)))
            .await?
            .knowledge_check()?;
        let epoch: Option<i64> = response.take(response.num_statements() - 2)?;
        Ok(CollectionSyncTicket {
            lease: lease.clone(),
            registration: registration.clone(),
            generation,
            epoch: epoch.ok_or(StoreError::Knowledge("collection sync is not current"))?,
        })
    }

    /// Publish only the traversal captured by this ticket. Pending reads from
    /// its source epoch prevent completion; earlier absent members stay hidden.
    pub async fn complete_knowledge_collection(
        &self,
        ticket: &CollectionSyncTicket,
    ) -> Result<(), StoreError> {
        let _mutation = ticket.lease.mutation().await;
        let registration = &ticket.registration;
        let collection = registration.descriptor.collection();
        ticket
            .lease
            .bind(
                self.client()
                    .query(include_str!("../queries/knowledge/coverage.surql")),
            )
            .bind(("sync", sync_record(ticket.generation, collection)))
            .bind(("source_epoch", ticket.epoch))
            .bind(("tenant", registration.tenant.to_string()))
            .bind(("generation", generation_record(ticket.generation)))
            .bind((
                "collection",
                collection_record(&registration.tenant, collection),
            ))
            .bind(("coverage", coverage_record(ticket.generation, collection)))
            .bind(("revision", registration.revision().to_string()))
            .await?
            .knowledge_check()?;
        Ok(())
    }
}
