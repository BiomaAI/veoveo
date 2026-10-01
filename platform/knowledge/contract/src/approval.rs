//! Installation approval for a source collection, independent of MCP transport.
use crate::{CollectionApproval, KnowledgeError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use veoveo_mcp_knowledge_extension::CollectionId;
use veoveo_types::{DataLabelId, GroupId};

/// A catalog topic for which an installation treats a collection as authoritative.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct KnowledgeSubject(String);
impl KnowledgeSubject {
    pub fn new(value: impl Into<String>) -> Result<Self, KnowledgeError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 256
            || value.trim() != value
            || value.chars().any(char::is_control)
        {
            return Err(KnowledgeError(
                "knowledge subject requires 1..=256 printable bytes without surrounding whitespace",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for KnowledgeSubject {
    type Error = KnowledgeError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<KnowledgeSubject> for String {
    fn from(value: KnowledgeSubject) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct KnowledgeCollectionApproval {
    pub collection: CollectionId,
    pub mode: CollectionApproval,
    pub stewards: BTreeSet<GroupId>,
    pub authoritative_for: BTreeSet<KnowledgeSubject>,
    /// Complete set of labels the installation permits this index to retain.
    /// Empty permits unlabelled records only. Caller clearance still applies.
    pub data_labels: BTreeSet<DataLabelId>,
}
impl KnowledgeCollectionApproval {
    pub fn validate(&self) -> Result<(), KnowledgeError> {
        if self.stewards.is_empty()
            || self.stewards.len() > 64
            || self.authoritative_for.len() > 64
            || self.data_labels.len() > 64
        {
            return Err(KnowledgeError(
                "knowledge approval requires 1..=64 stewards and at most 64 subjects or data labels",
            ));
        }
        Ok(())
    }
}

/// Explicit installation grant to a machine client. This never grants a human
/// caller's read access and cannot be requested through tool arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeIndexingRegistration {
    pub collections: BTreeSet<CollectionId>,
}
impl KnowledgeIndexingRegistration {
    pub fn validate(&self) -> Result<(), KnowledgeError> {
        if self.collections.is_empty() || self.collections.len() > 1024 {
            return Err(KnowledgeError(
                "indexing client requires 1..=1024 approved collections",
            ));
        }
        Ok(())
    }
}
