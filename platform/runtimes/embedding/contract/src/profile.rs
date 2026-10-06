//! Installation-admitted execution provenance and directional reuse qualification.
use crate::{EmbeddingError, EmbeddingPrecision, EmbeddingSpace};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_types::{Check, Checked, Sha256Digest};

#[veoveo_types::id(prefixed(EmbeddingDigestIds, "sha256:"))]
pub struct EmbeddingExecutionProfileId(String);
#[veoveo_types::id(prefixed(EmbeddingDigestIds, "sha256:"))]
pub struct EmbeddingQualificationId(String);
#[doc(hidden)]
pub struct EmbeddingDigestIds;
impl veoveo_types::IdProfile for EmbeddingDigestIds {
    type Error = EmbeddingError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> = veoveo_types::IdProfileSpec::hex(
        veoveo_types::HexGrammar {
            length: 64,
            case: veoveo_types::HexCase::Lower,
            nonzero: false,
        },
        |_, _, _| EmbeddingError("expected sha256: and 64 lowercase hexadecimal digits"),
    );
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct EmbeddingRuntimeVersion(String);
impl EmbeddingRuntimeVersion {
    pub fn new(value: String) -> Result<Self, EmbeddingError> {
        crate::validate_embedding_id(&value)?;
        Ok(Self(value))
    }
}
impl TryFrom<String> for EmbeddingRuntimeVersion {
    type Error = EmbeddingError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<EmbeddingRuntimeVersion> for String {
    fn from(value: EmbeddingRuntimeVersion) -> Self {
        value.0
    }
}
impl AsRef<str> for EmbeddingRuntimeVersion {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct EmbeddingGpuName(String);
impl EmbeddingGpuName {
    pub fn new(value: String) -> Result<Self, EmbeddingError> {
        crate::validate_embedding_id(&value)?;
        if !value.starts_with("NVIDIA ") {
            return Err(EmbeddingError(
                "only measured NVIDIA execution profiles are supported",
            ));
        }
        Ok(Self(value))
    }
}
impl TryFrom<String> for EmbeddingGpuName {
    type Error = EmbeddingError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<EmbeddingGpuName> for String {
    fn from(value: EmbeddingGpuName) -> Self {
        value.0
    }
}
impl AsRef<str> for EmbeddingGpuName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddingNvidiaEnvironment {
    pub gpu: EmbeddingGpuName,
    pub driver: EmbeddingRuntimeVersion,
    pub cuda: EmbeddingRuntimeVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum EmbeddingScheduling {
    #[vocabulary(rename = "priority")]
    Priority,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum EmbeddingGraphAllowance {
    #[vocabulary(rename = "graphs_allowed")]
    GraphsAllowed,
    #[vocabulary(rename = "enforce_eager")]
    EnforceEager,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum EmbeddingGraphExecution {
    #[vocabulary(rename = "eager")]
    Eager,
    #[vocabulary(rename = "cuda_graph")]
    CudaGraph,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct EmbeddingAttentionBackend(String);
impl EmbeddingAttentionBackend {
    pub fn new(value: String) -> Result<Self, EmbeddingError> {
        crate::validate_embedding_id(&value)?;
        Ok(Self(value))
    }
}
impl TryFrom<String> for EmbeddingAttentionBackend {
    type Error = EmbeddingError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<EmbeddingAttentionBackend> for String {
    fn from(value: EmbeddingAttentionBackend) -> Self {
        value.0
    }
}
impl AsRef<str> for EmbeddingAttentionBackend {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct EmbeddingKvCacheBytes(u64);
impl EmbeddingKvCacheBytes {
    pub const MAX: u64 = i64::MAX as u64;
    pub fn new(value: u64) -> Result<Self, EmbeddingError> {
        if value == 0 || value > Self::MAX {
            return Err(EmbeddingError(
                "explicit KV cache reservation must fit 1..=i64::MAX bytes",
            ));
        }
        Ok(Self(value))
    }
    pub fn get(self) -> u64 {
        self.0
    }
}
impl TryFrom<u64> for EmbeddingKvCacheBytes {
    type Error = EmbeddingError;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<EmbeddingKvCacheBytes> for u64 {
    fn from(value: EmbeddingKvCacheBytes) -> Self {
        value.0
    }
}
impl JsonSchema for EmbeddingKvCacheBytes {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EmbeddingKvCacheBytes".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = u64::json_schema(generator);
        schema.insert("minimum".into(), 1.into());
        schema.insert("maximum".into(), Self::MAX.into());
        schema
    }
}
fn required_cache<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Option<EmbeddingKvCacheBytes>, D::Error> {
    Option::<EmbeddingKvCacheBytes>::deserialize(decoder)
}
fn nullable_cache_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let mut schema = EmbeddingKvCacheBytes::json_schema(generator);
    schema.insert("type".into(), serde_json::json!(["integer", "null"]));
    schema
}

/// Effective values observed and qualified for the serving process, without auto settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddingServingConfiguration {
    pub precision: EmbeddingPrecision,
    pub scheduling: EmbeddingScheduling,
    pub graph_allowance: EmbeddingGraphAllowance,
    pub observed_graph_execution: EmbeddingGraphExecution,
    pub attention_backend: EmbeddingAttentionBackend,
    #[serde(deserialize_with = "required_cache")]
    #[schemars(required, schema_with = "nullable_cache_schema")]
    pub explicit_kv_cache_bytes: Option<EmbeddingKvCacheBytes>,
    pub max_input_tokens: crate::EmbeddingMaxInputTokens,
    #[schemars(range(min = 1, max = 1_048_576))]
    pub max_num_batched_tokens: u32,
    #[schemars(range(min = 1, max = 1024))]
    pub max_num_sequences: u16,
    #[schemars(range(min = 1, max = 10_000))]
    pub gpu_memory_basis_points: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddingExecutionContents {
    pub space: EmbeddingSpace,
    pub runtime_image: Sha256Digest,
    pub checkpoint_manifest: Sha256Digest,
    pub vllm_version: EmbeddingRuntimeVersion,
    pub environment: EmbeddingNvidiaEnvironment,
    pub serving: EmbeddingServingConfiguration,
}
impl Check for EmbeddingExecutionContents {
    type Error = EmbeddingError;
    fn check(&self) -> Result<(), Self::Error> {
        if (self.serving.graph_allowance == EmbeddingGraphAllowance::EnforceEager
            && self.serving.observed_graph_execution != EmbeddingGraphExecution::Eager)
            || !self.environment.gpu.as_ref().starts_with("NVIDIA ")
            || self.serving.precision != self.space.precision
            || self.serving.max_input_tokens != self.space.max_input_tokens
            || !(1..=1_048_576).contains(&self.serving.max_num_batched_tokens)
            || !(1..=1024).contains(&self.serving.max_num_sequences)
            || !(1..=10_000).contains(&self.serving.gpu_memory_basis_points)
        {
            return Err(EmbeddingError(
                "execution profile requires effective NVIDIA settings matching its space",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct EmbeddingExecutionProfile(Checked<ProfileWire>);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfileWire {
    id: EmbeddingExecutionProfileId,
    contents: EmbeddingExecutionContents,
}
impl EmbeddingExecutionProfile {
    pub fn new(contents: EmbeddingExecutionContents) -> Result<Self, EmbeddingError> {
        let id = profile_id(&contents);
        Checked::new(ProfileWire { id, contents }).map(Self)
    }
    pub fn id(&self) -> &EmbeddingExecutionProfileId {
        &self.0.id
    }
    pub fn contents(&self) -> &EmbeddingExecutionContents {
        &self.0.contents
    }
    pub fn space(&self) -> &EmbeddingSpace {
        &self.0.contents.space
    }
}
impl Check for ProfileWire {
    type Error = EmbeddingError;
    fn check(&self) -> Result<(), Self::Error> {
        self.contents.check()?;
        if self.id != profile_id(&self.contents) {
            return Err(EmbeddingError(
                "execution profile identity does not match immutable contents",
            ));
        }
        Ok(())
    }
}
pub(crate) fn digest<T: Serialize>(domain: &[u8], value: &T) -> Sha256Digest {
    let bytes = serde_json::to_vec(value).expect("closed embedding facts serialize");
    let mut hash = Sha256::new();
    hash.update((domain.len() as u64).to_be_bytes());
    hash.update(domain);
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
    Sha256Digest::from_bytes(hash.finalize().into())
}
fn profile_id(contents: &EmbeddingExecutionContents) -> EmbeddingExecutionProfileId {
    digest(b"veoveo.ai/embedding-execution-profile/v1", contents)
        .to_string()
        .parse()
        .expect("SHA-256 profile identity")
}

/// Report digests bind measured results. These facts are trusted installation input,
/// not proof derived from model discovery or successful transport requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddingQualificationEvidence {
    pub reference_report: Sha256Digest,
    pub retrieval_report: Sha256Digest,
    pub scheduling_report: Sha256Digest,
    pub capacity_report: Sha256Digest,
    #[schemars(range(min = 999_000, max = 1_000_000))]
    pub minimum_cosine_millionths: u32,
    pub retrieval_passed: bool,
    pub interactive_priority_passed: bool,
    pub capacity_passed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct EmbeddingQualification(Checked<QualificationWire>);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QualificationWire {
    id: EmbeddingQualificationId,
    contents: QualificationContents,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QualificationContents {
    query_profile: EmbeddingExecutionProfileId,
    producer_profile: EmbeddingExecutionProfileId,
    space: EmbeddingSpace,
    evidence: EmbeddingQualificationEvidence,
}
impl EmbeddingQualification {
    pub fn new(
        query: &EmbeddingExecutionProfile,
        producer: &EmbeddingExecutionProfile,
        evidence: EmbeddingQualificationEvidence,
    ) -> Result<Self, EmbeddingError> {
        if query.space() != producer.space() {
            return Err(EmbeddingError(
                "qualification cannot join different vector spaces",
            ));
        }
        let contents = QualificationContents {
            query_profile: query.id().clone(),
            producer_profile: producer.id().clone(),
            space: query.space().clone(),
            evidence,
        };
        let id = qualification_id(&contents);
        Checked::new(QualificationWire { id, contents }).map(Self)
    }
    pub fn id(&self) -> &EmbeddingQualificationId {
        &self.0.id
    }
    pub fn query_profile(&self) -> &EmbeddingExecutionProfileId {
        &self.0.contents.query_profile
    }
    pub fn producer_profile(&self) -> &EmbeddingExecutionProfileId {
        &self.0.contents.producer_profile
    }
    pub fn space(&self) -> &EmbeddingSpace {
        &self.0.contents.space
    }
    pub fn evidence(&self) -> &EmbeddingQualificationEvidence {
        &self.0.contents.evidence
    }
}
fn qualification_id(contents: &QualificationContents) -> EmbeddingQualificationId {
    digest(b"veoveo.ai/embedding-qualification/v1", contents)
        .to_string()
        .parse()
        .expect("SHA-256 qualification identity")
}
impl Check for QualificationWire {
    type Error = EmbeddingError;
    fn check(&self) -> Result<(), Self::Error> {
        if !(999_000..=1_000_000).contains(&self.contents.evidence.minimum_cosine_millionths)
            || !self.contents.evidence.retrieval_passed
            || !self.contents.evidence.interactive_priority_passed
            || !self.contents.evidence.capacity_passed
            || self.id != qualification_id(&self.contents)
        {
            return Err(EmbeddingError(
                "qualification requires matching immutable contents and passing reference, retrieval, scheduling and capacity reports",
            ));
        }
        Ok(())
    }
}

/// Selected execution and admitted directional compatibility matrix from an installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct QualifiedEmbeddingRuntime(Checked<RuntimeWire>);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeWire {
    effective_profile: EmbeddingExecutionProfile,
    profiles: Vec<EmbeddingExecutionProfile>,
    qualifications: Vec<EmbeddingQualification>,
}
impl QualifiedEmbeddingRuntime {
    pub fn new(
        effective_profile: EmbeddingExecutionProfile,
        profiles: Vec<EmbeddingExecutionProfile>,
        qualifications: Vec<EmbeddingQualification>,
    ) -> Result<Self, EmbeddingError> {
        Checked::new(RuntimeWire {
            effective_profile,
            profiles,
            qualifications,
        })
        .map(Self)
    }
    pub fn profile(&self) -> &EmbeddingExecutionProfile {
        &self.0.effective_profile
    }
    pub fn space(&self) -> &EmbeddingSpace {
        self.profile().space()
    }
    pub fn profiles(&self) -> &[EmbeddingExecutionProfile] {
        &self.0.profiles
    }
    pub fn qualifications(&self) -> &[EmbeddingQualification] {
        &self.0.qualifications
    }
    pub fn qualification_for(
        &self,
        producer: &EmbeddingExecutionProfileId,
    ) -> Result<&EmbeddingQualification, EmbeddingError> {
        self.0
            .qualifications
            .iter()
            .find(|entry| {
                entry.query_profile() == self.profile().id() && entry.producer_profile() == producer
            })
            .ok_or(EmbeddingError(
                "selected query runtime has no qualification for retained producer",
            ))
    }
    pub fn producer(
        &self,
        id: &EmbeddingExecutionProfileId,
    ) -> Result<&EmbeddingExecutionProfile, EmbeddingError> {
        self.0
            .profiles
            .iter()
            .find(|entry| entry.id() == id)
            .ok_or(EmbeddingError(
                "retained producer execution profile is unknown",
            ))
    }
}
impl Check for RuntimeWire {
    type Error = EmbeddingError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.profiles.is_empty()
            || self.profiles.len() > 64
            || self.qualifications.is_empty()
            || self.qualifications.len() > 4096
        {
            return Err(EmbeddingError(
                "runtime bundle requires 1..=64 profiles and 1..=4096 qualifications",
            ));
        }
        for (i, profile) in self.profiles.iter().enumerate() {
            if self.profiles[..i]
                .iter()
                .any(|old| old.id() == profile.id())
            {
                return Err(EmbeddingError(
                    "execution profile identities must be unique",
                ));
            }
        }
        if !self
            .profiles
            .iter()
            .any(|profile| profile == &self.effective_profile)
        {
            return Err(EmbeddingError(
                "effective deployment profile is absent or mismatched",
            ));
        }
        for (i, qualification) in self.qualifications.iter().enumerate() {
            let query = self
                .profiles
                .iter()
                .find(|p| p.id() == qualification.query_profile());
            let producer = self
                .profiles
                .iter()
                .find(|p| p.id() == qualification.producer_profile());
            if query.is_none_or(|p| p.space() != qualification.space())
                || producer.is_none_or(|p| p.space() != qualification.space())
                || self.qualifications[..i].iter().any(|old| {
                    old.query_profile() == qualification.query_profile()
                        && old.producer_profile() == qualification.producer_profile()
                })
            {
                return Err(EmbeddingError(
                    "qualification must name admitted matching profiles once",
                ));
            }
        }
        if !self.qualifications.iter().any(|q| {
            q.query_profile() == self.effective_profile.id()
                && q.producer_profile() == self.effective_profile.id()
        }) {
            return Err(EmbeddingError("selected runtime has no self qualification"));
        }
        Ok(())
    }
}
