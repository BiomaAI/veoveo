use std::{cmp::Reverse, error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{AuthorityBinding, TimeInstant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeWindowError {
    AuthorityMismatch,
    UnorderedBounds,
}

impl fmt::Display for TimeWindowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::AuthorityMismatch => "window bounds must use the same authority",
            Self::UnorderedBounds => "window end must follow its start",
        })
    }
}

impl Error for TimeWindowError {}

/// A nonempty half-open interval whose bounds use one authority pair.
/// ```compile_fail
/// use veoveo_time_mcp::{TimeInstant, TimeWindow};
/// fn cannot_invert(window: &mut TimeWindow, end: TimeInstant) {
///     window.end = end;
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "WindowWire", into = "WindowWire")]
pub struct TimeWindow(veoveo_types::Checked<WindowWire>);

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq, Eq)]
struct WindowWire {
    /// Inclusive lower bound.
    start: TimeInstant,
    /// Exclusive upper bound.
    end: TimeInstant,
}

impl TimeWindow {
    pub fn new(start: TimeInstant, end: TimeInstant) -> Result<Self, TimeWindowError> {
        veoveo_types::Checked::new(WindowWire { start, end }).map(Self)
    }

    pub fn start(&self) -> &TimeInstant {
        &self.0.start
    }
    pub fn end(&self) -> &TimeInstant {
        &self.0.end
    }
    pub fn authority(&self) -> &AuthorityBinding {
        &self.0.start.authority
    }

    /// Clip to the overlap. Touching intervals have no overlap. At equal coordinates,
    /// retain the larger endpoint uncertainty without moving the coordinate.
    pub fn intersection(&self, other: &Self) -> Result<Option<Self>, TimeWindowError> {
        if self.authority() != other.authority() {
            return Err(TimeWindowError::AuthorityMismatch);
        }
        let start = std::cmp::max_by_key(&self.0.start, &other.0.start, |instant| {
            (instant.total_nanoseconds(), instant.uncertainty_nanoseconds)
        });
        let end = std::cmp::min_by_key(&self.0.end, &other.0.end, |instant| {
            (
                instant.total_nanoseconds(),
                Reverse(instant.uncertainty_nanoseconds),
            )
        });
        if start.total_nanoseconds() >= end.total_nanoseconds() {
            return Ok(None);
        }
        Self::new(start.clone(), end.clone()).map(Some)
    }
}

impl veoveo_types::Check for WindowWire {
    type Error = TimeWindowError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.start.authority != self.end.authority {
            return Err(TimeWindowError::AuthorityMismatch);
        }
        if self.start.total_nanoseconds() >= self.end.total_nanoseconds() {
            return Err(TimeWindowError::UnorderedBounds);
        }

        Ok(())
    }
}
impl TryFrom<WindowWire> for TimeWindow {
    type Error = TimeWindowError;
    fn try_from(value: WindowWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}

impl From<TimeWindow> for WindowWire {
    fn from(value: TimeWindow) -> Self {
        value.0.into_inner()
    }
}
