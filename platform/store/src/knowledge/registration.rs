//! Catalog storage lookups derive from one checked registration and hydrate together.
use super::*;
use veoveo_knowledge_contract::{
    CollectionApproval, CollectionRegistration, KnowledgeCollectionApproval,
};
use veoveo_mcp_knowledge_extension::{ChangeSignal, EntityKind};
use veoveo_types::ScopeName;

#[derive(Clone, SurrealValue)]
pub(super) struct RegistrationRow {
    pub(super) id: RecordId,
    tenant: String,
    collection: String,
    enumeration_root: String,
    revision: String,
    approved: bool,
    approval: Document<KnowledgeCollectionApproval>,
    #[surreal(wrap)]
    required_scopes: Vec<ScopeName>,
    #[surreal(wrap)]
    change_signal: ChangeSignal,
    #[surreal(wrap)]
    entity_kind: EntityKind,
    pub(super) document: Document<CollectionRegistration>,
}
impl RegistrationRow {
    pub(super) fn new(registration: &CollectionRegistration) -> Result<Self, StoreError> {
        registration
            .validate()
            .map_err(|error| StoreError::Knowledge(error.0))?;
        Ok(Self {
            id: collection_record(&registration.tenant, registration.descriptor.collection()),
            tenant: registration.tenant.to_string(),
            collection: registration.descriptor.collection().to_string(),
            enumeration_root: veoveo_mcp_knowledge_extension::enumeration_uri(
                &registration.descriptor,
                None,
            )
            .map_err(|error| StoreError::Knowledge(error.0))?
            .to_string(),
            revision: registration.revision().to_string(),
            approved: registration.approval.mode == CollectionApproval::Index,
            approval: Document(registration.approval.clone()),
            required_scopes: registration
                .descriptor
                .required_scopes()
                .iter()
                .cloned()
                .collect(),
            change_signal: registration.descriptor.change_signal(),
            entity_kind: registration.descriptor.entity_kind().clone(),
            document: Document(registration.clone()),
        })
    }
    pub(super) fn checked(self, tenant: &TenantId) -> Result<CollectionRegistration, StoreError> {
        let expected = Self::new(&self.document.0)?;
        if self.document.0.tenant != *tenant
            || self.id != expected.id
            || self.tenant != expected.tenant
            || self.collection != expected.collection
            || self.enumeration_root != expected.enumeration_root
            || self.revision != expected.revision
            || self.approved != expected.approved
            || self.approval.0 != expected.approval.0
            || self.required_scopes != expected.required_scopes
            || self.change_signal != expected.change_signal
            || self.entity_kind != expected.entity_kind
        {
            return integrity();
        }
        Ok(self.document.0)
    }
}
