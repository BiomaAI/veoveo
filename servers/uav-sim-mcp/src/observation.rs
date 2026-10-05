//! Owner persistence table declarations for LIVE and changefeed observation.
use veoveo_modules::{ChangefeedRetention, ObservationReplay, ObservationTable, TableName};

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum UavObservationTable {
    #[vocabulary(rename = "uav_task")]
    UavTask,
    #[vocabulary(rename = "uav_vehicle_control_grant")]
    UavVehicleControlGrant,
    #[vocabulary(rename = "uav_vehicle_mission_plan")]
    UavVehicleMissionPlan,
}

impl From<UavObservationTable> for ObservationTable {
    fn from(table: UavObservationTable) -> Self {
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
        assert!("task".parse::<UavObservationTable>().is_err());
        assert!(
            "injected; DELETE task"
                .parse::<UavObservationTable>()
                .is_err()
        );
        for &table in UavObservationTable::ALL {
            let observed = ObservationTable::from(table);
            assert_eq!(observed.as_str(), table.as_str());
            assert_eq!(
                table.as_str().parse::<UavObservationTable>().unwrap(),
                table
            );
        }
    }
}
