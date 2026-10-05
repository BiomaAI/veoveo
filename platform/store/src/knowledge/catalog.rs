use super::*;
use crate::PlatformStore;
use std::collections::{BTreeMap, BTreeSet};
use veoveo_knowledge_contract::{CollectionRegistration, KnowledgeCollectionApproval};
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
#[derive(SurrealValue)]
struct SourceRow {
    server: String,
    registrations: Vec<RegistrationRow>,
}

pub(super) fn approvals_valid(
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
        self.client()
            .query(include_str!(
                "../queries/knowledge/catalog/register_knowledge_collection.surql"
            ))
            .bind((
                "record",
                collection_record(&registration.tenant, registration.descriptor.collection()),
            ))
            .bind(("row", RegistrationRow::new(registration)?))
            .bind(("expected", expected.map(ToString::to_string)))
            .await?
            .knowledge_check()?;
        Ok(())
    }

    /// Subscription roots are selected by URI, current approval and scopes in
    /// SQL. Ambiguous declarations fail closed instead of choosing one owner.
    pub async fn knowledge_collection_at_root(
        &self,
        tenant: &TenantId,
        root: &veoveo_types::ResourceUri,
        approvals: &BTreeMap<CollectionId, KnowledgeCollectionApproval>,
        scopes: &BTreeSet<ScopeName>,
    ) -> Result<Option<CollectionRegistration>, StoreError> {
        approvals_valid(approvals, scopes)?;
        let mut result = self
            .client()
            .query(include_str!("../queries/knowledge/read_root.surql"))
            .bind(("tenant", tenant.to_string()))
            .bind(("root", root.to_string()))
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
        let rows: Vec<RegistrationRow> = result.take(0)?;
        let mut rows = rows.into_iter();
        let Some(row) = rows.next() else {
            return Ok(None);
        };
        let registration = row.checked(tenant)?;
        if rows.next().is_some()
            || registration.tenant != *tenant
            || registration.validate().is_err()
            || veoveo_mcp_knowledge_extension::enumeration_uri(&registration.descriptor, None)
                .ok()
                .as_ref()
                != Some(root)
        {
            return integrity();
        }
        Ok(Some(registration))
    }

    /// Confirm a previously admitted active member for an indexing subscription.
    /// Initial generation builds subscribe to collection roots before source reads.
    pub async fn knowledge_member_observed(
        &self,
        registration: &CollectionRegistration,
        uri: &veoveo_types::ResourceUri,
    ) -> Result<bool, StoreError> {
        match self
            .knowledge_collection(&registration.tenant, registration.descriptor.collection())
            .await?
        {
            None => return Ok(false),
            Some(current) if current != *registration => return Ok(false),
            Some(_) => {}
        }
        let mut response = self
            .client()
            .query(include_str!("../queries/knowledge/observed_member.surql"))
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
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/knowledge/catalog/knowledge_collection.surql"
            ))
            .bind(("record", collection_record(tenant, collection)))
            .bind(("tenant", tenant.to_string()))
            .bind(("collection", collection.to_string()))
            .await?
            .knowledge_check()?;
        let document: Option<RegistrationRow> = response.take(0)?;
        let Some(row) = document else {
            return Ok(None);
        };
        let document = row.checked(tenant)?;
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
            .query(include_str!("../queries/knowledge/read_catalog.surql"))
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
        let rows: Vec<RegistrationRow> = response.take(0)?;
        rows.into_iter().map(|row| row.checked(tenant)).collect()
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
            .query(include_str!("../queries/knowledge/read_sources.surql"))
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
        let rows: Vec<SourceRow> = response.take(0)?;
        rows.into_iter()
            .map(|row| {
                let server: ServerSlug = row
                    .server
                    .parse()
                    .map_err(|_| StoreError::Knowledge("invalid stored source"))?;
                let mut registrations = row
                    .registrations
                    .into_iter()
                    .map(|registration| registration.checked(tenant))
                    .collect::<Result<Vec<_>, _>>()?;
                if registrations.is_empty()
                    || registrations
                        .iter()
                        .any(|r| r.descriptor.collection().server() != &server)
                {
                    return integrity();
                }
                registrations
                    .sort_by(|a, b| a.descriptor.collection().cmp(b.descriptor.collection()));
                Ok(CatalogSource {
                    server,
                    registrations,
                })
            })
            .collect()
    }
}
