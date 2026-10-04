//! Owner persistence table declarations for LIVE and changefeed observation.
use veoveo_modules::{ChangefeedRetention, ObservationReplay, ObservationTable, TableName};

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum AgentObservationTable {
    #[vocabulary(rename = "agent")]
    Agent,
    #[vocabulary(rename = "wake")]
    Wake,
    #[vocabulary(rename = "agent_episode")]
    AgentEpisode,
    #[vocabulary(rename = "agent_task")]
    AgentTask,
    #[vocabulary(rename = "agent_input_request")]
    AgentInputRequest,
    #[vocabulary(rename = "agent_definition")]
    AgentDefinition,
    #[vocabulary(rename = "managed_agent")]
    ManagedAgent,
    #[vocabulary(rename = "managed_agent_operation")]
    ManagedAgentOperation,
}

impl From<AgentObservationTable> for ObservationTable {
    fn from(table: AgentObservationTable) -> Self {
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
        assert!("task".parse::<AgentObservationTable>().is_err());
        assert!(
            "injected; DELETE task"
                .parse::<AgentObservationTable>()
                .is_err()
        );
        for &table in AgentObservationTable::ALL {
            let observed = ObservationTable::from(table);
            assert_eq!(observed.as_str(), table.as_str());
            assert_eq!(
                table.as_str().parse::<AgentObservationTable>().unwrap(),
                table
            );
        }
    }
}
