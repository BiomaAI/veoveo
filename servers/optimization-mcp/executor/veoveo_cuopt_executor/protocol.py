from __future__ import annotations

import asyncio
import json
import math
from typing import Any

from . import PROTOCOL_VERSION


class ProtocolError(ValueError):
    pass


def validation_diagnostic(error: ValidationError) -> str:
    """Expose validator kinds without payload values or discriminator messages."""
    kinds = sorted({item["type"] for item in error.errors(include_input=False, include_context=False, include_url=False)})
    return "invalid protocol input: " + ", ".join(kinds)


def require_mapping(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ProtocolError(f"{label} must be an object")
    return value


def require_protocol(request: dict[str, Any]) -> None:
    if request.get("protocol") != PROTOCOL_VERSION:
        raise ProtocolError(
            f"protocol must be exactly {PROTOCOL_VERSION}"
        )
    try:
        ExecutorRequest.model_validate(request)
    except ValidationError as error:
        raise ProtocolError(validation_diagnostic(error)) from error
    run_id = request.get("run_id")
    if not isinstance(run_id, str) or not run_id.startswith("run-"):
        raise ProtocolError("run_id must be a controlled run identifier")
    operation = require_mapping(request.get("operation"), "operation")
    if not isinstance(operation.get("operation"), str):
        raise ProtocolError("operation.operation must be a string")


async def read_frame(
    reader: asyncio.StreamReader, maximum_bytes: int
) -> dict[str, Any]:
    prefix = await reader.readexactly(8)
    length = int.from_bytes(prefix, byteorder="big", signed=False)
    if length > maximum_bytes:
        raise ProtocolError(
            f"request frame is {length} bytes and exceeds {maximum_bytes}"
        )
    body = await reader.readexactly(length)
    try:
        request = json.loads(body)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ProtocolError(f"request is not valid UTF-8 JSON: {error}") from error
    request = require_mapping(request, "request")
    require_protocol(request)
    return request


async def write_frame(
    writer: asyncio.StreamWriter,
    response: dict[str, Any],
    maximum_bytes: int,
) -> None:
    try:
        ExecutorResponse.model_validate(response)
    except ValidationError as error:
        raise ProtocolError(validation_diagnostic(error)) from error
    body = json.dumps(
        response,
        allow_nan=False,
        check_circular=True,
        separators=(",", ":"),
    ).encode("utf-8")
    if len(body) > maximum_bytes:
        raise ProtocolError(
            f"response frame is {len(body)} bytes and exceeds {maximum_bytes}"
        )
    writer.write(len(body).to_bytes(8, byteorder="big", signed=False))
    writer.write(body)
    await writer.drain()


def response(run_id: str, result: dict[str, Any]) -> dict[str, Any]:
    return {
        "protocol": PROTOCOL_VERSION,
        "run_id": run_id,
        "result": result,
    }


def error_response(
    run_id: str, code: str, message: str
) -> dict[str, Any]:
    return response(
        run_id,
        {
            "result": "error",
            "error": {
                "code": code,
                "message": message,
                "findings": [],
            },
        },
    )


def finite_or_none(value: Any) -> float | None:
    if value is None:
        return None
    value = float(value)
    return value if math.isfinite(value) else None

# Private cuOpt wire objects. Provider-native solver objects stay outside this adapter.
from typing import Annotated, Literal
from pydantic import BaseModel, ConfigDict, Field, ValidationError

U8 = Annotated[int, Field(ge=0, le=2**8 - 1)]
U32 = Annotated[int, Field(ge=0, le=2**32 - 1)]
U64 = Annotated[int, Field(ge=0, le=2**64 - 1)]
I32 = Annotated[int, Field(ge=-(2**31), le=2**31 - 1)]
NonNegative = Annotated[float, Field(ge=0)]


class _Model(BaseModel):
    model_config = ConfigDict(hide_input_in_errors=True, extra="forbid", strict=True, allow_inf_nan=False, protected_namespaces=())

VariableKind = Literal['continuous', 'integer', 'semi_continuous']

ObjectiveDirection = Literal['minimize', 'maximize']

RouteObjectiveMetric = Literal['cost', 'travel_time', 'route_size_variance', 'route_service_time_variance', 'prize', 'vehicle_fixed_cost']

ProblemFamily = Literal['routing', 'route_scenarios', 'convex', 'milp']

RouteNodeKind = Literal['depot', 'service', 'pickup', 'delivery', 'break']

class VerificationFinding(_Model):
    code: VerificationCode
    severity: VerificationSeverity
    message: str
    variable_id: str | None = None
    constraint_id: str | None = None
    order_id: str | None = None
    vehicle_id: str | None = None

VerificationCode = Literal['missing_variable', 'duplicate_variable', 'unknown_variable', 'variable_lower_bound', 'variable_upper_bound', 'variable_integrality', 'constraint_lower_bound', 'constraint_upper_bound', 'objective_mismatch', 'unknown_vehicle', 'duplicate_vehicle_route', 'invalid_route_endpoint', 'unknown_route_node', 'duplicate_route_node', 'missing_mandatory_order', 'partial_pickup_delivery', 'pickup_delivery_precedence', 'vehicle_order_restriction', 'order_time_window', 'vehicle_time_window', 'vehicle_capacity', 'vehicle_maximum_cost', 'vehicle_maximum_time', 'unavailable_travel_arc', 'arrival_sequence', 'solver_reported_failure']

VerificationSeverity = Literal['information', 'warning', 'error']

class ExecutorRequest(_Model):
    protocol: str
    run_id: str
    profile: ExecutorProfile | None = None
    operation: ExecutorOperation

class ExecutorOperationHealth(_Model):
    operation: Literal['health']

class ExecutorOperationCancel(_Model):
    operation: Literal['cancel']
    target_run_id: str

class ExecutorOperationSolveRoutes(_Model):
    operation: Literal['solve_routes']
    problem: CompiledRoutingProblem

class ExecutorOperationSolveRouteScenarios(_Model):
    operation: Literal['solve_route_scenarios']
    cases: list[CompiledRouteCase]

class ExecutorOperationSolveModel(_Model):
    operation: Literal['solve_model']
    family: ExecutorModelFamily
    model: CompiledMathematicalModel

class ExecutorOperationSolveModelFile(_Model):
    operation: Literal['solve_model_file']
    family: ExecutorModelFamily
    staged_path: str

ExecutorOperation = Annotated[ExecutorOperationHealth | ExecutorOperationCancel | ExecutorOperationSolveRoutes | ExecutorOperationSolveRouteScenarios | ExecutorOperationSolveModel | ExecutorOperationSolveModelFile, Field(discriminator='operation')]

class ExecutorProfile(_Model):
    name: str
    routing: RoutingSolverSettings
    convex: ConvexSolverSettings
    milp: MilpSolverSettings

class RoutingSolverSettings(_Model):
    time_limit_seconds: NonNegative
    verbose: bool = False

ConvexMethod = Literal['pdlp', 'barrier']

class ConvexSolverSettings(_Model):
    time_limit_seconds: NonNegative
    method: ConvexMethod
    optimality_tolerance: NonNegative
    presolve: bool

class MilpSolverSettings(_Model):
    time_limit_seconds: NonNegative
    relative_gap: NonNegative
    absolute_gap: NonNegative
    integrality_tolerance: NonNegative
    presolve: bool
    retain_incumbents: bool

class CompiledRouteCase(_Model):
    case_id: str
    problem: CompiledRoutingProblem

class CompiledRoutingProblem(_Model):
    location_ids: list[str]
    nodes: list[CompiledRouteNode]
    vehicles: list[CompiledVehicle]
    vehicle_type_ids: list[str]
    cost_matrices: list[CompiledDenseMatrix]
    transit_time_matrices: list[CompiledDenseMatrix] = Field(default_factory=list)
    capacity_dimensions: list[CompiledCapacityDimension] = Field(default_factory=list)
    pickup_delivery_pairs: list[CompiledPickupDeliveryPair] = Field(default_factory=list)
    order_vehicle_matches: list[CompiledOrderVehicleMatch] = Field(default_factory=list)
    objectives: list[CompiledRouteObjective]
    minimum_vehicles: U32
    initial_solution: CompiledInitialRoutingSolution | None = None

class CompiledInitialRoutingSolution(_Model):
    vehicle_indices: list[U32]
    route_nodes: list[U32]
    node_kinds: list[CompiledInitialRouteNodeKind]
    solution_offsets: list[U32]

CompiledInitialRouteNodeKind = Literal['depot', 'delivery', 'pickup', 'break']

class CompiledRouteNode(_Model):
    order_id: str
    location_id: str
    location_index: U32
    kind: RouteNodeKind
    service_duration: U32
    earliest: U32
    latest: U32
    prize: float

class CompiledVehicle(_Model):
    vehicle_id: str
    vehicle_type: U8
    start_location: U32
    end_location: U32
    earliest: U32
    latest: U32
    fixed_cost: float
    maximum_cost: float | None = None
    maximum_time: float | None = None
    omit_first_trip: bool
    omit_last_trip: bool
    breaks: list[CompiledVehicleBreak] = Field(default_factory=list)

class CompiledVehicleBreak(_Model):
    earliest: U32
    latest: U32
    duration: U32
    allowed_locations: list[U32] = Field(default_factory=list)

class CompiledDenseMatrix(_Model):
    vehicle_type: U8
    dimension: U32
    values: list[float]
    unavailable_cells: list[U32] = Field(default_factory=list)

class CompiledCapacityDimension(_Model):
    dimension_id: str
    demand: list[I32]
    capacity: list[U32]

class CompiledPickupDeliveryPair(_Model):
    pickup_node: U32
    delivery_node: U32

class CompiledOrderVehicleMatch(_Model):
    node: U32
    vehicles: list[U32]

class CompiledRouteObjective(_Model):
    metric: RouteObjectiveMetric
    weight: float

ExecutorModelFamily = Literal['convex', 'milp']

class CompiledMathematicalModel(_Model):
    variable_ids: list[str]
    variable_kinds: list[VariableKind]
    variable_lower_bounds: list[float | None]
    variable_upper_bounds: list[float | None]
    objective_direction: ObjectiveDirection
    objective_offset: float
    objective_coefficients: list[float]
    constraint_ids: list[str]
    constraint_matrix: CsrMatrix
    constraint_lower_bounds: list[float | None]
    constraint_upper_bounds: list[float | None]
    quadratic_objective: CsrMatrix | None = None
    quadratic_constraints: list[CompiledQuadraticConstraint] = Field(default_factory=list)
    initial_primal_solution: list[float] | None = None
    initial_dual_solution: list[float] | None = None

class CsrMatrix(_Model):
    rows: U32
    columns: U32
    offsets: list[U32]
    indices: list[U32]
    values: list[float]

class CompiledQuadraticConstraint(_Model):
    constraint_id: str
    linear_indices: list[U32]
    linear_values: list[float]
    rows: list[U32]
    columns: list[U32]
    values: list[float]
    sense: QuadraticConstraintSense
    rhs: float

QuadraticConstraintSense = Literal['less_than_or_equal', 'greater_than_or_equal']

class ExecutorResponse(_Model):
    protocol: str
    run_id: str
    result: ExecutorResult

class ExecutorResultHealth(_Model):
    result: Literal['health']
    health: ExecutorHealth

class ExecutorResultRoutes(_Model):
    result: Literal['routes']
    solution: ExecutorRoutingSolution

class ExecutorResultRouteScenarios(_Model):
    result: Literal['route_scenarios']
    solutions: list[ExecutorRouteCaseSolution]

class ExecutorResultModel(_Model):
    result: Literal['model']
    solution: ExecutorMathematicalSolution

class ExecutorResultError(_Model):
    result: Literal['error']
    error: ExecutorError

ExecutorResult = Annotated[ExecutorResultHealth | ExecutorResultRoutes | ExecutorResultRouteScenarios | ExecutorResultModel | ExecutorResultError, Field(discriminator='result')]

class ExecutorHealth(_Model):
    ready: bool
    cuopt_version: str
    cuda_runtime_version: str
    gpu_name: str
    gpu_uuid: str
    compute_capability: str

class ExecutorError(_Model):
    code: ExecutorErrorCode
    message: str
    findings: list[VerificationFinding] = Field(default_factory=list)

ExecutorErrorCode = Literal['invalid_request', 'unsupported_problem', 'out_of_memory', 'solver_failure', 'protocol_failure', 'gpu_unavailable']

ExecutorRoutingStatus = Literal['success', 'timeout', 'infeasible', 'failed', 'empty']

class ExecutorRoutingSolution(_Model):
    status: ExecutorRoutingStatus
    message: str
    objective: float
    objective_components: dict[RouteObjectiveMetric, float]
    vehicles_used: U32
    routes: list[ExecutorVehicleRoute] = Field(default_factory=list)
    undeliverable_nodes: list[U32] = Field(default_factory=list)
    solve_seconds: NonNegative

class ExecutorRouteCaseSolution(_Model):
    case_id: str
    solution: ExecutorRoutingSolution

class ExecutorVehicleRoute(_Model):
    vehicle: U32
    nodes: list[ExecutorRouteVisit]

class ExecutorRouteVisit(_Model):
    node: ExecutorRouteNode
    arrival: NonNegative

class ExecutorRouteNodeDepot(_Model):
    kind: Literal['depot']
    location: U32

class ExecutorRouteNodeOrder(_Model):
    kind: Literal['order']
    node: U32

class ExecutorRouteNodeBreak(_Model):
    kind: Literal['break']
    location: U32

ExecutorRouteNode = Annotated[ExecutorRouteNodeDepot | ExecutorRouteNodeOrder | ExecutorRouteNodeBreak, Field(discriminator='kind')]

ExecutorModelStatus = Literal['optimal', 'feasible', 'infeasible', 'unbounded', 'infeasible_or_unbounded', 'time_limit', 'iteration_limit', 'node_limit', 'numerical_failure', 'cancelled', 'failed']

class ExecutorMathematicalSolution(_Model):
    family: ProblemFamily
    status: ExecutorModelStatus
    primal_solution: list[float] = Field(default_factory=list)
    dual_solution: list[float] = Field(default_factory=list)
    primal_objective: float | None = None
    dual_objective: float | None = None
    best_bound: float | None = None
    relative_gap: NonNegative | None = None
    primal_residual: NonNegative | None = None
    dual_residual: NonNegative | None = None
    iterations: U64 | None = None
    nodes: U64 | None = None
    incumbents: list[ExecutorIncumbent] = Field(default_factory=list)
    solve_seconds: NonNegative

class ExecutorIncumbent(_Model):
    sequence: U64
    values: list[float]
    objective: float
    bound: float
    found_at_seconds: NonNegative

VerificationFinding.model_rebuild()
ExecutorRequest.model_rebuild()
ExecutorOperationHealth.model_rebuild()
ExecutorOperationCancel.model_rebuild()
ExecutorOperationSolveRoutes.model_rebuild()
ExecutorOperationSolveRouteScenarios.model_rebuild()
ExecutorOperationSolveModel.model_rebuild()
ExecutorOperationSolveModelFile.model_rebuild()
ExecutorProfile.model_rebuild()
RoutingSolverSettings.model_rebuild()
ConvexSolverSettings.model_rebuild()
MilpSolverSettings.model_rebuild()
CompiledRouteCase.model_rebuild()
CompiledRoutingProblem.model_rebuild()
CompiledInitialRoutingSolution.model_rebuild()
CompiledRouteNode.model_rebuild()
CompiledVehicle.model_rebuild()
CompiledVehicleBreak.model_rebuild()
CompiledDenseMatrix.model_rebuild()
CompiledCapacityDimension.model_rebuild()
CompiledPickupDeliveryPair.model_rebuild()
CompiledOrderVehicleMatch.model_rebuild()
CompiledRouteObjective.model_rebuild()
CompiledMathematicalModel.model_rebuild()
CsrMatrix.model_rebuild()
CompiledQuadraticConstraint.model_rebuild()
ExecutorResponse.model_rebuild()
ExecutorResultHealth.model_rebuild()
ExecutorResultRoutes.model_rebuild()
ExecutorResultRouteScenarios.model_rebuild()
ExecutorResultModel.model_rebuild()
ExecutorResultError.model_rebuild()
ExecutorHealth.model_rebuild()
ExecutorError.model_rebuild()
ExecutorRoutingSolution.model_rebuild()
ExecutorRouteCaseSolution.model_rebuild()
ExecutorVehicleRoute.model_rebuild()
ExecutorRouteVisit.model_rebuild()
ExecutorRouteNodeDepot.model_rebuild()
ExecutorRouteNodeOrder.model_rebuild()
ExecutorRouteNodeBreak.model_rebuild()
ExecutorMathematicalSolution.model_rebuild()
ExecutorIncumbent.model_rebuild()
