use std::env;

use tokio_util::sync::CancellationToken;
use veoveo_optimization_mcp::{
    contract::{
        ConstraintId, FiniteF64, LocationId, NonNegativeF64, ObjectiveDirection, OrderId,
        ProblemFamily, RouteNodeKind, RouteObjectiveMetric, RunId, VariableId, VariableKind,
        VehicleId,
    },
    executor::{
        CompiledDenseMatrix, CompiledMathematicalModel, CompiledQuadraticConstraint,
        CompiledRouteNode, CompiledRouteObjective, CompiledRoutingProblem, CompiledVehicle,
        ConvexMethod, ConvexSolverSettings, CsrMatrix, ExecutorClient, ExecutorModelFamily,
        ExecutorModelStatus, ExecutorOperation, ExecutorProfile, ExecutorResult,
        ExecutorRoutingStatus, MilpSolverSettings, QuadraticConstraintSense, RoutingSolverSettings,
    },
};

fn finite(value: f64) -> FiniteF64 {
    FiniteF64::new(value).unwrap()
}

fn non_negative(value: f64) -> NonNegativeF64 {
    NonNegativeF64::new(value).unwrap()
}

fn profile() -> ExecutorProfile {
    ExecutorProfile {
        name: "gpu-smoke".to_owned(),
        routing: RoutingSolverSettings {
            time_limit_seconds: non_negative(10.0),
            verbose: false,
        },
        convex: ConvexSolverSettings {
            time_limit_seconds: non_negative(10.0),
            method: ConvexMethod::Pdlp,
            optimality_tolerance: non_negative(1e-6),
            presolve: true,
        },
        milp: MilpSolverSettings {
            time_limit_seconds: non_negative(10.0),
            relative_gap: non_negative(0.0),
            absolute_gap: non_negative(0.0),
            integrality_tolerance: non_negative(1e-5),
            presolve: true,
            retain_incumbents: true,
        },
    }
}

fn routing_problem() -> CompiledRoutingProblem {
    CompiledRoutingProblem {
        location_ids: ["depot", "one", "two"]
            .into_iter()
            .map(|value| LocationId::parse(value).unwrap())
            .collect(),
        nodes: vec![
            CompiledRouteNode {
                order_id: OrderId::parse("order-one").unwrap(),
                location_id: LocationId::parse("one").unwrap(),
                location_index: 1,
                kind: RouteNodeKind::Service,
                service_duration: 0,
                earliest: 0,
                latest: 100,
                prize: 0.0,
            },
            CompiledRouteNode {
                order_id: OrderId::parse("order-two").unwrap(),
                location_id: LocationId::parse("two").unwrap(),
                location_index: 2,
                kind: RouteNodeKind::Service,
                service_duration: 0,
                earliest: 0,
                latest: 100,
                prize: 0.0,
            },
        ],
        vehicles: vec![CompiledVehicle {
            vehicle_id: VehicleId::parse("vehicle-one").unwrap(),
            vehicle_type: 0,
            start_location: 0,
            end_location: 0,
            earliest: 0,
            latest: 100,
            fixed_cost: 0.0,
            maximum_cost: None,
            maximum_time: None,
            omit_first_trip: false,
            omit_last_trip: false,
            breaks: vec![],
        }],
        vehicle_type_ids: vec!["default".to_owned()],
        cost_matrices: vec![CompiledDenseMatrix {
            vehicle_type: 0,
            dimension: 3,
            values: vec![0.0, 1.0, 2.0, 1.0, 0.0, 1.0, 2.0, 1.0, 0.0],
            unavailable_cells: vec![],
        }],
        transit_time_matrices: vec![],
        capacity_dimensions: vec![],
        pickup_delivery_pairs: vec![],
        order_vehicle_matches: vec![],
        objectives: vec![CompiledRouteObjective {
            metric: RouteObjectiveMetric::Cost,
            weight: 1.0,
        }],
        minimum_vehicles: 0,
        initial_solution: None,
    }
}

fn mathematical_model(variable_kind: VariableKind) -> CompiledMathematicalModel {
    CompiledMathematicalModel {
        variable_ids: ["x", "y"]
            .into_iter()
            .map(|value| VariableId::parse(value).unwrap())
            .collect(),
        variable_kinds: vec![variable_kind; 2],
        variable_lower_bounds: vec![Some(finite(0.0)), Some(finite(0.0))],
        variable_upper_bounds: vec![None, None],
        objective_direction: ObjectiveDirection::Minimize,
        objective_offset: finite(0.0),
        objective_coefficients: vec![finite(1.0), finite(1.0)],
        constraint_ids: vec![ConstraintId::parse("minimum").unwrap()],
        constraint_matrix: CsrMatrix {
            rows: 1,
            columns: 2,
            offsets: vec![0, 2],
            indices: vec![0, 1],
            values: vec![finite(1.0), finite(1.0)],
        },
        constraint_lower_bounds: vec![Some(finite(1.0))],
        constraint_upper_bounds: vec![None],
        quadratic_objective: None,
        quadratic_constraints: vec![],
        initial_primal_solution: None,
        initial_dual_solution: None,
    }
}

fn quadratic_program() -> CompiledMathematicalModel {
    let mut model = mathematical_model(VariableKind::Continuous);
    model.objective_coefficients = vec![finite(0.0), finite(0.0)];
    model.quadratic_objective = Some(CsrMatrix {
        rows: 2,
        columns: 2,
        offsets: vec![0, 1, 2],
        indices: vec![0, 1],
        values: vec![finite(1.0), finite(1.0)],
    });
    model
}

fn bound_only_model(variable_kind: VariableKind) -> CompiledMathematicalModel {
    let mut model = mathematical_model(variable_kind);
    model.variable_lower_bounds = vec![Some(finite(-1.5)); 2];
    model.variable_upper_bounds = vec![Some(finite(2.5)); 2];
    model.objective_coefficients = vec![finite(1.0), finite(-1.0)];
    model.objective_offset = finite(1.0);
    model.constraint_ids.clear();
    model.constraint_matrix.rows = 0;
    model.constraint_matrix.offsets = vec![0];
    model.constraint_matrix.indices.clear();
    model.constraint_matrix.values.clear();
    model.constraint_lower_bounds.clear();
    model.constraint_upper_bounds.clear();
    model.initial_dual_solution = Some(vec![]);
    model
}

#[tokio::test]
#[ignore = "requires the pinned cuOpt image and one NVIDIA GPU"]
async fn solves_models_with_empty_linear_matrices_on_the_gpu() {
    let socket = env::var("VEOVEO_CUOPT_TEST_SOCKET")
        .expect("VEOVEO_CUOPT_TEST_SOCKET must identify the executor socket");
    let client = ExecutorClient::with_default_limit(socket);
    let ExecutorResult::Health { health } = client.health().await.unwrap().result else {
        panic!("executor returned a non-health response");
    };
    assert!(health.ready);
    assert!(!health.gpu_uuid.is_empty());

    let linear = bound_only_model(VariableKind::Continuous);
    let mut integer = bound_only_model(VariableKind::Integer);
    integer.initial_dual_solution = None;
    let mut quadratic = linear.clone();
    quadratic.variable_lower_bounds = vec![Some(finite(1.0)), Some(finite(2.0))];
    quadratic.objective_coefficients = vec![finite(0.0); 2];
    quadratic.objective_offset = finite(0.0);
    quadratic.quadratic_objective = quadratic_program().quadratic_objective;
    let mut empty_row = linear.clone();
    empty_row.constraint_ids = vec![ConstraintId::parse("zero-row").unwrap()];
    empty_row.constraint_matrix.rows = 1;
    empty_row.constraint_matrix.offsets.push(0);
    empty_row.constraint_lower_bounds = vec![Some(finite(0.0))];
    empty_row.constraint_upper_bounds = vec![Some(finite(0.0))];
    empty_row.initial_dual_solution = Some(vec![finite(0.0)]);
    let mut semi_continuous = bound_only_model(VariableKind::SemiContinuous);
    semi_continuous.variable_lower_bounds = vec![Some(finite(2.0)); 2];
    semi_continuous.variable_upper_bounds = vec![Some(finite(5.0)); 2];
    semi_continuous.objective_coefficients = vec![finite(1.0); 2];
    semi_continuous.initial_dual_solution = None;
    let mut free_quadratic = quadratic.clone();
    free_quadratic.variable_lower_bounds = vec![None; 2];
    free_quadratic.variable_upper_bounds = vec![None; 2];
    free_quadratic.objective_coefficients = vec![finite(-2.0), finite(-4.0)];
    free_quadratic.objective_offset = finite(5.0);
    let mut quadratic_only = linear.clone();
    quadratic_only.quadratic_constraints =
        quadratically_constrained_program().quadratic_constraints;
    quadratic_only.constraint_ids = vec![ConstraintId::parse("unit-circle").unwrap()];
    quadratic_only.objective_coefficients = vec![finite(-1.0); 2];
    quadratic_only.objective_offset = finite(0.0);
    quadratic_only.initial_dual_solution = Some(vec![finite(0.0)]);
    let mut infeasible = empty_row.clone();
    infeasible.constraint_lower_bounds = vec![Some(finite(1.0))];
    infeasible.constraint_upper_bounds = vec![Some(finite(1.0))];
    let mut unbounded = linear.clone();
    unbounded.variable_upper_bounds = vec![None; 2];

    for (label, family, model, expected_objective) in [
        ("bound-only LP", ExecutorModelFamily::Convex, linear, -3.0),
        ("bound-only MILP", ExecutorModelFamily::Milp, integer, -2.0),
        ("bound-only QP", ExecutorModelFamily::Convex, quadratic, 5.0),
        (
            "empty linear row",
            ExecutorModelFamily::Convex,
            empty_row,
            -3.0,
        ),
        (
            "semi-continuous zero",
            ExecutorModelFamily::Milp,
            semi_continuous,
            1.0,
        ),
        (
            "free-variable QP",
            ExecutorModelFamily::Convex,
            free_quadratic,
            0.0,
        ),
        (
            "quadratic-only constraints",
            ExecutorModelFamily::Convex,
            quadratic_only,
            -std::f64::consts::SQRT_2,
        ),
    ] {
        let constraint_count = model.constraint_ids.len();
        let request = veoveo_optimization_mcp::executor::ExecutorRequest::new(
            RunId::new(),
            profile(),
            ExecutorOperation::SolveModel { family, model },
        );
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            client.execute(&request, CancellationToken::new()),
        )
        .await
        .unwrap_or_else(|_| panic!("{label} exceeded 30 seconds"))
        .unwrap_or_else(|error| panic!("{label} executor request failed: {error:?}"));
        let ExecutorResult::Model { solution } = response.result else {
            panic!("{label} executor returned {response:?}");
        };
        assert_eq!(solution.status, ExecutorModelStatus::Optimal, "{label}");
        assert_eq!(solution.primal_solution.len(), 2, "{label}");
        assert!(
            solution
                .incumbents
                .iter()
                .all(|incumbent| incumbent.values.len() == 2),
            "{label}"
        );
        assert!(
            solution.dual_solution.is_empty() || solution.dual_solution.len() == constraint_count,
            "{label} exposed a solver-only constraint"
        );
        let objective = solution.primal_objective.unwrap().get();
        assert!(
            (objective - expected_objective).abs() <= 1e-4,
            "{label} objective {objective}, expected {expected_objective}"
        );
    }

    for (label, model, status) in [
        (
            "infeasible empty row",
            infeasible,
            ExecutorModelStatus::Infeasible,
        ),
        (
            "unbounded LP",
            unbounded,
            // The selected PSLP presolver preserves this ambiguous terminal result.
            ExecutorModelStatus::InfeasibleOrUnbounded,
        ),
    ] {
        let request = veoveo_optimization_mcp::executor::ExecutorRequest::new(
            RunId::new(),
            profile(),
            ExecutorOperation::SolveModel {
                family: ExecutorModelFamily::Convex,
                model,
            },
        );
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            client.execute(&request, CancellationToken::new()),
        )
        .await
        .expect("executor exceeded 30 seconds")
        .unwrap();
        let ExecutorResult::Model { solution } = response.result else {
            panic!("{label} executor returned {response:?}");
        };
        assert_eq!(solution.status, status, "{label}");
    }
}

fn quadratically_constrained_program() -> CompiledMathematicalModel {
    let mut model = mathematical_model(VariableKind::Continuous);
    model
        .constraint_ids
        .push(ConstraintId::parse("unit-circle").unwrap());
    model
        .quadratic_constraints
        .push(CompiledQuadraticConstraint {
            constraint_id: ConstraintId::parse("unit-circle").unwrap(),
            linear_indices: vec![],
            linear_values: vec![],
            rows: vec![0, 1],
            columns: vec![0, 1],
            values: vec![finite(1.0), finite(1.0)],
            sense: QuadraticConstraintSense::LessThanOrEqual,
            rhs: finite(1.0),
        });
    model
}

fn second_order_cone_program() -> CompiledMathematicalModel {
    CompiledMathematicalModel {
        variable_ids: ["x", "y", "t"]
            .into_iter()
            .map(|value| VariableId::parse(value).unwrap())
            .collect(),
        variable_kinds: vec![VariableKind::Continuous; 3],
        variable_lower_bounds: vec![Some(finite(0.0)), Some(finite(0.0)), Some(finite(0.0))],
        variable_upper_bounds: vec![None; 3],
        objective_direction: ObjectiveDirection::Minimize,
        objective_offset: finite(0.0),
        objective_coefficients: vec![finite(0.0), finite(0.0), finite(1.0)],
        constraint_ids: vec![
            ConstraintId::parse("minimum").unwrap(),
            ConstraintId::parse("cone").unwrap(),
        ],
        constraint_matrix: CsrMatrix {
            rows: 1,
            columns: 3,
            offsets: vec![0, 2],
            indices: vec![0, 1],
            values: vec![finite(1.0), finite(1.0)],
        },
        constraint_lower_bounds: vec![Some(finite(2.0))],
        constraint_upper_bounds: vec![None],
        quadratic_objective: None,
        quadratic_constraints: vec![CompiledQuadraticConstraint {
            constraint_id: ConstraintId::parse("cone").unwrap(),
            linear_indices: vec![],
            linear_values: vec![],
            rows: vec![0, 1, 2],
            columns: vec![0, 1, 2],
            values: vec![finite(1.0), finite(1.0), finite(-1.0)],
            sense: QuadraticConstraintSense::LessThanOrEqual,
            rhs: finite(0.0),
        }],
        initial_primal_solution: None,
        initial_dual_solution: None,
    }
}

#[tokio::test]
#[ignore = "requires the pinned cuOpt image and one NVIDIA GPU"]
async fn solves_every_public_family_on_the_gpu() {
    let socket = env::var("VEOVEO_CUOPT_TEST_SOCKET")
        .expect("VEOVEO_CUOPT_TEST_SOCKET must identify the executor socket");
    let client = ExecutorClient::with_default_limit(socket);

    let response = client.health().await.unwrap();
    let ExecutorResult::Health { health } = response.result else {
        panic!("executor returned a non-health response");
    };
    assert!(health.ready);
    assert!(health.cuopt_version.starts_with("26.08"));
    assert!(!health.gpu_uuid.is_empty());

    let routing = veoveo_optimization_mcp::executor::ExecutorRequest::new(
        RunId::new(),
        profile(),
        ExecutorOperation::SolveRoutes {
            problem: routing_problem(),
        },
    );
    let response = client
        .execute(&routing, CancellationToken::new())
        .await
        .unwrap();
    let ExecutorResult::Routes { solution } = response.result else {
        panic!("executor returned a non-routing response");
    };
    assert_eq!(solution.status, ExecutorRoutingStatus::Success);
    assert_eq!(solution.vehicles_used, 1);
    assert!(!solution.routes.is_empty());

    for (label, family, model, expected_objective, tolerance) in [
        (
            "LP",
            ExecutorModelFamily::Convex,
            mathematical_model(VariableKind::Continuous),
            1.0,
            1e-4,
        ),
        (
            "QP",
            ExecutorModelFamily::Convex,
            quadratic_program(),
            0.5,
            1e-5,
        ),
        (
            "QCQP",
            ExecutorModelFamily::Convex,
            quadratically_constrained_program(),
            1.0,
            1e-5,
        ),
        (
            "SOCP",
            ExecutorModelFamily::Convex,
            second_order_cone_program(),
            std::f64::consts::SQRT_2,
            1e-5,
        ),
        (
            "MILP",
            ExecutorModelFamily::Milp,
            mathematical_model(VariableKind::Integer),
            1.0,
            1e-4,
        ),
    ] {
        let request = veoveo_optimization_mcp::executor::ExecutorRequest::new(
            RunId::new(),
            profile(),
            ExecutorOperation::SolveModel { family, model },
        );
        let response = client
            .execute(&request, CancellationToken::new())
            .await
            .unwrap_or_else(|error| panic!("{label} executor request failed: {error:?}"));
        let ExecutorResult::Model { solution } = response.result else {
            panic!("{label} executor returned a non-model response");
        };
        assert_eq!(
            solution.family,
            match family {
                ExecutorModelFamily::Convex => ProblemFamily::Convex,
                ExecutorModelFamily::Milp => ProblemFamily::Milp,
            }
        );
        assert!(!solution.primal_solution.is_empty(), "{label}");
        let objective = solution.primal_objective.unwrap().get();
        assert!(
            (objective - expected_objective).abs() <= tolerance,
            "{label} objective {objective}, expected {expected_objective}"
        );
    }
}
