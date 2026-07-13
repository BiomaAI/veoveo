use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::PathBuf,
};

use crate::contract::{
    AgentPlanResult, PlanInput, PlanOutput, PlanRequest, PlanSolverSummary, PlanStatus,
    PlanSummary, PlanningAgent, PlanningConstraint, PlanningObjective, PlanningOption,
    PlanningTableMapping, PlanningTask, SelectedOption, TaskPlanResult,
};
use anyhow::{Context, Result, bail};
use duckdb::{Connection, params, types::ValueRef};
use good_lp::{
    Expression, ProblemVariables, ResolutionError, Solution, SolverModel, Variable, default_solver,
    variable,
};
use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::{Scalars, TextDocument};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use veoveo_duckdb_runtime::{
    EngineSettings, FileAccess, HttpsSourcePolicy, RequestWorkspace, open_in_memory,
};
use veoveo_mcp_contract::{
    DuckDbFormat, DuckDbSource, duckdb_quote_literal, duckdb_read_function_sql,
    duckdb_read_options_sql,
};

pub const RRD_MIME_TYPE: &str = "application/vnd.veoveo.rerun-rrd";
pub const RRD_FILENAME: &str = "plan.rrd";
pub const DUCKDB_MIME_TYPE: &str = "application/vnd.duckdb";
pub const DUCKDB_FILENAME: &str = "plan.duckdb";

#[derive(Debug, Clone)]
pub struct PlanRun {
    pub output: PlanOutput,
    pub rrd: Option<PlanArtifactBytes>,
    pub duckdb: Option<PlanArtifactBytes>,
}

#[derive(Debug, Clone)]
pub struct PlanArtifactBytes {
    pub bytes: Vec<u8>,
    pub mime_type: &'static str,
    pub filename: &'static str,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
struct PlanningProblem {
    agents: Vec<PlanningAgent>,
    tasks: Vec<PlanningTask>,
    options: Vec<PlanningOption>,
    constraints: Vec<PlanningConstraint>,
    source_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RrdProvenance<'a> {
    task_id: &'a str,
    source_digest: Option<&'a str>,
    objective: &'a PlanningObjective,
    agents: usize,
    tasks: usize,
    options: usize,
}

pub fn run_plan(
    task_id: &str,
    request: &PlanRequest,
    source_policy: &HttpsSourcePolicy,
) -> Result<PlanRun> {
    let problem = load_problem(request, source_policy)?;
    validate_problem(&problem, &request.objective)?;
    let mut output = solve_problem(&problem, &request.objective)?;

    let source_digest = problem.source_digest.as_deref();
    let provenance = RrdProvenance {
        task_id,
        source_digest,
        objective: &request.objective,
        agents: problem.agents.len(),
        tasks: problem.tasks.len(),
        options: problem.options.len(),
    };
    let base_metadata = json!({
        "task_id": task_id,
        "source_digest": source_digest,
        "summary": output.summary,
        "solver": output.solver,
    });

    let duckdb = if request.artifacts.duckdb {
        Some(PlanArtifactBytes {
            bytes: write_duckdb(&output).context("writing DuckDB plan artifact")?,
            mime_type: DUCKDB_MIME_TYPE,
            filename: DUCKDB_FILENAME,
            metadata: json!({
                "artifact_format": "duckdb",
                "provenance": provenance,
                "plan": base_metadata,
            }),
        })
    } else {
        None
    };
    let rrd = if request.artifacts.rerun_rrd {
        Some(PlanArtifactBytes {
            bytes: write_rrd(task_id, &output, &provenance)
                .context("writing Rerun RRD artifact")?,
            mime_type: RRD_MIME_TYPE,
            filename: RRD_FILENAME,
            metadata: json!({
                "artifact_format": "rerun_rrd",
                "rrd_application_id": "veoveo_optimization_plan",
                "provenance": provenance,
                "plan": base_metadata,
            }),
        })
    } else {
        None
    };

    output.duckdb_artifact = None;
    output.rrd_artifact = None;
    Ok(PlanRun {
        output,
        rrd,
        duckdb,
    })
}

fn load_problem(
    request: &PlanRequest,
    source_policy: &HttpsSourcePolicy,
) -> Result<PlanningProblem> {
    match &request.input {
        PlanInput::Inline {
            agents,
            tasks,
            options,
            constraints,
        } => Ok(PlanningProblem {
            agents: agents.clone(),
            tasks: tasks.clone(),
            options: options.clone(),
            constraints: constraints.clone(),
            source_digest: None,
        }),
        PlanInput::DuckDbOptions {
            source,
            mapping,
            agents,
            tasks,
            constraints,
        } => Ok(PlanningProblem {
            agents: agents.clone(),
            tasks: tasks.clone(),
            options: load_options_from_duckdb(source, mapping, source_policy)?,
            constraints: constraints.clone(),
            source_digest: Some(source_digest(source)?),
        }),
    }
}

fn validate_problem(problem: &PlanningProblem, objective: &PlanningObjective) -> Result<()> {
    validate_finite("objective.cost_weight", objective.cost_weight)?;
    validate_finite("objective.risk_weight", objective.risk_weight)?;
    validate_finite("objective.duration_weight", objective.duration_weight)?;
    validate_finite("objective.priority_weight", objective.priority_weight)?;
    validate_finite("objective.confidence_weight", objective.confidence_weight)?;
    validate_finite("objective.resource_weight", objective.resource_weight)?;

    if problem.agents.is_empty() {
        bail!("at least one agent is required");
    }
    if problem.tasks.is_empty() {
        bail!("at least one task is required");
    }
    if problem.options.is_empty() {
        bail!("at least one option is required");
    }

    let mut agent_ids = BTreeSet::new();
    for agent in &problem.agents {
        validate_id("agent.id", &agent.id)?;
        if !agent_ids.insert(agent.id.clone()) {
            bail!("duplicate agent id `{}`", agent.id);
        }
        for (resource, limit) in &agent.resource_limits {
            validate_id("agent resource id", resource)?;
            validate_nonnegative_finite("agent.resource_limits", *limit)?;
        }
    }

    let mut task_ids = BTreeSet::new();
    for task in &problem.tasks {
        validate_id("task.id", &task.id)?;
        if !task_ids.insert(task.id.clone()) {
            bail!("duplicate task id `{}`", task.id);
        }
        validate_finite("task.priority", task.priority)?;
        if let Some(deadline) = task.deadline {
            validate_finite("task.deadline", deadline)?;
        }
    }

    let mut option_ids = BTreeSet::new();
    for option in &problem.options {
        validate_id("option.id", &option.id)?;
        if !option_ids.insert(option.id.clone()) {
            bail!("duplicate option id `{}`", option.id);
        }
        if !task_ids.contains(&option.task_id) {
            bail!(
                "option `{}` references unknown task `{}`",
                option.id,
                option.task_id
            );
        }
        if option.agent_ids.is_empty() {
            bail!("option `{}` must include at least one agent", option.id);
        }
        for agent_id in &option.agent_ids {
            if !agent_ids.contains(agent_id) {
                bail!(
                    "option `{}` references unknown agent `{agent_id}`",
                    option.id
                );
            }
        }
        validate_nonnegative_finite("option.cost", option.cost)?;
        validate_nonnegative_finite("option.risk", option.risk)?;
        validate_nonnegative_finite("option.confidence", option.confidence)?;
        if let Some(duration) = option.duration {
            validate_nonnegative_finite("option.duration", duration)?;
        }
        if let Some(start) = option.start {
            validate_finite("option.start", start)?;
        }
        if let Some(end) = option.end {
            validate_finite("option.end", end)?;
        }
        if let (Some(start), Some(end)) = (option.start, option.end)
            && end < start
        {
            bail!("option `{}` has end before start", option.id);
        }
        for (resource, amount) in &option.resource_usage {
            validate_id("option resource id", resource)?;
            validate_nonnegative_finite("option.resource_usage", *amount)?;
        }
    }

    for option in &problem.options {
        for required in &option.requires {
            if !option_ids.contains(required) {
                bail!(
                    "option `{}` requires unknown option `{required}`",
                    option.id
                );
            }
        }
        for excluded in &option.excludes {
            if !option_ids.contains(excluded) {
                bail!(
                    "option `{}` excludes unknown option `{excluded}`",
                    option.id
                );
            }
        }
    }

    for constraint in &problem.constraints {
        validate_constraint_references(constraint, &agent_ids, &task_ids, &option_ids)?;
    }

    Ok(())
}

fn validate_constraint_references(
    constraint: &PlanningConstraint,
    agent_ids: &BTreeSet<String>,
    task_ids: &BTreeSet<String>,
    option_ids: &BTreeSet<String>,
) -> Result<()> {
    match constraint {
        PlanningConstraint::ResourceLimit { resource, limit } => {
            validate_id("constraint.resource", resource)?;
            validate_nonnegative_finite("constraint.limit", *limit)
        }
        PlanningConstraint::MutualExclusion { option_ids: ids } => {
            validate_option_list(ids, option_ids)
        }
        PlanningConstraint::Dependency {
            option_id,
            depends_on,
        } => {
            validate_known_option(option_id, option_ids)?;
            validate_known_option(depends_on, option_ids)
        }
        PlanningConstraint::MaxSelected {
            option_ids: ids, ..
        }
        | PlanningConstraint::MinSelected {
            option_ids: ids, ..
        } => validate_option_list(ids, option_ids),
        PlanningConstraint::AgentMaxOptions { agent_id, .. } => {
            if !agent_ids.contains(agent_id) {
                bail!("constraint references unknown agent `{agent_id}`");
            }
            Ok(())
        }
        PlanningConstraint::TaskRequirement { task_id, .. } => {
            if !task_ids.contains(task_id) {
                bail!("constraint references unknown task `{task_id}`");
            }
            Ok(())
        }
    }
}

fn validate_option_list(ids: &[String], known: &BTreeSet<String>) -> Result<()> {
    if ids.is_empty() {
        bail!("option constraint list must not be empty");
    }
    for id in ids {
        validate_known_option(id, known)?;
    }
    Ok(())
}

fn validate_known_option(id: &str, known: &BTreeSet<String>) -> Result<()> {
    if !known.contains(id) {
        bail!("constraint references unknown option `{id}`");
    }
    Ok(())
}

fn solve_problem(problem: &PlanningProblem, objective: &PlanningObjective) -> Result<PlanOutput> {
    let agent_by_id: HashMap<_, _> = problem
        .agents
        .iter()
        .map(|agent| (agent.id.as_str(), agent))
        .collect();
    let task_by_id: HashMap<_, _> = problem
        .tasks
        .iter()
        .map(|task| (task.id.as_str(), task))
        .collect();

    let mut vars = ProblemVariables::new();
    let option_vars: Vec<Variable> = problem
        .options
        .iter()
        .map(|_| vars.add(variable().binary()))
        .collect();

    let mut coefficients = Vec::with_capacity(problem.options.len());
    let mut objective_expr = Expression::from(0.0);
    for (index, option) in problem.options.iter().enumerate() {
        let task = task_by_id[option.task_id.as_str()];
        let score = option_score(option, task, objective);
        let coefficient = -score;
        coefficients.push(coefficient);
        objective_expr += coefficient * option_vars[index];
    }

    let objective_for_eval = objective_expr.clone();
    let mut model = vars.minimise(objective_expr).using(default_solver);
    let mut constraints = 0_u64;

    for task in &problem.tasks {
        let task_expr = expression_for(problem, &option_vars, |option| option.task_id == task.id);
        if task.required_count > 0 {
            model = model.with(task_expr.clone().geq(f64::from(task.required_count)));
            constraints += 1;
            model = model.with(task_expr.leq(f64::from(task.required_count)));
            constraints += 1;
        } else {
            model = model.with(task_expr.leq(1.0));
            constraints += 1;
        }
    }

    for agent in &problem.agents {
        if let Some(max_options) = agent.max_options {
            let agent_expr = expression_for(problem, &option_vars, |option| {
                option.agent_ids.iter().any(|id| id == &agent.id)
            });
            model = model.with(agent_expr.leq(f64::from(max_options)));
            constraints += 1;
        }
        for (resource, limit) in &agent.resource_limits {
            let mut expr = Expression::from(0.0);
            for (index, option) in problem.options.iter().enumerate() {
                if option.agent_ids.iter().any(|id| id == &agent.id) {
                    expr += option.resource_usage.get(resource).copied().unwrap_or(0.0)
                        * option_vars[index];
                }
            }
            model = model.with(expr.leq(*limit));
            constraints += 1;
        }
    }

    for left in 0..problem.options.len() {
        for right in (left + 1)..problem.options.len() {
            let left_option = &problem.options[left];
            let right_option = &problem.options[right];
            if shares_agent(left_option, right_option)
                && fixed_windows_overlap(left_option, right_option)
            {
                model = model.with((option_vars[left] + option_vars[right]).leq(1.0));
                constraints += 1;
            }
        }
    }

    for (index, option) in problem.options.iter().enumerate() {
        if !capabilities_cover(option, &task_by_id, &agent_by_id) {
            model = model.with(Expression::from(option_vars[index]).leq(0.0));
            constraints += 1;
        }
        for required in &option.requires {
            let required_index = option_index(problem, required)?;
            model = model.with((option_vars[index] - option_vars[required_index]).leq(0.0));
            constraints += 1;
        }
        for excluded in &option.excludes {
            let excluded_index = option_index(problem, excluded)?;
            model = model.with((option_vars[index] + option_vars[excluded_index]).leq(1.0));
            constraints += 1;
        }
    }

    for constraint in &problem.constraints {
        match constraint {
            PlanningConstraint::ResourceLimit { resource, limit } => {
                let mut expr = Expression::from(0.0);
                for (index, option) in problem.options.iter().enumerate() {
                    expr += option.resource_usage.get(resource).copied().unwrap_or(0.0)
                        * option_vars[index];
                }
                model = model.with(expr.leq(*limit));
                constraints += 1;
            }
            PlanningConstraint::MutualExclusion { option_ids } => {
                model = model.with(option_ids_expr(problem, &option_vars, option_ids)?.leq(1.0));
                constraints += 1;
            }
            PlanningConstraint::Dependency {
                option_id,
                depends_on,
            } => {
                let option = option_vars[option_index(problem, option_id)?];
                let required = option_vars[option_index(problem, depends_on)?];
                model = model.with((option - required).leq(0.0));
                constraints += 1;
            }
            PlanningConstraint::MaxSelected { option_ids, max } => {
                model = model
                    .with(option_ids_expr(problem, &option_vars, option_ids)?.leq(f64::from(*max)));
                constraints += 1;
            }
            PlanningConstraint::MinSelected { option_ids, min } => {
                model = model
                    .with(option_ids_expr(problem, &option_vars, option_ids)?.geq(f64::from(*min)));
                constraints += 1;
            }
            PlanningConstraint::AgentMaxOptions { agent_id, max } => {
                let expr = expression_for(problem, &option_vars, |option| {
                    option.agent_ids.iter().any(|id| id == agent_id)
                });
                model = model.with(expr.leq(f64::from(*max)));
                constraints += 1;
            }
            PlanningConstraint::TaskRequirement { task_id, min, max } => {
                let expr =
                    expression_for(problem, &option_vars, |option| option.task_id == *task_id);
                model = model.with(expr.clone().geq(f64::from(*min)));
                constraints += 1;
                if let Some(max) = max {
                    model = model.with(expr.leq(f64::from(*max)));
                    constraints += 1;
                }
            }
        }
    }

    let solution = match model.solve() {
        Ok(solution) => Some(solution),
        Err(ResolutionError::Infeasible) => None,
        Err(err) => return Err(err).context("solving plan MILP"),
    };

    let Some(solution) = solution else {
        return Ok(infeasible_output(problem, constraints));
    };

    let objective_value = solution.eval(objective_for_eval);
    let mut selected_options = Vec::new();
    for (index, option) in problem.options.iter().enumerate() {
        if solution.value(option_vars[index]) >= 0.5 {
            let task = task_by_id[option.task_id.as_str()];
            selected_options.push(SelectedOption {
                option_id: option.id.clone(),
                task_id: option.task_id.clone(),
                agent_ids: option.agent_ids.clone(),
                score: option_score(option, task, objective),
                cost: option.cost,
                risk: option.risk,
                confidence: option.confidence,
                start: option.start,
                end: option.end,
            });
        }
    }
    selected_options.sort_by(|left, right| left.option_id.cmp(&right.option_id));
    Ok(plan_output(
        problem,
        selected_options,
        Some(objective_value),
        constraints,
        "optimal".to_string(),
    ))
}

fn infeasible_output(problem: &PlanningProblem, constraints: u64) -> PlanOutput {
    let mut output = plan_output(
        problem,
        Vec::new(),
        None,
        constraints,
        "infeasible".to_string(),
    );
    output.status = PlanStatus::Infeasible;
    output
}

fn plan_output(
    problem: &PlanningProblem,
    selected_options: Vec<SelectedOption>,
    objective_value: Option<f64>,
    constraints: u64,
    message: String,
) -> PlanOutput {
    let selected_ids: BTreeSet<_> = selected_options
        .iter()
        .map(|option| option.option_id.as_str())
        .collect();
    let mut task_results = Vec::with_capacity(problem.tasks.len());
    for task in &problem.tasks {
        let selected_count = problem
            .options
            .iter()
            .filter(|option| selected_ids.contains(option.id.as_str()) && option.task_id == task.id)
            .count() as u32;
        task_results.push(TaskPlanResult {
            task_id: task.id.clone(),
            required_count: task.required_count,
            selected_count,
            complete: selected_count >= task.required_count,
        });
    }
    task_results.sort_by(|left, right| left.task_id.cmp(&right.task_id));

    let mut agent_results = Vec::with_capacity(problem.agents.len());
    for agent in &problem.agents {
        let mut selected_count = 0_u32;
        let mut resource_usage = BTreeMap::new();
        for option in &problem.options {
            if !selected_ids.contains(option.id.as_str())
                || !option.agent_ids.iter().any(|id| id == &agent.id)
            {
                continue;
            }
            selected_count += 1;
            for (resource, amount) in &option.resource_usage {
                *resource_usage.entry(resource.clone()).or_insert(0.0) += amount;
            }
        }
        agent_results.push(AgentPlanResult {
            agent_id: agent.id.clone(),
            selected_count,
            resource_usage,
        });
    }
    agent_results.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));

    let completed_tasks = task_results
        .iter()
        .filter(|task| task.complete && task.required_count > 0)
        .count() as u64;
    PlanOutput {
        status: PlanStatus::Optimal,
        summary: PlanSummary {
            agents: problem.agents.len() as u64,
            tasks: problem.tasks.len() as u64,
            options: problem.options.len() as u64,
            selected: selected_options.len() as u64,
            completed_tasks,
            total_cost: selected_options.iter().map(|option| option.cost).sum(),
            total_risk: selected_options.iter().map(|option| option.risk).sum(),
            total_confidence: selected_options
                .iter()
                .map(|option| option.confidence)
                .sum(),
        },
        selected_options,
        task_results,
        agent_results,
        objective_value,
        solver: PlanSolverSummary {
            backend: "good_lp/microlp".to_string(),
            variables: problem.options.len() as u64,
            constraints,
            message,
        },
        duckdb_artifact: None,
        rrd_artifact: None,
    }
}

fn option_score(
    option: &PlanningOption,
    task: &PlanningTask,
    objective: &PlanningObjective,
) -> f64 {
    let duration = option.duration.or(match (option.start, option.end) {
        (Some(start), Some(end)) => Some(end - start),
        _ => None,
    });
    let resource_total: f64 = option.resource_usage.values().sum();
    objective.priority_weight * task.priority + objective.confidence_weight * option.confidence
        - objective.cost_weight * option.cost
        - objective.risk_weight * option.risk
        - objective.duration_weight * duration.unwrap_or(0.0)
        - objective.resource_weight * resource_total
}

fn expression_for(
    problem: &PlanningProblem,
    vars: &[Variable],
    mut matches: impl FnMut(&PlanningOption) -> bool,
) -> Expression {
    let mut expr = Expression::from(0.0);
    for (index, option) in problem.options.iter().enumerate() {
        if matches(option) {
            expr += vars[index];
        }
    }
    expr
}

fn option_ids_expr(
    problem: &PlanningProblem,
    vars: &[Variable],
    ids: &[String],
) -> Result<Expression> {
    let mut expr = Expression::from(0.0);
    for id in ids {
        expr += vars[option_index(problem, id)?];
    }
    Ok(expr)
}

fn option_index(problem: &PlanningProblem, option_id: &str) -> Result<usize> {
    problem
        .options
        .iter()
        .position(|option| option.id == option_id)
        .with_context(|| format!("unknown option `{option_id}`"))
}

fn shares_agent(left: &PlanningOption, right: &PlanningOption) -> bool {
    left.agent_ids
        .iter()
        .any(|agent_id| right.agent_ids.iter().any(|other| other == agent_id))
}

fn fixed_windows_overlap(left: &PlanningOption, right: &PlanningOption) -> bool {
    match (left.start, left.end, right.start, right.end) {
        (Some(left_start), Some(left_end), Some(right_start), Some(right_end)) => {
            left_start < right_end && right_start < left_end
        }
        _ => false,
    }
}

fn capabilities_cover(
    option: &PlanningOption,
    tasks: &HashMap<&str, &PlanningTask>,
    agents: &HashMap<&str, &PlanningAgent>,
) -> bool {
    let task = tasks[option.task_id.as_str()];
    if task.required_capabilities.is_empty() {
        return true;
    }
    let mut capabilities = BTreeSet::new();
    for agent_id in &option.agent_ids {
        if let Some(agent) = agents.get(agent_id.as_str()) {
            capabilities.extend(agent.capabilities.iter().cloned());
        }
    }
    task.required_capabilities.is_subset(&capabilities)
}

fn load_options_from_duckdb(
    source: &DuckDbSource,
    mapping: &PlanningTableMapping,
    source_policy: &HttpsSourcePolicy,
) -> Result<Vec<PlanningOption>> {
    validate_mapping(mapping)?;
    if let DuckDbSource::Uris { uris, .. } = source
        && uris.is_empty()
    {
        bail!("source.uris must not be empty");
    }

    let workspace = RequestWorkspace::new("veoveo-optimization-")?;
    let conn = open_in_memory(
        &FileAccess::RequestDirectory(workspace.request_dir().to_path_buf()),
        &EngineSettings::new(workspace.spill_dir()),
    )
    .context("opening DuckDB planning workspace")?;
    let table_sql = source_table_sql(source, &workspace, source_policy)?;
    conn.execute_batch(&format!(
        "CREATE TEMP TABLE options AS SELECT * FROM {table_sql};"
    ))
    .context("materializing planning options source")?;
    let options = read_options_table(&conn, mapping).context("reading planning options")?;
    Ok(options)
}

fn validate_mapping(mapping: &PlanningTableMapping) -> Result<()> {
    validate_id("mapping.option_id_column", &mapping.option_id_column)?;
    validate_id("mapping.task_id_column", &mapping.task_id_column)?;
    if mapping.agent_id_column.is_none() && mapping.agent_ids_column.is_none() {
        bail!("mapping must include agent_id_column or agent_ids_column");
    }
    Ok(())
}

fn source_table_sql(
    source: &DuckDbSource,
    workspace: &RequestWorkspace,
    source_policy: &HttpsSourcePolicy,
) -> Result<String> {
    match source {
        DuckDbSource::InlineCsv { csv, options, .. } => {
            let path = workspace.materialize_inline(
                "inline.csv",
                csv.as_bytes(),
                source_policy.max_bytes,
            )?;
            let options = duckdb_read_options_sql(options)?;
            Ok(format!(
                "read_csv({}{options})",
                duckdb_quote_literal(path.to_string_lossy().as_ref())
            ))
        }
        DuckDbSource::Uri {
            uri,
            format,
            options,
        } => {
            let filename = source_filename(0, format);
            let path = workspace.fetch_https(uri, &filename, source_policy)?;
            duckdb_read_function_sql(
                &duckdb_quote_literal(path.to_string_lossy().as_ref()),
                format,
                options,
            )
            .map_err(Into::into)
        }
        DuckDbSource::Uris {
            uris,
            format,
            options,
        } => {
            let list = uris
                .iter()
                .enumerate()
                .map(|(index, uri)| {
                    let filename = source_filename(index, format);
                    workspace
                        .fetch_https(uri, &filename, source_policy)
                        .map(|path| duckdb_quote_literal(path.to_string_lossy().as_ref()))
                })
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            duckdb_read_function_sql(&format!("[{list}]"), format, options).map_err(Into::into)
        }
        DuckDbSource::Artifact { .. } => {
            // Cross-server artifact:// input is served by the duckdb server, which
            // holds the artifact-plane client. Materialize the options table there
            // (e.g. via export) and pass it as inline or allowlisted rows.
            bail!(
                "artifact:// sources are not supported by optimization plan; \
                 read the artifact with the duckdb server instead"
            )
        }
    }
}

fn source_filename(index: usize, format: &DuckDbFormat) -> String {
    let extension = match format {
        DuckDbFormat::Auto | DuckDbFormat::Csv => "csv",
        DuckDbFormat::Parquet => "parquet",
        DuckDbFormat::Json => "json",
        DuckDbFormat::Ndjson => "ndjson",
    };
    format!("source-{index}.{extension}")
}

fn read_options_table(
    conn: &Connection,
    mapping: &PlanningTableMapping,
) -> Result<Vec<PlanningOption>> {
    let column_names = table_columns(conn)?;
    let required = |name: &str| -> Result<usize> {
        column_names
            .iter()
            .position(|column| column == name)
            .with_context(|| format!("source is missing column `{name}`"))
    };
    let optional = |name: &Option<String>| -> Option<usize> {
        name.as_ref()
            .and_then(|name| column_names.iter().position(|column| column == name))
    };

    let option_id_idx = required(&mapping.option_id_column)?;
    let task_id_idx = required(&mapping.task_id_column)?;
    let agent_id_idx = optional(&mapping.agent_id_column);
    let agent_ids_idx = optional(&mapping.agent_ids_column);
    let cost_idx = optional(&mapping.cost_column);
    let risk_idx = optional(&mapping.risk_column);
    let confidence_idx = optional(&mapping.confidence_column);
    let duration_idx = optional(&mapping.duration_column);
    let start_idx = optional(&mapping.start_column);
    let end_idx = optional(&mapping.end_column);
    let resources_idx = optional(&mapping.resource_usage_json_column);
    let requires_idx = optional(&mapping.requires_json_column);
    let excludes_idx = optional(&mapping.excludes_json_column);
    let tags_idx = optional(&mapping.tags_json_column);

    let mut stmt = conn
        .prepare("SELECT * FROM options")
        .context("preparing options scan")?;
    let mut rows = stmt.query([]).context("querying options table")?;
    let mut options = Vec::new();
    while let Some(row) = rows.next().context("reading options row")? {
        let mut agent_ids = Vec::new();
        if let Some(index) = agent_id_idx
            && let Some(agent_id) = string_at(row, index)?
        {
            agent_ids.push(agent_id);
        }
        if let Some(index) = agent_ids_idx
            && let Some(raw) = string_at(row, index)?
        {
            agent_ids.extend(parse_string_list(&raw)?);
        }
        agent_ids.sort();
        agent_ids.dedup();

        options.push(PlanningOption {
            id: required_string_at(row, option_id_idx, "option_id")?,
            task_id: required_string_at(row, task_id_idx, "task_id")?,
            agent_ids,
            cost: f64_at(row, cost_idx)?.unwrap_or(0.0),
            risk: f64_at(row, risk_idx)?.unwrap_or(0.0),
            confidence: f64_at(row, confidence_idx)?.unwrap_or(1.0),
            duration: f64_at(row, duration_idx)?,
            start: f64_at(row, start_idx)?,
            end: f64_at(row, end_idx)?,
            resource_usage: json_map_at(row, resources_idx)?.unwrap_or_default(),
            requires: json_list_at(row, requires_idx)?.unwrap_or_default(),
            excludes: json_list_at(row, excludes_idx)?.unwrap_or_default(),
            tags: json_list_at(row, tags_idx)?
                .unwrap_or_default()
                .into_iter()
                .collect(),
        });
    }
    Ok(options)
}

fn table_columns(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn
        .prepare("DESCRIBE options")
        .context("preparing options schema query")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut columns = Vec::new();
    for row in rows {
        columns.push(row?);
    }
    Ok(columns)
}

fn required_string_at(row: &duckdb::Row<'_>, index: usize, label: &str) -> Result<String> {
    string_at(row, index)?.with_context(|| format!("column `{label}` must not be null"))
}

fn string_at(row: &duckdb::Row<'_>, index: usize) -> Result<Option<String>> {
    match row.get_ref(index)? {
        ValueRef::Null => Ok(None),
        ValueRef::Text(bytes) => Ok(Some(String::from_utf8_lossy(bytes).trim().to_string())),
        ValueRef::Boolean(value) => Ok(Some(value.to_string())),
        ValueRef::TinyInt(value) => Ok(Some(value.to_string())),
        ValueRef::SmallInt(value) => Ok(Some(value.to_string())),
        ValueRef::Int(value) => Ok(Some(value.to_string())),
        ValueRef::BigInt(value) => Ok(Some(value.to_string())),
        ValueRef::UTinyInt(value) => Ok(Some(value.to_string())),
        ValueRef::USmallInt(value) => Ok(Some(value.to_string())),
        ValueRef::UInt(value) => Ok(Some(value.to_string())),
        ValueRef::UBigInt(value) => Ok(Some(value.to_string())),
        ValueRef::Float(value) => Ok(Some(value.to_string())),
        ValueRef::Double(value) => Ok(Some(value.to_string())),
        ValueRef::Decimal(value) => Ok(Some(value.to_string())),
        other => bail!("unsupported string-ish DuckDB value {other:?}"),
    }
}

fn f64_at(row: &duckdb::Row<'_>, index: Option<usize>) -> Result<Option<f64>> {
    let Some(index) = index else {
        return Ok(None);
    };
    match row.get_ref(index)? {
        ValueRef::Null => Ok(None),
        ValueRef::TinyInt(value) => Ok(Some(f64::from(value))),
        ValueRef::SmallInt(value) => Ok(Some(f64::from(value))),
        ValueRef::Int(value) => Ok(Some(f64::from(value))),
        ValueRef::BigInt(value) => Ok(Some(value as f64)),
        ValueRef::UTinyInt(value) => Ok(Some(f64::from(value))),
        ValueRef::USmallInt(value) => Ok(Some(f64::from(value))),
        ValueRef::UInt(value) => Ok(Some(value as f64)),
        ValueRef::UBigInt(value) => Ok(Some(value as f64)),
        ValueRef::Float(value) => Ok(Some(f64::from(value))),
        ValueRef::Double(value) => Ok(Some(value)),
        ValueRef::Text(bytes) => {
            let text = String::from_utf8_lossy(bytes);
            Ok(Some(text.trim().parse::<f64>()?))
        }
        other => bail!("unsupported numeric DuckDB value {other:?}"),
    }
}

fn json_map_at(
    row: &duckdb::Row<'_>,
    index: Option<usize>,
) -> Result<Option<BTreeMap<String, f64>>> {
    let Some(index) = index else {
        return Ok(None);
    };
    let Some(raw) = string_at(row, index)? else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&raw)?))
}

fn json_list_at(row: &duckdb::Row<'_>, index: Option<usize>) -> Result<Option<Vec<String>>> {
    let Some(index) = index else {
        return Ok(None);
    };
    let Some(raw) = string_at(row, index)? else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(parse_string_list(&raw)?))
}

fn parse_string_list(raw: &str) -> Result<Vec<String>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    if raw.starts_with('[') {
        return Ok(serde_json::from_str::<Vec<String>>(raw)?);
    }
    Ok(raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn write_duckdb(output: &PlanOutput) -> Result<Vec<u8>> {
    let path = temp_file("veoveo-optimization-plan", "duckdb");
    {
        let conn =
            Connection::open(&path).with_context(|| format!("opening {}", path.display()))?;
        conn.execute_batch(
            r#"
            CREATE TABLE selected_options (
                option_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                agent_ids_json TEXT NOT NULL,
                score DOUBLE NOT NULL,
                cost DOUBLE NOT NULL,
                risk DOUBLE NOT NULL,
                confidence DOUBLE NOT NULL,
                start DOUBLE,
                "end" DOUBLE
            );
            CREATE TABLE task_results (
                task_id TEXT NOT NULL,
                required_count UBIGINT NOT NULL,
                selected_count UBIGINT NOT NULL,
                complete BOOLEAN NOT NULL
            );
            CREATE TABLE agent_results (
                agent_id TEXT NOT NULL,
                selected_count UBIGINT NOT NULL,
                resource_usage_json TEXT NOT NULL
            );
            CREATE TABLE plan_summary (
                summary_json TEXT NOT NULL,
                solver_json TEXT NOT NULL
            );
            "#,
        )?;
        for option in &output.selected_options {
            conn.execute(
                r#"
                INSERT INTO selected_options
                    (option_id, task_id, agent_ids_json, score, cost, risk, confidence, start, "end")
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
                params![
                    option.option_id.as_str(),
                    option.task_id.as_str(),
                    serde_json::to_string(&option.agent_ids)?,
                    option.score,
                    option.cost,
                    option.risk,
                    option.confidence,
                    option.start,
                    option.end,
                ],
            )?;
        }
        for task in &output.task_results {
            conn.execute(
                "INSERT INTO task_results (task_id, required_count, selected_count, complete) VALUES (?1, ?2, ?3, ?4)",
                params![
                    task.task_id.as_str(),
                    u64::from(task.required_count),
                    u64::from(task.selected_count),
                    task.complete,
                ],
            )?;
        }
        for agent in &output.agent_results {
            conn.execute(
                "INSERT INTO agent_results (agent_id, selected_count, resource_usage_json) VALUES (?1, ?2, ?3)",
                params![
                    agent.agent_id.as_str(),
                    u64::from(agent.selected_count),
                    serde_json::to_string(&agent.resource_usage)?,
                ],
            )?;
        }
        conn.execute(
            "INSERT INTO plan_summary (summary_json, solver_json) VALUES (?1, ?2)",
            params![
                serde_json::to_string(&output.summary)?,
                serde_json::to_string(&output.solver)?,
            ],
        )?;
        conn.execute_batch("CHECKPOINT;")
            .context("checkpointing DuckDB artifact")?;
    }
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(PathBuf::from(format!("{}.wal", path.to_string_lossy())));
    Ok(bytes)
}

fn write_rrd(
    task_id: &str,
    output: &PlanOutput,
    provenance: &RrdProvenance<'_>,
) -> Result<Vec<u8>> {
    let path = temp_file("veoveo-optimization-plan", "rrd");
    let rec = RecordingStreamBuilder::new("veoveo_optimization_plan")
        .recording_id(task_id.to_owned())
        .recording_name(format!("plan {task_id}"))
        .save(&path)
        .context("opening Rerun RRD sink")?;

    rec.log(
        "/optimization/provenance",
        &TextDocument::new(serde_json::to_string_pretty(provenance)?)
            .with_media_type("application/json"),
    )?;
    rec.log(
        "/optimization/summary",
        &TextDocument::new(serde_json::to_string_pretty(&output.summary)?)
            .with_media_type("application/json"),
    )?;
    rec.log(
        "/optimization/selected_count",
        &Scalars::single(output.summary.selected as f64),
    )?;
    rec.log(
        "/optimization/total_cost",
        &Scalars::single(output.summary.total_cost),
    )?;
    rec.log(
        "/optimization/total_risk",
        &Scalars::single(output.summary.total_risk),
    )?;

    for (index, option) in output.selected_options.iter().enumerate() {
        rec.set_time_sequence("plan_step", index as i64);
        let segment = entity_segment(&option.option_id);
        rec.log(
            format!("/optimization/selected/{segment}/score"),
            &Scalars::single(option.score),
        )?;
        rec.log(
            format!("/optimization/selected/{segment}/cost"),
            &Scalars::single(option.cost),
        )?;
        rec.log(
            format!("/optimization/selected/{segment}/risk"),
            &Scalars::single(option.risk),
        )?;
        rec.log(
            format!("/optimization/selected/{segment}/details"),
            &TextDocument::new(serde_json::to_string_pretty(option)?)
                .with_media_type("application/json"),
        )?;
    }

    rec.flush_blocking().context("flushing Rerun RRD sink")?;
    drop(rec);
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let _ = fs::remove_file(&path);
    Ok(bytes)
}

fn temp_file(prefix: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}.{}",
        std::process::id(),
        uuid::Uuid::new_v4(),
        extension
    ))
}

fn source_digest(source: &DuckDbSource) -> Result<String> {
    let json = serde_json::to_vec(source)?;
    Ok(hex::encode(Sha256::digest(json)))
}

fn validate_id(label: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{label} must not be empty");
    }
    if value.contains('\0') {
        bail!("{label} must not contain NUL bytes");
    }
    Ok(())
}

fn validate_finite(label: &str, value: f64) -> Result<()> {
    if !value.is_finite() {
        bail!("{label} must be finite");
    }
    Ok(())
}

fn validate_nonnegative_finite(label: &str, value: f64) -> Result<()> {
    validate_finite(label, value)?;
    if value < 0.0 {
        bail!("{label} must be non-negative");
    }
    Ok(())
}

fn entity_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::contract::{PlanArtifactOptions, PlanningObjective};
    use veoveo_mcp_contract::DuckDbReadOptions;

    use super::*;

    #[test]
    fn selects_lowest_risk_required_option() {
        let request = PlanRequest {
            input: PlanInput::Inline {
                agents: vec![PlanningAgent {
                    id: "agent1".to_string(),
                    capabilities: BTreeSet::new(),
                    resource_limits: BTreeMap::new(),
                    max_options: Some(1),
                }],
                tasks: vec![PlanningTask {
                    id: "task1".to_string(),
                    required_count: 1,
                    priority: 10.0,
                    required_capabilities: BTreeSet::new(),
                    deadline: None,
                }],
                options: vec![
                    PlanningOption {
                        id: "fast".to_string(),
                        task_id: "task1".to_string(),
                        agent_ids: vec!["agent1".to_string()],
                        cost: 5.0,
                        risk: 10.0,
                        confidence: 0.9,
                        duration: None,
                        start: None,
                        end: None,
                        resource_usage: BTreeMap::new(),
                        requires: Vec::new(),
                        excludes: Vec::new(),
                        tags: BTreeSet::new(),
                    },
                    PlanningOption {
                        id: "careful".to_string(),
                        task_id: "task1".to_string(),
                        agent_ids: vec!["agent1".to_string()],
                        cost: 5.0,
                        risk: 1.0,
                        confidence: 0.8,
                        duration: None,
                        start: None,
                        end: None,
                        resource_usage: BTreeMap::new(),
                        requires: Vec::new(),
                        excludes: Vec::new(),
                        tags: BTreeSet::new(),
                    },
                ],
                constraints: Vec::new(),
            },
            objective: PlanningObjective::default(),
            artifacts: PlanArtifactOptions {
                duckdb: true,
                rerun_rrd: false,
            },
        };

        let run = run_plan("task-1", &request, &HttpsSourcePolicy::deny_network()).unwrap();
        assert_eq!(run.output.status, PlanStatus::Optimal);
        assert_eq!(run.output.selected_options.len(), 1);
        assert_eq!(run.output.selected_options[0].option_id, "careful");
        assert!(!run.duckdb.unwrap().bytes.is_empty());
    }

    #[test]
    fn loads_options_from_inline_csv_duckdb_source() {
        let request = PlanRequest {
            input: PlanInput::DuckDbOptions {
                source: DuckDbSource::InlineCsv {
                    csv: "option_id,task_id,agent_id,cost,risk\nopt1,task1,agent1,1,2\n"
                        .to_string(),
                    filename: None,
                    options: DuckDbReadOptions {
                        header: Some(true),
                        ..Default::default()
                    },
                },
                mapping: Box::new(PlanningTableMapping::default()),
                agents: vec![PlanningAgent {
                    id: "agent1".to_string(),
                    capabilities: BTreeSet::new(),
                    resource_limits: BTreeMap::new(),
                    max_options: None,
                }],
                tasks: vec![PlanningTask {
                    id: "task1".to_string(),
                    required_count: 1,
                    priority: 1.0,
                    required_capabilities: BTreeSet::new(),
                    deadline: None,
                }],
                constraints: Vec::new(),
            },
            objective: PlanningObjective::default(),
            artifacts: PlanArtifactOptions {
                duckdb: false,
                rerun_rrd: false,
            },
        };

        let run = run_plan("task-1", &request, &HttpsSourcePolicy::deny_network()).unwrap();
        assert_eq!(run.output.selected_options[0].option_id, "opt1");
    }

    #[test]
    fn resumable_plan_is_semantically_deterministic() {
        let request = PlanRequest {
            input: PlanInput::Inline {
                agents: vec![PlanningAgent {
                    id: "agent1".to_owned(),
                    capabilities: BTreeSet::new(),
                    resource_limits: BTreeMap::new(),
                    max_options: Some(1),
                }],
                tasks: vec![PlanningTask {
                    id: "task1".to_owned(),
                    required_count: 1,
                    priority: 1.0,
                    required_capabilities: BTreeSet::new(),
                    deadline: None,
                }],
                options: vec![PlanningOption {
                    id: "option1".to_owned(),
                    task_id: "task1".to_owned(),
                    agent_ids: vec!["agent1".to_owned()],
                    cost: 1.0,
                    risk: 2.0,
                    confidence: 0.9,
                    duration: None,
                    start: None,
                    end: None,
                    resource_usage: BTreeMap::new(),
                    requires: Vec::new(),
                    excludes: Vec::new(),
                    tags: BTreeSet::new(),
                }],
                constraints: Vec::new(),
            },
            objective: PlanningObjective::default(),
            artifacts: PlanArtifactOptions {
                duckdb: true,
                rerun_rrd: true,
            },
        };

        let first = run_plan(
            "019f0000-0000-7000-8000-000000000001",
            &request,
            &HttpsSourcePolicy::deny_network(),
        )
        .unwrap();
        let second = run_plan(
            "019f0000-0000-7000-8000-000000000001",
            &request,
            &HttpsSourcePolicy::deny_network(),
        )
        .unwrap();
        assert_eq!(first.output, second.output);

        let first_duckdb = first.duckdb.unwrap();
        let second_duckdb = second.duckdb.unwrap();
        assert_eq!(first_duckdb.mime_type, second_duckdb.mime_type);
        assert_eq!(first_duckdb.filename, second_duckdb.filename);
        assert_eq!(first_duckdb.metadata, second_duckdb.metadata);
        assert!(!first_duckdb.bytes.is_empty());
        assert!(!second_duckdb.bytes.is_empty());

        let first_rrd = first.rrd.unwrap();
        let second_rrd = second.rrd.unwrap();
        assert_eq!(first_rrd.mime_type, second_rrd.mime_type);
        assert_eq!(first_rrd.filename, second_rrd.filename);
        assert_eq!(first_rrd.metadata, second_rrd.metadata);
        assert!(!first_rrd.bytes.is_empty());
        assert!(!second_rrd.bytes.is_empty());
    }
}
