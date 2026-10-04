//! Forecast admission shared by native callers, MCP decoding and request schemas.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use veoveo_duckdb_mcp::contract::{DuckDbColumnName, DuckDbTabularSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeseriesRequestError {
    Horizon,
    NonFiniteFilter,
    EmptyFilterValues,
    EmptyPredicates,
}
impl fmt::Display for TimeseriesRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Horizon => "forecast horizon must be between 1 and 100000",
            Self::NonFiniteFilter => "filter numbers must be finite",
            Self::EmptyFilterValues => "filter in values must not be empty",
            Self::EmptyPredicates => "training filter must contain at least one predicate",
        })
    }
}
impl std::error::Error for TimeseriesRequestError {}

/// A forecast contains between one and 100,000 steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct TimeseriesForecastHorizon(u32);
impl TimeseriesForecastHorizon {
    pub fn new(steps: u32) -> Result<Self, TimeseriesRequestError> {
        if (1..=100_000).contains(&steps) {
            Ok(Self(steps))
        } else {
            Err(TimeseriesRequestError::Horizon)
        }
    }
    pub fn get(self) -> u32 {
        self.0
    }
}
impl TryFrom<u32> for TimeseriesForecastHorizon {
    type Error = TimeseriesRequestError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<TimeseriesForecastHorizon> for u32 {
    fn from(value: TimeseriesForecastHorizon) -> Self {
        value.0
    }
}
impl fmt::Display for TimeseriesForecastHorizon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl JsonSchema for TimeseriesForecastHorizon {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TimeseriesForecastHorizon".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"integer", "minimum":1, "maximum":100_000})
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimeseriesTableMapping {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_column: Option<DuckDbColumnName>,
    pub value_column: DuckDbColumnName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_column: Option<DuckDbColumnName>,
}
impl TimeseriesTableMapping {
    pub fn new(value_column: DuckDbColumnName) -> Self {
        Self {
            value_column,
            time_column: None,
            series_column: None,
        }
    }
    pub fn with_time_column(mut self, column: DuckDbColumnName) -> Self {
        self.time_column = Some(column);
        self
    }
    pub fn with_series_column(mut self, column: DuckDbColumnName) -> Self {
        self.series_column = Some(column);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
pub struct TimeseriesFilterNumber(f64);
impl TimeseriesFilterNumber {
    pub fn new(value: f64) -> Result<Self, TimeseriesRequestError> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(TimeseriesRequestError::NonFiniteFilter)
        }
    }
    pub fn get(self) -> f64 {
        self.0
    }
}
impl TryFrom<f64> for TimeseriesFilterNumber {
    type Error = TimeseriesRequestError;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<TimeseriesFilterNumber> for f64 {
    fn from(value: TimeseriesFilterNumber) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TimeseriesFilterValue {
    String(String),
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(TimeseriesFilterNumber),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "Vec<TimeseriesFilterValue>",
    into = "Vec<TimeseriesFilterValue>"
)]
pub struct TimeseriesFilterValues(Vec<TimeseriesFilterValue>);
impl TimeseriesFilterValues {
    pub fn new(
        first: TimeseriesFilterValue,
        rest: impl IntoIterator<Item = TimeseriesFilterValue>,
    ) -> Self {
        Self(std::iter::once(first).chain(rest).collect())
    }
    pub fn as_slice(&self) -> &[TimeseriesFilterValue] {
        &self.0
    }
}
impl TryFrom<Vec<TimeseriesFilterValue>> for TimeseriesFilterValues {
    type Error = TimeseriesRequestError;
    fn try_from(value: Vec<TimeseriesFilterValue>) -> Result<Self, Self::Error> {
        if value.is_empty() {
            Err(TimeseriesRequestError::EmptyFilterValues)
        } else {
            Ok(Self(value))
        }
    }
}
impl From<TimeseriesFilterValues> for Vec<TimeseriesFilterValue> {
    fn from(value: TimeseriesFilterValues) -> Self {
        value.0
    }
}
impl JsonSchema for TimeseriesFilterValues {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TimeseriesFilterValues".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = Vec::<TimeseriesFilterValue>::json_schema(generator);
        schema.insert("minItems".into(), 1.into());
        schema
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimeseriesFilterPredicate {
    Eq {
        column: DuckDbColumnName,
        value: TimeseriesFilterValue,
    },
    Ne {
        column: DuckDbColumnName,
        value: TimeseriesFilterValue,
    },
    In {
        column: DuckDbColumnName,
        values: TimeseriesFilterValues,
    },
    IsNotNull {
        column: DuckDbColumnName,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeseriesFilterCombination {
    #[default]
    All,
    Any,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "FilterWire", into = "FilterWire")]
pub struct TimeseriesRowFilter(veoveo_types::Checked<FilterWire>);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FilterWire {
    #[serde(default)]
    combination: TimeseriesFilterCombination,
    #[schemars(length(min = 1))]
    predicates: Vec<TimeseriesFilterPredicate>,
}
impl TimeseriesRowFilter {
    pub fn new(
        combination: TimeseriesFilterCombination,
        first: TimeseriesFilterPredicate,
        rest: impl IntoIterator<Item = TimeseriesFilterPredicate>,
    ) -> Self {
        Self(
            veoveo_types::Checked::new(FilterWire {
                combination,
                predicates: std::iter::once(first).chain(rest).collect(),
            })
            .unwrap_or_else(|_| {
                unreachable!("typed filter construction supplies the first predicate")
            }),
        )
    }
    pub fn combination(&self) -> TimeseriesFilterCombination {
        self.0.combination
    }
    pub fn predicates(&self) -> &[TimeseriesFilterPredicate] {
        &self.0.predicates
    }
}
impl veoveo_types::Check for FilterWire {
    type Error = TimeseriesRequestError;
    fn check(&self) -> Result<(), Self::Error> {
        let value = self;
        if value.predicates.is_empty() {
            Err(TimeseriesRequestError::EmptyPredicates)
        } else {
            Ok(())
        }
    }
}
impl TryFrom<FilterWire> for TimeseriesRowFilter {
    type Error = TimeseriesRequestError;
    fn try_from(value: FilterWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<TimeseriesRowFilter> for FilterWire {
    fn from(value: TimeseriesRowFilter) -> Self {
        value.0.into_inner()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeseriesForecastMethod {
    #[default]
    NaiveTrend,
}

/// Forecast requests use only the implemented inline/HTTPS source profile.
///
/// ```compile_fail
/// use veoveo_timeseries_mcp::contract::TimeseriesTableMapping;
/// TimeseriesTableMapping::new("value".to_owned());
/// ```
/// ```compile_fail
/// use veoveo_timeseries_mcp::contract::{TimeseriesForecastRequest, TimeseriesForecastHorizon, TimeseriesTableMapping};
/// use veoveo_duckdb_mcp::contract::DuckDbSource;
/// fn forecast(source: DuckDbSource, mapping: TimeseriesTableMapping, horizon: TimeseriesForecastHorizon) {
///     TimeseriesForecastRequest::new(source, mapping, horizon);
/// }
/// ```
/// ```compile_fail
/// use veoveo_timeseries_mcp::contract::{TimeseriesForecastRequest, TimeseriesTableMapping};
/// use veoveo_duckdb_mcp::contract::DuckDbTabularSource;
/// let source = DuckDbTabularSource::InlineCsv {
///     csv: "value\n1\n".into(), filename: None, options: Default::default(),
/// };
/// TimeseriesForecastRequest::new(source, TimeseriesTableMapping::new("value".parse().unwrap()), 0);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimeseriesForecastRequest {
    pub source: DuckDbTabularSource,
    pub mapping: TimeseriesTableMapping,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training_filter: Option<TimeseriesRowFilter>,
    pub horizon: TimeseriesForecastHorizon,
    #[serde(default)]
    pub method: TimeseriesForecastMethod,
}
impl TimeseriesForecastRequest {
    pub fn new(
        source: DuckDbTabularSource,
        mapping: TimeseriesTableMapping,
        horizon: TimeseriesForecastHorizon,
    ) -> Self {
        Self {
            source,
            mapping,
            horizon,
            training_filter: None,
            method: TimeseriesForecastMethod::default(),
        }
    }
    pub fn with_training_filter(mut self, filter: TimeseriesRowFilter) -> Self {
        self.training_filter = Some(filter);
        self
    }
}
