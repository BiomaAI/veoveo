//! Owner persistence table declarations for LIVE and changefeed observation.
use veoveo_modules::{ChangefeedRetention, ObservationReplay, ObservationTable, TableName};

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum FramesObservationTable {
    #[vocabulary(rename = "frame_world")]
    FrameWorld,
    #[vocabulary(rename = "frame_world_revision")]
    FrameWorldRevision,
    #[vocabulary(rename = "coordinate_operation")]
    CoordinateOperation,
}

impl From<FramesObservationTable> for ObservationTable {
    fn from(table: FramesObservationTable) -> Self {
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
        assert!("task".parse::<FramesObservationTable>().is_err());
        assert!(
            "injected; DELETE task"
                .parse::<FramesObservationTable>()
                .is_err()
        );
        for &table in FramesObservationTable::ALL {
            let observed = ObservationTable::from(table);
            assert_eq!(observed.as_str(), table.as_str());
            assert_eq!(
                table.as_str().parse::<FramesObservationTable>().unwrap(),
                table
            );
        }
    }
}
