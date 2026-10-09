//! Forecast, metadata and recording agreement at the installed consumer boundary.
use super::*;
use re_sdk::{external::re_log_types::LogMsg, log::Chunk};
use re_sdk_types::archetypes::{Scalars, TextDocument};
use sha2::{Digest, Sha256};
use veoveo_artifact_contract::{ArtifactMetadata, UploadSha256};
use veoveo_timeseries_mcp::contract::{
    TimeseriesForecastMetadata, TimeseriesForecastOutput, TimeseriesRecordingTask,
    TimeseriesRrdProvenance, TimeseriesSourceProvenance,
};
use veoveo_types::Sha256Digest;

pub(super) const RRD_MIME: &str = "application/vnd.veoveo.rerun-rrd";
pub(super) const MAX_RRD: usize = 2 * 1024 * 1024;

pub(super) fn metadata(
    request: &TimeseriesForecastRequest,
    output: &TimeseriesForecastOutput,
) -> Result<TimeseriesForecastMetadata> {
    let metadata: TimeseriesForecastMetadata =
        serde_json::from_value(output.artifact.metadata.clone())
            .map_err(|_| anyhow!("Timeseries output metadata failed owner admission"))?;
    ensure!(
        metadata.task_id.as_uuid().get_version_num() == 7,
        "Timeseries provenance requires a native UUIDv7 Task identity"
    );
    ensure!(
        metadata.task_id == metadata.provenance.task_id,
        "Timeseries artifact and provenance Task identities differ"
    );
    ensure!(
        metadata.artifact_format == "rerun_rrd"
            && metadata.rrd_application_id == "veoveo_timeseries_forecast",
        "Timeseries artifact format differs"
    );
    ensure!(
        metadata.summary == output.forecast,
        "Timeseries artifact summary differs from output"
    );
    require_provenance(request, &metadata.provenance)?;
    require_output(request, output)?;
    Ok(metadata)
}
pub(super) fn require_provenance(
    request: &TimeseriesForecastRequest,
    actual: &TimeseriesRrdProvenance,
) -> Result<()> {
    let DuckDbTabularSource::InlineCsv {
        csv,
        filename,
        options,
    } = &request.source
    else {
        bail!("installed Timeseries fixture requires inline CSV");
    };
    let digest = UploadSha256::parse(hex::encode(Sha256::digest(serde_json::to_vec(
        &request.source,
    )?)))?;
    let expected = TimeseriesSourceProvenance::InlineCsv {
        filename: filename.clone(),
        byte_len: csv.len(),
        options: options.clone(),
    };
    ensure!(
        actual.source_digest == digest
            && actual.source == expected
            && actual.mapping == request.mapping
            && actual.training_filter == request.training_filter
            && actual.method == request.method
            && actual.horizon == request.horizon,
        "Timeseries provenance differs from the submitted source or forecast"
    );
    Ok(())
}
fn require_output(
    request: &TimeseriesForecastRequest,
    output: &TimeseriesForecastOutput,
) -> Result<()> {
    ensure!(
        output.forecast.method == request.method
            && output.forecast.horizon == request.horizon
            && output.forecast.source_rows == 4
            && output.forecast.series.len() == 1,
        "Timeseries summary differs from the four-row fixture"
    );
    let series = &output.forecast.series[0];
    ensure!(
        series.series_id == "series" && series.observed_rows == 4 && series.forecast_rows == 2,
        "Timeseries series summary differs from the fixture"
    );
    ensure!(
        output.preview.len() == 1 && output.preview[0].series_id == "series",
        "Timeseries preview identity differs"
    );
    let preview = &output.preview[0];
    ensure!(
        preview.observed.len() == 4 && preview.forecast.len() == 2,
        "Timeseries preview dropped fixture observations or forecast steps"
    );
    for (actual, expected) in preview.observed.iter().zip([1.0, 2.0, 3.0, 4.0]) {
        ensure!(
            actual.value == expected,
            "Timeseries observed preview changed the source"
        );
    }
    for (actual, (step, expected)) in preview.forecast.iter().zip([(1, 5.0), (2, 6.0)]) {
        ensure!(
            actual.step == step && (actual.mean - expected).abs() < 1e-10,
            "Timeseries trend differs from the deterministic fixture"
        );
    }
    ensure!(
        output.result_uri.artifact_id() == output.artifact.artifact_id()
            && output.artifact.mime_type.as_deref() == Some(RRD_MIME)
            && output.artifact.filename.as_deref() == Some("forecast.rrd")
            && output.artifact.byte_len > 0
            && output.artifact.byte_len <= MAX_RRD as u64,
        "Timeseries Artifact handoff differs or exceeds two MiB"
    );
    Ok(())
}

pub(super) fn current_metadata(
    output: &TimeseriesForecastOutput,
    current: &ArtifactMetadata,
) -> Result<()> {
    ensure!(
        current.artifact_uri == current.artifact_id().plane_uri(),
        "current Artifact metadata is not neutral"
    );
    ensure!(
        current
            .clone()
            .without_download_url()
            .presented_under_scheme(&veoveo_timeseries_mcp::uris::SCHEME)
            == output.artifact,
        "current Artifact metadata differs from the forecast handoff"
    );
    Ok(())
}

pub(super) fn recording(
    request: &TimeseriesForecastRequest,
    output: &TimeseriesForecastOutput,
    metadata: &TimeseriesForecastMetadata,
    timeseries_bytes: &[u8],
    artifact_bytes: &[u8],
) -> Result<Sha256Digest> {
    ensure!(
        timeseries_bytes.len() as u64 == output.artifact.byte_len
            && timeseries_bytes == artifact_bytes,
        "Timeseries and public Artifact bytes or lengths differ"
    );
    let digest = Sha256Digest::from_bytes(Sha256::digest(timeseries_bytes).into());
    let messages =
        re_log_encoding::Decoder::<LogMsg>::decode_eager(std::io::Cursor::new(timeseries_bytes))
            .map_err(|_| anyhow!("Timeseries Artifact is not a current RRD"))?;
    let mut provenance = None;
    let mut task = None;
    let mut observed = std::collections::BTreeMap::new();
    let mut forecast = std::collections::BTreeMap::new();
    for (index, message) in messages.enumerate() {
        ensure!(
            index < 1024,
            "Timeseries fixture RRD exceeds message budget"
        );
        if let LogMsg::ArrowMsg(_, arrow) =
            message.map_err(|_| anyhow!("Timeseries RRD message failed decoding"))?
        {
            let chunk = Chunk::from_arrow_msg(&arrow)
                .map_err(|_| anyhow!("Timeseries RRD chunk failed decoding"))?;
            let path = chunk.entity_path().to_string();
            if path.starts_with("/timeseries/series/") {
                let (timeline, rows) = match path.as_str() {
                    "/timeseries/series/series/observed" => ("source_row", Some(&mut observed)),
                    "/timeseries/series/series/forecast/mean" => {
                        ("forecast_step", Some(&mut forecast))
                    }
                    "/timeseries/series/series/forecast/q10"
                    | "/timeseries/series/series/forecast/q90"
                    | "/timeseries/series/series/summary" => ("", None),
                    _ => bail!("Timeseries RRD contains an unexpected series"),
                };
                if let Some(rows) = rows {
                    let component = Scalars::descriptor_scalars().component;
                    let times: Vec<_> = chunk
                        .iter_component_indices(timeline.into(), component)
                        .collect();
                    let values: Vec<_> = chunk
                        .iter_component::<re_sdk_types::components::Scalar>(component)
                        .collect();
                    ensure!(
                        times.len() == values.len() && !values.is_empty(),
                        "Timeseries RRD scalar rows lack their timeline"
                    );
                    for ((time, _), values) in times.into_iter().zip(values) {
                        ensure!(
                            values.as_slice().len() == 1
                                && rows
                                    .insert(time.as_i64(), values.as_slice()[0].0.0)
                                    .is_none(),
                            "Timeseries RRD repeats or batches fixture scalar rows"
                        );
                    }
                }
            }
            for batch in chunk.iter_component::<re_sdk_types::components::Text>(
                TextDocument::descriptor_text().component,
            ) {
                for text in batch.as_slice() {
                    let text = text.0.to_string();
                    ensure!(
                        text.len() <= 65536,
                        "Timeseries recording document exceeds 64KiB"
                    );
                    match chunk.entity_path().to_string().as_str() {
                        "/timeseries/provenance" => {
                            ensure!(provenance.is_none(), "Timeseries RRD repeats provenance");
                            provenance = Some(
                                serde_json::from_str::<TimeseriesRrdProvenance>(&text).map_err(
                                    |_| anyhow!("Timeseries recording provenance failed admission"),
                                )?,
                            );
                        }
                        "/timeseries/task" => {
                            ensure!(task.is_none(), "Timeseries RRD repeats Task metadata");
                            task = Some(
                                serde_json::from_str::<TimeseriesRecordingTask>(&text).map_err(
                                    |_| anyhow!("Timeseries recording Task failed admission"),
                                )?,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    let provenance = provenance.context("Timeseries RRD omitted provenance")?;
    ensure!(
        provenance == metadata.provenance,
        "Timeseries RRD and public metadata provenance differ"
    );
    require_provenance(request, &provenance)?;
    let task = task.context("Timeseries RRD omitted Task metadata")?;
    ensure!(
        task.task_id == metadata.task_id
            && task.horizon == request.horizon
            && task.method == request.method,
        "Timeseries RRD Task identity or request facts differ"
    );
    ensure!(
        observed
            == [(0, 1.0), (1, 2.0), (2, 3.0), (3, 4.0)]
                .into_iter()
                .collect(),
        "Timeseries RRD observed rows differ from the source"
    );
    ensure!(
        forecast.len() == output.preview[0].forecast.len(),
        "Timeseries RRD omitted forecast means"
    );
    for point in &output.preview[0].forecast {
        ensure!(
            forecast
                .get(&i64::from(point.step))
                .is_some_and(|value| (*value - point.mean).abs() < 1e-10),
            "Timeseries RRD forecast step or mean differs from output"
        );
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_artifact_contract::ArtifactId;
    use veoveo_timeseries_mcp::contract::*;
    use veoveo_types::TaskId;

    fn fixture() -> Result<(
        TimeseriesForecastRequest,
        TimeseriesForecastOutput,
        TimeseriesForecastMetadata,
    )> {
        let request = forecast_request()?;
        let summary = TimeseriesForecastSummaryBuilder {
            method: request.method,
            horizon: request.horizon,
            source_rows: 4,
            series: vec![TimeseriesSeriesSummary {
                series_id: "series".into(),
                observed_rows: 4,
                forecast_rows: 2,
            }],
        }
        .build()?;
        let DuckDbTabularSource::InlineCsv {
            filename,
            csv,
            options,
        } = &request.source
        else {
            unreachable!()
        };
        let task_id = TaskId::new();
        let provenance = TimeseriesRrdProvenance {
            task_id,
            source_digest: UploadSha256::parse(hex::encode(Sha256::digest(serde_json::to_vec(
                &request.source,
            )?)))?,
            source: TimeseriesSourceProvenance::InlineCsv {
                filename: filename.clone(),
                byte_len: csv.len(),
                options: options.clone(),
            },
            mapping: request.mapping.clone(),
            training_filter: None,
            method: request.method,
            horizon: request.horizon,
        };
        let metadata = TimeseriesForecastMetadata::new(task_id, summary.clone(), provenance)?;
        let id = ArtifactId::new();
        let artifact = ArtifactMetadata {
            byte_len: 1,
            mime_type: Some(RRD_MIME.into()),
            filename: Some("forecast.rrd".into()),
            artifact_uri: id.plane_uri(),
            download_url: None,
            created_at: chrono::Utc::now(),
            release_state: Default::default(),
            compliance: Default::default(),
            metadata: serde_json::to_value(&metadata)?,
        }
        .presented_under_scheme(&veoveo_timeseries_mcp::uris::SCHEME);
        let output = TimeseriesForecastOutputBuilder {
            result_uri: TimeseriesArtifactUri::new(id),
            forecast: summary,
            preview: vec![TimeseriesSeriesPreview {
                series_id: "series".into(),
                observed: [1.0, 2.0, 3.0, 4.0]
                    .into_iter()
                    .map(|value| TimeseriesPreviewObservation {
                        event_time: None,
                        value,
                    })
                    .collect(),
                forecast: [(1, 5.0), (2, 6.0)]
                    .into_iter()
                    .map(|(step, mean)| TimeseriesPreviewForecastPoint {
                        step,
                        mean,
                        q10: mean - 1.0,
                        q90: mean + 1.0,
                    })
                    .collect(),
            }],
            artifact,
        }
        .build()?;
        Ok((request, output, metadata))
    }
    fn changed(output: &TimeseriesForecastOutput) -> TimeseriesForecastOutputBuilder {
        TimeseriesForecastOutputBuilder {
            result_uri: output.result_uri.clone(),
            forecast: output.forecast.clone(),
            preview: output.preview.clone(),
            artifact: output.artifact.clone(),
        }
    }
    #[test]
    fn owner_admitted_forecast_with_wrong_values_or_source_digest_is_rejected() -> Result<()> {
        let (request, output, metadata) = fixture()?;
        super::metadata(&request, &output)?;
        let mut wrong = changed(&output);
        wrong.preview[0].forecast[0].mean = 4.5;
        assert!(super::metadata(&request, &wrong.build()?).is_err());
        let mut provenance = metadata.provenance.clone();
        provenance.source_digest = UploadSha256::parse("ab".repeat(32))?;
        let wrong_metadata = TimeseriesForecastMetadata::new(
            metadata.task_id,
            metadata.summary.clone(),
            provenance,
        )?;
        let mut wrong = changed(&output);
        wrong.artifact.metadata = serde_json::to_value(wrong_metadata)?;
        assert!(super::metadata(&request, &wrong.build()?).is_err());
        let mut current = output.artifact.clone();
        current.artifact_uri = current.artifact_id().plane_uri();
        current_metadata(&output, &current)?;
        current.byte_len += 1;
        assert!(current_metadata(&output, &current).is_err());
        Ok(())
    }
    fn rrd(metadata: &TimeseriesForecastMetadata, values: Option<[f64; 4]>) -> Result<Vec<u8>> {
        let (rec, storage) =
            re_sdk::RecordingStreamBuilder::new("veoveo_timeseries_forecast").memory()?;
        rec.log_static(
            "/timeseries/provenance",
            &TextDocument::new(serde_json::to_string(&metadata.provenance)?),
        )?;
        rec.log_static(
            "/timeseries/task",
            &TextDocument::new(serde_json::to_string(&TimeseriesRecordingTask {
                task_id: metadata.task_id,
                method: metadata.provenance.method,
                horizon: metadata.provenance.horizon,
            })?),
        )?;
        if let Some(values) = values {
            for (row, value) in values.into_iter().enumerate() {
                rec.set_time_sequence("source_row", row as i64);
                rec.log(
                    "/timeseries/series/series/observed",
                    &Scalars::single(value),
                )?;
            }
            rec.disable_timeline("source_row");
            for (step, mean) in [(1, 5.0), (2, 6.0)] {
                rec.set_time_sequence("forecast_step", step);
                rec.log(
                    "/timeseries/series/series/forecast/mean",
                    &Scalars::single(mean),
                )?;
            }
        }
        rec.flush_blocking()?;
        let mut encoder = re_log_encoding::Encoder::local()?;
        for message in storage.take() {
            encoder.append(&message)?;
        }
        encoder.finish()?;
        Ok(encoder.into_inner()?)
    }
    #[test]
    fn actual_rrd_decoder_rejects_missing_or_changed_series_and_handoff_bytes() -> Result<()> {
        let (request, output, metadata) = fixture()?;
        for values in [None, Some([1.0, 2.0, 3.0, 9.0]), Some([1.0, 2.0, 3.0, 4.0])] {
            let bytes = rrd(&metadata, values)?;
            let mut current = changed(&output);
            current.artifact.byte_len = bytes.len() as u64;
            let current = current.build()?;
            let result = recording(&request, &current, &metadata, &bytes, &bytes);
            assert_eq!(result.is_ok(), values == Some([1.0, 2.0, 3.0, 4.0]));
            let mut different = bytes.clone();
            different[0] ^= 1;
            assert!(recording(&request, &current, &metadata, &bytes, &different).is_err());
        }
        Ok(())
    }
}
