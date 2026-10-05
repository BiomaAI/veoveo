//! Current owner and Work Context selection before limits, grouping or decoding.
mod transaction;
use crate::task_catalog::{CatalogRow, SOLVE_KINDS};
use surrealdb::{
    Connection,
    method::{Query, Transaction},
};
use veoveo_platform_store::{
    RecordId, TaskStatus, deterministic_principal_id, deterministic_tenant_id,
    deterministic_work_context_id, task_record_id,
};
use veoveo_task_runtime::{TaskOwner, TaskRuntime, TaskSnapshot};
use veoveo_types::TaskTypeDefinition;

use crate::{
    contract::{
        OPTIMIZATION_INDEX_PAGE_SIZE, OptimizationCollection, OptimizationCollectionUri,
        OptimizationIndexCursor, OptimizationSolutionUri, OptimizationToolOutput, ProblemFamily,
        ProblemId, RunId, SolutionId,
    },
    task_records::OptimizationTaskRequest,
};

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
        for kind in SOLVE_KINDS {
            tasks.require_contribution(&kind.name())?;
        }
        Ok(Self { tasks })
    }

    pub async fn page(
        &self,
        owner: &TaskOwner,
        request: &OptimizationCollectionUri,
    ) -> anyhow::Result<VisibleOptimizationTaskPage> {
        let tasks = self.tasks.clone();
        let owner = owner.clone();
        let request = request.clone();
        transaction::read(self.tasks.platform_store().client(), move |transaction| {
            Box::pin(async move {
                OptimizationReads::new(&tasks)?
                    .page_in(transaction, &owner, &request)
                    .await
            })
        })
        .await
    }

    async fn page_in<C: Connection>(
        &self,
        transaction: &Transaction<C>,
        owner: &TaskOwner,
        request: &OptimizationCollectionUri,
    ) -> anyhow::Result<VisibleOptimizationTaskPage> {
        let sql = match (
            request.collection() == OptimizationCollection::Solutions,
            request.cursor().is_some(),
        ) {
            (false, false) => include_str!("../queries/catalog/page_visible.surql"),
            (false, true) => include_str!("../queries/catalog/page_visible_cursor.surql"),
            (true, false) => include_str!("../queries/catalog/page_completed.surql"),
            (true, true) => include_str!("../queries/catalog/page_completed_cursor.surql"),
        };
        let query = transaction.query(sql);
        let mut query =
            bind_owner(query, owner)?.bind(("limit", (OPTIMIZATION_INDEX_PAGE_SIZE + 1) as i64));
        if let Some(cursor) = request.cursor() {
            query = query
                .bind(("after_created_at", cursor.created_at()))
                .bind(("after_task", task_record_id(cursor.task_id())));
        }
        let mut response = query.await?.check()?;
        let records: Vec<CatalogRow> = response.take(0)?;
        let has_more = records.len() > OPTIMIZATION_INDEX_PAGE_SIZE;
        let items = self
            .decode_catalog(
                transaction,
                owner,
                records
                    .into_iter()
                    .take(OPTIMIZATION_INDEX_PAGE_SIZE)
                    .collect(),
            )
            .await?;
        let next_cursor = if has_more {
            let last = items.last().ok_or_else(|| {
                anyhow::anyhow!("overfull Optimization catalog page has no validated Task")
            })?;
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
        self.find(owner, Selection::Problem(id.clone())).await
    }
    pub async fn run(
        &self,
        owner: &TaskOwner,
        id: &RunId,
    ) -> anyhow::Result<Option<VisibleOptimizationTask>> {
        self.find(owner, Selection::Run(id.clone())).await
    }
    pub async fn solution(
        &self,
        owner: &TaskOwner,
        uri: &OptimizationSolutionUri,
    ) -> anyhow::Result<Option<VisibleOptimizationTask>> {
        self.find(owner, Selection::Solution(uri.clone())).await
    }

    async fn find(
        &self,
        owner: &TaskOwner,
        selection: Selection,
    ) -> anyhow::Result<Option<VisibleOptimizationTask>> {
        let tasks = self.tasks.clone();
        let owner = owner.clone();
        transaction::read(self.tasks.platform_store().client(), move |transaction| {
            Box::pin(async move {
                OptimizationReads::new(&tasks)?
                    .find_in(transaction, &owner, selection)
                    .await
            })
        })
        .await
    }

    async fn find_in<C: Connection>(
        &self,
        transaction: &Transaction<C>,
        owner: &TaskOwner,
        selection: Selection,
    ) -> anyhow::Result<Option<VisibleOptimizationTask>> {
        let sql = match &selection {
            Selection::Problem(_) => include_str!("../queries/catalog/exact_problem.surql"),
            Selection::Run(_) => include_str!("../queries/catalog/exact_run.surql"),
            Selection::Solution(_) => include_str!("../queries/catalog/exact_solution.surql"),
        };
        let query = bind_owner(transaction.query(sql), owner)?;
        // Domain identities stay typed until this driver binding.
        let query = match selection {
            Selection::Problem(id) => query.bind(("identity", id.to_string())),
            Selection::Run(id) => query.bind(("identity", id.to_string())),
            Selection::Solution(uri) => query.bind(("identity", uri.to_string())),
        };
        let mut response = query.await?.check()?;
        let records: Vec<CatalogRow> = response.take(0)?;
        anyhow::ensure!(
            records.len() <= 1,
            "duplicate canonical Optimization identity"
        );
        Ok(self
            .decode_catalog(transaction, owner, records)
            .await?
            .into_iter()
            .next())
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

    async fn complete<T: Send + 'static>(
        &self,
        owner: &TaskOwner,
        collection: OptimizationCollection,
        needle: &str,
        limit: usize,
        parse: impl Fn(String) -> anyhow::Result<T> + Send + 'static,
    ) -> anyhow::Result<OptimizationCompletionPage<T>> {
        let tasks = self.tasks.clone();
        let owner = owner.clone();
        let needle = needle.to_owned();
        transaction::read(self.tasks.platform_store().client(), move |transaction| {
            Box::pin(async move {
                OptimizationReads::new(&tasks)?
                    .complete_in(transaction, &owner, collection, &needle, limit, parse)
                    .await
            })
        })
        .await
    }

    async fn complete_in<T, C: Connection>(
        &self,
        transaction: &Transaction<C>,
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
        let sql = match collection {
            OptimizationCollection::Problems => {
                include_str!("../queries/catalog/completion_problem.surql")
            }
            OptimizationCollection::Runs => include_str!("../queries/catalog/completion_run.surql"),
            OptimizationCollection::Solutions => {
                include_str!("../queries/catalog/completion_solution.surql")
            }
        };
        let mut response = bind_owner(transaction.query(sql), owner)?
            .bind(("needle", needle.to_ascii_lowercase()))
            .bind(("limit", (limit + 1) as i64))
            .await?
            .check()?;
        let values: Vec<String> = response.take(0)?;
        let has_more = values.len() > limit;
        let values = values.into_iter().take(limit).collect::<Vec<_>>();
        if !values.is_empty() {
            let sql = match collection {
                OptimizationCollection::Problems => {
                    include_str!("../queries/catalog/completion_rows_problem.surql")
                }
                OptimizationCollection::Runs => {
                    include_str!("../queries/catalog/completion_rows_run.surql")
                }
                OptimizationCollection::Solutions => {
                    include_str!("../queries/catalog/completion_rows_solution.surql")
                }
            };
            let mut response = bind_owner(transaction.query(sql), owner)?
                .bind(("values", values.clone()))
                .await?
                .check()?;
            let rows: Vec<CatalogRow> = response.take(0)?;
            anyhow::ensure!(
                rows.len() <= 1000,
                "Optimization completion has too many duplicate catalog rows"
            );
            let visible = self.decode_catalog(transaction, owner, rows).await?;
            let admitted = visible
                .iter()
                .filter_map(|row| match collection {
                    OptimizationCollection::Problems => row
                        .request
                        .common()
                        .map(|common| common.problem_id.to_string()),
                    OptimizationCollection::Runs => {
                        row.request.common().map(|common| common.run_id.to_string())
                    }
                    OptimizationCollection::Solutions => row
                        .output
                        .as_ref()
                        .map(|output| output.result_uri.to_string()),
                })
                .collect::<std::collections::BTreeSet<_>>();
            anyhow::ensure!(
                values.iter().all(|value| admitted.contains(value)),
                "SQL-selected Optimization completion identity has no validated catalog/Task match"
            );
            let values = values
                .into_iter()
                .map(parse)
                .collect::<anyhow::Result<Vec<_>>>()?;
            return Ok(OptimizationCompletionPage { values, has_more });
        }
        Ok(OptimizationCompletionPage {
            values: Vec::new(),
            has_more,
        })
    }

    async fn decode_catalog<C: Connection>(
        &self,
        transaction: &Transaction<C>,
        owner: &TaskOwner,
        rows: Vec<CatalogRow>,
    ) -> anyhow::Result<Vec<VisibleOptimizationTask>> {
        let ids = rows
            .iter()
            .map(CatalogRow::task_id)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let snapshots = self
            .tasks
            .for_owner(owner)
            .in_work_context()?
            .of_types(SOLVE_KINDS.map(|kind| kind.name()))?
            .get_many_in(transaction, &ids)
            .await?;
        let mut snapshots = snapshots
            .into_iter()
            .map(|snapshot| (snapshot.task_id, snapshot))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut visible = Vec::with_capacity(rows.len());
        for row in rows {
            let snapshot = snapshots.remove(&row.task_id()?).ok_or_else(|| anyhow::anyhow!("selected Optimization catalog Task is absent from the same owner-scoped read transaction"))?;
            let decoded = decode(snapshot)?;
            row.check(&decoded.snapshot, decoded.output.as_ref())?;
            visible.push(decoded);
        }
        Ok(visible)
    }
}

enum Selection {
    Problem(ProblemId),
    Run(RunId),
    Solution(OptimizationSolutionUri),
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
        .bind((
            "task_types",
            SOLVE_KINDS.map(|kind| kind.name().to_string()).to_vec(),
        )))
}

fn decode(snapshot: TaskSnapshot) -> anyhow::Result<VisibleOptimizationTask> {
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
