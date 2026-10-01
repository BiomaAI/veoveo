use crate::KnowledgeError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;
use veoveo_embedding_contract::EmbeddingSpace;
use veoveo_mcp_knowledge_extension::CollectionId;
use veoveo_types::Sha256Digest;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct GenerationId(Uuid);
impl GenerationId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}
impl Default for GenerationId {
    fn default() -> Self {
        Self::new()
    }
}
impl std::fmt::Display for GenerationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl TryFrom<String> for GenerationId {
    type Error = KnowledgeError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let id = Uuid::parse_str(&value).map_err(|_| KnowledgeError("invalid generation UUID"))?;
        if id.get_version_num() != 7
            || id.get_variant() != uuid::Variant::RFC4122
            || id.to_string() != value
        {
            return Err(KnowledgeError("generation requires a canonical RFC UUIDv7"));
        }
        Ok(Self(id))
    }
}
impl From<GenerationId> for String {
    fn from(value: GenerationId) -> Self {
        value.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "ChunkWire", into = "ChunkWire")]
pub struct ChunkSettings(ChunkWire);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChunkWire {
    version: String,
    max_characters: u32,
    overlap_characters: u32,
}
impl ChunkSettings {
    pub fn new(
        version: impl Into<String>,
        max_characters: u32,
        overlap_characters: u32,
    ) -> Result<Self, KnowledgeError> {
        ChunkWire {
            version: version.into(),
            max_characters,
            overlap_characters,
        }
        .try_into()
    }
    pub fn max_characters(&self) -> u32 {
        self.0.max_characters
    }
    pub fn overlap_characters(&self) -> u32 {
        self.0.overlap_characters
    }
    pub fn version(&self) -> &str {
        &self.0.version
    }
}
impl TryFrom<ChunkWire> for ChunkSettings {
    type Error = KnowledgeError;
    fn try_from(w: ChunkWire) -> Result<Self, Self::Error> {
        if w.version.is_empty()
            || w.version.len() > 128
            || w.version.chars().any(char::is_control)
            || !(1..=8192).contains(&w.max_characters)
            || w.overlap_characters >= w.max_characters
        {
            return Err(KnowledgeError(
                "invalid chunk version, character cap or overlap",
            ));
        }
        Ok(Self(w))
    }
}
impl From<ChunkSettings> for ChunkWire {
    fn from(v: ChunkSettings) -> Self {
        v.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "GenerationWire", into = "GenerationWire")]
pub struct GenerationSpec(GenerationWire);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerationWire {
    space: EmbeddingSpace,
    query_task: String,
    chunking: ChunkSettings,
    collections: BTreeMap<CollectionId, Sha256Digest>,
}
impl GenerationSpec {
    pub fn new(
        space: EmbeddingSpace,
        query_task: impl Into<String>,
        chunking: ChunkSettings,
        collections: BTreeMap<CollectionId, Sha256Digest>,
    ) -> Result<Self, KnowledgeError> {
        GenerationWire {
            space,
            query_task: query_task.into(),
            chunking,
            collections,
        }
        .try_into()
    }
    pub fn space(&self) -> &EmbeddingSpace {
        &self.0.space
    }
    pub fn query_task(&self) -> &str {
        &self.0.query_task
    }
    pub fn chunking(&self) -> &ChunkSettings {
        &self.0.chunking
    }
    pub fn collections(&self) -> &BTreeMap<CollectionId, Sha256Digest> {
        &self.0.collections
    }
    pub fn revision(&self) -> Sha256Digest {
        crate::digest(self)
    }
}
impl TryFrom<GenerationWire> for GenerationSpec {
    type Error = KnowledgeError;
    fn try_from(w: GenerationWire) -> Result<Self, Self::Error> {
        if w.query_task.trim().is_empty()
            || w.query_task.len() > 2048
            || w.query_task.chars().any(char::is_control)
            || w.collections.is_empty()
            || w.collections.len() > 1024
        {
            return Err(KnowledgeError(
                "generation requires a bounded query task and 1..=1024 approved collections",
            ));
        }
        Ok(Self(w))
    }
}
impl From<GenerationSpec> for GenerationWire {
    fn from(v: GenerationSpec) -> Self {
        v.0
    }
}
