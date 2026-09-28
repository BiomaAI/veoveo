use veoveo_time_mcp::contract::{
    AuthorityReleaseId, CalendarCursor, CalendarId, EpochCursor, EventCursor, MissionEpochId,
    SubsecondNanoseconds, TemporalEventId, TimeDocument, TimeResource, TimeVersion, TimeZoneId,
};
use veoveo_types::{ResourceAddress, ResourceUri};

fn version() -> TimeVersion {
    TimeVersion::new(12).unwrap()
}

#[test]
fn rfc6570_declarations_expand_to_the_typed_domain_builders() {
    use iri_string::template::simple_context::SimpleContext;
    use veoveo_time_mcp::uris;
    use veoveo_types::ResourceTemplateUri;

    let calendar = CalendarId::new("calendar-one").unwrap();
    let epoch = MissionEpochId::new("epoch-one").unwrap();
    let event = TemporalEventId::new("event-one").unwrap();
    let release = AuthorityReleaseId::new("time-release-iana").unwrap();
    let calendar_cursor = CalendarCursor::new(&calendar, version());
    let epoch_cursor = EpochCursor::new(&epoch, version());
    let event_cursor = EventCursor::new(&event, -42, SubsecondNanoseconds::MAX);
    let cases = [
        (
            uris::DOC_TEMPLATE,
            TimeResource::Document(TimeDocument::Agents),
            vec![("doc_id", "agents".into())],
        ),
        (
            uris::ZONE_TEMPLATE,
            TimeResource::Zone(TimeZoneId::new("America/New_York").unwrap()),
            vec![("zone_id", "America/New_York".into())],
        ),
        (
            uris::ZONE_TEMPLATE,
            TimeResource::Zone(TimeZoneId::new("Etc/GMT+5").unwrap()),
            vec![("zone_id", "Etc/GMT+5".into())],
        ),
        (
            uris::AUTHORITY_RELEASE_TEMPLATE,
            TimeResource::AuthorityRelease(release.clone()),
            vec![("release_id", release.to_string())],
        ),
        (
            uris::CALENDAR_TEMPLATE,
            TimeResource::Calendar {
                id: calendar.clone(),
                version: version(),
            },
            vec![
                ("calendar_id", calendar.to_string()),
                ("version", version().get().to_string()),
            ],
        ),
        (
            uris::EPOCH_TEMPLATE,
            TimeResource::Epoch(epoch.clone()),
            vec![("epoch_id", epoch.to_string())],
        ),
        (
            uris::EVENT_TEMPLATE,
            TimeResource::Event(event.clone()),
            vec![("event_id", event.to_string())],
        ),
        (
            uris::CALENDARS_TEMPLATE,
            TimeResource::Calendars { cursor: None },
            vec![],
        ),
        (
            uris::EPOCHS_TEMPLATE,
            TimeResource::Epochs { cursor: None },
            vec![],
        ),
        (
            uris::EVENTS_TEMPLATE,
            TimeResource::Events { cursor: None },
            vec![],
        ),
        (
            uris::CALENDARS_TEMPLATE,
            TimeResource::Calendars {
                cursor: Some(calendar_cursor.clone()),
            },
            vec![("cursor", calendar_cursor.as_str().to_owned())],
        ),
        (
            uris::EPOCHS_TEMPLATE,
            TimeResource::Epochs {
                cursor: Some(epoch_cursor.clone()),
            },
            vec![("cursor", epoch_cursor.as_str().to_owned())],
        ),
        (
            uris::EVENTS_TEMPLATE,
            TimeResource::Events {
                cursor: Some(event_cursor.clone()),
            },
            vec![("cursor", event_cursor.as_str().to_owned())],
        ),
    ];
    for (wire, address, bindings) in cases {
        let template = ResourceTemplateUri::new(wire).unwrap();
        let mut context = SimpleContext::new();
        for (name, value) in bindings {
            context.insert(name, value);
        }
        let uri = template.expand(&context).unwrap();
        assert_eq!(uri, address.to_uri().unwrap());
        assert_eq!(TimeResource::parse(uri.as_str()).unwrap(), address);
    }
}

#[test]
fn all_resource_families_preserve_the_published_wire_shape() {
    let examples = [
        (TimeResource::Docs, "time://docs"),
        (
            TimeResource::Document(TimeDocument::Agents),
            "time://docs/agents",
        ),
        (
            TimeResource::Document(TimeDocument::Design),
            "time://docs/design",
        ),
        (TimeResource::Contract, "time://contract"),
        (TimeResource::TimelineApp, "ui://time/timeline.html"),
        (TimeResource::ClockCurrent, "time://clock/current"),
        (TimeResource::ClockQuality, "time://clock/quality"),
        (
            TimeResource::AuthoritiesCurrent,
            "time://authorities/current",
        ),
        (
            TimeResource::AuthorityRelease(AuthorityReleaseId::new("time-release-iana").unwrap()),
            "time://authorities/releases/time-release-iana",
        ),
        (
            TimeResource::Zone(TimeZoneId::new("America/New_York").unwrap()),
            "time://zones/America/New_York",
        ),
        (
            TimeResource::Zone(TimeZoneId::new("Etc/GMT+5").unwrap()),
            "time://zones/Etc/GMT+5",
        ),
        (TimeResource::Calendars { cursor: None }, "time://calendars"),
        (
            TimeResource::Calendar {
                id: CalendarId::new("calendar-one").unwrap(),
                version: version(),
            },
            "time://calendars/calendar-one/versions/12",
        ),
        (TimeResource::Epochs { cursor: None }, "time://epochs"),
        (
            TimeResource::Epoch(MissionEpochId::new("epoch-one").unwrap()),
            "time://epochs/epoch-one",
        ),
        (TimeResource::Events { cursor: None }, "time://events"),
        (
            TimeResource::Event(TemporalEventId::new("event-one").unwrap()),
            "time://events/event-one",
        ),
    ];
    for (address, wire) in examples {
        assert_eq!(address.to_uri().unwrap().as_str(), wire);
        assert_eq!(TimeResource::parse(wire).unwrap(), address);
        assert_eq!(
            <TimeResource as ResourceAddress>::parse(&ResourceUri::new(wire).unwrap()).unwrap(),
            address
        );
        assert_eq!(serde_json::to_value(&address).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<TimeResource>(wire.into()).unwrap(),
            address
        );
    }
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(TimeResource)).unwrap()["type"],
        "string"
    );
}

#[test]
fn pages_round_trip_with_their_own_cursor_types_and_existing_v1_tokens() {
    let old = r#"{"version":1,"collection":"time://calendars","position":{"key":"calendar-one","version":12}}"#;
    let cursor = CalendarCursor::parse(hex::encode(old)).unwrap();
    assert_eq!(cursor.calendar_id().as_str(), "calendar-one");
    assert_eq!(cursor.version(), version());
    assert_eq!(CalendarCursor::new(cursor.calendar_id(), version()), cursor);
    assert_eq!(serde_json::to_value(&cursor).unwrap(), hex::encode(old));
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(CalendarCursor)).unwrap()["type"],
        "string"
    );
    for address in [
        TimeResource::Calendars {
            cursor: Some(cursor.clone()),
        },
        TimeResource::Epochs {
            cursor: Some(EpochCursor::new(
                &MissionEpochId::new("epoch-one").unwrap(),
                version(),
            )),
        },
        TimeResource::Events {
            cursor: Some(EventCursor::new(
                &TemporalEventId::new("event-one").unwrap(),
                -42,
                SubsecondNanoseconds::MAX,
            )),
        },
    ] {
        assert!(!address.is_subscribable());
        assert_eq!(
            TimeResource::parse(address.to_uri().unwrap().as_str()).unwrap(),
            address
        );
    }
    assert!(EpochCursor::parse(cursor.as_str()).is_err());
    assert!(EventCursor::parse(cursor.as_str()).is_err());
    let page = TimeResource::Calendars {
        cursor: Some(cursor.clone()),
    }
    .to_uri()
    .unwrap();
    let repeated = format!("{}&cursor={}", page.as_str(), cursor.as_str());
    assert!(TimeResource::parse(&repeated).is_err());
    assert!(
        TimeResource::parse(&format!("{}&%63ursor={}", page.as_str(), cursor.as_str())).is_err()
    );
    assert!(TimeResource::parse(&format!("{}&unknown=value", page.as_str())).is_err());
    assert!(TimeResource::parse(&format!("time://events?cursor={}", cursor.as_str())).is_err());
}

#[test]
fn cursors_validate_family_identity_position_and_envelope() {
    let valid = serde_json::json!({"version":1,"collection":"time://calendars","position":{"key":"calendar-one","version":12}});
    let mut wrongs = vec![];
    for wrong in [0, 2] {
        let mut value = valid.clone();
        value["version"] = wrong.into();
        wrongs.push(value);
    }
    let mut value = valid.clone();
    value["extra"] = true.into();
    wrongs.push(value);
    let mut value = valid.clone();
    value["position"]["extra"] = true.into();
    wrongs.push(value);
    let mut value = valid.clone();
    value["position"]["key"] = "epoch-one".into();
    wrongs.push(value);
    for wrong in [0, -1, i64::MAX as i128 + 1] {
        let mut value = valid.clone();
        value["position"]["version"] = serde_json::to_value(wrong).unwrap();
        wrongs.push(value);
    }
    for value in wrongs {
        assert!(CalendarCursor::parse(hex::encode(serde_json::to_vec(&value).unwrap())).is_err());
    }
    for token in ["".to_owned(), "0".into(), "gg".into(), "0".repeat(2050)] {
        assert!(CalendarCursor::parse(token).is_err());
    }
    for nanos in [-1, 1_000_000_000] {
        let value = serde_json::json!({"version":1,"collection":"time://events","position":{"event_key":"event-one","tai_seconds":0,"nanosecond":nanos}});
        assert!(EventCursor::parse(hex::encode(serde_json::to_vec(&value).unwrap())).is_err());
    }
}

#[test]
fn routes_reject_wrong_ids_aliases_and_unsupported_components() {
    for wire in [
        "time://calendars/epoch-one/versions/1",
        "time://calendars/calendar-one/versions/0",
        "time://calendars/calendar-one/versions/01",
        "time://calendars/calendar-one/versions/9223372036854775808",
        "time://calendars/calendar-one/versions/1?cursor=00",
        "time://calendars?",
        "time://calendars?offset=1",
        "time://epochs/calendar-one",
        "time://events/epoch-one",
        "time://events/event-one/extra",
        "time://events/event-%6fne",
        "time://events/event-one%2Fextra",
        "time://events/event-one#fragment",
        "time://events/../events/event-one",
        "time://events/{event_id}",
        "time://events/",
        "time://docs/unknown",
        "time://zones/America%2FNew_York",
        "time://zones//UTC",
        "time://zones/UTC?extra=1",
        "time://zones/Europe/../UTC",
        "time://zones/Etc/GMT%2B5",
        "time://zones",
        "time://zones/",
        "other://epochs/epoch-one",
        "ui://other/timeline.html",
    ] {
        assert!(TimeResource::parse(wire).is_err(), "accepted {wire}");
    }
    for name in [
        "", "/UTC", "UTC/", "../UTC", "UTC//GMT", "UTC?x", "UTC\n", "bad%zone",
    ] {
        assert!(TimeZoneId::new(name).is_err());
    }
    assert!(TimeVersion::new(0).is_err());
    assert!(TimeVersion::new(u64::MAX).is_err());
}

#[test]
fn subscriptions_share_resource_validation_and_exclude_cursor_pages() {
    for wire in [
        "time://clock/current",
        "time://clock/quality",
        "time://authorities/current",
        "time://calendars",
        "time://epochs",
        "time://events",
        "time://events/event-one",
        "time://epochs/epoch-one",
        "time://calendars/calendar-one/versions/1",
    ] {
        assert!(TimeResource::parse(wire).unwrap().is_subscribable());
    }
    for wire in [
        "time://docs",
        "time://contract",
        "time://docs/agents",
        "time://zones/UTC",
        "time://authorities/releases/time-release-one",
        "ui://time/timeline.html",
    ] {
        assert!(!TimeResource::parse(wire).unwrap().is_subscribable());
    }
}

#[test]
fn version_schema_and_admin_paths_enforce_the_database_range() {
    use veoveo_time_mcp::contract::CalendarVersionPath;
    let schema = serde_json::to_value(schemars::schema_for!(TimeVersion)).unwrap();
    assert_eq!(schema["minimum"], 1);
    assert_eq!(schema["maximum"], i64::MAX);
    for version in [0, -1, i64::MAX as i128 + 1] {
        assert!(
            serde_json::from_value::<CalendarVersionPath>(serde_json::json!({
                "calendar_id": "calendar-one", "version": version,
            }))
            .is_err()
        );
    }
    let path: CalendarVersionPath = serde_json::from_value(serde_json::json!({
        "calendar_id": "calendar-one", "version": 12,
    }))
    .unwrap();
    assert_eq!(path.version, version());
}
