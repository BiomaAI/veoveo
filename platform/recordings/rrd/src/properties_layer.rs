//! Deterministic, non-sensitive recording properties layers.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::path::Path;

use anyhow::{Context as _, Result, ensure};
use re_chunk::{Chunk, ChunkId, RowId};
use re_log_encoding::rrd::{CrateVersion, Encoder, EncodingOptions};
use re_log_types::{
    ApplicationId, LogMsg, SetStoreInfo, StoreId, StoreInfo, StoreSource, TimePoint,
};
use re_sdk_types::archetypes::TextDocument;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::recording_layer::{CanonicalRecordingLayer, inspect_canonical_recording_layer};

const MAX_PROPERTIES_JSON_BYTES: usize = 64 * 1024;
const MAX_METADATA_REVISIONS: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordingProperties {
    pub dataset_id: Uuid,
    pub recording_id: Uuid,
    pub dataset_key: String,
    pub producer_recording_key: String,
    pub lifecycle_state: String,
    pub started_at: String,
    pub ended_at: String,
    pub sealed_at: String,
    pub source_revision: i64,
    pub immutable_manifest_digest: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_revisions: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment_revisions: BTreeMap<String, String>,
}

pub fn build_properties_layer(
    path: &Path,
    properties: &RecordingProperties,
) -> Result<CanonicalRecordingLayer> {
    validate(properties)?;
    if path.exists() {
        return inspect_canonical_recording_layer(
            path,
            properties.dataset_id,
            properties.recording_id,
        );
    }
    let json = serde_json::to_vec(properties)?;
    ensure!(
        json.len() <= MAX_PROPERTIES_JSON_BYTES,
        "recording properties exceed the encoded size limit"
    );
    let store_id = StoreId::recording(
        ApplicationId::try_new(properties.dataset_id.to_string())?,
        properties.recording_id.to_string(),
    );
    let output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .with_context(|| format!("creating recording properties layer {}", path.display()))?;
    let mut encoder = Encoder::new_eager(
        CrateVersion::LOCAL,
        EncodingOptions::PROTOBUF_COMPRESSED,
        output,
    )?;
    let identity = hex::encode(Sha256::digest(&json));
    let store_info_row = deterministic_id::<RowId>(&identity, "store-info")?;
    encoder.append(
        &SetStoreInfo {
            row_id: *store_info_row,
            info: StoreInfo::new_unversioned(
                store_id.clone(),
                StoreSource::Other("veoveo-recording-properties".to_owned()),
            ),
        }
        .into(),
    )?;
    let chunk_id = deterministic_id::<ChunkId>(&identity, "properties-chunk")?;
    let row_id = deterministic_id::<RowId>(&identity, "properties-row")?;
    let document = TextDocument::new(String::from_utf8(json)?).with_media_type("application/json");
    let chunk = Chunk::builder_with_id(chunk_id, "/veoveo/recording/properties")
        .with_archetype(row_id, TimePoint::STATIC, &document)
        .build()?;
    encoder.append(&LogMsg::ArrowMsg(store_id, chunk.to_arrow_msg()?))?;
    encoder.finish()?;
    let output = encoder.into_inner()?;
    output.sync_all()?;
    drop(output);
    sync_parent(path)?;
    inspect_canonical_recording_layer(path, properties.dataset_id, properties.recording_id)
}

fn validate(properties: &RecordingProperties) -> Result<()> {
    ensure!(
        properties.dataset_id.get_version_num() == 7,
        "properties dataset identity must be UUIDv7"
    );
    ensure!(
        properties.recording_id.get_version_num() == 7,
        "properties recording identity must be UUIDv7"
    );
    for (name, value) in [
        ("dataset_key", properties.dataset_key.as_str()),
        (
            "producer_recording_key",
            properties.producer_recording_key.as_str(),
        ),
        ("lifecycle_state", properties.lifecycle_state.as_str()),
        ("started_at", properties.started_at.as_str()),
        ("ended_at", properties.ended_at.as_str()),
        ("sealed_at", properties.sealed_at.as_str()),
    ] {
        ensure!(
            !value.trim().is_empty() && value.len() <= 512,
            "recording property {name} must be 1..=512 characters"
        );
    }
    ensure!(
        valid_sha256(&properties.immutable_manifest_digest),
        "immutable manifest digest must be lowercase SHA-256"
    );
    validate_revisions("model_revisions", &properties.model_revisions)?;
    validate_revisions("environment_revisions", &properties.environment_revisions)?;
    Ok(())
}

fn validate_revisions(name: &str, values: &BTreeMap<String, String>) -> Result<()> {
    ensure!(
        values.len() <= MAX_METADATA_REVISIONS,
        "{name} exceeds its entry limit"
    );
    for (key, value) in values {
        ensure!(
            !key.trim().is_empty()
                && key.len() <= 128
                && !value.trim().is_empty()
                && value.len() <= 256,
            "{name} keys and values must be bounded nonempty strings"
        );
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn deterministic_id<T>(identity: &str, kind: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let digest = Sha256::digest(format!("{identity}:{kind}"));
    hex::encode(&digest[..16])
        .parse::<T>()
        .map_err(|error| anyhow::anyhow!("invalid deterministic Rerun {kind} id: {error}"))
}

fn sync_parent(path: &Path) -> Result<()> {
    File::open(path.parent().context("properties layer has no parent")?)?
        .sync_all()
        .context("syncing properties layer directory")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn properties() -> RecordingProperties {
        RecordingProperties {
            dataset_id: Uuid::now_v7(),
            recording_id: Uuid::now_v7(),
            dataset_key: "world".to_owned(),
            producer_recording_key: "flight-a".to_owned(),
            lifecycle_state: "sealed".to_owned(),
            started_at: "2026-08-26T01:00:00Z".to_owned(),
            ended_at: "2026-08-26T01:05:00Z".to_owned(),
            sealed_at: "2026-08-26T01:06:00Z".to_owned(),
            source_revision: 7,
            immutable_manifest_digest: "ab".repeat(32),
            model_revisions: BTreeMap::from([("detector".to_owned(), "sha256:model".to_owned())]),
            environment_revisions: BTreeMap::new(),
        }
    }

    #[test]
    fn properties_retry_reuses_stable_canonical_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let properties = properties();
        let first = directory.path().join("first.rrd");
        let first_result = build_properties_layer(&first, &properties).unwrap();
        let bytes = std::fs::read(&first).unwrap();
        let second_result = build_properties_layer(&first, &properties).unwrap();
        assert_eq!(first_result, second_result);
        assert_eq!(bytes, std::fs::read(first).unwrap());
    }

    #[test]
    fn properties_reject_unbounded_revision_maps() {
        let mut properties = properties();
        properties.model_revisions = (0..=MAX_METADATA_REVISIONS)
            .map(|index| (format!("model-{index}"), "revision".to_owned()))
            .collect();
        assert!(
            build_properties_layer(
                &tempfile::tempdir().unwrap().path().join("properties.rrd"),
                &properties,
            )
            .is_err()
        );
    }
}
