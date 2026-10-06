#[cfg(feature = "persistence")]
use surrealdb::types::SurrealValue;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[cfg_attr(feature = "persistence", vocabulary(surreal))]
pub enum AgentState {
    #[vocabulary(rename = "idle")]
    Idle,
    #[vocabulary(rename = "running")]
    Running,
    #[vocabulary(rename = "waiting")]
    Waiting,
    #[vocabulary(rename = "disabled")]
    Disabled,
    #[vocabulary(rename = "failed")]
    Failed,
}
