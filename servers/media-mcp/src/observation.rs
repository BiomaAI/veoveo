//! Owner persistence table declarations for LIVE and changefeed observation.
use veoveo_modules::{ChangefeedRetention, ObservationReplay, ObservationTable, TableName};

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum MediaObservationTable {
    #[vocabulary(rename = "media_task")]
    MediaTask,
    #[vocabulary(rename = "media_task_context")]
    MediaTaskContext,
    #[vocabulary(rename = "media_usage")]
    MediaUsage,
}

impl From<MediaObservationTable> for ObservationTable {
    fn from(table: MediaObservationTable) -> Self {
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
        assert!("task".parse::<MediaObservationTable>().is_err());
        assert!(
            "injected; DELETE task"
                .parse::<MediaObservationTable>()
                .is_err()
        );
        for &table in MediaObservationTable::ALL {
            let observed = ObservationTable::from(table);
            assert_eq!(observed.as_str(), table.as_str());
            assert_eq!(
                table.as_str().parse::<MediaObservationTable>().unwrap(),
                table
            );
        }
    }
}
