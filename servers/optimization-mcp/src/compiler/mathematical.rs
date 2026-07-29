use std::collections::BTreeMap;

use crate::{
    domain::{
        ConvexProblem, FiniteF64, LinearConstraint, LinearTerm, MilpProblem, ModelObjective,
        ModelVariable, QuadraticConstraint, QuadraticTerm, VariableId,
    },
    executor::{
        CompiledMathematicalModel, CompiledQuadraticConstraint, CsrMatrix, QuadraticConstraintSense,
    },
};

use super::CompileError;

pub fn compile_convex_problem(
    problem: &ConvexProblem,
) -> Result<CompiledMathematicalModel, CompileError> {
    problem
        .validate()
        .map_err(|error| CompileError::InvalidProblem(error.to_string()))?;
    compile_model(
        &problem.variables,
        &problem.objective,
        &problem.linear_constraints,
        &problem.quadratic_constraints,
        problem.initial_primal_solution.clone(),
        problem.initial_dual_solution.clone(),
    )
}

pub fn compile_milp_problem(
    problem: &MilpProblem,
) -> Result<CompiledMathematicalModel, CompileError> {
    problem
        .validate()
        .map_err(|error| CompileError::InvalidProblem(error.to_string()))?;
    compile_model(
        &problem.variables,
        &problem.objective,
        &problem.constraints,
        &[],
        problem.mip_start.clone(),
        None,
    )
}

fn compile_model(
    variables: &[ModelVariable],
    objective: &ModelObjective,
    linear_constraints: &[LinearConstraint],
    quadratic_constraints: &[QuadraticConstraint],
    initial_primal_solution: Option<Vec<FiniteF64>>,
    initial_dual_solution: Option<Vec<FiniteF64>>,
) -> Result<CompiledMathematicalModel, CompileError> {
    let variable_indices = variables
        .iter()
        .enumerate()
        .map(|(index, variable)| (variable.variable_id.clone(), index as u32))
        .collect::<BTreeMap<_, _>>();
    let constraint_matrix =
        compile_linear_constraints(linear_constraints, variables.len(), &variable_indices);
    let objective_coefficients =
        dense_linear_terms(&objective.linear_terms, variables.len(), &variable_indices);

    let mut constraint_ids = linear_constraints
        .iter()
        .map(|constraint| constraint.constraint_id.clone())
        .collect::<Vec<_>>();
    let mut compiled_quadratic = Vec::new();
    for constraint in quadratic_constraints {
        let mut sides = compile_quadratic_constraint(constraint, &variable_indices)?;
        constraint_ids.extend(sides.iter().map(|compiled| compiled.constraint_id.clone()));
        compiled_quadratic.append(&mut sides);
    }

    Ok(CompiledMathematicalModel {
        variable_ids: variables
            .iter()
            .map(|variable| variable.variable_id.clone())
            .collect(),
        variable_kinds: variables.iter().map(|variable| variable.kind).collect(),
        variable_lower_bounds: variables
            .iter()
            .map(|variable| variable.bounds.lower)
            .collect(),
        variable_upper_bounds: variables
            .iter()
            .map(|variable| variable.bounds.upper)
            .collect(),
        objective_direction: objective.direction,
        objective_offset: objective.offset,
        objective_coefficients,
        constraint_ids,
        constraint_matrix,
        constraint_lower_bounds: linear_constraints
            .iter()
            .map(|constraint| constraint.bounds.lower)
            .collect(),
        constraint_upper_bounds: linear_constraints
            .iter()
            .map(|constraint| constraint.bounds.upper)
            .collect(),
        quadratic_objective: (!objective.quadratic_terms.is_empty())
            .then(|| {
                compile_quadratic_csr(
                    &objective.quadratic_terms,
                    variables.len(),
                    &variable_indices,
                )
            })
            .transpose()?,
        quadratic_constraints: compiled_quadratic,
        initial_primal_solution,
        initial_dual_solution,
    })
}

fn compile_linear_constraints(
    constraints: &[LinearConstraint],
    columns: usize,
    variable_indices: &BTreeMap<VariableId, u32>,
) -> CsrMatrix {
    let mut offsets = Vec::with_capacity(constraints.len() + 1);
    let mut indices = Vec::new();
    let mut values = Vec::new();
    offsets.push(0);
    for constraint in constraints {
        let row = merged_linear_terms(&constraint.terms, variable_indices);
        for (index, value) in row {
            indices.push(index);
            values.push(FiniteF64::new(value).expect("validated finite coefficient"));
        }
        offsets.push(indices.len() as u32);
    }
    CsrMatrix {
        rows: constraints.len() as u32,
        columns: columns as u32,
        offsets,
        indices,
        values,
    }
}

fn dense_linear_terms(
    terms: &[LinearTerm],
    columns: usize,
    variable_indices: &BTreeMap<VariableId, u32>,
) -> Vec<FiniteF64> {
    let mut dense = vec![0.0; columns];
    for term in terms {
        dense[variable_indices[&term.variable_id] as usize] += term.coefficient.get();
    }
    dense
        .into_iter()
        .map(|value| FiniteF64::new(value).expect("sum of validated coefficients is finite"))
        .collect()
}

fn merged_linear_terms(
    terms: &[LinearTerm],
    variable_indices: &BTreeMap<VariableId, u32>,
) -> BTreeMap<u32, f64> {
    let mut merged = BTreeMap::new();
    for term in terms {
        *merged
            .entry(variable_indices[&term.variable_id])
            .or_insert(0.0) += term.coefficient.get();
    }
    merged.retain(|_, coefficient| *coefficient != 0.0);
    merged
}

fn compile_quadratic_csr(
    terms: &[QuadraticTerm],
    columns: usize,
    variable_indices: &BTreeMap<VariableId, u32>,
) -> Result<CsrMatrix, CompileError> {
    let mut rows = vec![BTreeMap::<u32, f64>::new(); columns];
    for term in terms {
        let row = variable_indices[&term.left_variable_id] as usize;
        let column = variable_indices[&term.right_variable_id];
        *rows[row].entry(column).or_insert(0.0) += term.coefficient.get();
    }
    let mut offsets = Vec::with_capacity(columns + 1);
    let mut indices = Vec::new();
    let mut values = Vec::new();
    offsets.push(0);
    for row in rows {
        for (column, coefficient) in row {
            if coefficient != 0.0 {
                indices.push(column);
                values.push(finite_sum(coefficient, "quadratic coefficient sum")?);
            }
        }
        offsets.push(indices.len() as u32);
    }
    Ok(CsrMatrix {
        rows: columns as u32,
        columns: columns as u32,
        offsets,
        indices,
        values,
    })
}

fn compile_quadratic_constraint(
    constraint: &QuadraticConstraint,
    variable_indices: &BTreeMap<VariableId, u32>,
) -> Result<Vec<CompiledQuadraticConstraint>, CompileError> {
    let linear = merged_linear_terms(&constraint.linear_terms, variable_indices);
    let mut quadratic = BTreeMap::<(u32, u32), f64>::new();
    for term in &constraint.quadratic_terms {
        *quadratic
            .entry((
                variable_indices[&term.left_variable_id],
                variable_indices[&term.right_variable_id],
            ))
            .or_insert(0.0) += term.coefficient.get();
    }
    let compile_side = |sense, rhs| {
        Ok(CompiledQuadraticConstraint {
            constraint_id: constraint.constraint_id.clone(),
            linear_indices: linear.keys().copied().collect(),
            linear_values: linear
                .values()
                .map(|value| finite_sum(*value, "quadratic constraint linear coefficient sum"))
                .collect::<Result<_, CompileError>>()?,
            rows: quadratic.keys().map(|(row, _)| *row).collect(),
            columns: quadratic.keys().map(|(_, column)| *column).collect(),
            values: quadratic
                .values()
                .map(|value| finite_sum(*value, "quadratic constraint coefficient sum"))
                .collect::<Result<_, CompileError>>()?,
            sense,
            rhs,
        })
    };

    match (constraint.bounds.lower, constraint.bounds.upper) {
        (Some(lower), Some(upper)) if lower == upper => Err(CompileError::InvalidProblem(format!(
            "quadratic equality constraint {} is not supported by cuOpt 26.06",
            constraint.constraint_id
        ))),
        (Some(lower), Some(upper)) => Ok(vec![
            compile_side(QuadraticConstraintSense::GreaterThanOrEqual, lower)?,
            compile_side(QuadraticConstraintSense::LessThanOrEqual, upper)?,
        ]),
        (Some(lower), None) => Ok(vec![compile_side(
            QuadraticConstraintSense::GreaterThanOrEqual,
            lower,
        )?]),
        (None, Some(upper)) => Ok(vec![compile_side(
            QuadraticConstraintSense::LessThanOrEqual,
            upper,
        )?]),
        (None, None) => Err(CompileError::InvalidProblem(format!(
            "quadratic constraint {} must have at least one bound",
            constraint.constraint_id
        ))),
    }
}

fn finite_sum(value: f64, field: &'static str) -> Result<FiniteF64, CompileError> {
    FiniteF64::new(value).map_err(|_| CompileError::NumericRange {
        field,
        value: value.to_string(),
        target: "finite float64",
    })
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        CONVEX_PROBLEM_VERSION, ConstraintId, ConvexProblemKind, ModelVariable, ObjectiveDirection,
        VariableBounds, VariableKind,
    };

    use super::*;

    #[test]
    fn compiler_merges_duplicate_terms_into_stable_csr() {
        let x = VariableId::new("x").unwrap();
        let problem = ConvexProblem {
            version: CONVEX_PROBLEM_VERSION.to_owned(),
            kind: ConvexProblemKind::LinearProgram,
            variables: vec![ModelVariable {
                variable_id: x.clone(),
                kind: VariableKind::Continuous,
                bounds: VariableBounds {
                    lower: None,
                    upper: None,
                },
            }],
            objective: ModelObjective {
                direction: ObjectiveDirection::Minimize,
                linear_terms: vec![],
                quadratic_terms: vec![],
                offset: FiniteF64::default(),
            },
            linear_constraints: vec![LinearConstraint {
                constraint_id: ConstraintId::new("row").unwrap(),
                terms: vec![
                    LinearTerm {
                        variable_id: x.clone(),
                        coefficient: FiniteF64::new(2.0).unwrap(),
                    },
                    LinearTerm {
                        variable_id: x,
                        coefficient: FiniteF64::new(3.0).unwrap(),
                    },
                ],
                bounds: VariableBounds {
                    lower: None,
                    upper: Some(FiniteF64::new(10.0).unwrap()),
                },
            }],
            quadratic_constraints: vec![],
            initial_primal_solution: None,
            initial_dual_solution: None,
        };

        let compiled = compile_convex_problem(&problem).unwrap();
        assert_eq!(compiled.constraint_matrix.offsets, vec![0, 1]);
        assert_eq!(compiled.constraint_matrix.values[0].get(), 5.0);
    }
}
