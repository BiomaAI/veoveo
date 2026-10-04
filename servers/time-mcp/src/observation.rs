//! Owner persistence table declarations for LIVE and changefeed observation.
use veoveo_modules::{ChangefeedRetention, ObservationReplay, ObservationTable, TableName};

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum TimeObservationTable {
    #[vocabulary(rename = "time_source")]
    TimeSource,
    #[vocabulary(rename = "time_authority_release")]
    TimeAuthorityRelease,
    #[vocabulary(rename = "time_active_authority")]
    TimeActiveAuthority,
    #[vocabulary(rename = "time_acquisition")]
    TimeAcquisition,
    #[vocabulary(rename = "time_calendar_version")]
    TimeCalendarVersion,
    #[vocabulary(rename = "time_mission_epoch")]
    TimeMissionEpoch,
    #[vocabulary(rename = "time_temporal_event")]
    TimeTemporalEvent,
    #[vocabulary(rename = "time_clock_policy")]
    TimeClockPolicy,
}

impl From<TimeObservationTable> for ObservationTable {
    fn from(table: TimeObservationTable) -> Self {
        Self::new(
            TableName::new(table.as_str()).expect("owner table declaration"),
            ObservationReplay::Changefeed(
                ChangefeedRetention::from_days(30).expect("qualified retention"),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declared_observation_names_pass_identifier_admission() {
        assert!("task".parse::<TimeObservationTable>().is_err());
        assert!(
            "injected; DELETE task"
                .parse::<TimeObservationTable>()
                .is_err()
        );
        for &table in TimeObservationTable::ALL {
            let observed = ObservationTable::from(table);
            assert_eq!(observed.as_str(), table.as_str());
            assert_eq!(
                table.as_str().parse::<TimeObservationTable>().unwrap(),
                table
            );
        }
    }
}
