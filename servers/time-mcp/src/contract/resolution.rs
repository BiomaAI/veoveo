use std::{error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{EffectiveTimeAuthority, TimeInstant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolutionAuthorityMismatch;

impl fmt::Display for ResolutionAuthorityMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("resolved instant and effective authority must use the same releases")
    }
}

impl Error for ResolutionAuthorityMismatch {}

/// Representations computed by the temporal engine from its loaded authority data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeProjection {
    pub utc_rfc3339: String,
    pub utc_is_leap_second: bool,
    pub military_dtg: String,
    pub unix_seconds: i64,
    pub gps_week: Option<u32>,
    pub gps_seconds_of_week: Option<f64>,
    pub julian_day_tai: f64,
}

/// A resolved instant with matching release metadata and its engine projections.
/// Construction checks authority agreement; the engine owns projection calculations.
/// ```compile_fail
/// use veoveo_time_mcp::{ResolveTimeOutput, TimeInstant};
/// fn cannot_relabel(output: &mut ResolveTimeOutput, instant: TimeInstant) {
///     output.instant = instant;
/// }
/// ```
/// ```compile_fail
/// use veoveo_time_mcp::{ResolveTimeOutput, EffectiveTimeAuthority};
/// fn cannot_replace_provenance(output: &mut ResolveTimeOutput, authority: EffectiveTimeAuthority) {
///     output.effective_authority = authority;
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "ResolutionWire", into = "ResolutionWire")]
pub struct ResolveTimeOutput(veoveo_types::Checked<ResolutionWire>);

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
struct ResolutionWire {
    instant: TimeInstant,
    effective_authority: EffectiveTimeAuthority,
    #[serde(flatten)]
    projection: TimeProjection,
}

impl ResolveTimeOutput {
    pub fn new(
        instant: TimeInstant,
        effective_authority: EffectiveTimeAuthority,
        projection: TimeProjection,
    ) -> Result<Self, ResolutionAuthorityMismatch> {
        veoveo_types::Checked::new(ResolutionWire {
            instant,
            effective_authority,
            projection,
        })
        .map(Self)
    }

    pub fn instant(&self) -> &TimeInstant {
        &self.0.instant
    }

    pub fn into_instant(self) -> TimeInstant {
        self.0.into_inner().instant
    }

    pub fn effective_authority(&self) -> &EffectiveTimeAuthority {
        &self.0.effective_authority
    }

    pub fn projection(&self) -> &TimeProjection {
        &self.0.projection
    }
}

impl veoveo_types::Check for ResolutionWire {
    type Error = ResolutionAuthorityMismatch;
    fn check(&self) -> Result<(), Self::Error> {
        if self.instant.authority != self.effective_authority.binding() {
            return Err(ResolutionAuthorityMismatch);
        }

        Ok(())
    }
}
impl TryFrom<ResolutionWire> for ResolveTimeOutput {
    type Error = ResolutionAuthorityMismatch;
    fn try_from(value: ResolutionWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}

impl From<ResolveTimeOutput> for ResolutionWire {
    fn from(value: ResolveTimeOutput) -> Self {
        value.0.into_inner()
    }
}
