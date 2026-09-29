//! Deterministic, bounded Apache Arrow projection over canonical RRD layers.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Context as _, Result, ensure};
use re_chunk_store::{ChunkStore, ChunkStoreConfig, ChunkStoreHandle};
use re_dataframe::{
    AbsoluteTimeRange, QueryEngine, QueryExpression, SparseFillStrategy, TimeInt, TimelineName,
};
use re_log_types::EntityPath;
use re_sdk_types::external::arrow;
use re_types_core::ComponentIdentifier;
use sha2::{Digest as _, Sha256};

use veoveo_recording_contract::{
    RecordingProjectionQuery, RecordingProjectionSampling, RecordingProjectionSparseFill,
};

/// A bounded query whose selectors have been parsed by the pinned Rerun implementation.
/// Construction performs no file access. Callers prepare this before source materialization.
#[derive(Clone, Debug)]
pub struct ArrowProjectionQuery {
    query: RecordingProjectionQuery,
    expression: QueryExpression,
}

impl ArrowProjectionQuery {
    pub fn new(query: RecordingProjectionQuery) -> Result<Self> {
        let expression = query_expression(&query)?;
        Ok(Self { query, expression })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArrowProjectionSummary {
    pub row_count: u64,
    pub omitted_sample_count: u64,
    pub schema_sha256: String,
    pub byte_len: u64,
    pub sha256: String,
}

pub fn write_arrow_projection(
    layer_paths: &[PathBuf],
    query: &ArrowProjectionQuery,
    output: &Path,
) -> Result<ArrowProjectionSummary> {
    write_arrow_projection_cancelable(layer_paths, query, output, Arc::new(AtomicBool::new(false)))
}

pub fn write_arrow_projection_cancelable(
    layer_paths: &[PathBuf],
    query: &ArrowProjectionQuery,
    output: &Path,
    cancelled: Arc<AtomicBool>,
) -> Result<ArrowProjectionSummary> {
    ensure!(
        !layer_paths.is_empty(),
        "projection has no immutable RRD layers"
    );
    ensure!(!output.exists(), "projection output already exists");
    let result = write_arrow_projection_inner(layer_paths, query, output, cancelled);
    if result.is_err() {
        match std::fs::remove_file(output) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("removing failed Arrow projection"),
        }
    }
    result
}

fn write_arrow_projection_inner(
    layer_paths: &[PathBuf],
    query: &ArrowProjectionQuery,
    output: &Path,
    cancelled: Arc<AtomicBool>,
) -> Result<ArrowProjectionSummary> {
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "Arrow projection was cancelled"
    );
    let engine = QueryEngine::from_store(combined_chunk_store(layer_paths, &cancelled)?);
    let mut handle = engine.query(query.expression.clone());
    let query = &query.query;
    let schema = handle.schema().clone();
    let schema_sha256 = canonical_schema_sha256(schema.as_ref());
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output)
        .with_context(|| format!("creating Arrow projection {}", output.display()))?;
    let bounded = BoundedHashWriter::new(file, query.maximum_bytes, cancelled.clone());
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(bounded, schema.as_ref())
        .context("writing Arrow projection schema")?;
    let mut row_count = 0_u64;
    for batch in handle.batch_iter() {
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Arrow projection was cancelled"
        );
        row_count = row_count
            .checked_add(u64::try_from(batch.num_rows())?)
            .context("Arrow projection row count overflow")?;
        ensure!(
            row_count <= query.maximum_rows,
            "Arrow projection exceeds maximum_rows"
        );
        ensure!(
            row_count <= u64::try_from(query.maximum_samples)?,
            "Arrow projection exceeds maximum_samples"
        );
        for column in batch.columns() {
            ensure_finite(column.as_ref())?;
        }
        writer
            .write(&batch)
            .context("writing Arrow projection record batch")?;
    }
    writer
        .finish()
        .context("finishing Arrow projection stream")?;
    let bounded = writer
        .into_inner()
        .context("closing Arrow projection stream")?;
    bounded.file.sync_all()?;
    let byte_len = bounded.written;
    let sha256 = hex::encode(bounded.digest.finalize());
    let requested_samples = match &query.sampling {
        RecordingProjectionSampling::LatestAt { .. } => 1,
        RecordingProjectionSampling::SampleGrid { values } => u64::try_from(values.len())?,
        RecordingProjectionSampling::Range { .. } => 0,
    };
    Ok(ArrowProjectionSummary {
        row_count,
        omitted_sample_count: requested_samples.saturating_sub(row_count),
        schema_sha256,
        byte_len,
        sha256,
    })
}

fn query_expression(query: &RecordingProjectionQuery) -> Result<QueryExpression> {
    let components = query
        .component_ids
        .iter()
        .map(|value| {
            ComponentIdentifier::try_new(value.clone())
                .context("invalid projection component identifier")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let view_contents = query
        .entity_paths
        .iter()
        .map(|value| {
            EntityPath::parse_strict(value)
                .context("invalid projection entity path")
                .map(|path| (path, Some(components.clone())))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    ensure!(
        view_contents.len() == query.entity_paths.len(),
        "projection selectors resolve to duplicate entity paths"
    );
    let view_contents = view_contents.into_iter().collect();
    let timeline = TimelineName::try_new(&query.timeline).context("invalid projection timeline")?;
    let sparse_fill_strategy = match query.sparse_fill {
        RecordingProjectionSparseFill::None => SparseFillStrategy::None,
        RecordingProjectionSparseFill::LatestAtGlobal => SparseFillStrategy::LatestAtGlobal,
    };
    let mut expression = QueryExpression {
        view_contents: Some(view_contents),
        filtered_index: Some(timeline),
        sparse_fill_strategy,
        ..Default::default()
    };
    match &query.sampling {
        RecordingProjectionSampling::Range { start, end } => {
            expression.filtered_index_range = Some(AbsoluteTimeRange::new(
                TimeInt::new_temporal(*start),
                TimeInt::new_temporal(*end),
            ));
        }
        RecordingProjectionSampling::LatestAt { at } => {
            expression.using_index_values = Some([TimeInt::new_temporal(*at)].into());
            expression.sparse_fill_strategy = SparseFillStrategy::LatestAtGlobal;
        }
        RecordingProjectionSampling::SampleGrid { values } => {
            expression.using_index_values =
                Some(values.iter().copied().map(TimeInt::new_temporal).collect());
        }
    }
    Ok(expression)
}

fn combined_chunk_store(
    layer_paths: &[PathBuf],
    cancelled: &AtomicBool,
) -> Result<ChunkStoreHandle> {
    let config = ChunkStoreConfig::DEFAULT;
    let mut combined: Option<ChunkStore> = None;
    let mut expected_store_id = None;
    let mut paths = layer_paths.to_vec();
    paths.sort();
    for path in paths {
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Arrow projection was cancelled"
        );
        let file = File::open(&path)
            .with_context(|| format!("opening projection layer {}", path.display()))?;
        let stores = ChunkStore::handle_from_rrd_reader(&config, file)
            .with_context(|| format!("decoding projection layer {}", path.display()))?;
        ensure!(
            stores.len() == 1,
            "projection layer must contain one Rerun store"
        );
        let (store_id, handle) = stores.into_iter().next().expect("one store was checked");
        if let Some(expected) = &expected_store_id {
            ensure!(
                expected == &store_id,
                "projection layers have mismatched Rerun Store IDs"
            );
        } else {
            expected_store_id = Some(store_id.clone());
            combined = Some(ChunkStore::new(store_id, config.clone()));
        }
        let source = handle.read();
        let destination = combined.as_mut().expect("combined store was initialized");
        for chunk in source.iter_physical_chunks() {
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "Arrow projection was cancelled"
            );
            destination.insert_chunk(chunk)?;
        }
    }
    let store = combined.context("projection has no decoded Rerun store")?;
    Ok(ChunkStoreHandle::new(store))
}

fn canonical_schema_sha256(schema: &arrow::datatypes::Schema) -> String {
    let bytes = arrow::ipc::convert::IpcSchemaEncoder::new()
        .schema_to_fb(schema)
        .finished_data()
        .to_vec();
    hex::encode(Sha256::digest(bytes))
}

fn ensure_finite(array: &dyn arrow::array::Array) -> Result<()> {
    use arrow::array::{
        FixedSizeListArray, Float32Array, Float64Array, LargeListArray, ListArray, StructArray,
    };
    use arrow::datatypes::DataType;

    match array.data_type() {
        DataType::Float32 => {
            let values = array
                .as_any()
                .downcast_ref::<Float32Array>()
                .context("Arrow Float32 column has the wrong array type")?;
            ensure!(
                values.iter().flatten().all(f32::is_finite),
                "Arrow projection contains a non-finite Float32 value"
            );
        }
        DataType::Float64 => {
            let values = array
                .as_any()
                .downcast_ref::<Float64Array>()
                .context("Arrow Float64 column has the wrong array type")?;
            ensure!(
                values.iter().flatten().all(f64::is_finite),
                "Arrow projection contains a non-finite Float64 value"
            );
        }
        DataType::List(_) => ensure_finite(
            array
                .as_any()
                .downcast_ref::<ListArray>()
                .context("Arrow List column has the wrong array type")?
                .values()
                .as_ref(),
        )?,
        DataType::LargeList(_) => ensure_finite(
            array
                .as_any()
                .downcast_ref::<LargeListArray>()
                .context("Arrow LargeList column has the wrong array type")?
                .values()
                .as_ref(),
        )?,
        DataType::FixedSizeList(_, _) => ensure_finite(
            array
                .as_any()
                .downcast_ref::<FixedSizeListArray>()
                .context("Arrow FixedSizeList column has the wrong array type")?
                .values()
                .as_ref(),
        )?,
        DataType::Struct(_) => {
            let values = array
                .as_any()
                .downcast_ref::<StructArray>()
                .context("Arrow Struct column has the wrong array type")?;
            for column in values.columns() {
                ensure_finite(column.as_ref())?;
            }
        }
        _ => {}
    }
    Ok(())
}

struct BoundedHashWriter {
    file: File,
    maximum: u64,
    written: u64,
    digest: Sha256,
    cancelled: Arc<AtomicBool>,
}

impl BoundedHashWriter {
    fn new(file: File, maximum: u64, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            file,
            maximum,
            written: 0,
            digest: Sha256::new(),
            cancelled,
        }
    }
}

impl Write for BoundedHashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(io::Error::other("Arrow projection was cancelled"));
        }
        let next = self
            .written
            .checked_add(u64::try_from(bytes.len()).map_err(io::Error::other)?)
            .ok_or_else(|| io::Error::other("Arrow projection byte length overflow"))?;
        if next > self.maximum {
            return Err(io::Error::other("Arrow projection exceeds maximum_bytes"));
        }
        self.file.write_all(bytes)?;
        self.written = next;
        self.digest.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use re_sdk::RecordingStreamBuilder;
    use re_sdk_types::archetypes::Scalars;

    use super::*;

    fn query_builder(
        maximum_bytes: u64,
    ) -> veoveo_recording_contract::RecordingProjectionQueryBuilder {
        veoveo_recording_contract::RecordingProjectionQueryBuilder {
            entity_paths: vec!["/sensor".to_owned()],
            component_ids: vec!["Scalars:scalars".to_owned()],
            timeline: "tick".to_owned(),
            sampling: RecordingProjectionSampling::Range { start: 0, end: 2 },
            sparse_fill: RecordingProjectionSparseFill::None,
            maximum_entities: 1,
            maximum_columns: 1,
            maximum_samples: 3,
            maximum_rows: 3,
            maximum_bytes,
        }
    }

    fn query(maximum_bytes: u64) -> ArrowProjectionQuery {
        ArrowProjectionQuery::new(query_builder(maximum_bytes).build().unwrap()).unwrap()
    }

    fn fixture(path: &Path, values: [f64; 3]) {
        let recording = RecordingStreamBuilder::new("projection-fixture")
            .recording_id("projection-fixture")
            .save(path)
            .unwrap();
        for (index, value) in values.into_iter().enumerate() {
            recording.set_time_sequence("tick", index as i64);
            recording.log("/sensor", &Scalars::single(value)).unwrap();
        }
        recording.flush_blocking().unwrap();
        drop(recording);
    }

    #[test]
    fn preparation_parses_selectors_without_reading_layers() {
        let mut invalid = query_builder(1024);
        invalid.entity_paths = vec!["/sensor[0]".into()];
        // The lightweight contract admits bounded text. The Rerun adapter owns grammar.
        let bounded = invalid.build().unwrap();
        assert!(ArrowProjectionQuery::new(bounded).is_err());

        let mut duplicates = query_builder(1024);
        duplicates.maximum_entities = 2;
        duplicates.entity_paths = vec!["/sensor".into(), "sensor".into()];
        assert!(ArrowProjectionQuery::new(duplicates.build().unwrap()).is_err());
    }

    #[test]
    fn range_sample_limit_removes_partial_result() {
        let directory = tempfile::tempdir().unwrap();
        let layer = directory.path().join("layer.rrd");
        fixture(&layer, [1.0, 2.0, 3.0]);
        let output = directory.path().join("limited.arrow");
        let mut builder = query_builder(1024 * 1024);
        builder.maximum_samples = 2;
        let query = ArrowProjectionQuery::new(builder.build().unwrap()).unwrap();
        let error = write_arrow_projection(&[layer], &query, &output).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Arrow projection exceeds maximum_samples"
        );
        assert!(!output.exists());
    }

    #[test]
    fn equal_projection_inputs_produce_equal_arrow_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let layer = directory.path().join("layer.rrd");
        fixture(&layer, [1.0, 2.0, 3.0]);
        let first = directory.path().join("first.arrow");
        let second = directory.path().join("second.arrow");
        let first_summary =
            write_arrow_projection(std::slice::from_ref(&layer), &query(1024 * 1024), &first)
                .unwrap();
        let second_summary =
            write_arrow_projection(std::slice::from_ref(&layer), &query(1024 * 1024), &second)
                .unwrap();
        assert_eq!(first_summary, second_summary);
        assert_eq!(first_summary.row_count, 3);
        assert_eq!(
            std::fs::read(first).unwrap(),
            std::fs::read(second).unwrap()
        );
    }

    #[test]
    fn byte_overflow_and_non_finite_values_leave_no_result() {
        let directory = tempfile::tempdir().unwrap();
        let finite = directory.path().join("finite.rrd");
        fixture(&finite, [1.0, 2.0, 3.0]);
        let too_small = directory.path().join("too-small.arrow");
        assert!(write_arrow_projection(&[finite], &query(1), &too_small).is_err());
        assert!(!too_small.exists());

        let invalid = directory.path().join("invalid.rrd");
        fixture(&invalid, [1.0, f64::NAN, 3.0]);
        let output = directory.path().join("invalid.arrow");
        assert!(write_arrow_projection(&[invalid], &query(1024 * 1024), &output).is_err());
        assert!(!output.exists());
    }

    #[test]
    fn cancellation_leaves_no_partial_arrow_result() {
        let directory = tempfile::tempdir().unwrap();
        let layer = directory.path().join("cancel.rrd");
        fixture(&layer, [1.0, 2.0, 3.0]);
        let output = directory.path().join("cancel.arrow");
        let cancelled = Arc::new(AtomicBool::new(true));
        assert!(
            write_arrow_projection_cancelable(&[layer], &query(1024 * 1024), &output, cancelled,)
                .is_err()
        );
        assert!(!output.exists());
    }
}
