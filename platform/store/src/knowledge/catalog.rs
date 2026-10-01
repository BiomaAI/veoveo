use super::*;
use crate::PlatformStore;
use std::collections::{BTreeMap, BTreeSet};
use veoveo_knowledge_contract::{
    CollectionApproval, CollectionRegistration, KnowledgeCollectionApproval,
};
use veoveo_types::{ScopeName, ServerSlug};

/// Selection is applied before decoding catalog documents.
pub enum CatalogSelection<'a> {
    All,
    Source(&'a ServerSlug),
    Collection(&'a CollectionId),
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CatalogSource {
    pub server: ServerSlug,
    pub registrations: Vec<CollectionRegistration>,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct CatalogRow {
    collection: CollectionId,
    document: CollectionRegistration,
}

fn approvals_valid(
    approvals: &BTreeMap<CollectionId, KnowledgeCollectionApproval>,
    scopes: &BTreeSet<ScopeName>,
) -> Result<(), StoreError> {
    if approvals.len() > 1024 || scopes.len() > 1024 {
        return Err(StoreError::Knowledge("catalog selection exceeds its bound"));
    }
    for (id, approval) in approvals {
        approval
            .validate()
            .map_err(|error| StoreError::Knowledge(error.0))?;
        if id != &approval.collection {
            return integrity();
        }
    }
    Ok(())
}

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

impl PlatformStore {
    /// Caller-visible catalog selection. Approval and scope admission precede
    /// deserialization; corrupt or revoked hidden registrations cannot leak.
    pub async fn readable_knowledge_collections(
        &self,
        tenant: &TenantId,
        approvals: &std::collections::BTreeMap<
            CollectionId,
            veoveo_knowledge_contract::KnowledgeCollectionApproval,
        >,
        scopes: &std::collections::BTreeSet<veoveo_types::ScopeName>,
        selection: CatalogSelection<'_>,
    ) -> Result<Vec<CollectionRegistration>, StoreError> {
        approvals_valid(approvals, scopes)?;
        let selected = match selection {
            CatalogSelection::All => None,
            CatalogSelection::Source(server) => Some(
                approvals
                    .keys()
                    .filter(|id| id.server() == server)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ),
            CatalogSelection::Collection(collection) => Some(vec![collection.to_string()]),
        };
        let mut response = self
            .client()
            .query(include_str!("read_catalog.surql"))
            .bind(("selected", selected))
            .bind(("tenant", tenant.to_string()))
            .bind((
                "approvals",
                approvals
                    .iter()
                    .map(|(id, approval)| (id.to_string(), Document(approval.clone())))
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "scopes",
                scopes.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ))
            .await?
            .knowledge_check()?;
        let rows: Vec<Document<CatalogRow>> = response.take(0)?;
        rows.into_iter()
            .map(|Document(row)| {
                if row.document.tenant != *tenant
                    || row.document.validate().is_err()
                    || row.collection != *row.document.descriptor.collection()
                {
                    return integrity();
                }
                Ok(row.document)
            })
            .collect()
    }
}

impl PlatformStore {
    /// Source grouping and the 100-source page plus lookahead run in SQL.
    pub async fn readable_knowledge_sources(
        &self,
        tenant: &TenantId,
        approvals: &BTreeMap<CollectionId, KnowledgeCollectionApproval>,
        scopes: &BTreeSet<ScopeName>,
        after: Option<&ServerSlug>,
    ) -> Result<Vec<CatalogSource>, StoreError> {
        approvals_valid(approvals, scopes)?;
        let mut response = self
            .client()
            .query(include_str!("read_sources.surql"))
            .bind(("tenant", tenant.to_string()))
            .bind((
                "approvals",
                approvals
                    .iter()
                    .map(|(id, a)| (id.to_string(), Document(a.clone())))
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "scopes",
                scopes.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ))
            .bind(("after", after.map(ToString::to_string)))
            .await?
            .knowledge_check()?;
        let rows: Vec<Document<CatalogSource>> = response.take(0)?;
        rows.into_iter()
            .map(|Document(mut row)| {
                if row.registrations.is_empty()
                    || row.registrations.iter().any(|r| {
                        r.tenant != *tenant
                            || r.validate().is_err()
                            || r.descriptor.collection().server() != &row.server
                    })
                {
                    return integrity();
                }
                row.registrations
                    .sort_by(|a, b| a.descriptor.collection().cmp(b.descriptor.collection()));
                Ok(row)
            })
            .collect()
    }
}
