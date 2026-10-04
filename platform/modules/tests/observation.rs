use veoveo_modules::{ChangefeedRetention, ObservationReplay, ObservationTable, TableName};
#[test]
fn checked_external_observation_declarations() {
    let table = ObservationTable::new(
        TableName::new("independent_owner_rows").unwrap(),
        ObservationReplay::LiveOnly,
    );
    assert_eq!(table.name().as_str(), "independent_owner_rows");
    assert_eq!(table.replay(), ObservationReplay::LiveOnly);
    assert!(TableName::new("owned; DELETE foreign").is_err());
    assert!(ChangefeedRetention::from_days(0).is_err());
    assert!(ChangefeedRetention::from_days(u32::MAX).is_err());
    assert_eq!(
        ChangefeedRetention::from_days(30).unwrap().seconds(),
        30 * 86400
    );
}
