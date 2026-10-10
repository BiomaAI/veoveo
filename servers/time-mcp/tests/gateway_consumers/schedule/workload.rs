//! A real recurrence scan with a small independently calculated output.
use super::*;
#[test]
fn historical_daily_scan_has_small_independent_output() -> Result<()> {
    use chrono::NaiveDate;
    use veoveo_time_mcp::{
        ScheduleOccurrence, TimeInstant, TimeWindow,
        authority::{AuthorityContext, LeapSecondTable},
        engine::TemporalEngine,
    };
    let base = super::tests::fixture()?;
    // UTC TZif is an actual admitted authority, with a fixed ten-second leap
    // table for this native fixture. Installed inputs supply their own releases.
    let authority = AuthorityContext::from_paths(
        base.authority.clone(),
        "/usr/share/zoneinfo",
        LeapSecondTable::from_iana_content("2272060800 10\n")?,
    )?;
    let engine = TemporalEngine::new(authority);
    let start = NaiveDate::from_ymd_opt(1, 1, 1).context("selected civil start")?;
    let last = start
        .checked_add_signed(chrono::Duration::days(999_999))
        .context("selected civil end")?;
    let unix = last.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
    let instant = |seconds: i64| {
        TimeInstant::from_total_nanoseconds(
            i128::from(seconds + 10) * 1_000_000_000,
            0,
            base.authority.binding(),
        )
    };
    let horizon = TimeWindow::new(instant(unix)?, instant(unix + 86_400)?)?;
    let mut calendar = serde_json::to_value(&base.request.calendar)?;
    calendar["zoneId"] = "UTC".into();
    calendar["excludedDates"] = serde_json::json!([]);
    calendar["windows"] = serde_json::json!([{
        "startLocal":"0001-01-01T00:00:00","endLocal":"0001-01-01T01:00:00",
        "recurrence":{"frequency":"daily","interval":1,"weekdays":[],"count":1000000,"until":null},
        "labels":[]
    }]);
    let request = ExpandScheduleRequest {
        calendar: serde_json::from_value(calendar)?,
        horizon,
        maximum_occurrences: 4,
    };
    let expected = ExpandScheduleOutput {
        occurrences: vec![ScheduleOccurrence {
            sequence: 0,
            window: TimeWindow::new(instant(unix)?, instant(unix + 3600)?)?,
            labels: vec![],
        }],
        truncated: false,
    };
    let began = std::time::Instant::now();
    let actual = engine.expand_schedule(&request)?;
    ensure!(
        actual == expected,
        "historical scan differs from independent UTC arithmetic"
    );
    eprintln!(
        "Time native million-day recurrence sample: {}ms; installed Working is still required",
        began.elapsed().as_millis()
    );
    Ok(())
}
