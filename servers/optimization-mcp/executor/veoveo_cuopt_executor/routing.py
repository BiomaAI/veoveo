import time
from typing import Any

import numpy as np


OBJECTIVES = {
    "cost": "COST",
    "travel_time": "TRAVEL_TIME",
    "route_size_variance": "VARIANCE_ROUTE_SIZE",
    "route_service_time_variance": "VARIANCE_ROUTE_SERVICE_TIME",
    "prize": "PRIZE",
    "vehicle_fixed_cost": "VEHICLE_FIXED_COST",
}


def solve_routes(
    problem: dict[str, Any], settings: dict[str, Any]
) -> dict[str, Any]:
    from cuopt import routing

    model = _build_model(problem, routing)
    solver_settings = _solver_settings(settings, routing)
    started = time.monotonic()
    solution = routing.Solve(model, solver_settings)
    elapsed = time.monotonic() - started
    return _solution(problem, solution, elapsed, routing)


def solve_route_scenarios(
    cases: list[dict[str, Any]], settings: dict[str, Any]
) -> list[dict[str, Any]]:
    from cuopt import routing

    models = [_build_model(case["problem"], routing) for case in cases]
    solver_settings = _solver_settings(settings, routing)
    started = time.monotonic()
    solutions, reported_time = routing.BatchSolve(models, solver_settings)
    elapsed = time.monotonic() - started
    solve_seconds = (
        float(reported_time)
        if reported_time is not None and np.isfinite(reported_time)
        else elapsed
    )
    return [
        {
            'caseId': case['caseId'],
            "solution": _solution(
                case["problem"], solution, solve_seconds, routing
            ),
        }
        for case, solution in zip(cases, solutions, strict=True)
    ]


def _build_model(problem: dict[str, Any], routing: Any) -> Any:
    import cudf

    nodes = problem["nodes"]
    vehicles = problem["vehicles"]
    model = routing.DataModel(
        len(problem['locationIds']), len(vehicles), len(nodes)
    )
    for matrix in problem['costMatrices']:
        model.add_cost_matrix(
            _matrix(matrix, cudf), int(matrix['vehicleType'])
        )
    for matrix in problem.get('transitTimeMatrices', []):
        model.add_transit_time_matrix(
            _matrix(matrix, cudf), int(matrix['vehicleType'])
        )

    model.set_vehicle_types(
        cudf.Series(
            [vehicle['vehicleType'] for vehicle in vehicles],
            dtype="uint8",
        )
    )
    model.set_vehicle_locations(
        cudf.Series(
            [vehicle['startLocation'] for vehicle in vehicles],
            dtype="int32",
        ),
        cudf.Series(
            [vehicle['endLocation'] for vehicle in vehicles],
            dtype="int32",
        ),
    )
    model.set_vehicle_time_windows(
        cudf.Series(
            [vehicle["earliest"] for vehicle in vehicles], dtype="int32"
        ),
        cudf.Series(
            [vehicle["latest"] for vehicle in vehicles], dtype="int32"
        ),
    )
    model.set_skip_first_trips(
        cudf.Series(
            [vehicle['omitFirstTrip'] for vehicle in vehicles],
            dtype="bool",
        )
    )
    model.set_drop_return_trips(
        cudf.Series(
            [vehicle['omitLastTrip'] for vehicle in vehicles],
            dtype="bool",
        )
    )
    model.set_vehicle_fixed_costs(
        cudf.Series(
            [vehicle['fixedCost'] for vehicle in vehicles], dtype="float32"
        )
    )
    if any(vehicle.get('maximumCost') is not None for vehicle in vehicles):
        model.set_vehicle_max_costs(
            cudf.Series(
                [
                    vehicle.get('maximumCost')
                    if vehicle.get('maximumCost') is not None
                    else np.finfo(np.float32).max
                    for vehicle in vehicles
                ],
                dtype="float32",
            )
        )
    if any(vehicle.get('maximumTime') is not None for vehicle in vehicles):
        model.set_vehicle_max_times(
            cudf.Series(
                [
                    vehicle.get('maximumTime')
                    if vehicle.get('maximumTime') is not None
                    else np.finfo(np.float32).max
                    for vehicle in vehicles
                ],
                dtype="float32",
            )
        )
    for vehicle_index, vehicle in enumerate(vehicles):
        for vehicle_break in vehicle.get("breaks", []):
            arguments: list[Any] = [
                vehicle_index,
                int(vehicle_break["earliest"]),
                int(vehicle_break["latest"]),
                int(vehicle_break["duration"]),
            ]
            allowed = vehicle_break.get('allowedLocations', [])
            if allowed:
                arguments.append(cudf.Series(allowed, dtype="int32"))
            model.add_vehicle_break(*arguments)

    model.set_order_locations(
        cudf.Series(
            [node['locationIndex'] for node in nodes], dtype="int32"
        )
    )
    model.set_order_time_windows(
        cudf.Series([node["earliest"] for node in nodes], dtype="int32"),
        cudf.Series([node["latest"] for node in nodes], dtype="int32"),
    )
    model.set_order_service_times(
        cudf.Series(
            [node['serviceDuration'] for node in nodes], dtype="int32"
        )
    )
    prizes = [node["prize"] for node in nodes]
    if any(prize > 0 for prize in prizes):
        model.set_order_prizes(cudf.Series(prizes, dtype="float32"))

    pairs = problem.get('pickupDeliveryPairs', [])
    if pairs:
        model.set_pickup_delivery_pairs(
            cudf.Series(
                [pair['pickupNode'] for pair in pairs], dtype="int32"
            ),
            cudf.Series(
                [pair['deliveryNode'] for pair in pairs], dtype="int32"
            ),
        )
    for restriction in problem.get('orderVehicleMatches', []):
        model.add_order_vehicle_match(
            int(restriction["node"]),
            cudf.Series(restriction["vehicles"], dtype="int32"),
        )
    for dimension in problem.get('capacityDimensions', []):
        model.add_capacity_dimension(
            dimension['dimensionId'],
            cudf.Series(dimension["demand"], dtype="int32"),
            cudf.Series(dimension["capacity"], dtype="int32"),
        )
    objectives = problem["objectives"]
    model.set_objective_function(
        cudf.Series(
            [
                getattr(routing.Objective, OBJECTIVES[objective["metric"]])
                for objective in objectives
            ]
        ),
        cudf.Series(
            [objective["weight"] for objective in objectives],
            dtype="float32",
        ),
    )
    minimum_vehicles = int(problem.get('minimumVehicles', 0))
    if minimum_vehicles:
        model.set_min_vehicles(minimum_vehicles)
    initial = problem.get('initialSolution')
    if initial is not None:
        kind_names = {
            "depot": "Depot",
            "delivery": "Delivery",
            "pickup": "Pickup",
            "break": "Break",
        }
        model.add_initial_solutions(
            cudf.Series(initial['vehicleIndices'], dtype="int32"),
            cudf.Series(initial['routeNodes'], dtype="int32"),
            cudf.Series(
                [kind_names[kind] for kind in initial['nodeKinds']],
                dtype="str",
            ),
            cudf.Series(initial['solutionOffsets'], dtype="int32"),
        )
    return model


def _matrix(matrix: dict[str, Any], cudf: Any) -> Any:
    dimension = int(matrix["dimension"])
    values = np.asarray(matrix["values"], dtype=np.float32).reshape(
        (dimension, dimension)
    )
    for index in matrix.get('unavailableCells', []):
        row, column = divmod(int(index), dimension)
        values[row, column] = np.finfo(np.float32).max
    return cudf.DataFrame(values)


def _solver_settings(settings: dict[str, Any], routing: Any) -> Any:
    result = routing.SolverSettings()
    result.set_time_limit(float(settings['timeLimitSeconds']))
    result.set_verbose_mode(bool(settings.get("verbose", False)))
    return result


def _solution(
    problem: dict[str, Any],
    solution: Any,
    elapsed: float,
    routing: Any,
) -> dict[str, Any]:
    error_status = solution.get_error_status()
    if int(error_status) != int(routing.ErrorStatus.Success):
        message = (
            f"{getattr(error_status, 'name', error_status)}: "
            f"{solution.get_error_message()}"
        )
        if int(error_status) == int(routing.ErrorStatus.OutOfMemoryError):
            raise MemoryError(message)
        raise RuntimeError(message)
    status_code = int(solution.get_status())
    status = {
        0: "success",
        1: "failed",
        2: "timeout",
        3: "empty",
    }.get(status_code, "failed")
    records = solution.get_route().to_pandas().to_dict(orient="records")
    grouped: dict[int, list[dict[str, Any]]] = {}
    for record in records:
        grouped.setdefault(int(record["truck_id"]), []).append(record)
    routes = []
    for vehicle, visits in grouped.items():
        route_visits = []
        for visit in visits:
            node_type = str(visit.get("type", "Delivery"))
            if node_type == "Depot":
                node = {
                    "kind": "depot",
                    "location": int(visit["location"]),
                }
            elif node_type == "Break":
                node = {
                    "kind": "break",
                    "location": int(visit["location"]),
                }
            else:
                node = {"kind": "order", "node": int(visit["route"])}
            route_visits.append(
                {
                    "node": node,
                    "arrival": float(visit["arrival_stamp"]),
                }
            )
        routes.append({"vehicle": vehicle, "nodes": route_visits})

    objective_components = {}
    for key, value in solution.get_objective_values().items():
        metric = {
            "COST": "cost",
            "TRAVEL_TIME": "travel_time",
            "VARIANCE_ROUTE_SIZE": "route_size_variance",
            "VARIANCE_ROUTE_SERVICE_TIME": "route_service_time_variance",
            "PRIZE": "prize",
            "VEHICLE_FIXED_COST": "vehicle_fixed_cost",
        }.get(key.name)
        if metric is not None:
            objective_components[metric] = float(value)
    infeasible = solution.get_infeasible_orders()
    undeliverable = (
        []
        if infeasible is None
        else [int(value) for value in infeasible.to_arrow().to_pylist()]
    )
    return {
        "status": status,
        "message": str(solution.get_message()),
        "objective": float(solution.get_total_objective()),
        'objectiveComponents': objective_components,
        'vehiclesUsed': int(solution.get_vehicle_count()),
        "routes": routes,
        'undeliverableNodes': undeliverable,
        'solveSeconds': float(max(0.0, elapsed)),
    }
