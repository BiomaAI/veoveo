//! Domain-owned fixtures for generic knowledge certification. Credentials and
//! lifecycle callbacks stay outside serializable profiles and reports.
use std::{collections::BTreeSet, future::Future, pin::Pin};

use veoveo_mcp_knowledge_extension::CollectionId;
use veoveo_types::{LocalToolName, ResourceUri};

use crate::ConformanceCredentials;

pub type KnowledgeProbeFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;

/// The owner supplies isolated data, real lifecycle operations and cleanup.
/// The runner judges the resulting protocol observations, not callback claims.
pub trait KnowledgeChangeDriver: Sync {
    /// Commit the next fixture change: update its member or remove its visibility.
    fn mutate(&self) -> KnowledgeProbeFuture<'_>;

    /// Stop and recreate the owning service/change source, preserving only its
    /// declared persistent storage. Return when the same endpoint is ready.
    fn restart(&self) -> KnowledgeProbeFuture<'_>;
}

pub enum KnowledgeChange {
    /// Update a readable member before restart and one afterwards; they may coincide.
    Update { members: [ResourceUri; 2] },
    /// Remove one member before restart and a distinct member afterwards.
    /// Removal must revoke full and conditional reads as well as enumeration.
    Remove { members: [ResourceUri; 2] },
}

pub struct KnowledgeChangeProbe<'a> {
    pub collection: CollectionId,
    pub change: KnowledgeChange,
    pub driver: &'a dyn KnowledgeChangeDriver,
}

impl<'a> KnowledgeChangeProbe<'a> {
    pub fn update(
        collection: CollectionId,
        member: ResourceUri,
        driver: &'a dyn KnowledgeChangeDriver,
    ) -> Self {
        Self::updates(collection, [member.clone(), member], driver)
    }

    pub fn updates(
        collection: CollectionId,
        members: [ResourceUri; 2],
        driver: &'a dyn KnowledgeChangeDriver,
    ) -> Self {
        Self {
            collection,
            change: KnowledgeChange::Update { members },
            driver,
        }
    }

    pub fn remove(
        collection: CollectionId,
        members: [ResourceUri; 2],
        driver: &'a dyn KnowledgeChangeDriver,
    ) -> Self {
        Self {
            collection,
            change: KnowledgeChange::Remove { members },
            driver,
        }
    }
}

pub enum KnowledgeSearchAccess {
    /// The same tool returns a strict subset for an authenticated reader.
    Results(BTreeSet<ResourceUri>),
    /// Source policy rejects the entire tool for an authenticated reader.
    Denied,
}

/// A populated search fixture has two authenticated readers.
pub struct KnowledgeSearchProbe {
    pub tool: LocalToolName,
    /// Domain-owned input is opaque to the generic runner.
    pub arguments: serde_json::Map<String, serde_json::Value>,
    pub expected: BTreeSet<ResourceUri>,
    pub restricted_credentials: ConformanceCredentials,
    pub restricted: KnowledgeSearchAccess,
}

/// Every declared listen collection and search tool requires exactly one probe.
#[derive(Default)]
pub struct KnowledgeProbes<'a> {
    pub changes: Vec<KnowledgeChangeProbe<'a>>,
    pub searches: Vec<KnowledgeSearchProbe>,
}
