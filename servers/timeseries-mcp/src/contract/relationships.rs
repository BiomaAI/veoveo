//! Portable forecast relationships and admission before publication.
use super::*;
type Result<T> = std::result::Result<T, TimeseriesForecastError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid Timeseries forecast: {0}")]
pub struct TimeseriesForecastError(&'static str);

fn require(condition: bool, message: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(TimeseriesForecastError(message))
    }
}

/// The even-stride preview retains its final point in addition to the 500 samples.
pub const MAX_PREVIEW_POINTS: usize = 501;

impl veoveo_types::Check for TimeseriesForecastSummaryBuilder {
    type Error = TimeseriesForecastError;
    fn check(&self) -> Result<()> {
        require(
            self.source_rows > 0 && !self.series.is_empty(),
            "completed forecast must contain usable observations and series",
        )?;
        let mut previous: Option<&str> = None;
        let mut source_rows = 0_u64;
        for series in &self.series {
            require(
                series.observed_rows > 0,
                "forecast series must contain usable observations",
            )?;
            require(
                previous.is_none_or(|prior| prior < series.series_id.as_str()),
                "forecast series must be unique and sorted",
            )?;
            previous = Some(&series.series_id);
            require(
                series.forecast_rows == u64::from(self.horizon.get()),
                "forecast count differs from horizon",
            )?;
            source_rows = source_rows
                .checked_add(series.observed_rows)
                .ok_or(TimeseriesForecastError("forecast source count overflow"))?;
        }
        require(
            source_rows == self.source_rows,
            "forecast source count differs from series",
        )?;
        Ok(())
    }
}

pub fn validate_forecast_preview(
    summary: &TimeseriesForecastSummary,
    preview: &[TimeseriesSeriesPreview],
) -> Result<()> {
    require(
        summary.series.len() == preview.len(),
        "forecast preview series differ from summary",
    )?;
    for (series, chart) in summary.series.iter().zip(preview) {
        require(
            series.series_id == chart.series_id,
            "forecast preview identity differs from summary",
        )?;
        require(
            chart.observed.len() <= MAX_PREVIEW_POINTS
                && chart.forecast.len() <= MAX_PREVIEW_POINTS,
            "forecast preview exceeds 501 points",
        )?;
        require(
            chart.observed.len() as u64 <= series.observed_rows,
            "observed preview exceeds source count",
        )?;
        require(
            (series.observed_rows == 0) == chart.observed.is_empty(),
            "observed preview absence differs from source count",
        )?;
        for point in &chart.observed {
            require(point.value.is_finite(), "observed preview must be finite")?;
        }
        let mut prior = 0;
        for point in &chart.forecast {
            validate_forecast_point(point.step, point.mean, point.q10, point.q90)?;
            require(
                point.step > prior && point.step <= summary.horizon.get(),
                "forecast preview steps must be increasing and within horizon",
            )?;
            prior = point.step;
        }
        require(
            prior == summary.horizon.get(),
            "forecast preview must retain its final step",
        )?;
    }
    Ok(())
}

pub fn validate_forecast_point(step: u32, mean: f64, q10: f64, q90: f64) -> Result<()> {
    require(
        step > 0 && mean.is_finite() && q10.is_finite() && q90.is_finite(),
        "forecast points must have a positive step and finite values",
    )?;
    require(
        q10 <= mean && mean <= q90,
        "forecast quantiles must enclose the mean",
    )?;
    Ok(())
}

impl veoveo_types::Check for TimeseriesForecastOutputBuilder {
    type Error = TimeseriesForecastError;
    fn check(&self) -> Result<()> {
        require(
            self.result_uri.artifact_id() == self.artifact.artifact_id(),
            "forecast result and Artifact identities differ",
        )?;
        validate_forecast_preview(&self.forecast, &self.preview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wire() -> serde_json::Value {
        let mut value: serde_json::Value =
            serde_json::from_str(include_str!("../../testdata/app-output.json")).unwrap();
        value["forecast"] = serde_json::json!({"method":"naive_trend","horizon":1000,"sourceRows":2,
            "series":[{"seriesId":"user label","observedRows":2,"forecastRows":1000}]});
        value["preview"] = serde_json::json!([{"seriesId":"user label","observed":[{"value":1.0}],
            "forecast":[{"step":1,"mean":2.0,"q10":1.0,"q90":3.0},
                {"step":1000,"mean":4.0,"q10":3.0,"q90":5.0}]}]);
        value
    }
    #[test]
    fn sparse_forecast_preview_and_terminal_handoff_preserve_current_bytes() {
        let value = wire();
        let admitted: TimeseriesForecastOutput = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(admitted).unwrap(), value);
    }
    #[test]
    fn actual_output_schema_preserves_object_root_and_501_point_bound() {
        let schema = serde_json::to_value(schemars::schema_for!(TimeseriesForecastOutput)).unwrap();
        assert_eq!(schema["type"], "object");
        let validator = jsonschema::validator_for(&schema).unwrap();
        let mut value = wire();
        value["forecast"]["horizon"] = serde_json::json!(501);
        value["forecast"]["series"][0]["forecastRows"] = serde_json::json!(501);
        value["preview"][0]["forecast"] = (1..=501)
            .map(|step| {
                serde_json::json!({
                    "step": step, "mean":2.0, "q10":1.0, "q90":3.0
                })
            })
            .collect::<Vec<_>>()
            .into();
        assert!(validator.is_valid(&value));
        let admitted: TimeseriesForecastOutput = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(admitted).unwrap(), value);
        value["forecast"]["horizon"] = serde_json::json!(502);
        value["forecast"]["series"][0]["forecastRows"] = serde_json::json!(502);
        value["preview"][0]["forecast"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "step":502, "mean":2.0, "q10":1.0, "q90":3.0
            }));
        assert!(!validator.is_valid(&value));
        assert!(serde_json::from_value::<TimeseriesForecastOutput>(value).is_err());
        for invalid in [
            serde_json::Value::Null,
            serde_json::json!([]),
            serde_json::json!("forecast"),
        ] {
            assert!(!validator.is_valid(&invalid));
        }
    }
    #[test]
    fn schema_valid_empty_or_zero_observation_products_are_not_completed_forecasts() {
        let schema = serde_json::to_value(schemars::schema_for!(TimeseriesForecastOutput)).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let mut empty = wire();
        empty["forecast"]["sourceRows"] = serde_json::json!(0);
        empty["forecast"]["series"] = serde_json::json!([]);
        empty["preview"] = serde_json::json!([]);
        assert!(validator.is_valid(&empty));
        assert!(serde_json::from_value::<TimeseriesForecastOutput>(empty).is_err());
        let mut zero = wire();
        zero["forecast"]["sourceRows"] = serde_json::json!(0);
        zero["forecast"]["series"][0]["observedRows"] = serde_json::json!(0);
        zero["preview"][0]["observed"] = serde_json::json!([]);
        assert!(validator.is_valid(&zero));
        assert!(serde_json::from_value::<TimeseriesForecastOutput>(zero).is_err());
        assert!(
            TimeseriesForecastSummaryBuilder {
                method: TimeseriesForecastMethod::NaiveTrend,
                horizon: TimeseriesForecastHorizon::new(1).unwrap(),
                source_rows: 0,
                series: vec![]
            }
            .build()
            .is_err()
        );
    }
    #[test]
    fn forecast_admission_rejects_detached_artifact_counts_series_and_quantiles() {
        let schema = serde_json::to_value(schemars::schema_for!(TimeseriesForecastOutput)).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let current = wire();
        assert!(validator.is_valid(&current));
        serde_json::from_value::<TimeseriesForecastOutput>(current).unwrap();
        for (pointer, invalid) in [
            (
                "/resultUri",
                serde_json::json!("timeseries://artifact/00000000-0000-7000-8000-000000000002"),
            ),
            ("/forecast/sourceRows", serde_json::json!(3)),
            ("/forecast/series/0/forecastRows", serde_json::json!(999)),
            ("/preview/0/seriesId", serde_json::json!("other label")),
            ("/preview/0/forecast/1/step", serde_json::json!(999)),
            ("/preview/0/forecast/0/q10", serde_json::json!(3.0)),
        ] {
            let mut value = wire();
            *value.pointer_mut(pointer).unwrap() = invalid;
            assert!(
                validator.is_valid(&value),
                "{pointer} must reach relationship admission"
            );
            assert!(
                serde_json::from_value::<TimeseriesForecastOutput>(value).is_err(),
                "{pointer}"
            );
        }
        let mut duplicate = wire();
        let series = duplicate["forecast"]["series"][0].clone();
        duplicate["forecast"]["series"]
            .as_array_mut()
            .unwrap()
            .push(series);
        assert!(serde_json::from_value::<TimeseriesForecastOutput>(duplicate).is_err());
        let summary = TimeseriesForecastSummaryBuilder {
            method: TimeseriesForecastMethod::NaiveTrend,
            horizon: TimeseriesForecastHorizon::new(1).unwrap(),
            source_rows: u64::MAX,
            series: vec![
                TimeseriesSeriesSummary {
                    series_id: "a".into(),
                    observed_rows: u64::MAX,
                    forecast_rows: 1,
                },
                TimeseriesSeriesSummary {
                    series_id: "b".into(),
                    observed_rows: 1,
                    forecast_rows: 1,
                },
            ],
        };
        assert!(
            summary
                .build()
                .unwrap_err()
                .to_string()
                .contains("overflow")
        );
    }
}
