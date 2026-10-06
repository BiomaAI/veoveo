//! Portable temporal syntax and metadata checks; loaded authority remains runtime-owned.
use chrono::NaiveDateTime;
use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid temporal value: {0}")]
pub struct TimeValueError(pub(crate) &'static str);

pub fn text(value: &str, max: usize) -> Result<(), TimeValueError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(TimeValueError("bounded printable text required"));
    }
    Ok(())
}

pub fn zone(value: &str) -> Result<(), TimeValueError> {
    if value.is_empty()
        || value.len() > 128
        || value.starts_with('/')
        || value.contains("..")
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'-' | b'+'))
    {
        return Err(TimeValueError("invalid expression zone identifier"));
    }
    Ok(())
}

pub fn local(value: &str) -> Result<NaiveDateTime, TimeValueError> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f"))
        .map_err(|_| TimeValueError("local datetime requires ISO 8601 without an offset"))
}

pub fn https(value: &str) -> Result<(), TimeValueError> {
    let url = Url::parse(value).map_err(|_| TimeValueError("absolute HTTPS source required"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(TimeValueError(
            "HTTPS source cannot contain credentials or a fragment",
        ));
    }
    Ok(())
}

pub fn absolute_path(value: &str) -> Result<(), TimeValueError> {
    if !value.starts_with('/') || value.contains("/../") || value.chars().any(char::is_control) {
        return Err(TimeValueError("confined absolute path required"));
    }
    Ok(())
}

pub fn gps(value: f64) -> Result<(), TimeValueError> {
    if !value.is_finite() || !(0.0..604800.0).contains(&value) {
        return Err(TimeValueError(
            "GPS seconds must be finite and in [0,604800)",
        ));
    }
    Ok(())
}

use anyhow::{Context, Result as AnyResult, bail};
use chrono::{FixedOffset, NaiveDate, TimeZone, Utc};
pub fn parse_military_dtg(value: &str) -> AnyResult<chrono::DateTime<Utc>> {
    let value = value.trim().to_uppercase();
    let has_seconds = value.len() == 14;
    if !value.is_ascii() || (value.len() != 12 && !has_seconds) {
        bail!("military DTG must use DDHHMMZMONYY or DDHHMMSSZMONYY");
    }
    let zone_index = if has_seconds { 8 } else { 6 };
    let zone_letter = value.as_bytes()[zone_index] as char;
    let digits = |range: std::ops::Range<usize>| -> AnyResult<u32> {
        value[range]
            .parse()
            .context("military DTG contains invalid digits")
    };
    let day = digits(0..2)?;
    let hour = digits(2..4)?;
    let minute = digits(4..6)?;
    let second = if has_seconds { digits(6..8)? } else { 0 };
    let month_start = zone_index + 1;
    let month = match &value[month_start..month_start + 3] {
        "JAN" => 1,
        "FEB" => 2,
        "MAR" => 3,
        "APR" => 4,
        "MAY" => 5,
        "JUN" => 6,
        "JUL" => 7,
        "AUG" => 8,
        "SEP" => 9,
        "OCT" => 10,
        "NOV" => 11,
        "DEC" => 12,
        _ => bail!("military DTG contains an invalid month"),
    };
    let short_year = digits(month_start + 3..month_start + 5)? as i32;
    let year = if short_year >= 70 {
        1900 + short_year
    } else {
        2000 + short_year
    };
    let offset_hours = nato_zone_offset_hours(zone_letter)?;
    let offset =
        FixedOffset::east_opt(offset_hours * 3600).context("invalid military zone offset")?;
    let naive = NaiveDate::from_ymd_opt(year, month, day)
        .and_then(|date| date.and_hms_opt(hour, minute, second))
        .context("military DTG is not a valid civil time")?;
    let zoned = offset
        .from_local_datetime(&naive)
        .single()
        .context("military DTG is ambiguous")?;
    Ok(zoned.with_timezone(&Utc))
}

fn nato_zone_offset_hours(letter: char) -> AnyResult<i32> {
    match letter {
        'Z' => Ok(0),
        'A'..='I' => Ok((letter as u8 - b'A' + 1).into()),
        'K'..='M' => Ok((letter as u8 - b'A').into()),
        'N'..='Y' => Ok(-i32::from(letter as u8 - b'N' + 1)),
        'J' => bail!("military zone J denotes local time and requires an explicit IANA zone"),
        _ => bail!("invalid military time-zone letter"),
    }
}

/// GPS week-local seconds preserve their f64 wire and binary representation.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct GpsSecondsOfWeek(f64);
impl GpsSecondsOfWeek {
    pub fn new(value: f64) -> Result<Self, TimeValueError> {
        gps(value)?;
        Ok(Self(value))
    }
    pub const fn get(self) -> f64 {
        self.0
    }
}
impl TryFrom<f64> for GpsSecondsOfWeek {
    type Error = TimeValueError;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<GpsSecondsOfWeek> for f64 {
    fn from(value: GpsSecondsOfWeek) -> Self {
        value.0
    }
}
impl schemars::JsonSchema for GpsSecondsOfWeek {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "GpsSecondsOfWeek".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = <f64 as schemars::JsonSchema>::json_schema(generator);
        schema.insert("minimum".into(), 0.into());
        schema.insert("exclusiveMaximum".into(), 604800.into());
        schema
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{TimeExpression, TimeExpressionValue};

    #[test]
    fn gps_week_seconds_share_constructor_decoder_and_schema_bounds() {
        let schema = serde_json::to_value(schemars::schema_for!(GpsSecondsOfWeek)).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        for value in [0.0, 1.0, 604799.999] {
            let admitted = GpsSecondsOfWeek::new(value).unwrap();
            let bytes = serde_json::to_vec(&admitted).unwrap();
            assert_eq!(
                serde_json::from_slice::<GpsSecondsOfWeek>(&bytes).unwrap(),
                admitted
            );
            assert!(validator.is_valid(&serde_json::json!(value)));
        }
        for value in [-1.0, 604800.0, 1e20] {
            assert!(GpsSecondsOfWeek::new(value).is_err());
            assert!(serde_json::from_value::<GpsSecondsOfWeek>(serde_json::json!(value)).is_err());
            assert!(!validator.is_valid(&serde_json::json!(value)));
        }
        assert!(GpsSecondsOfWeek::new(f64::NAN).is_err());
        assert!(GpsSecondsOfWeek::new(f64::INFINITY).is_err());
    }

    #[test]
    fn expression_builders_and_decoders_share_text_and_finite_admission() {
        for draft in [
            TimeExpressionValue::Rfc3339 {
                value: "2026-10-06T12:30:00Z".into(),
            },
            TimeExpressionValue::MilitaryDtg {
                value: "061230ZOCT26".into(),
            },
            TimeExpressionValue::JulianTai { day: 2461319.0 },
        ] {
            let wire = serde_json::to_vec(&draft).unwrap();
            let admitted = draft.build().unwrap();
            assert_eq!(
                serde_json::from_slice::<TimeExpression>(&wire).unwrap(),
                admitted
            );
        }
        for draft in [
            TimeExpressionValue::Rfc3339 {
                value: "invalid".into(),
            },
            TimeExpressionValue::MilitaryDtg {
                value: "é61230ZOCT26".into(),
            },
        ] {
            let wire = serde_json::to_vec(&draft).unwrap();
            assert!(draft.build().is_err());
            assert!(serde_json::from_slice::<TimeExpression>(&wire).is_err());
        }
        assert!(
            TimeExpressionValue::JulianTai { day: f64::INFINITY }
                .build()
                .is_err()
        );
    }
}
