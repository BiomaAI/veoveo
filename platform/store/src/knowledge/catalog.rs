use super::*;
use crate::PlatformStore;
use veoveo_knowledge_contract::{CollectionApproval, CollectionRegistration};
use veoveo_mcp_knowledge_extension::IndexingMode;

impl PlatformStore {
    pub async fn register_knowledge_collection(
        &self,
        registration: &CollectionRegistration,
        expected: Option<&veoveo_types::Sha256Digest>,
    ) -> Result<(), StoreError> {
        if registration.approval == CollectionApproval::Index
            && registration.descriptor.indexing() == IndexingMode::None
        {
            return Err(StoreError::Knowledge(
                "a non-indexable collection cannot be approved for indexing",
            ));
        }
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
            .bind(("approved", registration.approval == CollectionApproval::Index))
            .bind(("document", Document(registration.clone()))).await?.knowledge_check()?;
        Ok(())
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
        if &document.tenant != tenant || document.descriptor.collection() != collection {
            return integrity();
        }
        Ok(Some(document))
    }
}
