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
                        "found_at_seconds": max(
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

    model = _build_model(model_data, linear_programming)
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
    return _solution(
        family,
        solution,
        elapsed,
        recorder.instance.items if recorder is not None else [],
    )


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
    constraint_matrix = data["constraint_matrix"]
    model.set_csr_constraint_matrix(
        np.asarray(constraint_matrix["values"], dtype=np.float64),
        np.asarray(constraint_matrix["indices"], dtype=np.int32),
        np.asarray(constraint_matrix["offsets"], dtype=np.int32),
    )
    model.set_constraint_lower_bounds(
        _bounds(data["constraint_lower_bounds"], lower=True)
    )
    model.set_constraint_upper_bounds(
        _bounds(data["constraint_upper_bounds"], lower=False)
    )
    model.set_variable_lower_bounds(
        _bounds(data["variable_lower_bounds"], lower=True)
    )
    model.set_variable_upper_bounds(
        _bounds(data["variable_upper_bounds"], lower=False)
    )
    model.set_objective_coefficients(
        np.asarray(data["objective_coefficients"], dtype=np.float64)
    )
    model.set_objective_offset(float(data["objective_offset"]))
    model.set_maximize(data["objective_direction"] == "maximize")
    model.set_variable_types(
        np.asarray(
            [
                {
                    "continuous": "C",
                    "integer": "I",
                    "semi_continuous": "S",
                }[kind]
                for kind in data["variable_kinds"]
            ],
            dtype="U1",
        )
    )
    model.set_variable_names(
        np.asarray(data["variable_ids"], dtype="U")
    )
    if data.get("quadratic_objective") is not None:
        quadratic = data["quadratic_objective"]
        model.set_quadratic_objective_matrix(
            np.asarray(quadratic["values"], dtype=np.float64),
            np.asarray(quadratic["indices"], dtype=np.int32),
            np.asarray(quadratic["offsets"], dtype=np.int32),
        )
    for constraint in data.get("quadratic_constraints", []):
        model.add_quadratic_constraint(
            constraint_row_name=constraint["constraint_id"],
            linear_values=np.asarray(
                constraint["linear_values"], dtype=np.float64
            ),
            linear_indices=np.asarray(
                constraint["linear_indices"], dtype=np.int32
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
    if data.get("initial_primal_solution") is not None:
        model.set_initial_primal_solution(
            np.asarray(
                data["initial_primal_solution"], dtype=np.float64
            )
        )
    if data.get("initial_dual_solution") is not None:
        model.set_initial_dual_solution(
            np.asarray(data["initial_dual_solution"], dtype=np.float64)
        )
    return model


def _bounds(values: list[float | None], lower: bool) -> np.ndarray:
    infinity = -np.inf if lower else np.inf
    return np.asarray(
        [infinity if value is None else value for value in values],
        dtype=np.float64,
    )


def _requires_barrier(data: dict[str, Any]) -> bool:
    return data.get("quadratic_objective") is not None or bool(
        data.get("quadratic_constraints")
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
        "time_limit", float(selected["time_limit_seconds"])
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
            float(selected["optimality_tolerance"])
        )
    else:
        settings.set_parameter(
            "mip_relative_gap", float(selected["relative_gap"])
        )
        settings.set_parameter(
            "mip_absolute_gap", float(selected["absolute_gap"])
        )
        settings.set_parameter(
            "mip_integrality_tolerance",
            float(selected["integrality_tolerance"]),
        )
        if selected.get("retain_incumbents", False):
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
        "primal_solution": primal,
        "dual_solution": dual,
        "primal_objective": _optional_number(
            solution, "get_primal_objective"
        ),
        "dual_objective": _optional_number(
            solution, "get_dual_objective"
        ),
        "best_bound": finite_or_none(milp_stats.get("solution_bound")),
        "relative_gap": _non_negative_or_none(
            milp_stats.get("mip_gap")
        ),
        "primal_residual": _non_negative_or_none(
            lp_stats.get("primal_residual")
        ),
        "dual_residual": _non_negative_or_none(
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
        "solve_seconds": float(max(0.0, elapsed)),
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
