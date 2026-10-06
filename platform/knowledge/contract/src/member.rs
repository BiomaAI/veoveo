use crate::{CollectionApproval, CollectionRegistration, GenerationSpec, KnowledgeError};
use serde::Serialize;
use std::ops::Range;
use veoveo_embedding_contract::{EmbeddingVector, QualifiedEmbeddingRuntime};
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
        runtime: &QualifiedEmbeddingRuntime,
    ) -> Result<Self, KnowledgeError> {
        let text = text.get(range).ok_or(KnowledgeError(
            "chunk range must select complete UTF-8 characters",
        ))?;
        if text.trim().is_empty()
            || text.chars().count() > generation.chunking().max_characters() as usize
            || vector.space() != generation.space()
            || runtime.space() != generation.space()
            || vector.profile_id() != runtime.profile().id()
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
    title: crate::MemberTitle,
}
impl IndexedMember {
    pub fn new(
        registration: &CollectionRegistration,
        generation: &GenerationSpec,
        uri: ResourceUri,
        observation: Observation,
        text: &str,
        title: crate::MemberTitle,
        chunks: Vec<IndexedChunk>,
    ) -> Result<Self, KnowledgeError> {
        Self::admit(
            registration,
            generation,
            uri,
            observation,
            text,
            title,
            IndexingMode::Content,
            chunks,
        )
    }

    /// Metadata mode never accepts arbitrary body text as its chunk source.
    pub fn metadata(
        registration: &CollectionRegistration,
        generation: &GenerationSpec,
        uri: ResourceUri,
        observation: Observation,
        source_text: &str,
        title: crate::MemberTitle,
        chunks: Vec<IndexedChunk>,
    ) -> Result<Self, KnowledgeError> {
        Self::admit(
            registration,
            generation,
            uri,
            observation,
            source_text,
            title,
            IndexingMode::Metadata,
            chunks,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "one shared checked constructor for two index modes"
    )]
    fn admit(
        registration: &CollectionRegistration,
        generation: &GenerationSpec,
        uri: ResourceUri,
        observation: Observation,
        source_text: &str,
        title: crate::MemberTitle,
        expected_mode: IndexingMode,
        chunks: Vec<IndexedChunk>,
    ) -> Result<Self, KnowledgeError> {
        let metadata =
            (expected_mode == IndexingMode::Metadata).then(|| metadata_text(&title, &observation));
        let text = metadata.as_deref().unwrap_or(source_text);
        observation
            .validate_collection(&registration.descriptor)
            .map_err(|_| KnowledgeError("member observation disagrees with its collection"))?;
        registration.admit_observation(&observation)?;
        if registration.approval.mode != CollectionApproval::Index
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
            || content_digest(source_text) != *observation.content_sha256()
            || source_text.len() > 64 * 1024
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
            || chunks
                .windows(2)
                .any(|pair| pair[0].vector.profile_id() != pair[1].vector.profile_id())
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
        if registration.descriptor.indexing() != expected_mode {
            return Err(KnowledgeError(
                "index text does not match the collection indexing mode",
            ));
        }
        Ok(Self {
            generation_revision: generation.revision(),
            title,
            uri,
            observation,
            chunks,
        })
    }
    pub fn uri(&self) -> &ResourceUri {
        &self.uri
    }
    pub fn title(&self) -> &crate::MemberTitle {
        &self.title
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

/// Index only the title and this closed subset of provenance. Source body, access
/// subjects and external URLs (which may be signed) never enter metadata chunks.
pub fn metadata_text(title: &crate::MemberTitle, observation: &Observation) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Metadata<'a> {
        title: &'a crate::MemberTitle,
        collection: &'a veoveo_mcp_knowledge_extension::CollectionId,
        revision: &'a veoveo_mcp_knowledge_extension::Revision,
        modified_at: Option<String>,
        external_system: Option<&'a veoveo_mcp_knowledge_extension::ExternalSystemId>,
        external_record: Option<&'a veoveo_mcp_knowledge_extension::ExternalRecordId>,
    }
    serde_json::to_string(&Metadata {
        title,
        collection: observation.collection(),
        revision: observation.revision(),
        modified_at: observation.modified_at().map(|v| v.to_rfc3339()),
        external_system: observation.external().map(|v| &v.system),
        external_record: observation.external().map(|v| &v.native_id),
    })
    .expect("typed metadata serializes")
}
