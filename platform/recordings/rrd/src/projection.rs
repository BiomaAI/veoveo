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

pub const MAX_PROJECTION_ENTITIES: usize = 64;
pub const MAX_PROJECTION_COMPONENTS: usize = 64;
pub const MAX_PROJECTION_SAMPLES: usize = 10_000;
pub const MAX_PROJECTION_ROWS: u64 = 10_000;
pub const MAX_PROJECTION_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectionSampling {
    Range { start: i64, end: i64 },
    LatestAt { at: i64 },
    SampleGrid { values: Vec<i64> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionSparseFill {
    None,
    LatestAtGlobal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionQuery {
    pub entity_paths: Vec<String>,
    pub component_ids: Vec<String>,
    pub timeline: String,
    pub sampling: ProjectionSampling,
    pub sparse_fill: ProjectionSparseFill,
    pub maximum_entities: usize,
    pub maximum_columns: usize,
    pub maximum_samples: usize,
    pub maximum_rows: u64,
    pub maximum_bytes: u64,
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
    query: &ProjectionQuery,
    output: &Path,
) -> Result<ArrowProjectionSummary> {
    write_arrow_projection_cancelable(layer_paths, query, output, Arc::new(AtomicBool::new(false)))
}

pub fn write_arrow_projection_cancelable(
    layer_paths: &[PathBuf],
    query: &ProjectionQuery,
    output: &Path,
    cancelled: Arc<AtomicBool>,
) -> Result<ArrowProjectionSummary> {
    validate_query(query)?;
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
    query: &ProjectionQuery,
    output: &Path,
    cancelled: Arc<AtomicBool>,
) -> Result<ArrowProjectionSummary> {
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "Arrow projection was cancelled"
    );
    let engine = QueryEngine::from_store(combined_chunk_store(layer_paths, &cancelled)?);
    let expression = query_expression(query)?;
    let mut handle = engine.query(expression);
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
        ProjectionSampling::LatestAt { .. } => 1,
        ProjectionSampling::SampleGrid { values } => u64::try_from(values.len())?,
        ProjectionSampling::Range { .. } => 0,
    };
    Ok(ArrowProjectionSummary {
        row_count,
        omitted_sample_count: requested_samples.saturating_sub(row_count),
        schema_sha256,
        byte_len,
        sha256,
    })
}

fn validate_query(query: &ProjectionQuery) -> Result<()> {
    ensure!(
        !query.entity_paths.is_empty()
            && query.entity_paths.len() <= query.maximum_entities
            && query.maximum_entities <= MAX_PROJECTION_ENTITIES,
        "projection entity bounds are invalid"
    );
    ensure!(
        !query.component_ids.is_empty()
            && query.component_ids.len() <= query.maximum_columns
            && query.maximum_columns <= MAX_PROJECTION_COMPONENTS,
        "projection component bounds are invalid"
    );
    ensure!(
        (1..=MAX_PROJECTION_ROWS).contains(&query.maximum_rows),
        "projection maximum_rows is invalid"
    );
    ensure!(
        (1..=MAX_PROJECTION_BYTES).contains(&query.maximum_bytes),
        "projection maximum_bytes is invalid"
    );
    ensure!(
        (1..=MAX_PROJECTION_SAMPLES).contains(&query.maximum_samples),
        "projection maximum_samples is invalid"
    );
    ensure!(
        query.entity_paths.iter().collect::<BTreeSet<_>>().len() == query.entity_paths.len(),
        "projection entity paths must be unique"
    );
    ensure!(
        query.component_ids.iter().collect::<BTreeSet<_>>().len() == query.component_ids.len(),
        "projection component identifiers must be unique"
    );
    match &query.sampling {
        ProjectionSampling::Range { start, end } => {
            ensure!(start <= end, "projection range start must not exceed end");
        }
        ProjectionSampling::LatestAt { .. } => {
            ensure!(
                query.maximum_samples >= 1,
                "projection latest-at sample bound is invalid"
            );
        }
        ProjectionSampling::SampleGrid { values } => {
            ensure!(
                !values.is_empty() && values.len() <= query.maximum_samples,
                "projection sample grid exceeds maximum_samples"
            );
            ensure!(
                values.windows(2).all(|pair| pair[0] < pair[1]),
                "projection sample grid must be strictly increasing"
            );
        }
    }
    Ok(())
}

fn query_expression(query: &ProjectionQuery) -> Result<QueryExpression> {
    let components = query
        .component_ids
        .iter()
        .map(|value| {
            ComponentIdentifier::try_new(value.clone())
                .with_context(|| format!("invalid component identifier `{value}`"))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let view_contents = query
        .entity_paths
        .iter()
        .map(|value| {
            EntityPath::parse_strict(value)
                .with_context(|| format!("invalid entity path `{value}`"))
                .map(|path| (path, Some(components.clone())))
        })
        .collect::<Result<BTreeMap<_, _>>>()?
        .into_iter()
        .collect();
    let timeline = TimelineName::try_new(&query.timeline).context("invalid projection timeline")?;
    let sparse_fill_strategy = match query.sparse_fill {
        ProjectionSparseFill::None => SparseFillStrategy::None,
        ProjectionSparseFill::LatestAtGlobal => SparseFillStrategy::LatestAtGlobal,
    };
    let mut expression = QueryExpression {
        view_contents: Some(view_contents),
        filtered_index: Some(timeline),
        sparse_fill_strategy,
        ..Default::default()
    };
    match &query.sampling {
        ProjectionSampling::Range { start, end } => {
            expression.filtered_index_range = Some(AbsoluteTimeRange::new(
                TimeInt::new_temporal(*start),
                TimeInt::new_temporal(*end),
            ));
        }
        ProjectionSampling::LatestAt { at } => {
            expression.using_index_values = Some([TimeInt::new_temporal(*at)].into());
            expression.sparse_fill_strategy = SparseFillStrategy::LatestAtGlobal;
        }
        ProjectionSampling::SampleGrid { values } => {
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

    fn query(maximum_bytes: u64) -> ProjectionQuery {
        ProjectionQuery {
            entity_paths: vec!["/sensor".to_owned()],
            component_ids: vec!["Scalars:scalars".to_owned()],
            timeline: "tick".to_owned(),
            sampling: ProjectionSampling::Range { start: 0, end: 2 },
            sparse_fill: ProjectionSparseFill::None,
            maximum_entities: 1,
            maximum_columns: 1,
            maximum_samples: 3,
            maximum_rows: 3,
            maximum_bytes,
        }
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
