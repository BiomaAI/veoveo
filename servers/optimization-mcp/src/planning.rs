use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
};

use anyhow::{Context, Result, bail};
use good_lp::{
    Expression, ProblemVariables, ResolutionError, Solution, SolverModel, Variable, default_solver,
    variable,
};
use sha2::{Digest, Sha256};

use crate::{
    contract::{
        AgentGroupId, AgentId, AssignmentUnitKind, GovernedPlan, LaneId, MAX_AGENTS,
        MAX_GENERATED_CANDIDATES, MAX_GROUPS, MAX_LANES, MAX_MUTUAL_EXCLUSION_SETS,
        MAX_RESOURCE_BANDS, MAX_SHARED_RESOURCES, MAX_TASKS, MapGeometryReference,
        MapMobilityProfileUri, PLAN_ALGORITHM_REVISION, PLAN_SCHEMA_VERSION, PlanAssignment,
        PlanAssignmentId, PlanAuthority, PlanFinding, PlanFindingCode, PlanId, PlanMetrics,
        PlanObjectiveValues, PlanRecurrence, PlanRequest, PlanRequirementResult, PlanSolverSummary,
        PlanStatus, PlanningAgent, PlanningAgentGroup, RequirementSatisfaction, ResourceBandId,
        SharedResourceId, SpatialPlanningTask, SpatialTaskId, TaskAdmission, TaskTiming,
    },
    plan_artifacts::{PlanArtifactBytes, encode_duckdb, encode_plan_json, encode_rrd},
};

pub use crate::plan_artifacts::{
    DUCKDB_FILENAME, DUCKDB_MIME_TYPE, PLAN_JSON_FILENAME, PLAN_JSON_MIME_TYPE, RRD_FILENAME,
    RRD_MIME_TYPE,
};

#[derive(Debug, Clone)]
pub struct PlanRun {
    pub plan: GovernedPlan,
    pub plan_json: PlanArtifactBytes,
    pub rrd: Option<PlanArtifactBytes>,
    pub duckdb: Option<PlanArtifactBytes>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum UnitKey {
    Agent(AgentId),
    Group(AgentGroupId),
}

#[derive(Debug, Clone)]
struct PlanningUnit {
    key: UnitKey,
    agent_ids: BTreeSet<AgentId>,
    group_id: Option<AgentGroupId>,
    capabilities: BTreeSet<crate::contract::CapabilityId>,
    mobility_profiles: BTreeSet<MapMobilityProfileUri>,
    cost: f64,
    risk: f64,
    confidence: f64,
}

#[derive(Debug, Clone)]
struct Candidate {
    task_index: usize,
    unit: PlanningUnit,
    lane_id: Option<LaneId>,
    resource_band_id: Option<ResourceBandId>,
    stable_key: String,
    utility: f64,
}

#[derive(Debug)]
struct GeneratedProblem<'a> {
    request: &'a PlanRequest,
    candidates: Vec<Candidate>,
    eligible_units: Vec<usize>,
}

struct SolvedPlan {
    selected_candidates: Vec<usize>,
    active_tasks: Vec<bool>,
    weighted_objective: Option<f64>,
    constraints: u64,
    termination: &'static str,
}

pub fn run_plan(
    task_id: &str,
    request: &PlanRequest,
    authority: &PlanAuthority,
) -> Result<PlanRun> {
    validate_request(request)?;
    let request_digest = digest_json(request)?;
    let problem = generate_problem(request)?;
    let solved = solve_problem(&problem)?;
    let plan = build_plan(
        task_id,
        request,
        authority,
        &request_digest,
        &problem,
        solved,
    )?;
    let plan_json = encode_plan_json(&plan)?;
    let duckdb = request
        .artifacts
        .duckdb
        .then(|| encode_duckdb(&plan))
        .transpose()?;
    let rrd = request
        .artifacts
        .rerun_rrd
        .then(|| encode_rrd(&plan))
        .transpose()?;
    Ok(PlanRun {
        plan,
        plan_json,
        rrd,
        duckdb,
    })
}

fn validate_request(request: &PlanRequest) -> Result<()> {
    if request.schema_version != PLAN_SCHEMA_VERSION {
        bail!("unsupported plan request schema version");
    }
    if request.source_map_releases.is_empty() || request.source_map_releases.len() > 64 {
        bail!("source_map_releases must contain 1..=64 immutable Map releases");
    }
    bounded_nonempty("agents", request.agents.len(), MAX_AGENTS)?;
    bounded_nonempty("tasks", request.tasks.len(), MAX_TASKS)?;
    bounded("groups", request.groups.len(), MAX_GROUPS)?;
    bounded(
        "shared_resources",
        request.shared_resources.len(),
        MAX_SHARED_RESOURCES,
    )?;
    bounded("lanes", request.lanes.len(), MAX_LANES)?;
    bounded(
        "resource_bands",
        request.resource_bands.len(),
        MAX_RESOURCE_BANDS,
    )?;
    bounded(
        "mutual_exclusions",
        request.mutual_exclusions.len(),
        MAX_MUTUAL_EXCLUSION_SETS,
    )?;
    if !(1..=MAX_GENERATED_CANDIDATES).contains(&request.solver.maximum_generated_candidates) {
        bail!("solver.maximum_generated_candidates must be within 1..={MAX_GENERATED_CANDIDATES}");
    }

    let agents = unique_by("agent", request.agents.iter().map(|agent| &agent.agent_id))?;
    let profiles = request
        .agents
        .iter()
        .map(|agent| agent.mobility_profile.clone())
        .collect::<BTreeSet<_>>();
    for agent in &request.agents {
        if agent.maximum_assignments == 0 {
            bail!("agent `{}` has zero maximum_assignments", agent.agent_id);
        }
    }

    let groups = unique_by(
        "agent group",
        request.groups.iter().map(|group| &group.group_id),
    )?;
    for group in &request.groups {
        if group.member_agent_ids.is_empty() {
            bail!("agent group `{}` has no members", group.group_id);
        }
        for member in &group.member_agent_ids {
            require_known("agent group member", member, &agents)?;
        }
    }

    let resources = unique_by(
        "shared resource",
        request
            .shared_resources
            .iter()
            .map(|resource| &resource.resource_id),
    )?;
    let lanes = unique_by("lane", request.lanes.iter().map(|lane| &lane.lane_id))?;
    let bands = unique_by(
        "resource band",
        request
            .resource_bands
            .iter()
            .map(|band| &band.resource_band_id),
    )?;
    for lane in &request.lanes {
        if lane.capacity == 0 {
            bail!("lane `{}` has zero capacity", lane.lane_id);
        }
    }
    for band in &request.resource_bands {
        if band.capacity == 0 {
            bail!(
                "resource band `{}` has zero capacity",
                band.resource_band_id
            );
        }
    }
    for agent in &request.agents {
        for resource in agent.resource_capacities.keys() {
            require_known("agent resource capacity", resource, &resources)?;
        }
    }

    let tasks = unique_by(
        "spatial task",
        request.tasks.iter().map(|task| &task.task_id),
    )?;
    let release_ids = request
        .source_map_releases
        .iter()
        .map(|release| release.release_id())
        .collect::<BTreeSet<_>>();
    for task in &request.tasks {
        validate_task(
            task,
            &agents,
            &groups,
            &profiles,
            &tasks,
            &resources,
            &lanes,
            &bands,
            &release_ids,
        )?;
    }
    validate_dependency_graph(request, &tasks)?;

    let exclusions = unique_by(
        "mutual exclusion",
        request
            .mutual_exclusions
            .iter()
            .map(|exclusion| &exclusion.exclusion_id),
    )?;
    debug_assert_eq!(exclusions.len(), request.mutual_exclusions.len());
    for exclusion in &request.mutual_exclusions {
        if exclusion.task_ids.len() < 2
            || exclusion.maximum_active_tasks == 0
            || exclusion.maximum_active_tasks as usize >= exclusion.task_ids.len()
        {
            bail!(
                "mutual exclusion `{}` requires at least two tasks and a maximum below its task count",
                exclusion.exclusion_id
            );
        }
        for task in &exclusion.task_ids {
            require_known("mutual-exclusion task", task, &tasks)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_task(
    task: &SpatialPlanningTask,
    agents: &HashSet<AgentId>,
    groups: &HashSet<AgentGroupId>,
    profiles: &BTreeSet<MapMobilityProfileUri>,
    tasks: &HashSet<SpatialTaskId>,
    resources: &HashSet<SharedResourceId>,
    lanes: &HashSet<LaneId>,
    bands: &HashSet<ResourceBandId>,
    release_ids: &BTreeSet<&str>,
) -> Result<()> {
    if task.quantity.desired == 0 || task.quantity.minimum > task.quantity.desired {
        bail!(
            "task `{}` quantity must satisfy 0 <= minimum <= desired and desired > 0",
            task.task_id
        );
    }
    if task.admission == TaskAdmission::Required && task.quantity.minimum == 0 {
        bail!(
            "required task `{}` must declare a positive minimum quantity",
            task.task_id
        );
    }
    if let TaskTiming::FixedWindow { start, end } = task.timing
        && end <= start
    {
        bail!("task `{}` fixed window is empty or reversed", task.task_id);
    }
    match task.recurrence {
        PlanRecurrence::Once => {}
        PlanRecurrence::Loop { repetitions } if repetitions > 0 => {}
        PlanRecurrence::Periodic { occurrences, .. } if occurrences > 0 => {}
        _ => bail!("task `{}` recurrence count must be positive", task.task_id),
    }
    if let MapGeometryReference::SourceFeature { uri } = &task.target
        && !release_ids.contains(uri.release_id())
    {
        bail!(
            "task `{}` source feature release is absent from source_map_releases",
            task.task_id
        );
    }
    for id in &task.eligible_agent_ids {
        require_known("eligible agent", id, agents)?;
    }
    for id in &task.eligible_group_ids {
        require_known("eligible group", id, groups)?;
    }
    for profile in &task.allowed_mobility_profiles {
        if !profiles.contains(profile) {
            bail!(
                "task `{}` allows a mobility profile not declared by an agent",
                task.task_id
            );
        }
    }
    for dependency in &task.depends_on {
        require_known("task dependency", dependency, tasks)?;
        if dependency == &task.task_id {
            bail!("task `{}` depends on itself", task.task_id);
        }
    }
    for resource in task
        .shared_resource_demand
        .keys()
        .chain(task.agent_resource_demand.keys())
    {
        require_known("task resource demand", resource, resources)?;
    }
    for lane in &task.allowed_lane_ids {
        require_known("task lane", lane, lanes)?;
    }
    for band in &task.allowed_resource_band_ids {
        require_known("task resource band", band, bands)?;
    }
    Ok(())
}

fn validate_dependency_graph(request: &PlanRequest, tasks: &HashSet<SpatialTaskId>) -> Result<()> {
    fn visit(
        task: &SpatialTaskId,
        by_id: &HashMap<SpatialTaskId, &SpatialPlanningTask>,
        visiting: &mut HashSet<SpatialTaskId>,
        visited: &mut HashSet<SpatialTaskId>,
    ) -> Result<()> {
        if visited.contains(task) {
            return Ok(());
        }
        if !visiting.insert(task.clone()) {
            bail!("task dependency graph contains a cycle at `{task}`");
        }
        for dependency in &by_id[task].depends_on {
            visit(dependency, by_id, visiting, visited)?;
        }
        visiting.remove(task);
        visited.insert(task.clone());
        Ok(())
    }

    let by_id = request
        .tasks
        .iter()
        .map(|task| (task.task_id.clone(), task))
        .collect::<HashMap<_, _>>();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for task in tasks {
        visit(task, &by_id, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn bounded(label: &str, actual: usize, maximum: usize) -> Result<()> {
    if actual > maximum {
        bail!("{label} exceeds its {maximum}-item limit");
    }
    Ok(())
}

fn bounded_nonempty(label: &str, actual: usize, maximum: usize) -> Result<()> {
    if actual == 0 || actual > maximum {
        bail!("{label} must contain 1..={maximum} items");
    }
    Ok(())
}

fn unique_by<'a, T>(label: &str, values: impl Iterator<Item = &'a T>) -> Result<HashSet<T>>
where
    T: Clone + Eq + std::hash::Hash + fmt::Display + 'a,
{
    let mut result = HashSet::new();
    for value in values {
        if !result.insert(value.clone()) {
            bail!("duplicate {label} `{value}`");
        }
    }
    Ok(result)
}

fn require_known<T>(label: &str, value: &T, known: &HashSet<T>) -> Result<()>
where
    T: Eq + std::hash::Hash + fmt::Display,
{
    if !known.contains(value) {
        bail!("{label} `{value}` is unknown");
    }
    Ok(())
}

fn generate_problem(request: &PlanRequest) -> Result<GeneratedProblem<'_>> {
    let agent_by_id = request
        .agents
        .iter()
        .map(|agent| (agent.agent_id.clone(), agent))
        .collect::<HashMap<_, _>>();
    let individual_units = request
        .agents
        .iter()
        .map(individual_unit)
        .collect::<Vec<_>>();
    let group_units = request
        .groups
        .iter()
        .map(|group| group_unit(group, &agent_by_id))
        .collect::<Result<Vec<_>>>()?;

    let maximum = request.solver.maximum_generated_candidates as usize;
    let mut candidates = Vec::new();
    let mut eligible_units = Vec::with_capacity(request.tasks.len());
    for (task_index, task) in request.tasks.iter().enumerate() {
        let units = individual_units
            .iter()
            .filter(|_| {
                matches!(
                    task.assignment_unit,
                    AssignmentUnitKind::Agent | AssignmentUnitKind::AgentOrGroup
                )
            })
            .chain(group_units.iter().filter(|_| {
                matches!(
                    task.assignment_unit,
                    AssignmentUnitKind::Group | AssignmentUnitKind::AgentOrGroup
                )
            }))
            .filter(|unit| unit_is_eligible(unit, task))
            .cloned()
            .collect::<Vec<_>>();
        eligible_units.push(units.len());

        let lane_ids = optional_choices(&task.allowed_lane_ids);
        let band_ids = optional_choices(&task.allowed_resource_band_ids);
        for unit in units {
            for lane_id in &lane_ids {
                for resource_band_id in &band_ids {
                    if candidates.len() >= maximum {
                        bail!(
                            "compact request expands beyond solver.maximum_generated_candidates ({maximum})"
                        );
                    }
                    let stable_key = format!(
                        "{}|{}|{}|{}",
                        task.task_id,
                        unit_key_string(&unit.key),
                        lane_id
                            .as_ref()
                            .map_or_else(|| "-".to_owned(), ToString::to_string),
                        resource_band_id
                            .as_ref()
                            .map_or_else(|| "-".to_owned(), ToString::to_string)
                    );
                    let utility = candidate_utility(request, task, &unit, &stable_key);
                    candidates.push(Candidate {
                        task_index,
                        unit: unit.clone(),
                        lane_id: lane_id.clone(),
                        resource_band_id: resource_band_id.clone(),
                        stable_key,
                        utility,
                    });
                }
            }
        }
    }
    Ok(GeneratedProblem {
        request,
        candidates,
        eligible_units,
    })
}

fn individual_unit(agent: &PlanningAgent) -> PlanningUnit {
    PlanningUnit {
        key: UnitKey::Agent(agent.agent_id.clone()),
        agent_ids: BTreeSet::from([agent.agent_id.clone()]),
        group_id: None,
        capabilities: agent.capabilities.clone(),
        mobility_profiles: BTreeSet::from([agent.mobility_profile.clone()]),
        cost: agent.assignment_cost.get(),
        risk: agent.assignment_risk.get(),
        confidence: agent.confidence.get(),
    }
}

fn group_unit(
    group: &PlanningAgentGroup,
    agents: &HashMap<AgentId, &PlanningAgent>,
) -> Result<PlanningUnit> {
    let mut capabilities = group.capabilities.clone();
    let mut mobility_profiles = BTreeSet::new();
    let mut cost = 0.0;
    let mut risk = 0.0;
    let mut confidence: f64 = 1.0;
    for member in &group.member_agent_ids {
        let agent = agents
            .get(member)
            .with_context(|| format!("unknown member `{member}`"))?;
        capabilities.extend(agent.capabilities.iter().cloned());
        mobility_profiles.insert(agent.mobility_profile.clone());
        cost += agent.assignment_cost.get();
        risk += agent.assignment_risk.get();
        confidence = confidence.min(agent.confidence.get());
    }
    Ok(PlanningUnit {
        key: UnitKey::Group(group.group_id.clone()),
        agent_ids: group.member_agent_ids.clone(),
        group_id: Some(group.group_id.clone()),
        capabilities,
        mobility_profiles,
        cost,
        risk,
        confidence,
    })
}

fn unit_is_eligible(unit: &PlanningUnit, task: &SpatialPlanningTask) -> bool {
    if !task.required_capabilities.is_subset(&unit.capabilities) {
        return false;
    }
    if !task.allowed_mobility_profiles.is_empty()
        && !unit
            .mobility_profiles
            .is_subset(&task.allowed_mobility_profiles)
    {
        return false;
    }
    match &unit.key {
        UnitKey::Agent(agent) => {
            task.eligible_agent_ids.is_empty() || task.eligible_agent_ids.contains(agent)
        }
        UnitKey::Group(group) => {
            task.eligible_group_ids.is_empty() || task.eligible_group_ids.contains(group)
        }
    }
}

fn optional_choices<T: Clone + Ord>(values: &BTreeSet<T>) -> Vec<Option<T>> {
    if values.is_empty() {
        vec![None]
    } else {
        values.iter().cloned().map(Some).collect()
    }
}

fn unit_key_string(key: &UnitKey) -> String {
    match key {
        UnitKey::Agent(id) => format!("agent:{id}"),
        UnitKey::Group(id) => format!("group:{id}"),
    }
}

fn candidate_utility(
    request: &PlanRequest,
    task: &SpatialPlanningTask,
    unit: &PlanningUnit,
    stable_key: &str,
) -> f64 {
    let resource_total = task
        .shared_resource_demand
        .values()
        .chain(task.agent_resource_demand.values())
        .map(|amount| amount.get())
        .sum::<f64>();
    let objective = &request.objective;
    let priority = task.priority.get() / f64::from(task.quantity.desired);
    let cost = unit.cost + task.assignment_cost.get();
    let risk = unit.risk + task.assignment_risk.get();
    objective.priority_weight.get() * priority + objective.confidence_weight.get() * unit.confidence
        - objective.cost_weight.get() * cost
        - objective.risk_weight.get() * risk
        - objective.resource_weight.get() * resource_total
        + deterministic_tie_break(request.solver.deterministic_seed, stable_key)
}

fn deterministic_tie_break(seed: u64, key: &str) -> f64 {
    let mut digest = Sha256::new();
    digest.update(seed.to_le_bytes());
    digest.update(key.as_bytes());
    let bytes: [u8; 8] = digest.finalize()[..8]
        .try_into()
        .expect("SHA-256 prefix has eight bytes");
    (u64::from_le_bytes(bytes) as f64 / u64::MAX as f64) * 1.0e-9
}

fn solve_problem(problem: &GeneratedProblem<'_>) -> Result<SolvedPlan> {
    let request = problem.request;
    let mut variables = ProblemVariables::new();
    let candidate_variables = problem
        .candidates
        .iter()
        .map(|_| variables.add(variable().binary()))
        .collect::<Vec<_>>();
    let task_active_variables = request
        .tasks
        .iter()
        .map(|_| variables.add(variable().binary()))
        .collect::<Vec<_>>();

    let mut objective = Expression::from(0.0);
    for (index, candidate) in problem.candidates.iter().enumerate() {
        objective -= candidate.utility * candidate_variables[index];
    }
    let objective_for_evaluation = objective.clone();
    let mut model = variables.minimise(objective).using(default_solver);
    let mut constraints = 0_u64;

    let task_expressions = request
        .tasks
        .iter()
        .enumerate()
        .map(|(task_index, _)| {
            candidate_expression(problem, &candidate_variables, |candidate| {
                candidate.task_index == task_index
            })
        })
        .collect::<Vec<_>>();

    for (task_index, task) in request.tasks.iter().enumerate() {
        let selected = task_expressions[task_index].clone();
        let active = task_active_variables[task_index];
        model = model.with(
            selected
                .clone()
                .leq(f64::from(task.quantity.desired) * active),
        );
        constraints += 1;
        model = model.with((selected.clone() - active).geq(0.0));
        constraints += 1;
        if task.quantity.minimum > 0 {
            model =
                model.with((selected.clone() - f64::from(task.quantity.minimum) * active).geq(0.0));
            constraints += 1;
        }
        if task.admission == TaskAdmission::Required {
            model = model.with(Expression::from(active).eq(1.0));
            constraints += 1;
        }
    }

    let mut task_unit_candidates: BTreeMap<(usize, UnitKey), Vec<usize>> = BTreeMap::new();
    for (index, candidate) in problem.candidates.iter().enumerate() {
        task_unit_candidates
            .entry((candidate.task_index, candidate.unit.key.clone()))
            .or_default()
            .push(index);
    }
    for indices in task_unit_candidates.values() {
        model = model.with(index_expression(&candidate_variables, indices).leq(1.0));
        constraints += 1;
    }

    for agent in &request.agents {
        let indices = candidate_indices(problem, |candidate| {
            candidate.unit.agent_ids.contains(&agent.agent_id)
        });
        model = model.with(
            index_expression(&candidate_variables, &indices)
                .leq(f64::from(agent.maximum_assignments)),
        );
        constraints += 1;
        for (resource, capacity) in &agent.resource_capacities {
            let mut expression = Expression::from(0.0);
            for index in &indices {
                let task = &request.tasks[problem.candidates[*index].task_index];
                expression += task
                    .agent_resource_demand
                    .get(resource)
                    .map_or(0.0, |amount| amount.get())
                    * candidate_variables[*index];
            }
            model = model.with(expression.leq(capacity.get()));
            constraints += 1;
        }

        for left in 0..request.tasks.len() {
            let left_indices = candidate_indices(problem, |candidate| {
                candidate.task_index == left && candidate.unit.agent_ids.contains(&agent.agent_id)
            });
            if !left_indices.is_empty() {
                model = model.with(index_expression(&candidate_variables, &left_indices).leq(1.0));
                constraints += 1;
            }
            for right in (left + 1)..request.tasks.len() {
                if !request.tasks[left]
                    .timing
                    .overlaps(&request.tasks[right].timing)
                {
                    continue;
                }
                let right_indices = candidate_indices(problem, |candidate| {
                    candidate.task_index == right
                        && candidate.unit.agent_ids.contains(&agent.agent_id)
                });
                if left_indices.is_empty() || right_indices.is_empty() {
                    continue;
                }
                model = model.with(
                    (index_expression(&candidate_variables, &left_indices)
                        + index_expression(&candidate_variables, &right_indices))
                    .leq(1.0),
                );
                constraints += 1;
            }
        }
    }

    for resource in &request.shared_resources {
        let mut expression = Expression::from(0.0);
        for (index, candidate) in problem.candidates.iter().enumerate() {
            expression += request.tasks[candidate.task_index]
                .shared_resource_demand
                .get(&resource.resource_id)
                .map_or(0.0, |amount| amount.get())
                * candidate_variables[index];
        }
        model = model.with(expression.leq(resource.capacity.get()));
        constraints += 1;
    }
    for lane in &request.lanes {
        let indices = candidate_indices(problem, |candidate| {
            candidate.lane_id.as_ref() == Some(&lane.lane_id)
        });
        model = model
            .with(index_expression(&candidate_variables, &indices).leq(f64::from(lane.capacity)));
        constraints += 1;
    }
    for band in &request.resource_bands {
        let indices = candidate_indices(problem, |candidate| {
            candidate.resource_band_id.as_ref() == Some(&band.resource_band_id)
        });
        model = model
            .with(index_expression(&candidate_variables, &indices).leq(f64::from(band.capacity)));
        constraints += 1;
    }

    let task_by_id = request
        .tasks
        .iter()
        .enumerate()
        .map(|(index, task)| (task.task_id.clone(), index))
        .collect::<HashMap<_, _>>();
    for (task_index, task) in request.tasks.iter().enumerate() {
        for dependency in &task.depends_on {
            let dependency_index = task_by_id[dependency];
            model = model.with(
                (task_active_variables[task_index] - task_active_variables[dependency_index])
                    .leq(0.0),
            );
            constraints += 1;
            model = model.with(
                (task_expressions[dependency_index].clone()
                    - f64::from(request.tasks[dependency_index].quantity.desired)
                        * task_active_variables[task_index])
                    .geq(0.0),
            );
            constraints += 1;
        }
    }
    for exclusion in &request.mutual_exclusions {
        let expression = exclusion
            .task_ids
            .iter()
            .fold(Expression::from(0.0), |expression, task| {
                expression + task_active_variables[task_by_id[task]]
            });
        model = model.with(expression.leq(f64::from(exclusion.maximum_active_tasks)));
        constraints += 1;
    }

    let solution = match model.solve() {
        Ok(solution) => Some(solution),
        Err(ResolutionError::Infeasible) => None,
        Err(error) => return Err(error).context("solving compact spatial assignment MILP"),
    };
    let Some(solution) = solution else {
        return Ok(SolvedPlan {
            selected_candidates: Vec::new(),
            active_tasks: vec![false; request.tasks.len()],
            weighted_objective: None,
            constraints,
            termination: "infeasible",
        });
    };
    Ok(SolvedPlan {
        selected_candidates: candidate_variables
            .iter()
            .enumerate()
            .filter_map(|(index, variable)| (solution.value(*variable) >= 0.5).then_some(index))
            .collect(),
        active_tasks: task_active_variables
            .iter()
            .map(|variable| solution.value(*variable) >= 0.5)
            .collect(),
        weighted_objective: Some(-solution.eval(objective_for_evaluation)),
        constraints,
        termination: "optimal",
    })
}

fn candidate_expression(
    problem: &GeneratedProblem<'_>,
    variables: &[Variable],
    mut matches: impl FnMut(&Candidate) -> bool,
) -> Expression {
    let mut expression = Expression::from(0.0);
    for (index, candidate) in problem.candidates.iter().enumerate() {
        if matches(candidate) {
            expression += variables[index];
        }
    }
    expression
}

fn candidate_indices(
    problem: &GeneratedProblem<'_>,
    mut matches: impl FnMut(&Candidate) -> bool,
) -> Vec<usize> {
    problem
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| matches(candidate).then_some(index))
        .collect()
}

fn index_expression(variables: &[Variable], indices: &[usize]) -> Expression {
    indices
        .iter()
        .fold(Expression::from(0.0), |expression, index| {
            expression + variables[*index]
        })
}

fn build_plan(
    task_id: &str,
    request: &PlanRequest,
    authority: &PlanAuthority,
    request_digest: &str,
    problem: &GeneratedProblem<'_>,
    solved: SolvedPlan,
) -> Result<GovernedPlan> {
    let plan_key = format!("{task_id}:{request_digest}");
    let plan_id = PlanId::from_stable_key(plan_key.as_bytes());
    let resource_uri = format!("optimization://plan/{plan_id}");
    let selected = solved
        .selected_candidates
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let mut selected_candidates = solved
        .selected_candidates
        .iter()
        .map(|index| &problem.candidates[*index])
        .collect::<Vec<_>>();
    selected_candidates.sort_by(|left, right| left.stable_key.cmp(&right.stable_key));

    let mut next_ordinal = HashMap::<SpatialTaskId, u32>::new();
    let assignments = selected_candidates
        .into_iter()
        .map(|candidate| {
            let task = &request.tasks[candidate.task_index];
            let ordinal = next_ordinal.entry(task.task_id.clone()).or_insert(0);
            *ordinal += 1;
            let assignment_key = format!("{plan_id}:{}", candidate.stable_key);
            PlanAssignment {
                assignment_id: PlanAssignmentId::from_stable_key(assignment_key.as_bytes()),
                task_id: task.task_id.clone(),
                ordinal: *ordinal,
                agent_ids: candidate.unit.agent_ids.clone(),
                group_id: candidate.unit.group_id.clone(),
                mobility_profiles: candidate.unit.mobility_profiles.clone(),
                target: task.target.clone(),
                execution: task.execution.clone(),
                lane_id: candidate.lane_id.clone(),
                resource_band_id: candidate.resource_band_id.clone(),
                timing: task.timing.clone(),
                recurrence: task.recurrence.clone(),
                shared_resources: task.shared_resource_demand.clone(),
                cost: candidate.unit.cost + task.assignment_cost.get(),
                risk: candidate.unit.risk + task.assignment_risk.get(),
                confidence: candidate.unit.confidence,
            }
        })
        .collect::<Vec<_>>();

    let solver_infeasible = solved.weighted_objective.is_none();
    let requirements = request
        .tasks
        .iter()
        .enumerate()
        .map(|(task_index, task)| {
            let assigned = problem
                .candidates
                .iter()
                .enumerate()
                .filter(|(candidate_index, candidate)| {
                    selected.contains(candidate_index) && candidate.task_index == task_index
                })
                .count() as u32;
            let satisfaction = if assigned >= task.quantity.desired {
                RequirementSatisfaction::Complete
            } else if assigned > 0 {
                RequirementSatisfaction::Partial
            } else if task.admission == TaskAdmission::Optional && !solved.active_tasks[task_index]
            {
                RequirementSatisfaction::Inactive
            } else {
                RequirementSatisfaction::Unmet
            };
            PlanRequirementResult {
                task_id: task.task_id.clone(),
                minimum_quantity: task.quantity.minimum,
                desired_quantity: task.quantity.desired,
                assigned_quantity: assigned,
                satisfaction,
            }
        })
        .collect::<Vec<_>>();
    let mut findings = eligibility_findings(request, &problem.eligible_units);
    for requirement in &requirements {
        match requirement.satisfaction {
            RequirementSatisfaction::Partial => findings.push(PlanFinding {
                code: PlanFindingCode::DesiredQuantityUnsatisfied,
                task_id: Some(requirement.task_id.clone()),
                message: format!(
                    "task {} received {} of {} desired assignments",
                    requirement.task_id,
                    requirement.assigned_quantity,
                    requirement.desired_quantity
                ),
            }),
            RequirementSatisfaction::Unmet => findings.push(PlanFinding {
                code: PlanFindingCode::HardMinimumUnsatisfied,
                task_id: Some(requirement.task_id.clone()),
                message: format!(
                    "task {} did not meet its minimum of {} assignments",
                    requirement.task_id, requirement.minimum_quantity
                ),
            }),
            RequirementSatisfaction::Complete | RequirementSatisfaction::Inactive => {}
        }
    }
    if solver_infeasible {
        findings.push(PlanFinding {
            code: PlanFindingCode::SolverInfeasible,
            task_id: None,
            message: "the declared hard minima, dependencies, exclusions, capacities, and timing constraints are infeasible".to_owned(),
        });
    }
    findings.sort_by(|left, right| {
        left.task_id
            .cmp(&right.task_id)
            .then_with(|| format!("{:?}", left.code).cmp(&format!("{:?}", right.code)))
    });

    let status = if solver_infeasible {
        PlanStatus::Infeasible
    } else if requirements.iter().any(|requirement| {
        matches!(
            requirement.satisfaction,
            RequirementSatisfaction::Partial | RequirementSatisfaction::Unmet
        )
    }) {
        PlanStatus::Partial
    } else {
        PlanStatus::Optimal
    };
    let objective_values = objective_values(request, &assignments, solved.weighted_objective);
    let metrics = plan_metrics(request, problem, &assignments, &requirements);
    let mobility_profiles = request
        .agents
        .iter()
        .map(|agent| agent.mobility_profile.clone())
        .collect();
    let solver = PlanSolverSummary {
        backend: request.solver.backend,
        algorithm_revision: PLAN_ALGORITHM_REVISION.to_owned(),
        deterministic_seed: request.solver.deterministic_seed,
        variables: (problem.candidates.len() + request.tasks.len()) as u64,
        constraints: solved.constraints,
        generated_candidates: problem.candidates.len() as u64,
        termination: solved.termination.to_owned(),
    };
    let mut plan = GovernedPlan {
        schema_version: PLAN_SCHEMA_VERSION,
        plan_id,
        resource_uri,
        status,
        assignments,
        requirements,
        findings,
        source_map_releases: request.source_map_releases.clone(),
        frame_world_revision: request.frame_world_revision.clone(),
        mobility_profiles,
        objective_values,
        metrics,
        solver,
        algorithm_revision: PLAN_ALGORITHM_REVISION.to_owned(),
        request_digest_sha256: request_digest.to_owned(),
        plan_digest_sha256: String::new(),
        authority: authority.clone(),
        created_at: authority.submitted_at,
    };
    plan.plan_digest_sha256 = plan_digest(&plan)?;
    Ok(plan)
}

fn eligibility_findings(request: &PlanRequest, eligible_units: &[usize]) -> Vec<PlanFinding> {
    request
        .tasks
        .iter()
        .zip(eligible_units)
        .filter_map(|(task, eligible)| {
            if *eligible == 0 {
                Some(PlanFinding {
                    code: PlanFindingCode::NoEligibleUnit,
                    task_id: Some(task.task_id.clone()),
                    message: format!(
                        "task {} has no capability- and mobility-compatible assignment unit",
                        task.task_id
                    ),
                })
            } else if *eligible < task.quantity.desired as usize {
                Some(PlanFinding {
                    code: PlanFindingCode::InsufficientEligibleUnits,
                    task_id: Some(task.task_id.clone()),
                    message: format!(
                        "task {} has {} eligible units for {} desired assignments",
                        task.task_id, eligible, task.quantity.desired
                    ),
                })
            } else {
                None
            }
        })
        .collect()
}

fn objective_values(
    request: &PlanRequest,
    assignments: &[PlanAssignment],
    weighted_objective: Option<f64>,
) -> PlanObjectiveValues {
    let task_by_id = request
        .tasks
        .iter()
        .map(|task| (&task.task_id, task))
        .collect::<HashMap<_, _>>();
    let priority_utility = assignments
        .iter()
        .map(|assignment| {
            let task = task_by_id[&assignment.task_id];
            task.priority.get() / f64::from(task.quantity.desired)
        })
        .sum();
    let cost_penalty = assignments.iter().map(|assignment| assignment.cost).sum();
    let risk_penalty = assignments.iter().map(|assignment| assignment.risk).sum();
    let confidence_utility = assignments
        .iter()
        .map(|assignment| assignment.confidence)
        .sum();
    let resource_penalty = assignments
        .iter()
        .flat_map(|assignment| assignment.shared_resources.values())
        .map(|amount| amount.get())
        .sum();
    PlanObjectiveValues {
        weighted_objective,
        priority_utility,
        cost_penalty,
        risk_penalty,
        confidence_utility,
        resource_penalty,
    }
}

fn plan_metrics(
    request: &PlanRequest,
    problem: &GeneratedProblem<'_>,
    assignments: &[PlanAssignment],
    requirements: &[PlanRequirementResult],
) -> PlanMetrics {
    let mut shared_resource_usage = BTreeMap::new();
    let mut lane_usage = BTreeMap::new();
    let mut resource_band_usage = BTreeMap::new();
    for assignment in assignments {
        for (resource, amount) in &assignment.shared_resources {
            *shared_resource_usage.entry(resource.clone()).or_insert(0.0) += amount.get();
        }
        if let Some(lane) = &assignment.lane_id {
            *lane_usage.entry(lane.clone()).or_insert(0) += 1;
        }
        if let Some(band) = &assignment.resource_band_id {
            *resource_band_usage.entry(band.clone()).or_insert(0) += 1;
        }
    }
    PlanMetrics {
        agents: request.agents.len() as u64,
        groups: request.groups.len() as u64,
        tasks: request.tasks.len() as u64,
        generated_candidates: problem.candidates.len() as u64,
        assignments: assignments.len() as u64,
        complete_requirements: requirements
            .iter()
            .filter(|requirement| requirement.satisfaction == RequirementSatisfaction::Complete)
            .count() as u64,
        partial_requirements: requirements
            .iter()
            .filter(|requirement| requirement.satisfaction == RequirementSatisfaction::Partial)
            .count() as u64,
        unmet_requirements: requirements
            .iter()
            .filter(|requirement| requirement.satisfaction == RequirementSatisfaction::Unmet)
            .count() as u64,
        total_cost: assignments.iter().map(|assignment| assignment.cost).sum(),
        total_risk: assignments.iter().map(|assignment| assignment.risk).sum(),
        total_confidence: assignments
            .iter()
            .map(|assignment| assignment.confidence)
            .sum(),
        shared_resource_usage,
        lane_usage,
        resource_band_usage,
    }
}

fn plan_digest(plan: &GovernedPlan) -> Result<String> {
    let mut value = serde_json::to_value(plan)?;
    value
        .as_object_mut()
        .context("governed plan did not serialize as an object")?
        .remove("plan_digest_sha256");
    digest_json(&value)
}

fn digest_json(value: &impl serde::Serialize) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(value)?)))
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use veoveo_mcp_contract::{FrameWorldRevisionUri, PolicyVersion, PrincipalId, WorkContextId};

    use super::*;
    use crate::contract::{
        AgentGroupId, AgentId, ArtifactTrajectoryUri, CapabilityId, Confidence, LaneId,
        MapMobilityProfileUri, MapReleaseUri, MapRouteUri, MapSourceFeatureUri, MutualExclusionId,
        NonNegative, PlanArtifactOptions, PlanExecutionReference, PlanningAgent,
        PlanningAgentGroup, PlanningLane, PlanningObjective, PlanningResourceBand,
        PlanningSolverPolicy, Positive, ResourceBandId, SharedResource, SharedResourceId,
        SpatialPlanningTask, TaskMutualExclusion, TaskQuantity,
    };

    fn authority() -> PlanAuthority {
        PlanAuthority {
            principal_id: PrincipalId::new("issuer#planner").unwrap(),
            work_context: WorkContextId::new("mission").unwrap(),
            policy_revision: PolicyVersion::new("1").unwrap(),
            submitted_at: DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
        }
    }

    fn request() -> PlanRequest {
        let release = MapReleaseUri::parse("map://dataset/base/release/release-immutable").unwrap();
        let profile =
            MapMobilityProfileUri::parse("map://mobility-profile/mobility-air/3").unwrap();
        PlanRequest {
            schema_version: PLAN_SCHEMA_VERSION,
            source_map_releases: BTreeSet::from([release]),
            frame_world_revision: FrameWorldRevisionUri::parse(
                "frames://world/test/revision/revision-1".to_owned(),
            )
            .unwrap(),
            agents: vec![
                PlanningAgent {
                    agent_id: AgentId::new("agent-a").unwrap(),
                    mobility_profile: profile.clone(),
                    capabilities: BTreeSet::from([CapabilityId::new("inspect").unwrap()]),
                    resource_capacities: BTreeMap::from([(
                        SharedResourceId::new("energy").unwrap(),
                        Positive::new(2.0).unwrap(),
                    )]),
                    maximum_assignments: 1,
                    assignment_cost: NonNegative::new(1.0).unwrap(),
                    assignment_risk: NonNegative::new(0.1).unwrap(),
                    confidence: Confidence::new(0.9).unwrap(),
                },
                PlanningAgent {
                    agent_id: AgentId::new("agent-b").unwrap(),
                    mobility_profile: profile.clone(),
                    capabilities: BTreeSet::from([CapabilityId::new("inspect").unwrap()]),
                    resource_capacities: BTreeMap::from([(
                        SharedResourceId::new("energy").unwrap(),
                        Positive::new(2.0).unwrap(),
                    )]),
                    maximum_assignments: 1,
                    assignment_cost: NonNegative::new(2.0).unwrap(),
                    assignment_risk: NonNegative::new(0.2).unwrap(),
                    confidence: Confidence::new(0.8).unwrap(),
                },
            ],
            groups: vec![PlanningAgentGroup {
                group_id: AgentGroupId::new("team").unwrap(),
                member_agent_ids: BTreeSet::from([
                    AgentId::new("agent-a").unwrap(),
                    AgentId::new("agent-b").unwrap(),
                ]),
                capabilities: BTreeSet::new(),
            }],
            tasks: vec![SpatialPlanningTask {
                task_id: SpatialTaskId::new("inspect-target").unwrap(),
                quantity: TaskQuantity {
                    minimum: 1,
                    desired: 1,
                },
                admission: TaskAdmission::Required,
                assignment_unit: AssignmentUnitKind::Agent,
                priority: Positive::new(10.0).unwrap(),
                target: MapGeometryReference::SourceFeature {
                    uri: MapSourceFeatureUri::parse(
                        "map://source-feature/release-immutable/source-feature-1",
                    )
                    .unwrap(),
                },
                execution: PlanExecutionReference::MapRoute {
                    uri: MapRouteUri::parse("map://route/route-1").unwrap(),
                },
                required_capabilities: BTreeSet::from([CapabilityId::new("inspect").unwrap()]),
                allowed_mobility_profiles: BTreeSet::from([profile]),
                eligible_agent_ids: BTreeSet::new(),
                eligible_group_ids: BTreeSet::new(),
                depends_on: BTreeSet::new(),
                shared_resource_demand: BTreeMap::from([(
                    SharedResourceId::new("energy").unwrap(),
                    NonNegative::new(1.0).unwrap(),
                )]),
                agent_resource_demand: BTreeMap::from([(
                    SharedResourceId::new("energy").unwrap(),
                    NonNegative::new(1.0).unwrap(),
                )]),
                allowed_lane_ids: BTreeSet::from([LaneId::new("lane-a").unwrap()]),
                allowed_resource_band_ids: BTreeSet::from([ResourceBandId::new("band-a").unwrap()]),
                timing: TaskTiming::Unscheduled,
                recurrence: PlanRecurrence::Once,
                assignment_cost: NonNegative::default(),
                assignment_risk: NonNegative::default(),
            }],
            shared_resources: vec![SharedResource {
                resource_id: SharedResourceId::new("energy").unwrap(),
                capacity: Positive::new(2.0).unwrap(),
            }],
            lanes: vec![PlanningLane {
                lane_id: LaneId::new("lane-a").unwrap(),
                capacity: 1,
                geometry: None,
            }],
            resource_bands: vec![PlanningResourceBand {
                resource_band_id: ResourceBandId::new("band-a").unwrap(),
                capacity: 1,
            }],
            mutual_exclusions: Vec::new(),
            objective: PlanningObjective::default(),
            solver: PlanningSolverPolicy {
                deterministic_seed: 7,
                ..Default::default()
            },
            artifacts: PlanArtifactOptions {
                duckdb: false,
                rerun_rrd: false,
            },
        }
    }

    #[test]
    fn compact_request_generates_and_selects_complete_assignments() {
        let run = run_plan(
            "019f0000-0000-7000-8000-000000000001",
            &request(),
            &authority(),
        )
        .unwrap();
        assert_eq!(run.plan.status, PlanStatus::Optimal);
        assert_eq!(run.plan.assignments.len(), 1);
        assert_eq!(
            run.plan.assignments[0].agent_ids,
            BTreeSet::from([AgentId::new("agent-a").unwrap()])
        );
        assert_eq!(
            run.plan.assignments[0].lane_id,
            Some(LaneId::new("lane-a").unwrap())
        );
        assert_eq!(run.plan.plan_digest_sha256.len(), 64);
        assert!(!run.plan_json.bytes.is_empty());
    }

    #[test]
    fn optional_analytical_artifacts_encode_the_same_governed_plan() {
        let mut request = request();
        request.artifacts = PlanArtifactOptions {
            duckdb: true,
            rerun_rrd: true,
        };
        let run = run_plan(
            "019f0000-0000-7000-8000-000000000006",
            &request,
            &authority(),
        )
        .unwrap();
        let decoded: GovernedPlan = serde_json::from_slice(&run.plan_json.bytes).unwrap();
        assert_eq!(decoded, run.plan);
        assert!(!run.duckdb.unwrap().bytes.is_empty());
        assert!(!run.rrd.unwrap().bytes.is_empty());
    }

    #[test]
    fn required_capacity_conflict_produces_typed_infeasible_plan() {
        let mut request = request();
        request.tasks[0].quantity = TaskQuantity {
            minimum: 2,
            desired: 2,
        };
        request.agents[0].maximum_assignments = 0;
        assert!(validate_request(&request).is_err());

        request.agents[0].maximum_assignments = 1;
        request.agents[1].maximum_assignments = 1;
        request.lanes[0].capacity = 1;
        let run = run_plan(
            "019f0000-0000-7000-8000-000000000002",
            &request,
            &authority(),
        )
        .unwrap();
        assert_eq!(run.plan.status, PlanStatus::Infeasible);
        assert!(
            run.plan
                .findings
                .iter()
                .any(|finding| finding.code == PlanFindingCode::SolverInfeasible)
        );
    }

    #[test]
    fn optional_quantity_can_return_a_partial_plan() {
        let mut request = request();
        request.tasks[0].admission = TaskAdmission::Optional;
        request.tasks[0].quantity = TaskQuantity {
            minimum: 0,
            desired: 2,
        };
        request.lanes[0].capacity = 1;
        let run = run_plan(
            "019f0000-0000-7000-8000-000000000003",
            &request,
            &authority(),
        )
        .unwrap();
        assert_eq!(run.plan.status, PlanStatus::Partial);
        assert_eq!(run.plan.assignments.len(), 1);
    }

    #[test]
    fn dependencies_and_mutual_exclusion_are_validated_before_solving() {
        let mut request = request();
        let first = request.tasks[0].clone();
        let mut second = first.clone();
        second.task_id = SpatialTaskId::new("other").unwrap();
        second.depends_on = BTreeSet::from([first.task_id.clone()]);
        second.execution = PlanExecutionReference::ArtifactTrajectory {
            uri: ArtifactTrajectoryUri::parse("artifact://019f0000-0000-7000-8000-000000000004")
                .unwrap(),
        };
        request.tasks.push(second);
        request.mutual_exclusions.push(TaskMutualExclusion {
            exclusion_id: MutualExclusionId::new("exclusive").unwrap(),
            task_ids: request
                .tasks
                .iter()
                .map(|task| task.task_id.clone())
                .collect(),
            maximum_active_tasks: 1,
        });
        let run = run_plan(
            "019f0000-0000-7000-8000-000000000005",
            &request,
            &authority(),
        )
        .unwrap();
        assert_eq!(run.plan.status, PlanStatus::Infeasible);
    }
}
