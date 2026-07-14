use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
    sync::{Arc, RwLock},
};

use anyhow::{Context, Result, bail};
use chrono::{Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, TimeZone as _, Utc};
use hifitime::{Epoch, TimeScale as HifiTimeScale};
use jiff::{Timestamp, civil, tz};
use rangemap::RangeSet;

use crate::{
    authority::AuthorityContext,
    contract::{
        ConvertTimeOutput, ConvertTimeRequest, Disambiguation, EvaluateWindowsOutput,
        EvaluateWindowsRequest, ExpandScheduleOutput, ExpandScheduleRequest, MissionEpoch,
        RecurrenceFrequency, ResolveTimeOutput, ResolveTimeRequest, ScaleRepresentation,
        ScheduleOccurrence, TimeExpression, TimeInstant, TimeScale, TimeWindow, TimelineViolation,
        ValidateTimelineOutput, ValidateTimelineRequest, Weekday, WindowOperation,
        ZonedRepresentation,
    },
};

const NANOS_PER_SECOND: i128 = 1_000_000_000;
const SECONDS_PER_WEEK: f64 = 604_800.0;
const GPS_EPOCH_TAI_SECONDS_SINCE_1970: i64 = 315_964_819;
const JULIAN_DAY_AT_1970_TAI: f64 = 2_440_587.5;

#[derive(Clone)]
pub struct TemporalEngine {
    authority: Arc<AuthorityContext>,
    epochs: Arc<RwLock<BTreeMap<String, MissionEpoch>>>,
}

impl TemporalEngine {
    pub fn new(authority: AuthorityContext) -> Self {
        Self {
            authority: Arc::new(authority),
            epochs: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn authority(&self) -> &AuthorityContext {
        &self.authority
    }

    pub fn fork(&self) -> Self {
        Self::new(self.authority.as_ref().clone())
    }

    pub fn replace_epochs(&self, epochs: impl IntoIterator<Item = MissionEpoch>) {
        let mut active = self.epochs.write().expect("mission epoch lock poisoned");
        active.clear();
        for epoch in epochs {
            let key = epoch.epoch_id.to_string();
            match active.get(&key) {
                Some(current) if current.version >= epoch.version => {}
                _ => {
                    active.insert(key, epoch);
                }
            }
        }
    }

    pub fn resolve(&self, request: &ResolveTimeRequest) -> Result<ResolveTimeOutput> {
        let mut instant = self.resolve_expression(&request.expression)?;
        instant.uncertainty_nanoseconds = instant
            .uncertainty_nanoseconds
            .saturating_add(request.additional_uncertainty_nanoseconds);
        self.project(instant)
    }

    pub fn convert(&self, request: &ConvertTimeRequest) -> Result<ConvertTimeOutput> {
        self.ensure_authority(&request.instant)?;
        let canonical = self.project(request.instant.clone())?;
        let (utc_timestamp, utc_is_leap_second) =
            timestamp_from_instant(&request.instant, &self.authority)?;
        let mut zoned = Vec::with_capacity(request.zone_ids.len());
        for zone_id in &request.zone_ids {
            validate_zone_id(zone_id)?;
            let zone = self.authority.tzdb.get(zone_id)?;
            zoned.push(ZonedRepresentation {
                zone_id: zone_id.clone(),
                rfc9557: render_leap_second(
                    utc_timestamp.to_zoned(zone).to_string(),
                    utc_is_leap_second,
                )?,
            });
        }
        let tai_julian_day = julian_day_tai(&request.instant);
        let epoch = Epoch::from_jde_tai(tai_julian_day);
        let scales = request
            .scales
            .iter()
            .copied()
            .map(|scale| scale_representation(scale, epoch, &canonical))
            .collect();
        Ok(ConvertTimeOutput {
            canonical,
            zoned,
            scales,
        })
    }

    pub fn evaluate_windows(
        &self,
        request: &EvaluateWindowsRequest,
    ) -> Result<EvaluateWindowsOutput> {
        let authority = request
            .left
            .first()
            .or_else(|| request.right.first())
            .map(|window| window.start.authority.clone())
            .unwrap_or_else(|| self.authority.binding.clone());
        if authority != self.authority.binding {
            bail!("window operation references a non-active temporal authority");
        }
        for window in request.left.iter().chain(&request.right) {
            validate_window(window)?;
            if window.start.authority != authority || window.end.authority != authority {
                bail!("window authorities must match");
            }
        }
        let left = window_set(&request.left);
        let right = window_set(&request.right);
        let ranges: RangeSet<i128> = match request.operation {
            WindowOperation::Union => left.union(&right).collect(),
            WindowOperation::Intersection => left.intersection(&right).collect(),
            WindowOperation::Difference => {
                let mut difference = left.clone();
                for range in right.iter() {
                    difference.remove(range.clone());
                }
                difference
            }
        };
        Ok(EvaluateWindowsOutput {
            windows: ranges
                .iter()
                .cloned()
                .map(|range| TimeWindow {
                    start: instant_from_nanos(range.start, authority.clone()),
                    end: instant_from_nanos(range.end, authority.clone()),
                })
                .collect(),
        })
    }

    pub fn expand_schedule(&self, request: &ExpandScheduleRequest) -> Result<ExpandScheduleOutput> {
        validate_window(&request.horizon)?;
        self.ensure_authority(&request.horizon.start)?;
        self.ensure_authority(&request.horizon.end)?;
        if request.maximum_occurrences == 0 || request.maximum_occurrences > 1_000_000 {
            bail!("maximum_occurrences must be in 1..=1000000");
        }
        if request.calendar.version == 0 || request.calendar.name.trim().is_empty() {
            bail!("calendar version and name must be set");
        }
        validate_zone_id(&request.calendar.zone_id)?;
        let zone = self.authority.tzdb.get(&request.calendar.zone_id)?;
        let excluded: BTreeSet<_> = request
            .calendar
            .excluded_dates
            .iter()
            .map(|value| {
                NaiveDate::parse_from_str(value, "%Y-%m-%d")
                    .map(|date| date.format("%Y-%m-%d").to_string())
                    .context("calendar excluded dates must use YYYY-MM-DD")
            })
            .collect::<Result<_>>()?;
        let mut occurrences = Vec::new();
        let mut truncated = false;
        for calendar_window in &request.calendar.windows {
            if calendar_window.recurrence.interval == 0 {
                bail!("calendar recurrence interval must be positive");
            }
            if calendar_window.recurrence.count == Some(0) {
                bail!("calendar recurrence count must be positive");
            }
            if let Some(until) = &calendar_window.recurrence.until {
                self.ensure_authority(until)?;
            }
            let start = parse_local_datetime(&calendar_window.start_local)?;
            let end = parse_local_datetime(&calendar_window.end_local)?;
            if end <= start {
                bail!("calendar window end must follow its start");
            }
            let duration = end - start;
            let mut local = start;
            let mut emitted = 0_u32;
            let mut examined_days = 0_u64;
            loop {
                if examined_days > 3_660_000 {
                    bail!("calendar expansion exceeds the ten-thousand-year safety horizon");
                }
                let include = recurrence_matches(
                    start,
                    local,
                    calendar_window.recurrence.frequency,
                    calendar_window.recurrence.interval,
                    &calendar_window.recurrence.weekdays,
                );
                if include && !excluded.contains(&local.date().format("%Y-%m-%d").to_string()) {
                    let start_instant =
                        civil_to_instant(local, &zone, Disambiguation::Reject, &self.authority)?;
                    let end_instant = civil_to_instant(
                        local + duration,
                        &zone,
                        Disambiguation::Reject,
                        &self.authority,
                    )?;
                    if calendar_window
                        .recurrence
                        .until
                        .as_ref()
                        .is_some_and(|until| {
                            start_instant.total_nanoseconds() > until.total_nanoseconds()
                        })
                    {
                        break;
                    }
                    emitted += 1;
                    if calendar_window
                        .recurrence
                        .count
                        .is_some_and(|count| emitted > count)
                    {
                        break;
                    }
                    let window = TimeWindow {
                        start: start_instant,
                        end: end_instant,
                    };
                    if window.start.total_nanoseconds() >= request.horizon.end.total_nanoseconds() {
                        break;
                    }
                    if window.end.total_nanoseconds() > request.horizon.start.total_nanoseconds() {
                        if occurrences.len() >= request.maximum_occurrences as usize {
                            truncated = true;
                            break;
                        }
                        occurrences.push(ScheduleOccurrence {
                            sequence: 0,
                            window,
                            labels: calendar_window.labels.clone(),
                        });
                    }
                }
                local += Duration::days(1);
                examined_days += 1;
            }
            if truncated {
                break;
            }
        }
        occurrences.sort_by_key(|occurrence| occurrence.window.start.total_nanoseconds());
        for (index, occurrence) in occurrences.iter_mut().enumerate() {
            occurrence.sequence = index.try_into().unwrap_or(u32::MAX);
        }
        Ok(ExpandScheduleOutput {
            occurrences,
            truncated,
        })
    }

    pub fn validate_timeline(
        &self,
        request: &ValidateTimelineRequest,
    ) -> Result<ValidateTimelineOutput> {
        if request.points.len() > 100_000 || request.constraints.len() > 1_000_000 {
            bail!("timeline exceeds the supported point or constraint limit");
        }
        let mut points = BTreeMap::new();
        for point in &request.points {
            if point.name.trim().is_empty() || points.contains_key(&point.name) {
                bail!("timeline point names must be non-empty and unique");
            }
            points.insert(point.name.clone(), self.resolve_expression(&point.at)?);
        }
        let mut violations = Vec::new();
        for (index, constraint) in request.constraints.iter().enumerate() {
            if constraint
                .maximum_separation_nanoseconds
                .is_some_and(|maximum| maximum < constraint.minimum_separation_nanoseconds)
            {
                bail!("timeline maximum separation must not be below its minimum");
            }
            let Some(predecessor) = points.get(&constraint.predecessor) else {
                bail!("timeline constraint references an unknown predecessor");
            };
            let Some(successor) = points.get(&constraint.successor) else {
                bail!("timeline constraint references an unknown successor");
            };
            let separation = successor.total_nanoseconds() - predecessor.total_nanoseconds();
            if separation < i128::from(constraint.minimum_separation_nanoseconds) {
                violations.push(TimelineViolation {
                    constraint_index: index.try_into().unwrap_or(u32::MAX),
                    message: "minimum separation is not satisfied".to_owned(),
                });
            } else if constraint
                .maximum_separation_nanoseconds
                .is_some_and(|maximum| separation > i128::from(maximum))
            {
                violations.push(TimelineViolation {
                    constraint_index: index.try_into().unwrap_or(u32::MAX),
                    message: "maximum separation is exceeded".to_owned(),
                });
            }
        }
        Ok(ValidateTimelineOutput {
            valid: violations.is_empty(),
            violations,
        })
    }

    fn resolve_expression(&self, expression: &TimeExpression) -> Result<TimeInstant> {
        let (tai_seconds_since_1970, nanosecond) = match expression {
            TimeExpression::Rfc3339 { value } => {
                let timestamp = Timestamp::from_str(value).context("invalid RFC 3339 timestamp")?;
                instant_parts_from_timestamp(timestamp, &self.authority)?
            }
            TimeExpression::Rfc9557 {
                value,
                disambiguation,
            } => {
                let parser = jiff::fmt::temporal::DateTimeParser::new()
                    .disambiguation(to_jiff_disambiguation(*disambiguation));
                let zoned = parser
                    .parse_zoned_with(&self.authority.tzdb, value)
                    .context("invalid RFC 9557 timestamp")?;
                instant_parts_from_timestamp(zoned.timestamp(), &self.authority)?
            }
            TimeExpression::Civil { value } => {
                if value.tzdb_release_id != self.authority.binding.tzdb_release_id {
                    bail!("civil time references a non-active TZDB authority");
                }
                validate_zone_id(&value.zone_id)?;
                let zone = self.authority.tzdb.get(&value.zone_id)?;
                let datetime = civil::DateTime::from_str(&value.local_datetime)
                    .context("invalid civil datetime")?;
                let zoned = zone
                    .to_ambiguous_zoned(datetime)
                    .disambiguate(to_jiff_disambiguation(value.disambiguation))?;
                instant_parts_from_timestamp(zoned.timestamp(), &self.authority)?
            }
            TimeExpression::Unix {
                seconds,
                nanosecond,
            } => {
                validate_nanosecond(*nanosecond)?;
                (
                    seconds
                        .checked_add(self.authority.leap_seconds.offset_for_utc(*seconds)?)
                        .context("Unix timestamp exceeds the supported range")?,
                    *nanosecond,
                )
            }
            TimeExpression::Tai {
                seconds_since_1970,
                nanosecond,
            } => {
                validate_nanosecond(*nanosecond)?;
                (*seconds_since_1970, *nanosecond)
            }
            TimeExpression::Gps {
                week,
                seconds_of_week,
            } => {
                if !seconds_of_week.is_finite()
                    || !(0.0..SECONDS_PER_WEEK).contains(seconds_of_week)
                {
                    bail!("GPS seconds_of_week must be finite and in [0, 604800)");
                }
                let total = f64::from(*week) * SECONDS_PER_WEEK + seconds_of_week;
                split_fractional_seconds(total, GPS_EPOCH_TAI_SECONDS_SINCE_1970)?
            }
            TimeExpression::JulianTai { day } => {
                if !day.is_finite() {
                    bail!("Julian TAI day must be finite");
                }
                let epoch = Epoch::from_jde_tai(*day);
                let validated_day = epoch.to_jde_tai_days();
                split_fractional_seconds((validated_day - JULIAN_DAY_AT_1970_TAI) * 86_400.0, 0)?
            }
            TimeExpression::MilitaryDtg { value } => {
                let timestamp = parse_military_dtg(value)?;
                instant_parts_from_unix(
                    timestamp.timestamp(),
                    timestamp.timestamp_subsec_nanos(),
                    &self.authority,
                )?
            }
            TimeExpression::EpochRelative {
                epoch_id,
                offset_nanoseconds,
            } => {
                let epochs = self.epochs.read().expect("mission epoch lock poisoned");
                let epoch = epochs
                    .get(epoch_id.as_str())
                    .context("mission epoch is not active")?;
                let total = epoch.instant.total_nanoseconds() + i128::from(*offset_nanoseconds);
                let instant = instant_from_nanos(total, self.authority.binding.clone());
                return Ok(instant);
            }
        };
        Ok(TimeInstant {
            tai_seconds_since_1970,
            nanosecond,
            uncertainty_nanoseconds: 0,
            authority: self.authority.binding.clone(),
        })
    }

    fn project(&self, instant: TimeInstant) -> Result<ResolveTimeOutput> {
        self.ensure_authority(&instant)?;
        let utc_coordinate = self
            .authority
            .leap_seconds
            .utc_from_tai(instant.tai_seconds_since_1970)?;
        let utc_seconds = utc_coordinate.unix_seconds;
        let timestamp = Timestamp::new(utc_seconds, instant.nanosecond as i32)?;
        let gps_seconds = instant.tai_seconds_since_1970 - GPS_EPOCH_TAI_SECONDS_SINCE_1970;
        let (gps_week, gps_seconds_of_week) = if gps_seconds < 0 {
            (None, None)
        } else {
            (
                Some((gps_seconds / SECONDS_PER_WEEK as i64) as u32),
                Some(
                    (gps_seconds % SECONDS_PER_WEEK as i64) as f64
                        + f64::from(instant.nanosecond) / 1_000_000_000.0,
                ),
            )
        };
        let utc = Utc
            .timestamp_opt(utc_seconds, instant.nanosecond)
            .single()
            .context("instant is outside the UTC projection range")?;
        let julian_day_tai = julian_day_tai(&instant);
        Ok(ResolveTimeOutput {
            instant,
            utc_rfc3339: render_leap_second(timestamp.to_string(), utc_coordinate.is_leap_second)?,
            utc_is_leap_second: utc_coordinate.is_leap_second,
            military_dtg: utc.format("%d%H%MZ%b%y").to_string().to_uppercase(),
            unix_seconds: utc_seconds,
            gps_week,
            gps_seconds_of_week,
            julian_day_tai,
        })
    }

    fn ensure_authority(&self, instant: &TimeInstant) -> Result<()> {
        if instant.authority != self.authority.binding {
            bail!("instant references a non-active temporal authority");
        }
        validate_nanosecond(instant.nanosecond)
    }
}

fn to_jiff_disambiguation(value: Disambiguation) -> tz::Disambiguation {
    match value {
        Disambiguation::Reject => tz::Disambiguation::Reject,
        Disambiguation::Earlier => tz::Disambiguation::Earlier,
        Disambiguation::Later => tz::Disambiguation::Later,
    }
}

fn validate_zone_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || value.starts_with('/')
        || value.contains("..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'+'))
    {
        bail!("invalid IANA zone identifier");
    }
    Ok(())
}

fn validate_nanosecond(value: u32) -> Result<()> {
    if value >= 1_000_000_000 {
        bail!("nanosecond must be below one billion");
    }
    Ok(())
}

fn instant_parts_from_timestamp(
    timestamp: Timestamp,
    authority: &AuthorityContext,
) -> Result<(i64, u32)> {
    let total = timestamp.as_nanosecond();
    let utc_seconds = total.div_euclid(NANOS_PER_SECOND) as i64;
    let nanosecond = total.rem_euclid(NANOS_PER_SECOND) as u32;
    instant_parts_from_unix(utc_seconds, nanosecond, authority)
}

fn instant_parts_from_unix(
    utc_seconds: i64,
    nanosecond: u32,
    authority: &AuthorityContext,
) -> Result<(i64, u32)> {
    validate_nanosecond(nanosecond)?;
    Ok((
        utc_seconds
            .checked_add(authority.leap_seconds.offset_for_utc(utc_seconds)?)
            .context("timestamp exceeds the supported range")?,
        nanosecond,
    ))
}

fn timestamp_from_instant(
    instant: &TimeInstant,
    authority: &AuthorityContext,
) -> Result<(Timestamp, bool)> {
    let coordinate = authority
        .leap_seconds
        .utc_from_tai(instant.tai_seconds_since_1970)?;
    Ok((
        Timestamp::new(coordinate.unix_seconds, instant.nanosecond as i32)?,
        coordinate.is_leap_second,
    ))
}

fn render_leap_second(mut value: String, is_leap_second: bool) -> Result<String> {
    if !is_leap_second {
        return Ok(value);
    }
    let time = value
        .find('T')
        .context("UTC projection has no time component")?;
    let seconds = time + 7;
    if value.get(seconds..seconds + 2) != Some("59") {
        bail!("leap-second projection does not follow second 59");
    }
    value.replace_range(seconds..seconds + 2, "60");
    Ok(value)
}

fn split_fractional_seconds(seconds: f64, base: i64) -> Result<(i64, u32)> {
    if !seconds.is_finite() {
        bail!("time coordinate must be finite");
    }
    let whole = seconds.floor();
    if whole < i64::MIN as f64 || whole > i64::MAX as f64 {
        bail!("time coordinate exceeds the supported range");
    }
    let mut nanosecond = ((seconds - whole) * 1_000_000_000.0).round() as i64;
    let mut whole = whole as i64;
    if nanosecond == 1_000_000_000 {
        whole += 1;
        nanosecond = 0;
    }
    Ok((
        base.checked_add(whole)
            .context("time coordinate overflow")?,
        nanosecond as u32,
    ))
}

fn julian_day_tai(instant: &TimeInstant) -> f64 {
    JULIAN_DAY_AT_1970_TAI
        + instant.tai_seconds_since_1970 as f64 / 86_400.0
        + f64::from(instant.nanosecond) / 86_400_000_000_000.0
}

fn scale_representation(
    scale: TimeScale,
    epoch: Epoch,
    canonical: &ResolveTimeOutput,
) -> ScaleRepresentation {
    let (seconds, reference_epoch) = match scale {
        TimeScale::Utc => (
            canonical.unix_seconds as f64 + f64::from(canonical.instant.nanosecond) / 1e9,
            "1970-01-01T00:00:00Z",
        ),
        TimeScale::Tai => (
            canonical.instant.tai_seconds_since_1970 as f64
                + f64::from(canonical.instant.nanosecond) / 1e9,
            "1970-01-01T00:00:00 TAI",
        ),
        TimeScale::Tt => (
            epoch.to_time_scale(HifiTimeScale::TT).duration.to_seconds(),
            "J1900 TT",
        ),
        TimeScale::Tdb => (
            epoch
                .to_time_scale(HifiTimeScale::TDB)
                .duration
                .to_seconds(),
            "J2000 TDB",
        ),
        TimeScale::Gpst => (epoch.to_gpst_seconds(), "1980-01-06T00:00:00 GPST"),
        TimeScale::Gst => (
            epoch
                .to_time_scale(HifiTimeScale::GST)
                .duration
                .to_seconds(),
            "1999-08-22T00:00:00 GST",
        ),
    };
    ScaleRepresentation {
        scale,
        seconds,
        reference_epoch: reference_epoch.to_owned(),
    }
}

fn validate_window(window: &TimeWindow) -> Result<()> {
    validate_nanosecond(window.start.nanosecond)?;
    validate_nanosecond(window.end.nanosecond)?;
    if window.start.authority != window.end.authority {
        bail!("window bounds must use the same authority");
    }
    if window.start.total_nanoseconds() >= window.end.total_nanoseconds() {
        bail!("window end must follow its start");
    }
    Ok(())
}

fn window_set(windows: &[TimeWindow]) -> RangeSet<i128> {
    windows
        .iter()
        .map(|window| window.start.total_nanoseconds()..window.end.total_nanoseconds())
        .collect()
}

fn instant_from_nanos(total: i128, authority: crate::contract::AuthorityBinding) -> TimeInstant {
    TimeInstant {
        tai_seconds_since_1970: total.div_euclid(NANOS_PER_SECOND) as i64,
        nanosecond: total.rem_euclid(NANOS_PER_SECOND) as u32,
        uncertainty_nanoseconds: 0,
        authority,
    }
}

fn parse_local_datetime(value: &str) -> Result<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f"))
        .context("calendar local datetime must use ISO 8601 without an offset")
}

fn civil_to_instant(
    local: NaiveDateTime,
    zone: &tz::TimeZone,
    disambiguation: Disambiguation,
    authority: &AuthorityContext,
) -> Result<TimeInstant> {
    let datetime = civil::DateTime::from_str(&local.format("%Y-%m-%dT%H:%M:%S%.f").to_string())?;
    let zoned = zone
        .to_ambiguous_zoned(datetime)
        .disambiguate(to_jiff_disambiguation(disambiguation))?;
    let (tai_seconds_since_1970, nanosecond) =
        instant_parts_from_timestamp(zoned.timestamp(), authority)?;
    Ok(TimeInstant {
        tai_seconds_since_1970,
        nanosecond,
        uncertainty_nanoseconds: 0,
        authority: authority.binding.clone(),
    })
}

fn recurrence_matches(
    start: NaiveDateTime,
    candidate: NaiveDateTime,
    frequency: RecurrenceFrequency,
    interval: u32,
    weekdays: &[Weekday],
) -> bool {
    let elapsed_days = (candidate.date() - start.date()).num_days();
    match frequency {
        RecurrenceFrequency::Daily => elapsed_days % i64::from(interval) == 0,
        RecurrenceFrequency::Weekly => {
            let week = elapsed_days.div_euclid(7);
            week % i64::from(interval) == 0
                && (weekdays.is_empty() && candidate.weekday() == start.weekday()
                    || weekdays
                        .iter()
                        .any(|weekday| chrono_weekday(*weekday) == candidate.weekday()))
        }
    }
}

fn chrono_weekday(weekday: Weekday) -> chrono::Weekday {
    match weekday {
        Weekday::Monday => chrono::Weekday::Mon,
        Weekday::Tuesday => chrono::Weekday::Tue,
        Weekday::Wednesday => chrono::Weekday::Wed,
        Weekday::Thursday => chrono::Weekday::Thu,
        Weekday::Friday => chrono::Weekday::Fri,
        Weekday::Saturday => chrono::Weekday::Sat,
        Weekday::Sunday => chrono::Weekday::Sun,
    }
}

fn parse_military_dtg(value: &str) -> Result<chrono::DateTime<Utc>> {
    let value = value.trim().to_uppercase();
    let has_seconds = value.len() == 14;
    if value.len() != 12 && !has_seconds {
        bail!("military DTG must use DDHHMMZMONYY or DDHHMMSSZMONYY");
    }
    let zone_index = if has_seconds { 8 } else { 6 };
    let zone_letter = value.as_bytes()[zone_index] as char;
    let digits = |range: std::ops::Range<usize>| -> Result<u32> {
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

fn nato_zone_offset_hours(letter: char) -> Result<i32> {
    match letter {
        'Z' => Ok(0),
        'A'..='I' => Ok((letter as u8 - b'A' + 1).into()),
        'K'..='M' => Ok((letter as u8 - b'A').into()),
        'N'..='Y' => Ok(-i32::from(letter as u8 - b'N' + 1)),
        'J' => bail!("military zone J denotes local time and requires an explicit IANA zone"),
        _ => bail!("invalid military time-zone letter"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        authority::LeapSecondTable,
        contract::{
            AuthorityReleaseId, CalendarId, CalendarWindow, MissionEpochId, OperationalCalendar,
            RecurrenceRule, TimelineConstraint, TimelinePoint,
        },
    };

    const LEAPS: &str = "2272060800 10\n2287785600 11\n2303683200 12\n2335219200 13\n2366755200 14\n2398291200 15\n2429913600 16\n2461449600 17\n2492985600 18\n2524521600 19\n2571782400 20\n2603318400 21\n2634854400 22\n2698012800 23\n2776982400 24\n2840140800 25\n2871676800 26\n2918937600 27\n2950473600 28\n2982009600 29\n3029443200 30\n3076704000 31\n3124137600 32\n3345062400 33\n3439756800 34\n3550089600 35\n3644697600 36\n3692217600 37\n";

    fn engine() -> TemporalEngine {
        let authority = AuthorityContext::from_paths(
            AuthorityReleaseId::new("time-release-tzdb-test").unwrap(),
            AuthorityReleaseId::new("time-release-leaps-test").unwrap(),
            "/usr/share/zoneinfo",
            LeapSecondTable::from_iana_content(LEAPS).unwrap(),
        )
        .unwrap();
        TemporalEngine::new(authority)
    }

    #[test]
    fn resolves_rfc3339_gps_and_military_dtg_to_one_instant() {
        let engine = engine();
        let rfc = engine
            .resolve(&ResolveTimeRequest {
                expression: TimeExpression::Rfc3339 {
                    value: "2024-06-01T12:30:00Z".to_owned(),
                },
                additional_uncertainty_nanoseconds: 0,
            })
            .unwrap();
        let dtg = engine
            .resolve(&ResolveTimeRequest {
                expression: TimeExpression::MilitaryDtg {
                    value: "011230ZJUN24".to_owned(),
                },
                additional_uncertainty_nanoseconds: 0,
            })
            .unwrap();
        assert_eq!(rfc.instant, dtg.instant);
        let gps = engine
            .resolve(&ResolveTimeRequest {
                expression: TimeExpression::Gps {
                    week: rfc.gps_week.unwrap(),
                    seconds_of_week: rfc.gps_seconds_of_week.unwrap(),
                },
                additional_uncertainty_nanoseconds: 0,
            })
            .unwrap();
        assert_eq!(rfc.instant, gps.instant);
    }

    #[test]
    fn rejects_dst_fold_without_an_explicit_choice() {
        let engine = engine();
        let request = ResolveTimeRequest {
            expression: TimeExpression::Civil {
                value: crate::contract::CivilTime {
                    local_datetime: "2024-11-03T01:30:00".to_owned(),
                    zone_id: "America/New_York".to_owned(),
                    tzdb_release_id: engine.authority.binding.tzdb_release_id.clone(),
                    disambiguation: Disambiguation::Reject,
                },
            },
            additional_uncertainty_nanoseconds: 0,
        };
        assert!(engine.resolve(&request).is_err());
    }

    #[test]
    fn half_open_window_algebra_coalesces_adjacent_ranges() {
        let engine = engine();
        let authority = engine.authority.binding.clone();
        let instant = |seconds| TimeInstant {
            tai_seconds_since_1970: seconds,
            nanosecond: 0,
            uncertainty_nanoseconds: 0,
            authority: authority.clone(),
        };
        let output = engine
            .evaluate_windows(&EvaluateWindowsRequest {
                operation: WindowOperation::Union,
                left: vec![TimeWindow {
                    start: instant(100),
                    end: instant(200),
                }],
                right: vec![TimeWindow {
                    start: instant(200),
                    end: instant(300),
                }],
            })
            .unwrap();
        assert_eq!(output.windows.len(), 1);
        assert_eq!(output.windows[0].start.tai_seconds_since_1970, 100);
        assert_eq!(output.windows[0].end.tai_seconds_since_1970, 300);
    }

    #[test]
    fn expands_local_schedule_across_a_dst_transition() {
        let engine = engine();
        let resolve = |value: &str| {
            engine
                .resolve(&ResolveTimeRequest {
                    expression: TimeExpression::Rfc3339 {
                        value: value.to_owned(),
                    },
                    additional_uncertainty_nanoseconds: 0,
                })
                .unwrap()
                .instant
        };
        let output = engine
            .expand_schedule(&ExpandScheduleRequest {
                calendar: OperationalCalendar {
                    calendar_id: CalendarId::new("calendar-dst-test").unwrap(),
                    version: 1,
                    name: "Eastern operations".to_owned(),
                    zone_id: "America/New_York".to_owned(),
                    windows: vec![CalendarWindow {
                        start_local: "2024-03-08T09:00:00".to_owned(),
                        end_local: "2024-03-08T17:00:00".to_owned(),
                        recurrence: RecurrenceRule {
                            frequency: RecurrenceFrequency::Daily,
                            interval: 1,
                            weekdays: Vec::new(),
                            count: Some(4),
                            until: None,
                        },
                        labels: vec!["day-shift".to_owned()],
                    }],
                    excluded_dates: Vec::new(),
                },
                horizon: TimeWindow {
                    start: resolve("2024-03-08T00:00:00Z"),
                    end: resolve("2024-03-13T00:00:00Z"),
                },
                maximum_occurrences: 10,
            })
            .unwrap();
        let starts: Vec<_> = output
            .occurrences
            .iter()
            .map(|occurrence| {
                engine
                    .project(occurrence.window.start.clone())
                    .unwrap()
                    .utc_rfc3339
            })
            .collect();
        assert_eq!(output.occurrences.len(), 4);
        assert_eq!(starts[0], "2024-03-08T14:00:00Z");
        assert_eq!(starts[1], "2024-03-09T14:00:00Z");
        assert_eq!(starts[2], "2024-03-10T13:00:00Z");
        assert_eq!(starts[3], "2024-03-11T13:00:00Z");
    }

    #[test]
    fn reports_timeline_separation_violations() {
        let engine = engine();
        let output = engine
            .validate_timeline(&ValidateTimelineRequest {
                points: vec![
                    TimelinePoint {
                        name: "depart".to_owned(),
                        at: TimeExpression::Rfc3339 {
                            value: "2024-06-01T12:00:00Z".to_owned(),
                        },
                    },
                    TimelinePoint {
                        name: "arrive".to_owned(),
                        at: TimeExpression::Rfc3339 {
                            value: "2024-06-01T12:05:00Z".to_owned(),
                        },
                    },
                ],
                constraints: vec![TimelineConstraint {
                    predecessor: "depart".to_owned(),
                    successor: "arrive".to_owned(),
                    minimum_separation_nanoseconds: 600_000_000_000,
                    maximum_separation_nanoseconds: Some(900_000_000_000),
                }],
            })
            .unwrap();
        assert!(!output.valid);
        assert_eq!(output.violations.len(), 1);
        assert_eq!(output.violations[0].constraint_index, 0);
    }

    #[test]
    fn mission_relative_resolution_selects_the_highest_epoch_version() {
        let engine = engine();
        let authority = engine.authority.binding.clone();
        let epoch = |version, seconds| MissionEpoch {
            epoch_id: MissionEpochId::new("epoch-launch").unwrap(),
            name: "Launch".to_owned(),
            instant: TimeInstant {
                tai_seconds_since_1970: seconds,
                nanosecond: 0,
                uncertainty_nanoseconds: 0,
                authority: authority.clone(),
            },
            version,
        };
        engine.replace_epochs([epoch(4, 4_000), epoch(2, 2_000), epoch(3, 3_000)]);
        let resolved = engine
            .resolve(&ResolveTimeRequest {
                expression: TimeExpression::EpochRelative {
                    epoch_id: MissionEpochId::new("epoch-launch").unwrap(),
                    offset_nanoseconds: 1_000_000_000,
                },
                additional_uncertainty_nanoseconds: 0,
            })
            .unwrap();
        assert_eq!(resolved.instant.tai_seconds_since_1970, 4_001);
    }

    #[test]
    fn projects_the_positive_leap_second_without_collapsing_its_identity() {
        let engine = engine();
        let midnight = engine
            .resolve(&ResolveTimeRequest {
                expression: TimeExpression::Rfc3339 {
                    value: "2017-01-01T00:00:00Z".to_owned(),
                },
                additional_uncertainty_nanoseconds: 0,
            })
            .unwrap();
        let leap = TimeInstant {
            tai_seconds_since_1970: midnight.instant.tai_seconds_since_1970 - 1,
            nanosecond: 500_000_000,
            uncertainty_nanoseconds: 0,
            authority: engine.authority.binding.clone(),
        };
        let projected = engine
            .convert(&ConvertTimeRequest {
                instant: leap,
                zone_ids: vec!["UTC".to_owned(), "America/New_York".to_owned()],
                scales: vec![TimeScale::Tai],
            })
            .unwrap();
        assert!(projected.canonical.utc_is_leap_second);
        assert_eq!(projected.canonical.utc_rfc3339, "2016-12-31T23:59:60.5Z");
        assert!(projected.zoned[0].rfc9557.contains("23:59:60.5"));
        assert!(projected.zoned[1].rfc9557.contains("18:59:60.5"));
    }
}
