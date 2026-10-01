use super::*;
use crate::PlatformStore;
use veoveo_knowledge_contract::{CollectionApproval, CollectionRegistration};

impl PlatformStore {
    pub async fn register_knowledge_collection(
        &self,
        registration: &CollectionRegistration,
        expected: Option<&veoveo_types::Sha256Digest>,
    ) -> Result<(), StoreError> {
        registration
            .validate()
            .map_err(|e| StoreError::Knowledge(e.0))?;
        self.client().query("BEGIN TRANSACTION;
            LET $prior = (SELECT * FROM ONLY $record);
            IF $prior.revision != $expected AND $prior.revision != $revision { THROW 'knowledge_catalog_revision_changed'; };
            UPSERT $record CONTENT {tenant: $tenant, collection: $collection, revision: $revision, approved: $approved, document: $document};
            COMMIT TRANSACTION;")
            .bind(("record", collection_record(&registration.tenant, registration.descriptor.collection())))
            .bind(("tenant", registration.tenant.to_string()))
            .bind(("collection", registration.descriptor.collection().to_string()))
            .bind(("revision", registration.revision().to_string()))
            .bind(("expected", expected.map(ToString::to_string)))
            .bind(("approved", registration.approval.mode == CollectionApproval::Index))
            .bind(("document", Document(registration.clone()))).await?.knowledge_check()?;
        Ok(())
    }

    /// Confirm a previously admitted active member for an indexing subscription.
    /// Initial generation builds subscribe to collection roots before source reads.
    pub async fn knowledge_member_observed(
        &self,
        registration: &CollectionRegistration,
        uri: &veoveo_types::ResourceUri,
    ) -> Result<bool, StoreError> {
        let mut response = self
            .client()
            .query(include_str!("observed_member.surql"))
            .bind(("tenant", registration.tenant.to_string()))
            .bind((
                "active",
                RecordId::new("knowledge_active", registration.tenant.as_str()),
            ))
            .bind((
                "collection",
                collection_record(&registration.tenant, registration.descriptor.collection()),
            ))
            .bind(("revision", registration.revision().to_string()))
            .bind(("uri", uri.to_string()))
            .await?
            .knowledge_check()?;
        let matched: Vec<bool> = response.take(0)?;
        Ok(matched == [true])
    }

    pub async fn knowledge_collection(
        &self,
        tenant: &TenantId,
        collection: &CollectionId,
    ) -> Result<Option<CollectionRegistration>, StoreError> {
        let mut response = self.client().query("SELECT VALUE document FROM ONLY $record WHERE tenant = $tenant AND collection = $collection;")
            .bind(("record", collection_record(tenant, collection))).bind(("tenant", tenant.to_string()))
            .bind(("collection", collection.to_string())).await?.knowledge_check()?;
        let document: Option<Document<CollectionRegistration>> = response.take(0)?;
        let Some(Document(document)) = document else {
            return Ok(None);
        };
        if &document.tenant != tenant
            || document.descriptor.collection() != collection
            || document.validate().is_err()
        {
            return integrity();
        }
        Ok(Some(document))
    }
}
