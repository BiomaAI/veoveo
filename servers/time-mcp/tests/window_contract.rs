use serde_json::json;
use veoveo_time_mcp::*;

fn authority() -> AuthorityBinding {
    AuthorityBinding::new(
        AuthorityReleaseId::parse("time-release-tzdb").unwrap(),
        AuthorityReleaseId::parse("time-release-leaps").unwrap(),
    )
    .unwrap()
}

fn instant(total: i128, uncertainty: u64) -> TimeInstant {
    TimeInstant::from_total_nanoseconds(total, uncertainty, authority()).unwrap()
}

#[test]
fn window_constructor_and_wire_require_ordered_bounds_with_one_authority() {
    let start = instant(-1, 7);
    let end = instant(1, 11);
    let window = TimeWindow::new(start.clone(), end.clone()).unwrap();
    assert_eq!(window.start(), &start);
    assert_eq!(window.end(), &end);
    assert_eq!(window.authority(), &authority());
    let wire = json!({"start":start,"end":end});
    assert_eq!(serde_json::to_value(&window).unwrap(), wire);
    assert_eq!(
        serde_json::from_value::<TimeWindow>(wire.clone()).unwrap(),
        window
    );
    for bounds in [(start.clone(), start.clone()), (end.clone(), start.clone())] {
        assert_eq!(
            TimeWindow::new(bounds.0.clone(), bounds.1.clone()),
            Err(TimeWindowError::UnorderedBounds)
        );
        let bad = json!({"start":bounds.0,"end":bounds.1});
        assert!(serde_json::from_value::<TimeWindow>(bad.clone()).is_err());
        assert!(
            serde_json::from_value::<EvaluateWindowsRequest>(
                json!({"operation":"union","left":[bad]})
            )
            .is_err()
        );
    }
    let mut foreign = end;
    foreign.authority = AuthorityBinding::new(
        AuthorityReleaseId::parse("time-release-other-tzdb").unwrap(),
        AuthorityReleaseId::parse("time-release-other-leaps").unwrap(),
    )
    .unwrap();
    assert_eq!(
        TimeWindow::new(start.clone(), foreign.clone()),
        Err(TimeWindowError::AuthorityMismatch)
    );
    assert!(serde_json::from_value::<TimeWindow>(json!({"start":start,"end":foreign})).is_err());
    for field in ["start", "end"] {
        let mut missing = wire.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<TimeWindow>(missing).is_err());
    }
    let schema = serde_json::to_value(schemars::schema_for!(TimeWindow)).unwrap();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["required"], json!(["start", "end"]));
    assert_eq!(schema["properties"].as_object().unwrap().len(), 2);
}

#[test]
fn intersection_preserves_selected_bound_metadata_and_maximum_tied_uncertainty() {
    let outer = TimeWindow::new(instant(-10, 2), instant(10, 3)).unwrap();
    let inner = TimeWindow::new(instant(-5, 7), instant(5, 11)).unwrap();
    assert_eq!(outer.intersection(&inner).unwrap(), Some(inner.clone()));
    assert_eq!(inner.intersection(&outer).unwrap(), Some(inner));
    let same = TimeWindow::new(instant(-10, 13), instant(10, 1)).unwrap();
    let clipped = outer.intersection(&same).unwrap().unwrap();
    assert_eq!(clipped.start().uncertainty_nanoseconds, 13);
    assert_eq!(clipped.end().uncertainty_nanoseconds, 3);
    assert_eq!(outer.start().uncertainty_nanoseconds, 2);
    assert_eq!(same.end().uncertainty_nanoseconds, 1);
    for start in [10, 11] {
        let after = TimeWindow::new(instant(start, 5), instant(20, 7)).unwrap();
        assert_eq!(outer.intersection(&after).unwrap(), None);
        assert_eq!(after.intersection(&outer).unwrap(), None);
    }
    let mut first = instant(-5, 0);
    first.authority = AuthorityBinding::new(
        AuthorityReleaseId::parse("time-release-foreign-tzdb").unwrap(),
        AuthorityReleaseId::parse("time-release-foreign-leaps").unwrap(),
    )
    .unwrap();
    let mut last = first.clone();
    last.tai_seconds_since_1970 += 1;
    let foreign = TimeWindow::new(first, last).unwrap();
    assert_eq!(
        outer.intersection(&foreign),
        Err(TimeWindowError::AuthorityMismatch)
    );
}

#[test]
fn windows_span_the_full_coordinate_range_without_narrowing() {
    let minimum = i128::from(i64::MIN) * 1_000_000_000;
    let maximum = i128::from(i64::MAX) * 1_000_000_000 + 999_999_999;
    let whole = TimeWindow::new(instant(minimum, 7), instant(maximum, 11)).unwrap();
    let tail = TimeWindow::new(instant(maximum - 1, 13), instant(maximum, 17)).unwrap();
    assert_eq!(whole.intersection(&tail).unwrap(), Some(tail));
    assert_eq!(whole.start().total_nanoseconds(), minimum);
    assert_eq!(whole.end().total_nanoseconds(), maximum);
}

#[test]
fn checked_window_wire_is_closed_before_relationship_admission() {
    let window = TimeWindow::new(instant(0, 0), instant(1, 0)).unwrap();
    let mut wire = serde_json::to_value(window).unwrap();
    wire["undeclared"] = json!(true);
    assert!(serde_json::from_value::<TimeWindow>(wire).is_err());
}
