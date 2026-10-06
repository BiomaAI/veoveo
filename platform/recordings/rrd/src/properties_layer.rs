//! Deterministic, non-sensitive recording properties layers.

#[cfg(test)]
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::path::Path;

use anyhow::{Context as _, Result, ensure};
use re_chunk::{Chunk, ChunkId, RowId};
use re_log_encoding::{
    ToTransport as _,
    rrd::{CrateVersion, Encoder, EncodingOptions, RawRrdManifest, RrdFooter, RrdManifestBuilder},
};
use re_log_types::{
    ApplicationId, LogMsg, SetStoreInfo, StoreId, StoreInfo, StoreSource, TimePoint,
};
use re_sdk_types::archetypes::TextDocument;
use sha2::{Digest as _, Sha256};
#[cfg(test)]
use veoveo_recording_contract::{RecordingDatasetId, RecordingId};
pub use veoveo_recording_contract::{RecordingProperties, RecordingPropertiesBuilder};

use crate::recording_layer::{CanonicalRecordingLayer, inspect_canonical_recording_layer};

const MAX_PROPERTIES_JSON_BYTES: usize = 64 * 1024;
const PROPERTIES_ENCODING: EncodingOptions = EncodingOptions::PROTOBUF_COMPRESSED;
#[cfg(test)]
const MAX_METADATA_REVISIONS: usize = 64;

pub fn build_properties_layer(
    path: &Path,
    properties: &RecordingProperties,
) -> Result<CanonicalRecordingLayer> {
    build_properties_layer_with_admission(path, properties, |_| Ok(()))
}

/// Admits prepared bytes before reusing or installing the final properties file.
/// The caller must still fence publication against current persisted state.
pub fn build_properties_layer_with_admission(
    path: &Path,
    properties: &RecordingProperties,
    admit: impl FnOnce(&CanonicalRecordingLayer) -> Result<()>,
) -> Result<CanonicalRecordingLayer> {
    let json = serde_json::to_vec(properties)?;
    ensure!(
        json.len() <= MAX_PROPERTIES_JSON_BYTES,
        "recording properties exceed the encoded size limit"
    );
    let store_id = StoreId::recording(
        ApplicationId::try_new(properties.dataset_id.to_string())?,
        properties.recording_id.to_string(),
    );
    let partial = path.with_file_name(format!(
        ".{}.properties.partial",
        path.file_name()
            .and_then(|name| name.to_str())
            .context("properties filename is not UTF-8")?
    ));
    match std::fs::remove_file(&partial) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("removing stale properties partial"),
    }
    let output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&partial)
        .with_context(|| format!("creating recording properties layer {}", path.display()))?;
    let _cleanup = crate::recording_layer::PartialCleanup(partial.clone());
    let mut encoder = Encoder::new_eager(CrateVersion::LOCAL, PROPERTIES_ENCODING, output)?;
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
    finish_properties_chunk(&mut encoder, store_id, chunk)?;
    let output = encoder.into_inner()?;
    output.sync_all()?;
    drop(output);
    let expected = inspect_canonical_recording_layer(
        &partial,
        properties.dataset_id,
        properties.recording_id,
    )?;
    admit(&expected)?;
    if path.exists() {
        let retained = inspect_canonical_recording_layer(
            path,
            properties.dataset_id,
            properties.recording_id,
        )?;
        ensure!(
            retained == expected,
            "retained properties bytes differ from admitted properties"
        );
        return Ok(retained);
    }
    std::fs::rename(&partial, path).context("installing admitted properties layer")?;
    sync_parent(path)?;
    Ok(expected)
}

/// Writes the only data chunk and its complete deterministic footer.
/// Store-info has already been encoded with the same global options.
fn finish_properties_chunk(
    encoder: &mut Encoder<File>,
    store_id: StoreId,
    chunk: Chunk,
) -> Result<()> {
    let batch = chunk.to_chunk_batch()?;
    let message = LogMsg::ArrowMsg(store_id.clone(), chunk.to_arrow_msg()?);
    let transport = message.to_transport(PROPERTIES_ENCODING.compression)?;
    // SAFETY: This private writer creates the transport from its admitted owned chunk
    // with exactly the compression used to construct this encoder. The returned span
    // and uncompressed length describe the bytes actually written, not guessed offsets.
    let (span, uncompressed_len) = unsafe { encoder.append_transport_without_footer(&transport)? };
    let mut builder = RrdManifestBuilder::default();
    builder.append(&batch, span, uncompressed_len)?;
    let mut manifest = builder.build(store_id.clone())?;
    let digest = manifest.sorbet_schema_sha256;
    let metadata = manifest.sorbet_schema.metadata().clone();
    let mut fields = manifest.sorbet_schema.fields().to_vec();
    fields.sort();
    manifest.sorbet_schema =
        re_chunk::external::arrow::datatypes::Schema::new_with_metadata(fields, metadata);
    ensure!(
        RawRrdManifest::compute_sorbet_schema_sha256(&manifest.sorbet_schema)? == digest,
        "deterministic properties footer changed its schema digest",
    );
    manifest.sanity_check_heavy()?;
    let footer = RrdFooter {
        manifests: std::collections::HashMap::from([(store_id, manifest)]),
    };
    // SAFETY: The built-in footer was disabled by the transport append above. This
    // singleton footer was built from the exact owned chunk and recorded byte span/
    // uncompressed length; its unchanged schema digest and complete manifest passed
    // the maintained heavy validator. No offsets, chunk data or identity are changed.
    unsafe { encoder.finish_with_custom_footer(&footer)? };
    Ok(())
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
        RecordingPropertiesBuilder {
            dataset_id: RecordingDatasetId::new(),
            recording_id: RecordingId::new(),
            dataset_key: "world".to_owned(),
            producer_recording_key: "flight-a".to_owned(),
            lifecycle_state: veoveo_recording_contract::RecordingState::Sealed,
            started_at: "2026-08-26T01:00:00Z".to_owned(),
            ended_at: "2026-08-26T01:05:00Z".to_owned(),
            sealed_at: "2026-08-26T01:06:00Z".to_owned(),
            source_revision: 7,
            immutable_manifest_digest: veoveo_types::Sha256Digest::from_hex("ab".repeat(32))
                .unwrap(),
            model_revisions: BTreeMap::from([("detector".to_owned(), "sha256:model".to_owned())]),
            environment_revisions: BTreeMap::new(),
        }
        .build()
        .unwrap()
    }

    fn verify_properties_footer(bytes: &[u8], properties: &RecordingProperties) {
        use re_log_encoding::rrd::{Decoder, Encodable as _, MessageHeader};
        let manifests = RawRrdManifest::from_rrd_bytes(bytes).unwrap();
        assert_eq!(
            manifests.len(),
            1,
            "properties require a complete singleton footer"
        );
        let manifest = &manifests[0];
        manifest.sanity_check_heavy().unwrap();
        assert_eq!(
            manifest.store_id,
            StoreId::recording(
                ApplicationId::try_new(properties.dataset_id.to_string()).unwrap(),
                properties.recording_id.to_string(),
            ),
        );
        assert!(manifest.sorbet_schema.fields().is_sorted());
        assert_eq!(
            manifest.sorbet_schema_sha256,
            RawRrdManifest::compute_sorbet_schema_sha256(&manifest.sorbet_schema).unwrap(),
        );
        let messages = Decoder::<LogMsg>::decode_eager(std::io::Cursor::new(bytes))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(messages.len(), 2);
        let LogMsg::ArrowMsg(store_id, arrow) = &messages[1] else {
            panic!("properties data message must be an Arrow chunk");
        };
        assert_eq!(store_id, &manifest.store_id);
        let chunk = Chunk::from_chunk_record_batch(&arrow.batch).unwrap();
        assert_eq!(
            manifest.col_chunk_id_iter().unwrap().collect::<Vec<_>>(),
            vec![chunk.id()],
        );
        let offset = manifest
            .col_chunk_byte_offset_iter()
            .unwrap()
            .next()
            .unwrap();
        let size = manifest.col_chunk_byte_size_iter().unwrap().next().unwrap();
        let transport = messages[1]
            .to_transport(PROPERTIES_ENCODING.compression)
            .unwrap();
        let mut encoded = Vec::new();
        transport.to_rrd_bytes(&mut encoded).unwrap();
        let payload = &encoded[MessageHeader::ENCODED_SIZE_BYTES..];
        assert_eq!(size as usize, payload.len());
        assert!(
            &bytes[offset as usize..(offset + size) as usize] == payload,
            "manifest span must locate the actual encoded owned chunk",
        );
        assert_eq!(
            manifest
                .col_chunk_byte_size_uncompressed_iter()
                .unwrap()
                .collect::<Vec<_>>(),
            vec![transport.byte_size_uncompressed()],
        );
    }

    #[test]
    fn properties_retry_reuses_stable_canonical_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let properties = properties();
        let first = directory.path().join("first.rrd");
        let first_result = build_properties_layer(&first, &properties).unwrap();
        let bytes = std::fs::read(&first).unwrap();
        verify_properties_footer(&bytes, &properties);
        // Each independent build gets fresh HashMap seeds in the upstream manifest
        // builder. The entire footer and messages must still produce identical bytes.
        for iteration in 0..16 {
            let prepared = directory.path().join(format!("prepared-{iteration}.rrd"));
            let prepared_result = build_properties_layer(&prepared, &properties).unwrap();
            let prepared_bytes = std::fs::read(&prepared).unwrap();
            let first_difference = bytes
                .iter()
                .zip(&prepared_bytes)
                .position(|(retained, prepared)| retained != prepared);
            assert_eq!(
                first_result,
                prepared_result,
                "prepared properties differ on build {iteration}: first byte mismatch {first_difference:?}; retained length {}, prepared length {}",
                bytes.len(),
                prepared_bytes.len(),
            );
            assert!(
                bytes == prepared_bytes,
                "properties byte mismatch at {first_difference:?}"
            );
            verify_properties_footer(&prepared_bytes, &properties);
        }
        let second_result = build_properties_layer(&first, &properties).unwrap();
        assert_eq!(first_result, second_result);
        assert!(
            bytes == std::fs::read(first).unwrap(),
            "retry must not rewrite retained bytes"
        );
    }

    #[test]
    fn preinstall_refusal_leaves_missing_destination_and_removes_partial() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("properties.rrd");
        let properties = properties();
        assert!(
            build_properties_layer_with_admission(&path, &properties, |expected| {
                assert!(expected.byte_len > 0);
                anyhow::bail!("fixture stage contradiction")
            })
            .is_err()
        );
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        let admitted =
            build_properties_layer_with_admission(&path, &properties, |_| Ok(())).unwrap();
        assert_eq!(
            admitted,
            inspect_canonical_recording_layer(
                &path,
                properties.dataset_id,
                properties.recording_id
            )
            .unwrap()
        );
    }

    #[test]
    fn properties_reject_unbounded_revision_maps() {
        let mut value = serde_json::to_value(properties()).unwrap();
        value["model_revisions"] = serde_json::json!(
            (0..=MAX_METADATA_REVISIONS)
                .map(|i| (format!("model-{i}"), "revision".to_owned()))
                .collect::<BTreeMap<_, _>>()
        );
        assert!(serde_json::from_value::<RecordingProperties>(value).is_err());
    }
    #[test]
    fn retained_properties_must_equal_admitted_properties_without_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("properties.rrd");
        let properties = properties();
        build_properties_layer(&path, &properties).unwrap();
        let original = std::fs::read(&path).unwrap();
        let mut changed = serde_json::to_value(properties).unwrap();
        changed["producer_recording_key"] = serde_json::json!("other-open-producer-name");
        let changed: RecordingProperties = serde_json::from_value(changed).unwrap();
        assert!(build_properties_layer(&path, &changed).is_err());
        assert_eq!(original, std::fs::read(&path).unwrap());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
