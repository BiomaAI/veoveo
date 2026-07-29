use std::collections::{BTreeMap, BTreeSet};

use crate::domain::{
    ConstraintValue, ConvexProblem, FiniteF64, LinearConstraint, LinearTerm, MilpProblem,
    ModelObjective, ModelVariable, NonNegativeF64, ObjectiveDirection, QuadraticConstraint,
    QuadraticTerm, VariableId, VariableKind, VariableValue, VerificationCode, VerificationFinding,
    VerificationReport, VerificationSeverity,
};

use super::{VerificationTolerance, non_negative, report};

#[derive(Debug, Clone, PartialEq)]
pub struct CandidateVerification {
    pub report: VerificationReport,
    pub objective: Option<FiniteF64>,
    pub variables: Vec<VariableValue>,
    pub constraints: Vec<ConstraintValue>,
    pub maximum_constraint_violation: NonNegativeF64,
    pub maximum_integrality_violation: NonNegativeF64,
    pub maximum_bound_violation: NonNegativeF64,
}

pub fn verify_convex_candidate(
    problem: &ConvexProblem,
    candidate: &[VariableValue],
    reported_objective: Option<FiniteF64>,
    tolerance: VerificationTolerance,
) -> CandidateVerification {
    verify_candidate(
        &problem.variables,
        &problem.objective,
        &problem.linear_constraints,
        &problem.quadratic_constraints,
        candidate,
        reported_objective,
        tolerance,
        false,
    )
}

pub fn verify_milp_candidate(
    problem: &MilpProblem,
    candidate: &[VariableValue],
    reported_objective: Option<FiniteF64>,
    tolerance: VerificationTolerance,
) -> CandidateVerification {
    verify_candidate(
        &problem.variables,
        &problem.objective,
        &problem.constraints,
        &[],
        candidate,
        reported_objective,
        tolerance,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn verify_candidate(
    variables: &[ModelVariable],
    objective: &ModelObjective,
    linear_constraints: &[LinearConstraint],
    quadratic_constraints: &[QuadraticConstraint],
    candidate: &[VariableValue],
    reported_objective: Option<FiniteF64>,
    tolerance: VerificationTolerance,
    verify_integrality: bool,
) -> CandidateVerification {
    let expected = variables
        .iter()
        .map(|variable| variable.variable_id.clone())
        .collect::<BTreeSet<_>>();
    let mut values = BTreeMap::new();
    let mut findings = Vec::new();
    for entry in candidate {
        if !expected.contains(&entry.variable_id) {
            findings.push(variable_finding(
                VerificationCode::UnknownVariable,
                format!("candidate contains unknown variable {}", entry.variable_id),
                entry.variable_id.clone(),
            ));
        } else if values
            .insert(entry.variable_id.clone(), entry.value.get())
            .is_some()
        {
            findings.push(variable_finding(
                VerificationCode::DuplicateVariable,
                format!(
                    "candidate contains variable {} more than once",
                    entry.variable_id
                ),
                entry.variable_id.clone(),
            ));
        }
    }
    for variable in variables {
        if !values.contains_key(&variable.variable_id) {
            findings.push(variable_finding(
                VerificationCode::MissingVariable,
                format!("candidate omits variable {}", variable.variable_id),
                variable.variable_id.clone(),
            ));
        }
    }

    let mut maximum_bound_violation: f64 = 0.0;
    let mut maximum_integrality_violation: f64 = 0.0;
    for variable in variables {
        let Some(value) = values.get(&variable.variable_id).copied() else {
            continue;
        };
        if let Some(lower) = variable.bounds.lower
            && !(variable.kind == VariableKind::SemiContinuous && value == 0.0)
        {
            let violation = (lower.get() - value).max(0.0);
            maximum_bound_violation = maximum_bound_violation.max(violation);
            if violation > tolerance.allowed(lower.get()) {
                findings.push(variable_finding(
                    VerificationCode::VariableLowerBound,
                    format!(
                        "variable {} value {value} is below lower bound {}",
                        variable.variable_id,
                        lower.get()
                    ),
                    variable.variable_id.clone(),
                ));
            }
        }
        if let Some(upper) = variable.bounds.upper {
            let violation = (value - upper.get()).max(0.0);
            maximum_bound_violation = maximum_bound_violation.max(violation);
            if violation > tolerance.allowed(upper.get()) {
                findings.push(variable_finding(
                    VerificationCode::VariableUpperBound,
                    format!(
                        "variable {} value {value} exceeds upper bound {}",
                        variable.variable_id,
                        upper.get()
                    ),
                    variable.variable_id.clone(),
                ));
            }
        }
        if verify_integrality && variable.kind == VariableKind::Integer {
            let violation = (value - value.round()).abs();
            maximum_integrality_violation = maximum_integrality_violation.max(violation);
            if violation > tolerance.allowed(value) {
                findings.push(variable_finding(
                    VerificationCode::VariableIntegrality,
                    format!(
                        "integer variable {} has non-integral value {value}",
                        variable.variable_id
                    ),
                    variable.variable_id.clone(),
                ));
            }
        }
        if verify_integrality
            && variable.kind == VariableKind::SemiContinuous
            && value != 0.0
            && variable
                .bounds
                .lower
                .is_some_and(|lower| value < lower.get() - tolerance.allowed(lower.get()))
        {
            let lower = variable
                .bounds
                .lower
                .expect("checked semi-continuous bound");
            let violation = lower.get() - value;
            maximum_integrality_violation = maximum_integrality_violation.max(violation);
            findings.push(variable_finding(
                VerificationCode::VariableIntegrality,
                format!(
                    "semi-continuous variable {} must be zero or at least {}",
                    variable.variable_id,
                    lower.get()
                ),
                variable.variable_id.clone(),
            ));
        }
    }

    let mut maximum_constraint_violation: f64 = 0.0;
    let mut constraint_values = Vec::new();
    for constraint in linear_constraints {
        if let Some(activity) = linear_activity(&constraint.terms, &values) {
            maximum_constraint_violation = check_constraint_bounds(
                constraint.constraint_id.clone(),
                activity,
                constraint.bounds.lower.map(|value| value.get()),
                constraint.bounds.upper.map(|value| value.get()),
                tolerance,
                &mut findings,
                maximum_constraint_violation,
            );
            constraint_values.push(ConstraintValue {
                constraint_id: constraint.constraint_id.clone(),
                activity: FiniteF64::new(activity).expect("finite validated activity"),
                dual_value: None,
            });
        }
    }
    for constraint in quadratic_constraints {
        if let Some(activity) = quadratic_activity(
            &constraint.linear_terms,
            &constraint.quadratic_terms,
            &values,
        ) {
            maximum_constraint_violation = check_constraint_bounds(
                constraint.constraint_id.clone(),
                activity,
                constraint.bounds.lower.map(|value| value.get()),
                constraint.bounds.upper.map(|value| value.get()),
                tolerance,
                &mut findings,
                maximum_constraint_violation,
            );
            constraint_values.push(ConstraintValue {
                constraint_id: constraint.constraint_id.clone(),
                activity: FiniteF64::new(activity).expect("finite validated activity"),
                dual_value: None,
            });
        }
    }

    let objective_value = objective_activity(objective, &values);
    if let (Some(calculated), Some(reported)) = (objective_value, reported_objective) {
        let violation = (calculated - reported.get()).abs();
        if violation > tolerance.allowed(calculated) {
            findings.push(VerificationFinding {
                code: VerificationCode::ObjectiveMismatch,
                severity: VerificationSeverity::Error,
                message: format!(
                    "reported objective {} differs from independently calculated objective {calculated}",
                    reported.get()
                ),
                variable_id: None,
                constraint_id: None,
                order_id: None,
                vehicle_id: None,
            });
        }
    }

    let objective_value = objective_value.and_then(|value| FiniteF64::new(value).ok());
    let normalized_variables = variables
        .iter()
        .filter_map(|variable| {
            values
                .get(&variable.variable_id)
                .and_then(|value| FiniteF64::new(*value).ok())
                .map(|value| VariableValue {
                    variable_id: variable.variable_id.clone(),
                    value,
                })
        })
        .collect();
    let report = report(
        findings,
        tolerance,
        Some(maximum_constraint_violation),
        verify_integrality.then_some(maximum_integrality_violation),
        Some(maximum_bound_violation),
    );
    CandidateVerification {
        report,
        objective: objective_value,
        variables: normalized_variables,
        constraints: constraint_values,
        maximum_constraint_violation: non_negative(maximum_constraint_violation),
        maximum_integrality_violation: non_negative(maximum_integrality_violation),
        maximum_bound_violation: non_negative(maximum_bound_violation),
    }
}

fn linear_activity(terms: &[LinearTerm], values: &BTreeMap<VariableId, f64>) -> Option<f64> {
    terms.iter().try_fold(0.0, |activity, term| {
        values
            .get(&term.variable_id)
            .map(|value| activity + term.coefficient.get() * value)
    })
}

fn quadratic_activity(
    linear_terms: &[LinearTerm],
    quadratic_terms: &[QuadraticTerm],
    values: &BTreeMap<VariableId, f64>,
) -> Option<f64> {
    let linear = linear_activity(linear_terms, values)?;
    quadratic_terms.iter().try_fold(linear, |activity, term| {
        let left = values.get(&term.left_variable_id)?;
        let right = values.get(&term.right_variable_id)?;
        Some(activity + term.coefficient.get() * left * right)
    })
}

fn objective_activity(
    objective: &ModelObjective,
    values: &BTreeMap<VariableId, f64>,
) -> Option<f64> {
    let raw = quadratic_activity(&objective.linear_terms, &objective.quadratic_terms, values)?
        + objective.offset.get();
    Some(match objective.direction {
        ObjectiveDirection::Minimize | ObjectiveDirection::Maximize => raw,
    })
}

#[allow(clippy::too_many_arguments)]
fn check_constraint_bounds(
    constraint_id: crate::domain::ConstraintId,
    activity: f64,
    lower: Option<f64>,
    upper: Option<f64>,
    tolerance: VerificationTolerance,
    findings: &mut Vec<VerificationFinding>,
    current_maximum: f64,
) -> f64 {
    let mut maximum = current_maximum;
    if let Some(lower) = lower {
        let violation = (lower - activity).max(0.0);
        maximum = maximum.max(violation);
        if violation > tolerance.allowed(lower) {
            findings.push(constraint_finding(
                VerificationCode::ConstraintLowerBound,
                format!(
                    "constraint {constraint_id} activity {activity} is below lower bound {lower}"
                ),
                constraint_id.clone(),
            ));
        }
    }
    if let Some(upper) = upper {
        let violation = (activity - upper).max(0.0);
        maximum = maximum.max(violation);
        if violation > tolerance.allowed(upper) {
            findings.push(constraint_finding(
                VerificationCode::ConstraintUpperBound,
                format!(
                    "constraint {constraint_id} activity {activity} exceeds upper bound {upper}"
                ),
                constraint_id,
            ));
        }
    }
    maximum
}

fn variable_finding(
    code: VerificationCode,
    message: String,
    variable_id: VariableId,
) -> VerificationFinding {
    VerificationFinding {
        code,
        severity: VerificationSeverity::Error,
        message,
        variable_id: Some(variable_id),
        constraint_id: None,
        order_id: None,
        vehicle_id: None,
    }
}

fn constraint_finding(
    code: VerificationCode,
    message: String,
    constraint_id: crate::domain::ConstraintId,
) -> VerificationFinding {
    VerificationFinding {
        code,
        severity: VerificationSeverity::Error,
        message,
        variable_id: None,
        constraint_id: Some(constraint_id),
        order_id: None,
        vehicle_id: None,
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        CONVEX_PROBLEM_VERSION, ConstraintId, ConvexProblemKind, ModelVariable, VariableBounds,
    };

    use super::*;

    #[test]
    fn verifier_rejects_constraint_and_bound_violations() {
        let x = VariableId::new("x").unwrap();
        let problem = ConvexProblem {
            version: CONVEX_PROBLEM_VERSION.to_owned(),
            kind: ConvexProblemKind::LinearProgram,
            variables: vec![ModelVariable {
                variable_id: x.clone(),
                kind: VariableKind::Continuous,
                bounds: VariableBounds {
                    lower: Some(FiniteF64::new(0.0).unwrap()),
                    upper: Some(FiniteF64::new(5.0).unwrap()),
                },
            }],
            objective: ModelObjective {
                direction: ObjectiveDirection::Minimize,
                linear_terms: vec![LinearTerm {
                    variable_id: x.clone(),
                    coefficient: FiniteF64::new(1.0).unwrap(),
                }],
                quadratic_terms: vec![],
                offset: FiniteF64::default(),
            },
            linear_constraints: vec![LinearConstraint {
                constraint_id: ConstraintId::new("minimum").unwrap(),
                terms: vec![LinearTerm {
                    variable_id: x.clone(),
                    coefficient: FiniteF64::new(1.0).unwrap(),
                }],
                bounds: VariableBounds {
                    lower: Some(FiniteF64::new(2.0).unwrap()),
                    upper: None,
                },
            }],
            quadratic_constraints: vec![],
            initial_primal_solution: None,
            initial_dual_solution: None,
        };
        let candidate = vec![VariableValue {
            variable_id: x,
            value: FiniteF64::new(-1.0).unwrap(),
        }];

        let verified =
            verify_convex_candidate(&problem, &candidate, None, VerificationTolerance::default());
        assert!(!verified.report.verified);
        assert_eq!(verified.report.findings.len(), 2);
        assert_eq!(verified.maximum_bound_violation.get(), 1.0);
        assert_eq!(verified.maximum_constraint_violation.get(), 3.0);
    }
}
