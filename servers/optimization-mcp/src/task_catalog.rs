//! Typed catalog rows contributed in the Task creation/settlement transaction.
use crate::{
    contract::{
        OptimizationProfileUri, OptimizationSolutionUri, OptimizationTaskKind,
        OptimizationToolOutput, ProblemFamily, ProblemId, RunId,
    },
    task_records::OptimizationTaskRequest,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};
use veoveo_modules::TableName;
use veoveo_task_runtime::{
    OwnedTaskTable, TaskContribution, TaskContributions, TaskCreation, TaskError, TaskRuntime,
    TaskSettlement, TaskSnapshot, TaskStatus,
};
use veoveo_types::{TaskId, TaskTypeDefinition, TaskTypeName};

pub const SOLVE_KINDS: [OptimizationTaskKind; 4] = [
    OptimizationTaskKind::OptimizeRoutes,
    OptimizationTaskKind::OptimizeRouteScenarios,
    OptimizationTaskKind::SolveConvex,
    OptimizationTaskKind::SolveMilp,
];
const ALL_KINDS: [OptimizationTaskKind; 5] = [
    OptimizationTaskKind::OptimizeRoutes,
    OptimizationTaskKind::OptimizeRouteScenarios,
    OptimizationTaskKind::SolveConvex,
    OptimizationTaskKind::SolveMilp,
    OptimizationTaskKind::VerifySolution,
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub(crate) struct CatalogIdentity {
    #[surreal(wrap)]
    pub problem_id: ProblemId,
    #[surreal(wrap)]
    pub run_id: RunId,
    #[surreal(wrap)]
    pub family: ProblemFamily,
    #[surreal(wrap)]
    pub profile_uri: OptimizationProfileUri,
    pub submitted_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(crate) enum CatalogTerminal {
    #[vocabulary(rename = "succeeded")]
    Succeeded,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub(crate) struct CatalogSettlement {
    #[surreal(wrap)]
    pub status: CatalogTerminal,
    pub completed_at: DateTime<Utc>,
    #[surreal(wrap)]
    pub result_uri: Option<OptimizationSolutionUri>,
}
#[derive(Clone, Debug, SurrealValue)]
pub(crate) struct CatalogRow {
    pub id: RecordId,
    pub task: RecordId,
    #[surreal(wrap)]
    pub task_type: TaskTypeName,
    pub created_at: DateTime<Utc>,
    pub identity: CatalogIdentity,
    pub settlement: Option<CatalogSettlement>,
}
impl CatalogRow {
    pub fn task_id(&self) -> anyhow::Result<TaskId> {
        anyhow::ensure!(
            self.task.table.as_str() == "task",
            "Optimization catalog has a foreign Task link"
        );
        let id: TaskId = match &self.task.key {
            RecordIdKey::Uuid(key) => key.to_string().parse()?,
            _ => anyhow::bail!("Optimization catalog Task link is not a native identity"),
        };
        anyhow::ensure!(
            self.task == veoveo_platform_store::task_record_id(id),
            "Optimization catalog has a noncanonical Task link"
        );
        anyhow::ensure!(
            self.id == RecordId::new("optimization_task", id.to_string()),
            "Optimization catalog row and Task identities disagree"
        );
        Ok(id)
    }
    pub fn check(
        &self,
        snapshot: &TaskSnapshot,
        output: Option<&OptimizationToolOutput>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.task_id()? == snapshot.task_id
                && self.task_type == snapshot.task_type
                && self.created_at == snapshot.created_at,
            "Optimization catalog and Task identity, type or creation time disagree"
        );
        let expected = identity(&snapshot.task_type, &snapshot.request)?
            .ok_or_else(|| anyhow::anyhow!("verify Task appears in solve catalog"))?;
        anyhow::ensure!(
            self.identity == expected,
            "Optimization catalog and retained Task parents disagree"
        );
        match (&self.settlement, snapshot.status) {
            (
                None,
                TaskStatus::Queued
                | TaskStatus::Running
                | TaskStatus::Waiting
                | TaskStatus::CancelRequested,
            ) => {}
            (Some(settlement), status) => {
                let terminal = match status {
                    TaskStatus::Succeeded => CatalogTerminal::Succeeded,
                    TaskStatus::Failed => CatalogTerminal::Failed,
                    TaskStatus::Cancelled => CatalogTerminal::Cancelled,
                    _ => anyhow::bail!(
                        "Optimization catalog settlement disagrees with nonterminal Task"
                    ),
                };
                anyhow::ensure!(
                    settlement.status == terminal
                        && Some(settlement.completed_at) == snapshot.completed_at
                        && settlement.result_uri.as_ref()
                            == output.map(|output| &output.result_uri),
                    "Optimization catalog settlement and Task result disagree"
                );
            }
            _ => anyhow::bail!("terminal Optimization Task has no catalog settlement"),
        }
        Ok(())
    }
}

pub struct OptimizationTaskContributions {
    table: OwnedTaskTable,
    kinds: Vec<TaskTypeName>,
}
impl OptimizationTaskContributions {
    pub fn new() -> Result<Self, TaskError> {
        let declaration = crate::schema::ownership()
            .map_err(|error| TaskError::InvalidRecord(error.to_string()))?;
        let table = OwnedTaskTable::new(
            &declaration,
            TableName::new("optimization_task")
                .map_err(|error| TaskError::InvalidRecord(error.to_string()))?,
        )?;
        Ok(Self {
            table,
            kinds: ALL_KINDS.map(|kind| kind.name()).to_vec(),
        })
    }
    pub fn bind(runtime: TaskRuntime) -> Result<TaskRuntime, TaskError> {
        runtime
            .requiring_contributions(ALL_KINDS.map(|kind| kind.name()))?
            .bind_contributions(std::sync::Arc::new(Self::new()?))
    }
}
impl TaskContributions for OptimizationTaskContributions {
    fn table(&self) -> &OwnedTaskTable {
        &self.table
    }
    fn task_types(&self) -> &[TaskTypeName] {
        &self.kinds
    }
    fn created(&self, creation: TaskCreation<'_>) -> Result<TaskContribution, TaskError> {
        match identity(&creation.draft.task_type, &creation.draft.request)? {
            Some(identity) => TaskContribution::create(self.table.clone(), identity),
            None => Ok(TaskContribution::none()),
        }
    }
    fn settled(
        &self,
        current: &TaskSnapshot,
        settlement: TaskSettlement<'_>,
        completed_at: DateTime<Utc>,
    ) -> Result<TaskContribution, TaskError> {
        let Some(identity) = identity(&current.task_type, &current.request)? else {
            return Ok(TaskContribution::none());
        };
        let (status, result_uri) = match settlement {
            TaskSettlement::Succeeded { result } => {
                let output = solve_output(result, &identity)?;
                (
                    CatalogTerminal::Succeeded,
                    Some(crate::contract::OptimizationToolOutputValue::from(output).result_uri),
                )
            }
            TaskSettlement::Failed { .. } => (CatalogTerminal::Failed, None),
            TaskSettlement::Cancelled => (CatalogTerminal::Cancelled, None),
        };
        TaskContribution::settle(
            self.table.clone(),
            identity,
            CatalogSettlement {
                status,
                completed_at,
                result_uri,
            },
        )
    }
}

fn identity(
    kind: &TaskTypeName,
    request: &serde_json::Value,
) -> Result<Option<CatalogIdentity>, TaskError> {
    let request: OptimizationTaskRequest = serde_json::from_value(request.clone())?;
    if request.task_type() != *kind {
        return Err(TaskError::InvalidRecord(
            "Optimization request and Task operation disagree".into(),
        ));
    }
    let family = match &request {
        OptimizationTaskRequest::OptimizeRoutes { .. } => ProblemFamily::Routing,
        OptimizationTaskRequest::OptimizeRouteScenarios { .. } => ProblemFamily::RouteScenarios,
        OptimizationTaskRequest::SolveConvex { .. } => ProblemFamily::Convex,
        OptimizationTaskRequest::SolveMilp { .. } => ProblemFamily::Milp,
        OptimizationTaskRequest::VerifySolution { .. } => return Ok(None),
    };
    let common = request.common().expect("solve request common");
    if common.family != family {
        return Err(TaskError::InvalidRecord(
            "Optimization request operation and family disagree".into(),
        ));
    }
    Ok(Some(CatalogIdentity {
        problem_id: common.problem_id.clone(),
        run_id: common.run_id.clone(),
        family,
        profile_uri: common.profile_uri.clone(),
        submitted_at: common.submitted_at,
    }))
}

fn solve_output(
    result: &serde_json::Value,
    identity: &CatalogIdentity,
) -> Result<OptimizationToolOutput, TaskError> {
    if result
        .get("isError")
        .is_some_and(|value| value.as_bool() != Some(false))
    {
        return Err(TaskError::InvalidRecord(
            "successful Optimization result contains an error or malformed isError".into(),
        ));
    }
    let output: OptimizationToolOutput =
        serde_json::from_value(result.get("structuredContent").cloned().ok_or_else(|| {
            TaskError::InvalidRecord("Optimization result has no structured content".into())
        })?)?;
    if output.family != identity.family
        || output.problem_uri.id() != &identity.problem_id
        || output.run_uri.id() != &identity.run_id
    {
        return Err(TaskError::InvalidRecord(
            "Optimization result disagrees with retained Task parents".into(),
        ));
    }
    Ok(output)
}
