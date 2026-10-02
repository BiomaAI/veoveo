use super::*;
use veoveo_time_mcp::contract::*;

pub(super) struct Scheduled {
    pub calendar: EvaluationMemberId,
    pub epoch: EvaluationMemberId,
    pub event: EvaluationMemberId,
}
fn id<T: std::str::FromStr<Err = String>>(value: impl AsRef<str>) -> Result<T> {
    value.as_ref().parse().map_err(anyhow::Error::msg)
}
fn instant(index: usize) -> Result<TimeInstant> {
    Ok(TimeInstant {
        tai_seconds_since_1970: 1_791_158_400 + index as i64 * 86400,
        nanosecond: SubsecondNanoseconds::ZERO,
        uncertainty_nanoseconds: 1000,
        authority: AuthorityBinding::new(
            id("time-release-fixture-acquired-tzdb")?,
            id("time-release-fixture-acquired-leaps")?,
        )?,
    })
}
pub(super) fn add(builder: &mut Builder, s: &Scenario, index: usize) -> Result<Scheduled> {
    let calendar = OperationalCalendar {
        calendar_id: id(format!("calendar-{}", s.key))?,
        version: TimeVersion::FIRST,
        name: s.title.clone(),
        zone_id: "America/El_Salvador".into(),
        windows: vec![CalendarWindow {
            start_local: "2026-10-02T08:00:00".into(),
            end_local: "2026-10-02T16:00:00".into(),
            recurrence: RecurrenceRule {
                frequency: RecurrenceFrequency::Weekly,
                interval: 1,
                weekdays: vec![Weekday::Friday],
                count: Some(4),
                until: None,
            },
            labels: vec![s.action.clone()],
        }],
        excluded_dates: vec!["2026-10-16".into()],
    };
    let epoch = MissionEpoch {
        epoch_id: id(format!("epoch-{}", s.key))?,
        name: format!("{} mission reference", s.title),
        instant: instant(index)?,
        version: TimeVersion::FIRST,
    };
    let event = TemporalEvent {
        event_id: id(format!("event-{}", s.key))?,
        name: s.action.clone(),
        due: instant(index)?,
        state: TemporalEventState::Scheduled,
        record_version: TimeVersion::FIRST,
    };
    Ok(Scheduled {
        calendar: builder.add(
            TimeKnowledgeCollection::Calendars.descriptor(),
            TimeResource::Calendar {
                id: calendar.calendar_id.clone(),
                version: calendar.version,
            }
            .to_uri()?,
            &calendar.name,
            serde_json::to_string(&calendar)?,
            ReadPolicy::Tenant {},
        )?,
        epoch: builder.add(
            TimeKnowledgeCollection::Epochs.descriptor(),
            TimeResource::EpochVersion {
                id: epoch.epoch_id.clone(),
                version: epoch.version,
            }
            .to_uri()?,
            &epoch.name,
            serde_json::to_string(&epoch)?,
            ReadPolicy::Tenant {},
        )?,
        event: builder.add(
            TimeKnowledgeCollection::Events.descriptor(),
            TimeResource::Event(event.event_id.clone()).to_uri()?,
            &event.name,
            serde_json::to_string(&event)?,
            ReadPolicy::Subjects {},
        )?,
    })
}
pub(super) fn authorities(builder: &mut Builder) -> Result<()> {
    for (key, kind, title, query) in [
        (
            "tzdb",
            AuthorityDatasetKind::Tzdb,
            "fixture time-zone rule set",
            "Find the acquired time-zone authority release used to interpret local wall-clock schedules.",
        ),
        (
            "leaps",
            AuthorityDatasetKind::LeapSeconds,
            "fixture leap-second bulletin",
            "Find the acquired leap-second authority release used to relate atomic time to civil UTC.",
        ),
    ] {
        for bootstrap in [false, true] {
            let release_id = id(format!(
                "time-release-fixture-{}-{key}",
                if bootstrap { "bootstrap" } else { "acquired" }
            ))?;
            let uri = if bootstrap {
                TimeAuthorityReleaseUri::bootstrap(&release_id)
            } else {
                TimeAuthorityReleaseUri::new(&release_id)
            };
            let source = if bootstrap {
                TimeAuthoritySource::Bootstrap
            } else {
                TimeAuthoritySource::Acquisition {
                    source_id: id(format!("time-source-fixture-{key}"))?,
                    acquisition_id: id(format!("time-acquisition-fixture-{key}"))?,
                }
            };
            let reference = TimeAuthorityReference::new(
                uri.clone(),
                kind,
                source,
                content_digest(title),
                title.into(),
            )?;
            let collection = if bootstrap {
                TimeKnowledgeCollection::BootstrapAuthorities
            } else {
                TimeKnowledgeCollection::AuthorityReleases
            };
            let member = builder.add(
                collection.descriptor(),
                uri.to_uri()?,
                title,
                serde_json::to_string(&reference)?,
                ReadPolicy::Tenant {},
            )?;
            builder.query(
                &format!(
                    "authority-{}-{key}",
                    if bootstrap { "bootstrap" } else { "acquired" }
                ),
                &if bootstrap {
                    query.replace("acquired", "packaged bootstrap")
                } else {
                    query.into()
                },
                [member],
            )?;
        }
    }
    Ok(())
}
