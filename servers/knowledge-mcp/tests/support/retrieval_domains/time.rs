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
    let calendar = OperationalCalendarValue {
        calendar_id: id(format!("calendar-{}", s.key))?,
        version: TimeVersion::FIRST,
        name: s.title.clone(),
        zone_id: "America/El_Salvador".into(),
        windows: vec![
            CalendarWindowValue {
                start_local: "2026-10-02T08:00:00".into(),
                end_local: "2026-10-02T16:00:00".into(),
                recurrence: RecurrenceRuleValue {
                    frequency: RecurrenceFrequency::Weekly,
                    interval: 1,
                    weekdays: vec![Weekday::Friday],
                    count: Some(4),
                    until: None,
                }
                .build()?,
                labels: vec![s.action.clone()],
            }
            .build()?,
        ],
        excluded_dates: vec!["2026-10-16".into()],
    }
    .build()?;
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
                TimeAuthoritySource::Bootstrap {}
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

#[test]
fn time_corpus_hashes_current_owner_bytes_and_access_policy() -> Result<()> {
    let scenarios: Vec<Scenario> =
        serde_json::from_str(include_str!("../../../evaluation/scenarios.json"))?;
    let mut builder = Builder::default();
    add(&mut builder, &scenarios[0], 0)?;
    assert_eq!(builder.members.len(), 3);
    for member in &builder.members {
        let wire: serde_json::Value = serde_json::from_str(&member.text)?;
        let (current, retired, roundtrip) = if wire.get("calendarId").is_some() {
            let owner: OperationalCalendar = serde_json::from_str(&member.text)?;
            ("calendarId", "calendar_id", serde_json::to_string(&owner)?)
        } else if wire.get("epochId").is_some() {
            let owner: MissionEpoch = serde_json::from_str(&member.text)?;
            ("epochId", "epoch_id", serde_json::to_string(&owner)?)
        } else {
            let owner: TemporalEvent = serde_json::from_str(&member.text)?;
            ("eventId", "event_id", serde_json::to_string(&owner)?)
        };
        assert_eq!(roundtrip, member.text);
        assert_eq!(
            member.observation.content_sha256(),
            &content_digest(&roundtrip)
        );
        let revision = content_digest(&serde_json::to_string(&(
            &member.text,
            member.observation.access(),
        ))?);
        assert_eq!(
            member.observation.revision().to_string(),
            revision.to_string()
        );
        for mixed in [false, true] {
            let mut old = wire.clone();
            let value = old[current].clone();
            if !mixed {
                old.as_object_mut().unwrap().remove(current);
            }
            old[retired] = value;
            let refused = match current {
                "calendarId" => serde_json::from_value::<OperationalCalendar>(old.clone()).is_err(),
                "epochId" => serde_json::from_value::<MissionEpoch>(old.clone()).is_err(),
                _ => serde_json::from_value::<TemporalEvent>(old.clone()).is_err(),
            };
            assert!(refused, "{current} mixed={mixed}");
            assert_ne!(
                content_digest(&serde_json::to_string(&old)?),
                *member.observation.content_sha256()
            );
        }
    }
    Ok(())
}
