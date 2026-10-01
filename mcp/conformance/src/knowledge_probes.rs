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
    /// Commit a change to the selected member's text or access descriptor.
    fn mutate(&self) -> KnowledgeProbeFuture<'_>;

    /// Stop and recreate the owning service/change source, preserving only its
    /// declared persistent storage. Return when the same endpoint is ready.
    fn restart(&self) -> KnowledgeProbeFuture<'_>;
}

pub struct KnowledgeChangeProbe<'a> {
    pub collection: CollectionId,
    pub member: ResourceUri,
    pub driver: &'a dyn KnowledgeChangeDriver,
}

/// A populated search fixture has two authenticated readers. Restricted hits
/// must be a strict subset of ordinary hits for the same tool arguments.
pub struct KnowledgeSearchProbe {
    pub tool: LocalToolName,
    /// Domain-owned input is opaque to the generic runner.
    pub arguments: serde_json::Map<String, serde_json::Value>,
    pub expected: BTreeSet<ResourceUri>,
    pub restricted_credentials: ConformanceCredentials,
    pub restricted_expected: BTreeSet<ResourceUri>,
}

/// Every declared listen collection and search tool requires exactly one probe.
#[derive(Default)]
pub struct KnowledgeProbes<'a> {
    pub changes: Vec<KnowledgeChangeProbe<'a>>,
    pub searches: Vec<KnowledgeSearchProbe>,
}
