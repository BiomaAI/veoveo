use std::env;

use tokio_util::sync::CancellationToken;
use veoveo_optimization_mcp::{
    domain::{
        ConstraintId, FiniteF64, LocationId, NonNegativeF64, ObjectiveDirection, OrderId,
        ProblemFamily, RouteNodeKind, RouteObjectiveMetric, RunId, VariableId, VariableKind,
        VehicleId,
    },
    executor::{
        CompiledDenseMatrix, CompiledMathematicalModel, CompiledRouteNode, CompiledRouteObjective,
        CompiledRoutingProblem, CompiledVehicle, ConvexMethod, ConvexSolverSettings, CsrMatrix,
        ExecutorClient, ExecutorModelFamily, ExecutorOperation, ExecutorProfile, ExecutorResult,
        ExecutorRoutingStatus, MilpSolverSettings, RoutingSolverSettings,
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
            .map(|value| LocationId::new(value).unwrap())
            .collect(),
        nodes: vec![
            CompiledRouteNode {
                order_id: OrderId::new("order-one").unwrap(),
                location_id: LocationId::new("one").unwrap(),
                location_index: 1,
                kind: RouteNodeKind::Service,
                service_duration: 0,
                earliest: 0,
                latest: 100,
                prize: 0.0,
            },
            CompiledRouteNode {
                order_id: OrderId::new("order-two").unwrap(),
                location_id: LocationId::new("two").unwrap(),
                location_index: 2,
                kind: RouteNodeKind::Service,
                service_duration: 0,
                earliest: 0,
                latest: 100,
                prize: 0.0,
            },
        ],
        vehicles: vec![CompiledVehicle {
            vehicle_id: VehicleId::new("vehicle-one").unwrap(),
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
    }
}

fn mathematical_model(variable_kind: VariableKind) -> CompiledMathematicalModel {
    CompiledMathematicalModel {
        variable_ids: ["x", "y"]
            .into_iter()
            .map(|value| VariableId::new(value).unwrap())
            .collect(),
        variable_kinds: vec![variable_kind; 2],
        variable_lower_bounds: vec![Some(finite(0.0)), Some(finite(0.0))],
        variable_upper_bounds: vec![None, None],
        objective_direction: ObjectiveDirection::Minimize,
        objective_offset: finite(0.0),
        objective_coefficients: vec![finite(1.0), finite(1.0)],
        constraint_ids: vec![ConstraintId::new("minimum").unwrap()],
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

#[tokio::test]
#[ignore = "requires the pinned cuOpt image and one NVIDIA GPU"]
async fn solves_routing_convex_and_milp_on_the_gpu() {
    let socket = env::var("VEOVEO_CUOPT_TEST_SOCKET")
        .expect("VEOVEO_CUOPT_TEST_SOCKET must identify the executor socket");
    let client = ExecutorClient::with_default_limit(socket);

    let response = client.health().await.unwrap();
    let ExecutorResult::Health { health } = response.result else {
        panic!("executor returned a non-health response");
    };
    assert!(health.ready);
    assert!(health.cuopt_version.starts_with("26.06"));
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

    for (family, variable_kind) in [
        (ExecutorModelFamily::Convex, VariableKind::Continuous),
        (ExecutorModelFamily::Milp, VariableKind::Integer),
    ] {
        let request = veoveo_optimization_mcp::executor::ExecutorRequest::new(
            RunId::new(),
            profile(),
            ExecutorOperation::SolveModel {
                family,
                model: mathematical_model(variable_kind),
            },
        );
        let response = client
            .execute(&request, CancellationToken::new())
            .await
            .unwrap();
        let ExecutorResult::Model { solution } = response.result else {
            panic!("executor returned a non-model response");
        };
        assert_eq!(
            solution.family,
            match family {
                ExecutorModelFamily::Convex => ProblemFamily::Convex,
                ExecutorModelFamily::Milp => ProblemFamily::Milp,
            }
        );
        assert_eq!(solution.primal_solution.len(), 2);
        let objective = solution.primal_objective.unwrap().get();
        assert!((objective - 1.0).abs() <= 1e-4, "{objective}");
    }
}
