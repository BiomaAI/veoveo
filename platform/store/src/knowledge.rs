//! Knowledge persistence. Source policy stays in the service; SQL narrows every
//! caller-visible candidate before decoding and pagination.
mod admission;
mod catalog;
mod catalog_sync;
mod collections;
mod completion;
mod coordinator;
mod evaluations;
mod generations;
mod members;
mod reads;
mod resource_selection;
mod search;
mod statistics;
use crate::StoreError;
pub use catalog::{CatalogSelection, CatalogSource};
pub use catalog_sync::KnowledgeCatalogTicket;
pub use collections::CollectionSyncTicket;
pub use completion::{CatalogCompletion, CatalogCompletionValue};
use coordinator::sync_record;
pub use coordinator::{CoordinatorId, CoordinatorLease};
pub use members::MemberReadTicket;
pub use reads::{CandidateCursor, CandidateScope, KnowledgeCandidate};
pub use search::{HybridSearchPage, RankedCandidate, SearchWindow};
use serde::{Serialize, de::DeserializeOwned};
pub use statistics::CollectionStatisticsSnapshot;
use surrealdb::types::{Array, Error, Kind, RecordId, SurrealValue, Uuid, Value};
use veoveo_knowledge_contract::GenerationId;
use veoveo_mcp_knowledge_extension::CollectionId;
use veoveo_types::TenantId;

fn generation_record(id: GenerationId) -> RecordId {
    RecordId::new("knowledge_generation", Uuid::from(id.as_uuid()))
}
fn collection_record(tenant: &TenantId, collection: &CollectionId) -> RecordId {
    RecordId::new(
        "knowledge_collection",
        Array::from(vec![tenant.to_string(), collection.to_string()]),
    )
}
fn coverage_record(generation: GenerationId, collection: &CollectionId) -> RecordId {
    RecordId::new(
        "knowledge_coverage",
        Array::from(vec![generation.to_string(), collection.to_string()]),
    )
}
/// Only a checked UUID produces a SQL identifier. User URI, model and collection
/// strings are always bound values, including all record keys.
fn chunk_table(generation: GenerationId) -> String {
    format!("knowledge_chunk_{}", generation.as_uuid().simple())
}
#[derive(Debug, Clone)]
struct Document<T>(T);
impl<T: Serialize + DeserializeOwned> SurrealValue for Document<T> {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        crate::json_value::into_surreal(
            serde_json::to_value(self.0).expect("typed knowledge serialization"),
        )
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        serde_json::from_value(crate::json_value::from_surreal(value)?)
            .map(Self)
            .map_err(|_| Error::internal("invalid knowledge document".into()))
    }
}
fn serialize_subject(subject: &veoveo_types::AccessSubject) -> String {
    serde_json::to_string(subject).expect("typed access subject serialization")
}
fn integrity<T>() -> Result<T, StoreError> {
    Err(StoreError::Knowledge(
        "stored metadata disagrees with its document",
    ))
}

trait KnowledgeResponse: Sized {
    fn knowledge_check(self) -> Result<Self, StoreError>;
}
impl KnowledgeResponse for surrealdb::IndexedResults {
    fn knowledge_check(mut self) -> Result<Self, StoreError> {
        if let Some(error) = crate::primary_transaction_error(self.take_errors()) {
            return Err(error.into());
        }
        Ok(self)
    }
}
