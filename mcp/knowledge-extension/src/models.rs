//! Closed wire models. Builders and deserialization share relationship checks.
use crate::{
    CollectionId, EntityKind, ExternalRecordId, ExternalSystemId, KnowledgeError, Revision,
};
use chrono::{DateTime, Utc};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_types::{
    AccessSubject, DataLabelId, GatewayProfileId, HttpsUrl, PrincipalId, ResourceTemplateUri,
    ResourceUri, Sha256Digest, TenantId, WorkContextId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "bool", into = "bool")]
pub struct ImmutableTrue;
impl TryFrom<bool> for ImmutableTrue {
    type Error = KnowledgeError;
    fn try_from(value: bool) -> Result<Self, Self::Error> {
        value
            .then_some(Self)
            .ok_or(KnowledgeError("immutable must be true"))
    }
}
impl From<ImmutableTrue> for bool {
    fn from(_: ImmutableTrue) -> Self {
        true
    }
}

fn true_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({"const": true})
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum Freshness {
    Immutable {
        #[schemars(schema_with = "true_schema")]
        immutable: ImmutableTrue,
    },
    MaxAge {
        #[serde(rename = "maxAgeSeconds")]
        max_age_seconds: u32,
    },
}
impl Freshness {
    pub const fn immutable() -> Self {
        Self::Immutable {
            immutable: ImmutableTrue,
        }
    }
    pub const fn max_age(seconds: u32) -> Self {
        Self::MaxAge {
            max_age_seconds: seconds,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeSignal {
    Listen,
    Immutable,
    Revalidate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AccessModel {
    WorkContext,
    Profile,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum IndexingMode {
    Content,
    Metadata,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CollectionWire {
    collection: CollectionId,
    entity_kind: EntityKind,
    enumerate: ResourceTemplateUri,
    freshness: Freshness,
    change_signal: ChangeSignal,
    access: AccessModel,
    indexing: IndexingMode,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "CollectionWire", into = "CollectionWire")]
pub struct CollectionDescriptor(CollectionWire);
impl CollectionDescriptor {
    pub fn new(
        collection: CollectionId,
        entity_kind: EntityKind,
        enumerate: ResourceTemplateUri,
        freshness: Freshness,
        change_signal: ChangeSignal,
        access: AccessModel,
        indexing: IndexingMode,
    ) -> Result<Self, KnowledgeError> {
        CollectionWire {
            collection,
            entity_kind,
            enumerate,
            freshness,
            change_signal,
            access,
            indexing,
        }
        .try_into()
    }
    pub fn collection(&self) -> &CollectionId {
        &self.0.collection
    }
    pub fn entity_kind(&self) -> &EntityKind {
        &self.0.entity_kind
    }
    pub fn enumerate(&self) -> &ResourceTemplateUri {
        &self.0.enumerate
    }
    pub fn freshness(&self) -> Freshness {
        self.0.freshness
    }
    pub fn change_signal(&self) -> ChangeSignal {
        self.0.change_signal
    }
    pub fn access(&self) -> AccessModel {
        self.0.access
    }
    pub fn indexing(&self) -> IndexingMode {
        self.0.indexing
    }
}
impl TryFrom<CollectionWire> for CollectionDescriptor {
    type Error = KnowledgeError;
    fn try_from(w: CollectionWire) -> Result<Self, Self::Error> {
        if matches!(w.freshness, Freshness::Immutable { .. })
            != (w.change_signal == ChangeSignal::Immutable)
        {
            return Err(KnowledgeError(
                "immutable freshness and change signal must agree",
            ));
        }
        Ok(Self(w))
    }
}
impl From<CollectionDescriptor> for CollectionWire {
    fn from(v: CollectionDescriptor) -> Self {
        v.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ReadPolicy {
    /// Any reader admitted to the collection in the record's tenant.
    Tenant {},
    /// Only the owner and explicit grant subjects.
    Subjects {},
    /// Work Context members, the owner, or explicit grant subjects.
    WorkContext {},
    /// Membership grants read only in the caller's selected Work Context.
    /// Owner and live subject grants independently permit read.
    SelectedWorkContext {},
    /// Owner/grant access additionally requires the active Work Context and,
    /// when recorded, the same gateway profile as the source operation.
    SubjectsInContext {
        #[serde(skip_serializing_if = "Option::is_none")]
        profile: Option<GatewayProfileId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadGrant {
    pub subject: AccessSubject,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

impl ReadGrant {
    pub fn new(subject: AccessSubject) -> Self {
        Self {
            subject,
            expires_at: None,
        }
    }
    pub fn until(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessDescriptor {
    pub tenant: TenantId,
    pub work_context: WorkContextId,
    pub read_policy: ReadPolicy,
    pub owner: AccessSubject,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grants: Vec<ReadGrant>,
    pub data_labels: Vec<DataLabelId>,
    /// Deadline for every read path, including tenant, context and owner access.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Attribution uses the platform principal namespace for both humans and services.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum ModifiedBy {
    Principal(PrincipalId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExternalRecord {
    pub system: ExternalSystemId,
    pub native_id: ExternalRecordId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<HttpsUrl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirrored_at: Option<DateTime<Utc>>,
}

fn hex_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
}
fn is_false(v: &bool) -> bool {
    !v
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ObservationWire {
    collection: CollectionId,
    revision: Revision,
    #[serde(with = "veoveo_types::sha256_hex")]
    #[schemars(schema_with = "hex_schema")]
    content_sha256: Sha256Digest,
    observed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    modified_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    modified_by: Option<ModifiedBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    access: Option<AccessDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    external: Option<ExternalRecord>,
    #[serde(default, skip_serializing_if = "is_false")]
    not_modified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "ObservationWire", into = "ObservationWire")]
pub struct Observation(ObservationWire);
impl Observation {
    pub fn builder(
        collection: CollectionId,
        revision: Revision,
        content_sha256: Sha256Digest,
        observed_at: DateTime<Utc>,
    ) -> ObservationBuilder {
        ObservationBuilder(ObservationWire {
            collection,
            revision,
            content_sha256,
            observed_at,
            modified_at: None,
            modified_by: None,
            access: None,
            external: None,
            not_modified: false,
        })
    }
    pub fn collection(&self) -> &CollectionId {
        &self.0.collection
    }
    pub fn revision(&self) -> &Revision {
        &self.0.revision
    }
    pub fn content_sha256(&self) -> &Sha256Digest {
        &self.0.content_sha256
    }
    pub fn observed_at(&self) -> DateTime<Utc> {
        self.0.observed_at
    }
    pub fn modified_at(&self) -> Option<DateTime<Utc>> {
        self.0.modified_at
    }
    pub fn modified_by(&self) -> Option<&ModifiedBy> {
        self.0.modified_by.as_ref()
    }
    pub fn external(&self) -> Option<&ExternalRecord> {
        self.0.external.as_ref()
    }
    pub fn access(&self) -> Option<&AccessDescriptor> {
        self.0.access.as_ref()
    }
    pub fn not_modified(&self) -> bool {
        self.0.not_modified
    }
    #[cfg(feature = "mcp")]
    pub(crate) fn set_not_modified(&mut self, value: bool) {
        self.0.not_modified = value;
    }
    pub fn validate_collection(
        &self,
        collection: &CollectionDescriptor,
    ) -> Result<(), KnowledgeError> {
        if self.collection() != collection.collection() {
            return Err(KnowledgeError("observation collection mismatch"));
        }
        if (collection.access() == AccessModel::WorkContext) != self.access().is_some() {
            return Err(KnowledgeError(
                "observation access descriptor disagrees with collection",
            ));
        }
        Ok(())
    }
}
impl TryFrom<ObservationWire> for Observation {
    type Error = KnowledgeError;
    fn try_from(w: ObservationWire) -> Result<Self, Self::Error> {
        Ok(Self(w))
    }
}
impl From<Observation> for ObservationWire {
    fn from(v: Observation) -> Self {
        v.0
    }
}

pub struct ObservationBuilder(ObservationWire);
impl ObservationBuilder {
    pub fn modified_at(mut self, value: DateTime<Utc>) -> Self {
        self.0.modified_at = Some(value);
        self
    }
    pub fn modified_by(mut self, value: ModifiedBy) -> Self {
        self.0.modified_by = Some(value);
        self
    }
    pub fn access(mut self, value: AccessDescriptor) -> Self {
        self.0.access = Some(value);
        self
    }
    pub fn external(mut self, value: ExternalRecord) -> Self {
        self.0.external = Some(value);
        self
    }
    pub fn build(self, collection: &CollectionDescriptor) -> Result<Observation, KnowledgeError> {
        let observation: Observation = self.0.try_into()?;
        observation.validate_collection(collection)?;
        Ok(observation)
    }
}

pub fn content_digest(text: &str) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(text.as_bytes()).into())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadCondition {
    pub if_none_match: Revision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SearchRole {
    Search,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "SearchWire", into = "SearchWire")]
pub struct SearchDeclaration(SearchWire);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchWire {
    role: SearchRole,
    collections: Vec<CollectionId>,
}
impl SearchDeclaration {
    pub fn new(collections: Vec<CollectionId>) -> Result<Self, KnowledgeError> {
        SearchWire {
            role: SearchRole::Search,
            collections,
        }
        .try_into()
    }
    pub fn collections(&self) -> &[CollectionId] {
        &self.0.collections
    }
}
impl TryFrom<SearchWire> for SearchDeclaration {
    type Error = KnowledgeError;
    fn try_from(w: SearchWire) -> Result<Self, Self::Error> {
        let unique: std::collections::BTreeSet<_> = w.collections.iter().collect();
        if unique.is_empty() || unique.len() != w.collections.len() {
            return Err(KnowledgeError(
                "search collections must be nonempty and unique",
            ));
        }
        Ok(Self(w))
    }
}
impl From<SearchDeclaration> for SearchWire {
    fn from(v: SearchDeclaration) -> Self {
        v.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "SearchHitWire", into = "SearchHitWire")]
pub struct SearchHit(SearchHitWire);
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchHitWire {
    uri: ResourceUri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 320))]
    snippet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    score: Option<f64>,
}
impl SearchHit {
    pub fn new(
        uri: ResourceUri,
        title: Option<String>,
        snippet: Option<String>,
        score: Option<f64>,
    ) -> Result<Self, KnowledgeError> {
        SearchHitWire {
            uri,
            title,
            snippet,
            score,
        }
        .try_into()
    }
    pub fn uri(&self) -> &ResourceUri {
        &self.0.uri
    }
    pub fn title(&self) -> Option<&str> {
        self.0.title.as_deref()
    }
    pub fn snippet(&self) -> Option<&str> {
        self.0.snippet.as_deref()
    }
}
impl TryFrom<SearchHitWire> for SearchHit {
    type Error = KnowledgeError;
    fn try_from(w: SearchHitWire) -> Result<Self, Self::Error> {
        if w.snippet.as_ref().is_some_and(|s| s.chars().count() > 320)
            || w.score.is_some_and(|v| !v.is_finite())
        {
            return Err(KnowledgeError("invalid search snippet or score"));
        }
        Ok(Self(w))
    }
}
impl From<SearchHit> for SearchHitWire {
    fn from(v: SearchHit) -> Self {
        v.0
    }
}

/// A bounded search response shared by domain servers and consumers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "SearchResultsWire", into = "SearchResultsWire")]
pub struct SearchResults(SearchResultsWire);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchResultsWire {
    #[schemars(length(max = 100))]
    results: Vec<SearchHit>,
}

impl SearchResults {
    pub fn new(results: Vec<SearchHit>) -> Result<Self, KnowledgeError> {
        SearchResultsWire { results }.try_into()
    }
    pub fn results(&self) -> &[SearchHit] {
        &self.0.results
    }
}
impl TryFrom<SearchResultsWire> for SearchResults {
    type Error = KnowledgeError;
    fn try_from(value: SearchResultsWire) -> Result<Self, Self::Error> {
        let unique: std::collections::BTreeSet<_> =
            value.results.iter().map(SearchHit::uri).collect();
        if value.results.len() > 100 || unique.len() != value.results.len() {
            return Err(KnowledgeError("search requires at most 100 unique results"));
        }
        Ok(Self(value))
    }
}
impl From<SearchResults> for SearchResultsWire {
    fn from(value: SearchResults) -> Self {
        value.0
    }
}
