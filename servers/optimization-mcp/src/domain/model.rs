use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    ArtifactUri, CONVEX_PROBLEM_VERSION, ConstraintId, FiniteF64, MAX_INLINE_MODEL_NONZEROS,
    MILP_PROBLEM_VERSION, NonNegativeF64, OptimizationContractError, OptimizationProblemUri,
    OptimizationSolutionUri, SolverPolicyRef, VariableId, require_collection,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VariableBounds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower: Option<FiniteF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upper: Option<FiniteF64>,
}

impl VariableBounds {
    fn validate(&self, label: &str) -> Result<(), OptimizationContractError> {
        if let (Some(lower), Some(upper)) = (self.lower, self.upper)
            && lower.get() > upper.get()
        {
            return Err(OptimizationContractError::InvalidProblem(format!(
                "{label} lower bound must not exceed upper bound"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VariableKind {
    Continuous,
    Integer,
    SemiContinuous,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModelVariable {
    pub variable_id: VariableId,
    pub kind: VariableKind,
    pub bounds: VariableBounds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LinearTerm {
    pub variable_id: VariableId,
    pub coefficient: FiniteF64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuadraticTerm {
    pub left_variable_id: VariableId,
    pub right_variable_id: VariableId,
    pub coefficient: FiniteF64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModelObjective {
    pub direction: ObjectiveDirection,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linear_terms: Vec<LinearTerm>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quadratic_terms: Vec<QuadraticTerm>,
    #[serde(default)]
    pub offset: FiniteF64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveDirection {
    Minimize,
    Maximize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LinearConstraint {
    pub constraint_id: ConstraintId,
    pub terms: Vec<LinearTerm>,
    pub bounds: VariableBounds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuadraticConstraint {
    pub constraint_id: ConstraintId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linear_terms: Vec<LinearTerm>,
    pub quadratic_terms: Vec<QuadraticTerm>,
    pub bounds: VariableBounds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConvexProblemKind {
    LinearProgram,
    QuadraticProgram,
    QuadraticallyConstrainedProgram,
    SecondOrderConeProgram,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConvexProblem {
    pub version: String,
    pub kind: ConvexProblemKind,
    pub variables: Vec<ModelVariable>,
    pub objective: ModelObjective,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linear_constraints: Vec<LinearConstraint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quadratic_constraints: Vec<QuadraticConstraint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_primal_solution: Option<Vec<FiniteF64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_dual_solution: Option<Vec<FiniteF64>>,
}

impl ConvexProblem {
    pub fn validate(&self) -> Result<(), OptimizationContractError> {
        validate_model(
            &self.version,
            CONVEX_PROBLEM_VERSION,
            &self.variables,
            &self.objective,
            &self.linear_constraints,
            &self.quadratic_constraints,
        )?;
        if self
            .variables
            .iter()
            .any(|variable| variable.kind != VariableKind::Continuous)
        {
            return Err(OptimizationContractError::InvalidProblem(
                "convex models may only contain continuous variables".to_owned(),
            ));
        }
        match self.kind {
            ConvexProblemKind::LinearProgram => {
                if !self.objective.quadratic_terms.is_empty()
                    || !self.quadratic_constraints.is_empty()
                {
                    return Err(OptimizationContractError::InvalidProblem(
                        "linear programs cannot contain quadratic terms".to_owned(),
                    ));
                }
            }
            ConvexProblemKind::QuadraticProgram => {
                if self.objective.quadratic_terms.is_empty()
                    || !self.quadratic_constraints.is_empty()
                {
                    return Err(OptimizationContractError::InvalidProblem(
                        "quadratic programs require a quadratic objective and no quadratic constraints"
                            .to_owned(),
                    ));
                }
            }
            ConvexProblemKind::QuadraticallyConstrainedProgram
            | ConvexProblemKind::SecondOrderConeProgram => {
                if self.quadratic_constraints.is_empty() {
                    return Err(OptimizationContractError::InvalidProblem(
                        "quadratically constrained models require quadratic constraints".to_owned(),
                    ));
                }
            }
        }
        validate_initial_solution(
            "initial primal solution",
            self.initial_primal_solution.as_deref(),
            self.variables.len(),
        )?;
        if let Some(dual) = &self.initial_dual_solution
            && dual.len() != self.linear_constraints.len() + self.quadratic_constraints.len()
        {
            return Err(OptimizationContractError::InvalidProblem(
                "initial dual solution length must match the constraint count".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MilpProblem {
    pub version: String,
    pub variables: Vec<ModelVariable>,
    pub objective: ModelObjective,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<LinearConstraint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mip_start: Option<Vec<FiniteF64>>,
}

impl MilpProblem {
    pub fn validate(&self) -> Result<(), OptimizationContractError> {
        validate_model(
            &self.version,
            MILP_PROBLEM_VERSION,
            &self.variables,
            &self.objective,
            &self.constraints,
            &[],
        )?;
        if !self.objective.quadratic_terms.is_empty() {
            return Err(OptimizationContractError::InvalidProblem(
                "MILP objectives must be linear".to_owned(),
            ));
        }
        if self
            .variables
            .iter()
            .all(|variable| variable.kind == VariableKind::Continuous)
        {
            return Err(OptimizationContractError::InvalidProblem(
                "MILP requires at least one integer or semi-continuous variable".to_owned(),
            ));
        }
        validate_initial_solution("MIP start", self.mip_start.as_deref(), self.variables.len())?;
        Ok(())
    }
}

fn validate_model(
    actual_version: &str,
    required_version: &str,
    variables: &[ModelVariable],
    objective: &ModelObjective,
    linear_constraints: &[LinearConstraint],
    quadratic_constraints: &[QuadraticConstraint],
) -> Result<(), OptimizationContractError> {
    if actual_version != required_version {
        return Err(OptimizationContractError::InvalidProblem(format!(
            "problem version must be {required_version}"
        )));
    }
    require_collection("variables", variables.len(), 1, i32::MAX as usize)?;
    let variable_ids: BTreeSet<_> = variables
        .iter()
        .map(|variable| &variable.variable_id)
        .collect();
    if variable_ids.len() != variables.len() {
        return Err(OptimizationContractError::InvalidProblem(
            "variable ids must be unique".to_owned(),
        ));
    }
    for variable in variables {
        variable
            .bounds
            .validate(&format!("variable {}", variable.variable_id))?;
    }
    let constraint_ids: BTreeSet<_> = linear_constraints
        .iter()
        .map(|constraint| &constraint.constraint_id)
        .chain(
            quadratic_constraints
                .iter()
                .map(|constraint| &constraint.constraint_id),
        )
        .collect();
    if constraint_ids.len() != linear_constraints.len() + quadratic_constraints.len() {
        return Err(OptimizationContractError::InvalidProblem(
            "constraint ids must be unique".to_owned(),
        ));
    }

    let nonzeros = objective.linear_terms.len()
        + objective.quadratic_terms.len()
        + linear_constraints
            .iter()
            .map(|constraint| constraint.terms.len())
            .sum::<usize>()
        + quadratic_constraints
            .iter()
            .map(|constraint| constraint.linear_terms.len() + constraint.quadratic_terms.len())
            .sum::<usize>();
    if nonzeros > MAX_INLINE_MODEL_NONZEROS {
        return Err(OptimizationContractError::InvalidProblem(format!(
            "inline model has {nonzeros} nonzeros and exceeds {MAX_INLINE_MODEL_NONZEROS}; use an artifact model"
        )));
    }
    validate_terms(
        objective
            .linear_terms
            .iter()
            .map(|term| &term.variable_id)
            .chain(
                objective
                    .quadratic_terms
                    .iter()
                    .flat_map(|term| [&term.left_variable_id, &term.right_variable_id]),
            )
            .chain(
                linear_constraints
                    .iter()
                    .flat_map(|constraint| constraint.terms.iter().map(|term| &term.variable_id)),
            )
            .chain(quadratic_constraints.iter().flat_map(|constraint| {
                constraint
                    .linear_terms
                    .iter()
                    .map(|term| &term.variable_id)
                    .chain(
                        constraint
                            .quadratic_terms
                            .iter()
                            .flat_map(|term| [&term.left_variable_id, &term.right_variable_id]),
                    )
            })),
        &variable_ids,
    )?;
    for constraint in linear_constraints {
        constraint
            .bounds
            .validate(&format!("constraint {}", constraint.constraint_id))?;
        if constraint.terms.is_empty() {
            return Err(OptimizationContractError::InvalidProblem(format!(
                "constraint {} has no terms",
                constraint.constraint_id
            )));
        }
    }
    for constraint in quadratic_constraints {
        constraint
            .bounds
            .validate(&format!("constraint {}", constraint.constraint_id))?;
        if constraint.quadratic_terms.is_empty() {
            return Err(OptimizationContractError::InvalidProblem(format!(
                "quadratic constraint {} has no quadratic terms",
                constraint.constraint_id
            )));
        }
    }
    Ok(())
}

fn validate_terms<'a>(
    terms: impl Iterator<Item = &'a VariableId>,
    variables: &BTreeSet<&VariableId>,
) -> Result<(), OptimizationContractError> {
    if let Some(unknown) = terms
        .filter(|variable| !variables.contains(variable))
        .next()
    {
        return Err(OptimizationContractError::InvalidProblem(format!(
            "model term references unknown variable {unknown}"
        )));
    }
    Ok(())
}

fn validate_initial_solution(
    label: &str,
    solution: Option<&[FiniteF64]>,
    variables: usize,
) -> Result<(), OptimizationContractError> {
    if let Some(solution) = solution
        && solution.len() != variables
    {
        return Err(OptimizationContractError::InvalidProblem(format!(
            "{label} length must match the variable count"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactModelFormat {
    Mps,
    Lp,
    OptimizationPackageV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactModelSource {
    pub uri: ArtifactUri,
    pub format: ArtifactModelFormat,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ConvexProblemSource {
    Inline { problem: ConvexProblem },
    Resource { uri: OptimizationProblemUri },
    Artifact { model: ArtifactModelSource },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum MilpProblemSource {
    Inline { problem: MilpProblem },
    Resource { uri: OptimizationProblemUri },
    Artifact { model: ArtifactModelSource },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct MathematicalOutputPolicy {
    #[serde(default)]
    pub retain_solver_log: bool,
    #[serde(default)]
    pub retain_warm_start: bool,
    #[serde(default)]
    pub retain_incumbents: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SolveConvexRequest {
    pub problem: ConvexProblemSource,
    pub policy: SolverPolicyRef,
    #[serde(default)]
    pub output: MathematicalOutputPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SolveMilpRequest {
    pub problem: MilpProblemSource,
    pub policy: SolverPolicyRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_solution: Option<OptimizationSolutionUri>,
    #[serde(default)]
    pub output: MathematicalOutputPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VerifySolutionRequest {
    pub solution_uri: OptimizationSolutionUri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub absolute_tolerance: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_tolerance: Option<NonNegativeF64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variable(id: &str, kind: VariableKind) -> ModelVariable {
        ModelVariable {
            variable_id: VariableId::new(id).unwrap(),
            kind,
            bounds: VariableBounds {
                lower: Some(FiniteF64::new(0.0).unwrap()),
                upper: None,
            },
        }
    }

    #[test]
    fn linear_program_rejects_integer_variables() {
        let problem = ConvexProblem {
            version: CONVEX_PROBLEM_VERSION.to_owned(),
            kind: ConvexProblemKind::LinearProgram,
            variables: vec![variable("x", VariableKind::Integer)],
            objective: ModelObjective {
                direction: ObjectiveDirection::Minimize,
                linear_terms: vec![],
                quadratic_terms: vec![],
                offset: FiniteF64::default(),
            },
            linear_constraints: vec![],
            quadratic_constraints: vec![],
            initial_primal_solution: None,
            initial_dual_solution: None,
        };
        assert!(problem.validate().is_err());
    }

    #[test]
    fn milp_requires_an_integer_variable() {
        let problem = MilpProblem {
            version: MILP_PROBLEM_VERSION.to_owned(),
            variables: vec![variable("x", VariableKind::Continuous)],
            objective: ModelObjective {
                direction: ObjectiveDirection::Minimize,
                linear_terms: vec![],
                quadratic_terms: vec![],
                offset: FiniteF64::default(),
            },
            constraints: vec![],
            mip_start: None,
        };
        assert!(problem.validate().is_err());
    }
}
