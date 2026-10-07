import math
import time
from typing import Any

import numpy as np

from .protocol import finite_or_none


class _IncumbentRecorder:
    def __init__(self, callback_base: type[Any]) -> None:
        class Recorder(callback_base):
            def __init__(inner_self) -> None:
                super().__init__()
                inner_self.started = time.monotonic()
                inner_self.items: list[dict[str, Any]] = []

            def get_solution(
                inner_self,
                solution: Any,
                solution_cost: Any,
                solution_bound: Any,
                user_data: Any,
            ) -> None:
                del user_data
                inner_self.items.append(
                    {
                        "sequence": len(inner_self.items) + 1,
                        "values": [
                            float(value) for value in solution.tolist()
                        ],
                        "objective": float(solution_cost[0]),
                        "bound": float(solution_bound[0]),
                        'foundAtSeconds': max(
                            0.0, time.monotonic() - inner_self.started
                        ),
                    }
                )

        self.instance = Recorder()


def solve_model(
    family: str,
    model_data: dict[str, Any],
    profile: dict[str, Any],
) -> dict[str, Any]:
    from cuopt import linear_programming

    needs_auxiliary = not model_data['constraintMatrix']["values"]
    native_data = _with_auxiliary_row(model_data) if needs_auxiliary else model_data
    model = _build_model(native_data, linear_programming)
    settings, recorder = _settings(
        family,
        profile,
        linear_programming,
        requires_barrier=_requires_barrier(model_data),
    )
    started = time.monotonic()
    solution = linear_programming.Solve(
        model, solver_settings=settings
    )
    elapsed = time.monotonic() - started
    result = _solution(
        family,
        solution,
        elapsed,
        recorder.instance.items if recorder is not None else [],
    )
    if needs_auxiliary:
        # Solver-only dimensions never enter the public solution or warm starts.
        result['primalSolution'] = result['primalSolution'][:-1]
        for incumbent in result["incumbents"]:
            incumbent["values"] = incumbent["values"][:-1]
        row = model_data['constraintMatrix']["rows"]
        dual = result['dualSolution']
        result['dualSolution'] = dual[:row] + dual[row + 1:]
    return result


def _with_auxiliary_row(data: dict[str, Any]) -> dict[str, Any]:
    """Represent an empty CSR using an independent continuous variable fixed at zero.

    cuOpt 26.08's Python wrapper omits empty CSR input. Its MIP heuristic also
    faults on an all-zero coefficient matrix. The equation z=0 supplies one
    nonzero while preserving the original feasible set and objective exactly.
    Retire this adapter when a pinned cuOpt release qualifies empty CSR directly.
    """
    native = dict(data)
    variable_ids = data['variableIds']
    auxiliary_name = "__veoveo_auxiliary"
    while auxiliary_name in variable_ids:
        auxiliary_name += "_"
    for key, value in [
        ('variableIds', auxiliary_name),
        ('variableKinds', "continuous"),
        ('variableLowerBounds', 0.0),
        ('variableUpperBounds', 0.0),
        ('objectiveCoefficients', 0.0),
        ('constraintLowerBounds', 0.0),
        ('constraintUpperBounds', 0.0),
    ]:
        native[key] = [*data[key], value]
    matrix = data['constraintMatrix']
    native['constraintMatrix'] = {
        "rows": matrix["rows"] + 1,
        "columns": matrix["columns"] + 1,
        "offsets": [*matrix["offsets"], 1],
        "indices": [matrix["columns"]],
        "values": [1.0],
    }
    if data.get('quadraticObjective') is not None:
        quadratic = data['quadraticObjective']
        native['quadraticObjective'] = {
            **quadratic,
            "rows": quadratic["rows"] + 1,
            "columns": quadratic["columns"] + 1,
            "offsets": [*quadratic["offsets"], quadratic["offsets"][-1]],
        }
    if data.get('initialPrimalSolution') is not None:
        native['initialPrimalSolution'] = [*data['initialPrimalSolution'], 0.0]
    if data.get('initialDualSolution') is not None:
        row = matrix["rows"]
        dual = data['initialDualSolution']
        native['initialDualSolution'] = [*dual[:row], 0.0, *dual[row:]]
    return native


def solve_model_file(
    family: str, path: str, profile: dict[str, Any]
) -> dict[str, Any]:
    from cuopt import linear_programming

    model = linear_programming.Read(path)
    settings, recorder = _settings(
        family,
        profile,
        linear_programming,
        requires_barrier=family == "convex",
    )
    started = time.monotonic()
    solution = linear_programming.Solve(
        model, solver_settings=settings
    )
    elapsed = time.monotonic() - started
    return _solution(
        family,
        solution,
        elapsed,
        recorder.instance.items if recorder is not None else [],
    )


def _build_model(
    data: dict[str, Any], linear_programming: Any
) -> Any:
    model = linear_programming.DataModel()
    constraint_matrix = data['constraintMatrix']
    model.set_csr_constraint_matrix(
        np.asarray(constraint_matrix["values"], dtype=np.float64),
        np.asarray(constraint_matrix["indices"], dtype=np.int32),
        np.asarray(constraint_matrix["offsets"], dtype=np.int32),
    )
    model.set_constraint_lower_bounds(
        _bounds(data['constraintLowerBounds'], lower=True)
    )
    model.set_constraint_upper_bounds(
        _bounds(data['constraintUpperBounds'], lower=False)
    )
    model.set_variable_lower_bounds(
        _bounds(data['variableLowerBounds'], lower=True)
    )
    model.set_variable_upper_bounds(
        _bounds(data['variableUpperBounds'], lower=False)
    )
    model.set_objective_coefficients(
        np.asarray(data['objectiveCoefficients'], dtype=np.float64)
    )
    model.set_objective_offset(float(data['objectiveOffset']))
    model.set_maximize(data['objectiveDirection'] == "maximize")
    model.set_variable_types(
        np.asarray(
            [
                {
                    "continuous": "C",
                    "integer": "I",
                    "semi_continuous": "S",
                }[kind]
                for kind in data['variableKinds']
            ],
            dtype="U1",
        )
    )
    model.set_variable_names(
        np.asarray(data['variableIds'], dtype="U")
    )
    if data.get('quadraticObjective') is not None:
        quadratic = data['quadraticObjective']
        model.set_quadratic_objective_matrix(
            np.asarray(quadratic["values"], dtype=np.float64),
            np.asarray(quadratic["indices"], dtype=np.int32),
            np.asarray(quadratic["offsets"], dtype=np.int32),
        )
    for constraint in data.get('quadraticConstraints', []):
        model.add_quadratic_constraint(
            constraint_row_name=constraint['constraintId'],
            linear_values=np.asarray(
                constraint['linearValues'], dtype=np.float64
            ),
            linear_indices=np.asarray(
                constraint['linearIndices'], dtype=np.int32
            ),
            rhs_value=float(constraint["rhs"]),
            vals=np.asarray(constraint["values"], dtype=np.float64),
            rows=np.asarray(constraint["rows"], dtype=np.int32),
            cols=np.asarray(constraint["columns"], dtype=np.int32),
            sense=(
                "L"
                if constraint["sense"] == "less_than_or_equal"
                else "G"
            ),
        )
    if data.get('initialPrimalSolution') is not None:
        model.set_initial_primal_solution(
            np.asarray(
                data['initialPrimalSolution'], dtype=np.float64
            )
        )
    if data.get('initialDualSolution') is not None:
        model.set_initial_dual_solution(
            np.asarray(data['initialDualSolution'], dtype=np.float64)
        )
    return model


def _bounds(values: list[float | None], lower: bool) -> np.ndarray:
    infinity = -np.inf if lower else np.inf
    return np.asarray(
        [infinity if value is None else value for value in values],
        dtype=np.float64,
    )


def _requires_barrier(data: dict[str, Any]) -> bool:
    return data.get('quadraticObjective') is not None or bool(
        data.get('quadraticConstraints')
    )


def _settings(
    family: str,
    profile: dict[str, Any],
    linear_programming: Any,
    *,
    requires_barrier: bool = False,
) -> tuple[Any, _IncumbentRecorder | None]:
    settings = linear_programming.SolverSettings()
    selected = profile[family]
    settings.set_parameter(
        "time_limit", float(selected['timeLimitSeconds'])
    )
    settings.set_parameter(
        "presolve",
        (2 if selected.get("presolve", True) else 0)
        if family == "convex"
        else (1 if selected.get("presolve", True) else 0),
    )
    recorder = None
    if family == "convex":
        settings.set_parameter(
            "method",
            (
                linear_programming.SolverMethod.Barrier
                if requires_barrier or selected["method"] == "barrier"
                else linear_programming.SolverMethod.PDLP
            ),
        )
        settings.set_optimality_tolerance(
            float(selected['optimalityTolerance'])
        )
    else:
        settings.set_parameter(
            "mip_relative_gap", float(selected['relativeGap'])
        )
        settings.set_parameter(
            "mip_absolute_gap", float(selected['absoluteGap'])
        )
        settings.set_parameter(
            "mip_integrality_tolerance",
            float(selected['integralityTolerance']),
        )
        if selected.get('retainIncumbents', False):
            from cuopt.linear_programming.internals import GetSolutionCallback

            recorder = _IncumbentRecorder(GetSolutionCallback)
            settings.set_mip_callback(recorder.instance, None)
    return settings, recorder


def _solution(
    family: str,
    solution: Any,
    elapsed: float,
    incumbents: list[dict[str, Any]],
) -> dict[str, Any]:
    error_status = solution.get_error_status()
    if getattr(error_status, "name", str(error_status)) != "Success":
        raise RuntimeError(
            f"{getattr(error_status, 'name', error_status)}: "
            f"{solution.get_error_message()}"
        )
    status_name = solution.get_termination_status().name
    status = {
        "Optimal": "optimal",
        "FeasibleFound": "feasible",
        "PrimalFeasible": "feasible",
        "Infeasible": "infeasible",
        "PrimalInfeasible": "infeasible",
        "Unbounded": "unbounded",
        "DualInfeasible": "unbounded",
        "UnboundedOrInfeasible": "infeasible_or_unbounded",
        "TimeLimit": "time_limit",
        "IterationLimit": "iteration_limit",
        "NumericalError": "numerical_failure",
        "NoTermination": "failed",
    }.get(status_name, "failed")
    primal = _optional_array(solution, "get_primal_solution")
    dual = _optional_array(solution, "get_dual_solution")
    lp_stats = _optional_mapping(solution, "get_lp_stats")
    milp_stats = _optional_mapping(solution, "get_milp_stats")
    return {
        "family": family,
        "status": status,
        'primalSolution': primal,
        'dualSolution': dual,
        'primalObjective': _optional_number(
            solution, "get_primal_objective"
        ),
        'dualObjective': _optional_number(
            solution, "get_dual_objective"
        ),
        'bestBound': finite_or_none(milp_stats.get("solution_bound")),
        'relativeGap': _non_negative_or_none(
            milp_stats.get("mip_gap")
        ),
        'primalResidual': _non_negative_or_none(
            lp_stats.get("primal_residual")
        ),
        'dualResidual': _non_negative_or_none(
            lp_stats.get("dual_residual")
        ),
        "iterations": _integer_or_none(
            lp_stats.get(
                "nb_iterations",
                milp_stats.get("num_simplex_iterations"),
            )
        ),
        "nodes": _integer_or_none(milp_stats.get("num_nodes")),
        "incumbents": incumbents,
        'solveSeconds': float(max(0.0, elapsed)),
    }


def _optional_array(solution: Any, method: str) -> list[float]:
    try:
        value = getattr(solution, method)()
    except AttributeError:
        return []
    if value is None:
        return []
    items = [float(item) for item in value.tolist()]
    return items if all(math.isfinite(item) for item in items) else []


def _optional_mapping(solution: Any, method: str) -> dict[str, Any]:
    try:
        value = getattr(solution, method)()
    except AttributeError:
        return {}
    return {} if value is None else dict(value)


def _optional_number(solution: Any, method: str) -> float | None:
    try:
        return finite_or_none(getattr(solution, method)())
    except AttributeError:
        return None


def _non_negative_or_none(value: Any) -> float | None:
    value = finite_or_none(value)
    return None if value is None else max(0.0, value)


def _integer_or_none(value: Any) -> int | None:
    if value is None:
        return None
    value = int(value)
    return value if value >= 0 else None
