use crate::{CollectionApproval, CollectionRegistration, GenerationSpec, KnowledgeError};
use serde::Serialize;
use std::ops::Range;
use veoveo_embedding_contract::EmbeddingVector;
use veoveo_mcp_knowledge_extension::{IndexingMode, Observation, content_digest};
use veoveo_types::{ResourceUri, ResourceUriParts, Sha256Digest};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IndexedChunk {
    text: String,
    vector: EmbeddingVector,
}
impl IndexedChunk {
    pub fn from_range(
        text: &str,
        range: Range<usize>,
        vector: EmbeddingVector,
        generation: &GenerationSpec,
    ) -> Result<Self, KnowledgeError> {
        let text = text.get(range).ok_or(KnowledgeError(
            "chunk range must select complete UTF-8 characters",
        ))?;
        if text.trim().is_empty()
            || text.chars().count() > generation.chunking().max_characters() as usize
            || vector.space() != generation.space()
        {
            return Err(KnowledgeError(
                "chunk exceeds its character cap or uses another embedding space",
            ));
        }
        Ok(Self {
            text: text.into(),
            vector,
        })
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn vector(&self) -> &EmbeddingVector {
        &self.vector
    }
}

/// A complete source read and its admitted chunks. Construction binds the source
/// digest; storage rechecks generation, collection approval and read fencing.
#[derive(Debug, Clone)]
pub struct IndexedMember {
    generation_revision: Sha256Digest,
    uri: ResourceUri,
    observation: Observation,
    chunks: Vec<IndexedChunk>,
}
impl IndexedMember {
    pub fn new(
        registration: &CollectionRegistration,
        generation: &GenerationSpec,
        uri: ResourceUri,
        observation: Observation,
        text: &str,
        chunks: Vec<IndexedChunk>,
    ) -> Result<Self, KnowledgeError> {
        observation
            .validate_collection(&registration.descriptor)
            .map_err(|_| KnowledgeError("member observation disagrees with its collection"))?;
        if registration.approval != CollectionApproval::Index
            || registration.descriptor.indexing() == IndexingMode::None
            || generation
                .collections()
                .get(registration.descriptor.collection())
                != Some(&registration.revision())
        {
            return Err(KnowledgeError(
                "collection is not approved in this generation",
            ));
        }
        if observation.not_modified()
            || content_digest(text) != *observation.content_sha256()
            || text.len() > 64 * 1024
            || observation
                .access()
                .is_some_and(|access| access.tenant != registration.tenant)
        {
            return Err(KnowledgeError(
                "member content, tenant or observation is invalid",
            ));
        }
        let address = ResourceUriParts::parse(uri.as_str())
            .map_err(|_| KnowledgeError("invalid member URI"))?;
        let enumeration = registration
            .descriptor
            .enumerate()
            .expand_scalars(&Default::default())
            .map_err(|_| KnowledgeError("invalid enumeration root"))?;
        if address.scheme()
            != ResourceUriParts::parse(enumeration.as_str())
                .map_err(|_| KnowledgeError("invalid enumeration URI"))?
                .scheme()
            || chunks.len() > 256
            || chunks.is_empty()
        {
            return Err(KnowledgeError(
                "member requires an owned URI and 1..=256 chunks",
            ));
        }
        for chunk in &chunks {
            if chunk.text.trim().is_empty()
                || chunk.text.chars().count() > generation.chunking().max_characters() as usize
                || chunk.vector.space() != generation.space()
                || !text.contains(&chunk.text)
            {
                return Err(KnowledgeError(
                    "chunk does not belong to the source text or generation",
                ));
            }
        }
        // Metadata indexing uses a separately constructed metadata document in
        // the service; admitting arbitrary source text here would index its body.
        if registration.descriptor.indexing() != IndexingMode::Content {
            return Err(KnowledgeError(
                "content ingestion requires content indexing; metadata ingestion is separate",
            ));
        }
        Ok(Self {
            generation_revision: generation.revision(),
            uri,
            observation,
            chunks,
        })
    }
    pub fn uri(&self) -> &ResourceUri {
        &self.uri
    }
    pub fn generation_revision(&self) -> &Sha256Digest {
        &self.generation_revision
    }
    pub fn observation(&self) -> &Observation {
        &self.observation
    }
    pub fn chunks(&self) -> &[IndexedChunk] {
        &self.chunks
    }
}
