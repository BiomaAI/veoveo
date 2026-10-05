//! Atomic catalog replacement against the active installation and prior source set.
use super::*;
use crate::PlatformStore;
use veoveo_knowledge_contract::{CollectionApproval, CollectionRegistration};
use veoveo_types::Sha256Digest;

/// A Store snapshot taken before discovery. Private fields prevent a caller from
/// substituting another tenant, control revision or prior registration set.
pub struct KnowledgeCatalogTicket {
    tenant: TenantId,
    control: RecordId,
    digest: Sha256Digest,
    prior: Vec<Fingerprint>,
}
#[derive(SurrealValue)]
struct Fingerprint {
    collection: String,
    revision: String,
}
#[derive(SurrealValue)]
struct Snapshot {
    control: RecordId,
    digest: String,
    prior: Vec<Fingerprint>,
}
#[derive(SurrealValue)]
struct RegistrationRow {
    id: RecordId,
    tenant: String,
    collection: String,
    enumeration_root: String,
    revision: String,
    approved: bool,
    document: Document<CollectionRegistration>,
}

impl PlatformStore {
    pub async fn begin_knowledge_catalog(
        &self,
        tenant: &TenantId,
        control: &Sha256Digest,
    ) -> Result<KnowledgeCatalogTicket, StoreError> {
        let mut response = self
            .client()
            .query(include_str!("../queries/knowledge/catalog_begin.surql"))
            .bind(("tenant", tenant.to_string()))
            .bind(("digest", control.hex().to_owned()))
            .await?
            .knowledge_check()?;
        let snapshot: Option<Snapshot> = response.take(response.num_statements() - 2)?;
        let snapshot =
            snapshot.ok_or(StoreError::Knowledge("catalog control snapshot is absent"))?;
        if snapshot.digest != control.hex() || snapshot.prior.len() > 1024 {
            return Err(StoreError::Knowledge(
                "catalog control snapshot does not match discovery",
            ));
        }
        Ok(KnowledgeCatalogTicket {
            tenant: tenant.clone(),
            control: snapshot.control,
            digest: control.clone(),
            prior: snapshot.prior,
        })
    }

    /// Empty catalogs revoke every prior registration. A failed or incomplete
    /// discovery must never call this method with a partial selection.
    pub async fn replace_knowledge_catalog(
        &self,
        ticket: KnowledgeCatalogTicket,
        registrations: &[CollectionRegistration],
    ) -> Result<(), StoreError> {
        if registrations.len() > 1024 {
            return Err(StoreError::Knowledge("catalog exceeds 1024 collections"));
        }
        let mut selected = std::collections::BTreeSet::new();
        let mut rows = Vec::with_capacity(registrations.len());
        for registration in registrations {
            registration
                .validate()
                .map_err(|error| StoreError::Knowledge(error.0))?;
            if registration.tenant != ticket.tenant
                || registration.control_revision != ticket.digest
                || !selected.insert(registration.descriptor.collection().to_string())
            {
                return Err(StoreError::Knowledge(
                    "catalog has duplicate or foreign registrations",
                ));
            }
            rows.push(RegistrationRow {
                id: collection_record(&ticket.tenant, registration.descriptor.collection()),
                tenant: ticket.tenant.to_string(),
                collection: registration.descriptor.collection().to_string(),
                enumeration_root: veoveo_mcp_knowledge_extension::enumeration_uri(
                    &registration.descriptor,
                    None,
                )
                .map_err(|error| StoreError::Knowledge(error.0))?
                .to_string(),
                revision: registration.revision().to_string(),
                approved: registration.approval.mode == CollectionApproval::Index,
                document: Document(registration.clone()),
            });
        }
        self.client()
            .query(include_str!("../queries/knowledge/catalog_replace.surql"))
            .bind(("tenant", ticket.tenant.to_string()))
            .bind(("control", ticket.control))
            .bind(("digest", ticket.digest.hex().to_owned()))
            .bind(("prior", ticket.prior))
            .bind(("selected", selected.into_iter().collect::<Vec<_>>()))
            .bind(("registrations", rows))
            .await?
            .knowledge_check()?;
        Ok(())
    }
}
