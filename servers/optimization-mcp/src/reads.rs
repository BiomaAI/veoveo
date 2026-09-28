//! Current owner and Work Context selection before limits, grouping or decoding.
use surrealdb::{Connection, method::Query};
use veoveo_platform_store::{
    RecordId, TaskRecord, TaskStatus, deterministic_principal_id, deterministic_tenant_id,
    deterministic_work_context_id, task_record_id,
};
use veoveo_task_runtime::{TaskOwner, TaskRuntime, TaskSnapshot};

use crate::{
    contract::{
        OPTIMIZATION_INDEX_PAGE_SIZE, OptimizationCollection, OptimizationCollectionUri,
        OptimizationIndexCursor, OptimizationSolutionUri, OptimizationToolOutput, ProblemFamily,
        ProblemId, RunId, SolutionId,
    },
    task_records::{
        OPTIMIZE_ROUTE_SCENARIOS_TASK, OPTIMIZE_ROUTES_TASK, OptimizationTaskRequest,
        SOLVE_CONVEX_TASK, SOLVE_MILP_TASK,
    },
};

const VISIBLE: &str = "server = $server AND tenant = $tenant AND owner = $owner
    AND profile = $profile AND work_context = $work_context
    AND request.owner.principal_key = $principal_key
    AND request.owner.profile = $profile_key
    AND (request.owner.tenant_key ?? NONE) = $tenant_key
    AND request.owner.data_labels ALLINSIDE $labels
    AND authority.context_key = $work_context_key
    AND request.owner.authority.work_context = $work_context_key
    AND request.owner.authority.tenant = $authority_tenant";
const SOLVE: &str = "task_type IN $task_types AND request.input.kind = task_type";
const COMPLETED: &str = "status = 'succeeded' AND (result.payload.isError ?? false) = false
    AND result.payload.structuredContent.result_uri != NONE";
const SOLVE_TASK_TYPES: [&str; 4] = [
    OPTIMIZE_ROUTES_TASK,
    OPTIMIZE_ROUTE_SCENARIOS_TASK,
    SOLVE_CONVEX_TASK,
    SOLVE_MILP_TASK,
];

pub struct OptimizationReads<'a> {
    tasks: &'a TaskRuntime,
}

pub struct VisibleOptimizationTask {
    pub snapshot: TaskSnapshot,
    pub request: OptimizationTaskRequest,
    pub output: Option<OptimizationToolOutput>,
}
pub struct VisibleOptimizationTaskPage {
    pub items: Vec<VisibleOptimizationTask>,
    pub next_cursor: Option<OptimizationIndexCursor>,
}
pub struct OptimizationCompletionPage<T> {
    pub values: Vec<T>,
    pub has_more: bool,
}

impl<'a> OptimizationReads<'a> {
    pub fn new(tasks: &'a TaskRuntime) -> anyhow::Result<Self> {
        anyhow::ensure!(
            tasks.server() == "optimization",
            "expected Optimization Task runtime"
        );
        Ok(Self { tasks })
    }

    pub async fn page(
        &self,
        owner: &TaskOwner,
        request: &OptimizationCollectionUri,
    ) -> anyhow::Result<VisibleOptimizationTaskPage> {
        let terminal = if request.collection() == OptimizationCollection::Solutions {
            COMPLETED
        } else {
            "true"
        };
        let after = if request.cursor().is_some() {
            "AND (created_at > $after_created_at OR (created_at = $after_created_at AND id > $after_task))"
        } else {
            ""
        };
        let query = self.tasks.platform_store().client().query(format!(
            "SELECT * FROM task WHERE {VISIBLE} AND {SOLVE} AND {terminal} {after}
             ORDER BY created_at ASC, id ASC LIMIT $limit;"
        ));
        let mut query =
            bind_owner(query, owner)?.bind(("limit", (OPTIMIZATION_INDEX_PAGE_SIZE + 1) as i64));
        if let Some(cursor) = request.cursor() {
            query = query
                .bind(("after_created_at", cursor.created_at()))
                .bind(("after_task", task_record_id(cursor.task_id())));
        }
        let mut response = query.await?.check()?;
        let records: Vec<TaskRecord> = response.take(0)?;
        let has_more = records.len() > OPTIMIZATION_INDEX_PAGE_SIZE;
        let items = records
            .into_iter()
            .take(OPTIMIZATION_INDEX_PAGE_SIZE)
            .map(decode)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let next_cursor = if has_more {
            let last = items.last().expect("overfull page has a returned item");
            Some(OptimizationIndexCursor::new(
                request.collection(),
                last.snapshot.created_at,
                last.snapshot.task_id,
            )?)
        } else {
            None
        };
        Ok(VisibleOptimizationTaskPage { items, next_cursor })
    }

    /// ```compile_fail
    /// use veoveo_optimization_mcp::reads::OptimizationReads;
    /// use veoveo_task_runtime::TaskOwner;
    /// async fn wrong(reads: OptimizationReads<'_>, owner: TaskOwner) {
    ///     reads.problem(&owner, "problem-id").await;
    /// }
    /// ```
    pub async fn problem(
        &self,
        owner: &TaskOwner,
        id: &ProblemId,
    ) -> anyhow::Result<Option<VisibleOptimizationTask>> {
        self.find(owner, Selection::Problem(id)).await
    }
    pub async fn run(
        &self,
        owner: &TaskOwner,
        id: &RunId,
    ) -> anyhow::Result<Option<VisibleOptimizationTask>> {
        self.find(owner, Selection::Run(id)).await
    }
    pub async fn solution(
        &self,
        owner: &TaskOwner,
        uri: &OptimizationSolutionUri,
    ) -> anyhow::Result<Option<VisibleOptimizationTask>> {
        self.find(owner, Selection::Solution(uri)).await
    }

    async fn find(
        &self,
        owner: &TaskOwner,
        selection: Selection<'_>,
    ) -> anyhow::Result<Option<VisibleOptimizationTask>> {
        let predicate = match selection {
            Selection::Problem(_) => "request.input.common.problem_id = $identity",
            Selection::Run(_) => "request.input.common.run_id = $identity",
            Selection::Solution(_) => "result.payload.structuredContent.result_uri = $identity",
        };
        let terminal = if matches!(selection, Selection::Solution(_)) {
            COMPLETED
        } else {
            "true"
        };
        let query = bind_owner(self.tasks.platform_store().client().query(format!(
            "SELECT * FROM task WHERE {VISIBLE} AND {SOLVE} AND {terminal} AND {predicate} LIMIT 2;"
        )), owner)?;
        // Domain identities stay typed until this driver binding.
        let query = match selection {
            Selection::Problem(id) => query.bind(("identity", id.to_string())),
            Selection::Run(id) => query.bind(("identity", id.to_string())),
            Selection::Solution(uri) => query.bind(("identity", uri.to_string())),
        };
        let mut response = query.await?.check()?;
        let records: Vec<TaskRecord> = response.take(0)?;
        anyhow::ensure!(
            records.len() <= 1,
            "duplicate canonical Optimization identity"
        );
        records.into_iter().next().map(decode).transpose()
    }

    pub async fn complete_problems(
        &self,
        owner: &TaskOwner,
        needle: &str,
        limit: usize,
    ) -> anyhow::Result<OptimizationCompletionPage<ProblemId>> {
        self.complete(
            owner,
            OptimizationCollection::Problems,
            needle,
            limit,
            |value| Ok(ProblemId::parse(value)?),
        )
        .await
    }
    pub async fn complete_runs(
        &self,
        owner: &TaskOwner,
        needle: &str,
        limit: usize,
    ) -> anyhow::Result<OptimizationCompletionPage<RunId>> {
        self.complete(
            owner,
            OptimizationCollection::Runs,
            needle,
            limit,
            |value| Ok(RunId::parse(value)?),
        )
        .await
    }
    pub async fn complete_solutions(
        &self,
        owner: &TaskOwner,
        needle: &str,
        limit: usize,
    ) -> anyhow::Result<OptimizationCompletionPage<SolutionId>> {
        self.complete(
            owner,
            OptimizationCollection::Solutions,
            needle,
            limit,
            |value| Ok(OptimizationSolutionUri::parse(value)?.id().clone()),
        )
        .await
    }

    async fn complete<T>(
        &self,
        owner: &TaskOwner,
        collection: OptimizationCollection,
        needle: &str,
        limit: usize,
        parse: impl Fn(String) -> anyhow::Result<T>,
    ) -> anyhow::Result<OptimizationCompletionPage<T>> {
        anyhow::ensure!(
            (1..=100).contains(&limit),
            "Optimization completion limit must be between 1 and 100"
        );
        let field = match collection {
            OptimizationCollection::Problems => "request.input.common.problem_id",
            OptimizationCollection::Runs => "request.input.common.run_id",
            OptimizationCollection::Solutions => "result.payload.structuredContent.result_uri",
        };
        let terminal = if collection == OptimizationCollection::Solutions {
            COMPLETED
        } else {
            "true"
        };
        let mut response = bind_owner(
            self.tasks.platform_store().client().query(format!(
                "SELECT VALUE {field} FROM task WHERE {VISIBLE} AND {SOLVE} AND {terminal}
             AND {field} CONTAINS $needle GROUP BY {field} ORDER BY {field} ASC LIMIT $limit;"
            )),
            owner,
        )?
        .bind(("needle", needle.to_ascii_lowercase()))
        .bind(("limit", (limit + 1) as i64))
        .await?
        .check()?;
        let values: Vec<String> = response.take(0)?;
        let has_more = values.len() > limit;
        let values = values
            .into_iter()
            .take(limit)
            .map(parse)
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(OptimizationCompletionPage { values, has_more })
    }
}

enum Selection<'a> {
    Problem(&'a ProblemId),
    Run(&'a RunId),
    Solution(&'a OptimizationSolutionUri),
}

fn bind_owner<'q, C: Connection>(
    query: Query<'q, C>,
    owner: &TaskOwner,
) -> anyhow::Result<Query<'q, C>> {
    anyhow::ensure!(
        owner.authority.tenant.as_str() == owner.tenant_key(),
        "Optimization owner and Work Context belong to different tenants"
    );
    Ok(query
        .bind(("server", RecordId::new("mcp_server", "optimization")))
        .bind((
            "tenant",
            deterministic_tenant_id(owner.tenant_key())?.record_id(),
        ))
        .bind((
            "owner",
            deterministic_principal_id(owner.tenant_key(), &owner.principal_key)?.record_id(),
        ))
        .bind(("profile", RecordId::new("profile", owner.profile.clone())))
        .bind((
            "work_context",
            deterministic_work_context_id(
                owner.tenant_key(),
                owner.authority.work_context.as_str(),
            )?
            .record_id(),
        ))
        .bind(("principal_key", owner.principal_key.clone()))
        .bind(("profile_key", owner.profile.clone()))
        .bind(("tenant_key", owner.tenant_key.clone()))
        .bind(("labels", owner.data_labels.clone()))
        .bind(("work_context_key", owner.authority.work_context.to_string()))
        .bind(("authority_tenant", owner.authority.tenant.to_string()))
        .bind(("task_types", SOLVE_TASK_TYPES.map(str::to_owned).to_vec())))
}

fn decode(record: TaskRecord) -> anyhow::Result<VisibleOptimizationTask> {
    let snapshot = TaskSnapshot::try_from(record)?;
    let request: OptimizationTaskRequest = serde_json::from_value(snapshot.request.clone())?;
    let family = match &request {
        OptimizationTaskRequest::OptimizeRoutes { .. } => ProblemFamily::Routing,
        OptimizationTaskRequest::OptimizeRouteScenarios { .. } => ProblemFamily::RouteScenarios,
        OptimizationTaskRequest::SolveConvex { .. } => ProblemFamily::Convex,
        OptimizationTaskRequest::SolveMilp { .. } => ProblemFamily::Milp,
        OptimizationTaskRequest::VerifySolution { .. } => {
            anyhow::bail!("selected Optimization Task is not a solve")
        }
    };
    let common = request.common().expect("solve request has common fields");
    anyhow::ensure!(
        request.task_type() == snapshot.task_type && common.family == family,
        "retained Optimization Task type and problem family disagree"
    );
    let output: Option<OptimizationToolOutput> = if snapshot.status == TaskStatus::Succeeded {
        let result = snapshot
            .result
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("successful Optimization Task has no result"))?;
        let is_error = result
            .get("isError")
            .map(|value| {
                value
                    .as_bool()
                    .ok_or_else(|| anyhow::anyhow!("Optimization result isError must be a boolean"))
            })
            .transpose()?
            .unwrap_or(false);
        anyhow::ensure!(
            !is_error,
            "successful Optimization Task contains an error result"
        );
        Some(serde_json::from_value(
            result
                .get("structuredContent")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Optimization result has no structured content"))?,
        )?)
    } else {
        None
    };
    if let Some(output) = &output {
        anyhow::ensure!(
            output.family == family
                && output.problem_uri.id() == &common.problem_id
                && output.run_uri.id() == &common.run_id,
            "retained Optimization result disagrees with its Task parents"
        );
    }
    Ok(VisibleOptimizationTask {
        snapshot,
        request,
        output,
    })
}
