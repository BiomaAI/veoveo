//! Checked immutable identities captured for one analysis run, without local paths.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    num::NonZeroU64,
    ops::Deref,
};
use veoveo_recording_contract::{RecordingDatasetId, RecordingId, RecordingLayerId};
use veoveo_types::{Sha256Digest, sha256_hex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordingSourceError;
impl fmt::Display for RecordingSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("recording sources require positive bytes, valid layer facts and distinct source identities")
    }
}
impl std::error::Error for RecordingSourceError {}

macro_rules! checked {
    ($model:ident, $builder:ident) => {
        #[derive(Clone, Debug, Serialize, JsonSchema, Eq, PartialEq)]
        #[serde(transparent)]
        #[schemars(transparent)]
        pub struct $model($builder);
        impl Deref for $model {
            type Target = $builder;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
        impl<'de> Deserialize<'de> for $model {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                <$builder>::deserialize(d)?
                    .build()
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}

/// Construction facts for ordered inputs. Builders preserve order for digest identity.
/// ```compile_fail
/// use veoveo_recording_video::contract::RecordingSourceSnapshot;
/// fn mutate_sources(snapshot: &mut RecordingSourceSnapshot) { snapshot.sources.clear(); }
/// ```
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
#[schemars(
    rename = "RecordingSourceSnapshot",
    description = "Ordered source identities captured for one recording analysis."
)]
pub struct RecordingSourceSnapshotBuilder {
    pub recording_id: RecordingId,
    pub dataset_id: RecordingDatasetId,
    pub captured_at: DateTime<Utc>,
    #[schemars(length(min = 1))]
    pub sources: Vec<RecordingSourceIdentity>,
}
impl RecordingSourceSnapshotBuilder {
    pub fn build(self) -> Result<RecordingSourceSnapshot, RecordingSourceError> {
        let mut layers = BTreeMap::new();
        let mut names = BTreeMap::new();
        let mut identities = BTreeSet::new();
        let mut total = 0_u64;
        if self.sources.is_empty() {
            return Err(RecordingSourceError);
        }
        for source in &self.sources {
            let facts = (&source.layer_name, source.layer_ordinal, source.kind);
            if layers
                .insert(source.layer_id, facts)
                .is_some_and(|previous| previous != facts)
                || names
                    .insert(&source.layer_name, source.layer_id)
                    .is_some_and(|previous| previous != source.layer_id)
                || !identities.insert((source.layer_id, source.part_sequence))
            {
                return Err(RecordingSourceError);
            }
            total = total
                .checked_add(source.byte_len.get())
                .ok_or(RecordingSourceError)?;
        }
        Ok(RecordingSourceSnapshot(self))
    }
}
checked!(RecordingSourceSnapshot, RecordingSourceSnapshotBuilder);
impl RecordingSourceSnapshot {
    /// Hashes the declared wire fields in their established order, excluding local paths.
    pub fn digest_sha256(&self) -> Result<Sha256Digest, serde_json::Error> {
        serde_json::to_vec(self).map(|bytes| Sha256Digest::from_bytes(Sha256::digest(bytes).into()))
    }
}

/// Facts for one complete committed layer or one acknowledged live ingest part.
/// ```compile_fail
/// use veoveo_recording_video::contract::RecordingSourceIdentity;
/// fn change_part(source: &mut RecordingSourceIdentity) { source.part_sequence = None; }
/// ```
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
#[schemars(
    rename = "RecordingSourceIdentity",
    description = "Identity and integrity of one committed layer or acknowledged live part."
)]
pub struct RecordingSourceIdentityBuilder {
    pub layer_id: RecordingLayerId,
    #[schemars(length(min = 1, max = 256))]
    pub layer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 9_223_372_036_854_775_807_u64))]
    pub layer_ordinal: Option<u64>,
    pub kind: RecordingSourceIdentityKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_sequence: Option<u64>,
    pub byte_len: NonZeroU64,
    #[serde(with = "sha256_hex")]
    #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
    pub sha256: Sha256Digest,
}
impl RecordingSourceIdentityBuilder {
    pub fn build(self) -> Result<RecordingSourceIdentity, RecordingSourceError> {
        if self.layer_name.is_empty()
            || self.layer_name.len() > 256
            || self.layer_name.trim() != self.layer_name
            || self.layer_name.chars().any(char::is_control)
            || self.layer_ordinal.is_some_and(|n| n > i64::MAX as u64)
            || self.part_sequence.is_some()
                != (self.kind == RecordingSourceIdentityKind::LiveIngestPart)
        {
            return Err(RecordingSourceError);
        }
        Ok(RecordingSourceIdentity(self))
    }
}
checked!(RecordingSourceIdentity, RecordingSourceIdentityBuilder);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecordingSourceIdentityKind {
    CommittedLayer,
    LiveIngestPart,
}
