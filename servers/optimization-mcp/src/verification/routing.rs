use std::collections::{BTreeMap, BTreeSet};

use crate::{
    domain::{
        FiniteF64, NonNegativeF64, OrderId, RouteNodeKind, RouteStopResult, VehicleId,
        VehicleRoute, VerificationCode, VerificationFinding, VerificationReport,
        VerificationSeverity,
    },
    executor::{
        CompiledDenseMatrix, CompiledRoutingProblem, ExecutorRouteNode, ExecutorRoutingSolution,
    },
};

use super::{VerificationTolerance, report};

#[derive(Debug, Clone, PartialEq)]
pub struct RoutingVerification {
    pub report: VerificationReport,
    pub routes: Vec<VehicleRoute>,
    pub served_orders: BTreeSet<OrderId>,
    pub dropped_orders: BTreeSet<OrderId>,
    pub independently_calculated_cost: FiniteF64,
}

pub fn verify_routing_solution(
    problem: &CompiledRoutingProblem,
    solution: &ExecutorRoutingSolution,
    tolerance: VerificationTolerance,
) -> RoutingVerification {
    let mut findings = Vec::new();
    let mut seen_vehicles = BTreeSet::new();
    let mut seen_nodes = BTreeMap::<u32, (u32, usize)>::new();
    let mut routes = Vec::new();
    let mut total_cost = 0.0;

    for route in &solution.routes {
        let Ok(vehicle_index) = usize::try_from(route.vehicle) else {
            continue;
        };
        let Some(vehicle) = problem.vehicles.get(vehicle_index) else {
            findings.push(route_finding(
                VerificationCode::UnknownVehicle,
                format!("route references unknown vehicle index {}", route.vehicle),
                None,
                None,
            ));
            continue;
        };
        if !seen_vehicles.insert(route.vehicle) {
            findings.push(route_finding(
                VerificationCode::DuplicateVehicleRoute,
                format!(
                    "vehicle {} appears in more than one route",
                    vehicle.vehicle_id
                ),
                None,
                Some(vehicle.vehicle_id.clone()),
            ));
        }
        let matrix = matrix_for_vehicle(problem, vehicle.vehicle_type, false);
        let time_matrix = matrix_for_vehicle(problem, vehicle.vehicle_type, true).or(matrix);
        if let Some(first) = route.nodes.first()
            && !matches!(
                first.node,
                ExecutorRouteNode::Depot { location } if location == vehicle.start_location
            )
        {
            findings.push(route_finding(
                VerificationCode::InvalidRouteEndpoint,
                format!(
                    "vehicle {} route does not begin at start location {}",
                    vehicle.vehicle_id, vehicle.start_location
                ),
                None,
                Some(vehicle.vehicle_id.clone()),
            ));
        }
        if !vehicle.omit_last_trip
            && let Some(last) = route.nodes.last()
            && !matches!(
                last.node,
                ExecutorRouteNode::Depot { location } if location == vehicle.end_location
            )
        {
            findings.push(route_finding(
                VerificationCode::InvalidRouteEndpoint,
                format!(
                    "vehicle {} route does not end at return location {}",
                    vehicle.vehicle_id, vehicle.end_location
                ),
                None,
                Some(vehicle.vehicle_id.clone()),
            ));
        }
        let mut route_cost = 0.0;
        let mut previous_location = vehicle.start_location;
        let mut previous_departure = vehicle.earliest as f64;
        let mut loads = problem
            .capacity_dimensions
            .iter()
            .map(|dimension| (dimension.dimension_id.clone(), 0_i64))
            .collect::<BTreeMap<_, _>>();
        let mut output_stops = Vec::new();

        for (sequence, visit) in route.nodes.iter().enumerate() {
            let (location, source_node, node_kind, order_id, service_duration) = match visit.node {
                ExecutorRouteNode::Depot { location } => {
                    (location, None, RouteNodeKind::Depot, None, 0)
                }
                ExecutorRouteNode::Break { location } => {
                    (location, None, RouteNodeKind::Break, None, 0)
                }
                ExecutorRouteNode::Order { node } => {
                    let Some(compiled) = problem.nodes.get(node as usize) else {
                        findings.push(route_finding(
                            VerificationCode::UnknownRouteNode,
                            format!("route references unknown order-node index {node}"),
                            None,
                            Some(vehicle.vehicle_id.clone()),
                        ));
                        continue;
                    };
                    if let Some((prior_vehicle, prior_sequence)) =
                        seen_nodes.insert(node, (route.vehicle, sequence))
                    {
                        findings.push(route_finding(
                                VerificationCode::DuplicateRouteNode,
                                format!(
                                    "order node {node} was already served by vehicle index {prior_vehicle} at sequence {prior_sequence}"
                                ),
                                Some(compiled.order_id.clone()),
                                Some(vehicle.vehicle_id.clone()),
                            ));
                    }
                    check_order_vehicle_match(
                        problem,
                        node,
                        route.vehicle,
                        compiled.order_id.clone(),
                        vehicle.vehicle_id.clone(),
                        &mut findings,
                    );
                    if visit.arrival.get() + tolerance.allowed(compiled.earliest as f64)
                        < compiled.earliest as f64
                        || visit.arrival.get()
                            > compiled.latest as f64 + tolerance.allowed(compiled.latest as f64)
                    {
                        findings.push(route_finding(
                            VerificationCode::OrderTimeWindow,
                            format!(
                                "order {} arrival {} is outside [{}, {}]",
                                compiled.order_id,
                                visit.arrival.get(),
                                compiled.earliest,
                                compiled.latest
                            ),
                            Some(compiled.order_id.clone()),
                            Some(vehicle.vehicle_id.clone()),
                        ));
                    }
                    for dimension in &problem.capacity_dimensions {
                        let demand = dimension.demand[node as usize] as i64;
                        let load = loads
                            .get_mut(&dimension.dimension_id)
                            .expect("initialized capacity dimension");
                        *load += demand;
                        let capacity = dimension.capacity[vehicle_index] as i64;
                        if *load < 0 || *load > capacity {
                            findings.push(route_finding(
                                VerificationCode::VehicleCapacity,
                                format!(
                                    "vehicle {} load {} for {} is outside [0, {capacity}]",
                                    vehicle.vehicle_id, *load, dimension.dimension_id
                                ),
                                Some(compiled.order_id.clone()),
                                Some(vehicle.vehicle_id.clone()),
                            ));
                        }
                    }
                    (
                        compiled.location_index,
                        Some(node),
                        compiled.kind,
                        Some(compiled.order_id.clone()),
                        compiled.service_duration,
                    )
                }
            };

            let skip_first = sequence == 0 && vehicle.omit_first_trip && source_node.is_some();
            let travel_cost = if skip_first {
                0.0
            } else {
                matrix_value(matrix, previous_location, location, &mut findings, vehicle)
            };
            let travel_time = if skip_first {
                0.0
            } else {
                matrix_value(
                    time_matrix,
                    previous_location,
                    location,
                    &mut findings,
                    vehicle,
                )
            };
            let earliest_arrival = previous_departure + travel_time;
            if visit.arrival.get() + tolerance.allowed(earliest_arrival) < earliest_arrival {
                findings.push(route_finding(
                    VerificationCode::ArrivalSequence,
                    format!(
                        "vehicle {} arrival {} precedes independently calculated earliest arrival {earliest_arrival}",
                        vehicle.vehicle_id,
                        visit.arrival.get()
                    ),
                    order_id.clone(),
                    Some(vehicle.vehicle_id.clone()),
                ));
            }
            route_cost += travel_cost;
            previous_location = location;
            previous_departure = visit.arrival.get() + service_duration as f64;
            output_stops.push(RouteStopResult {
                sequence: sequence as u32,
                order_id,
                location_id: problem.location_ids[location as usize].clone(),
                node_kind,
                arrival: visit.arrival,
                departure: NonNegativeF64::new(previous_departure)
                    .expect("route departure is non-negative and finite"),
                cumulative_cost: NonNegativeF64::new(route_cost)
                    .expect("route cost is non-negative and finite"),
                load: loads.clone(),
            });
        }

        if !vehicle.omit_last_trip {
            route_cost += matrix_value(
                matrix,
                previous_location,
                vehicle.end_location,
                &mut findings,
                vehicle,
            );
            previous_departure += matrix_value(
                time_matrix,
                previous_location,
                vehicle.end_location,
                &mut findings,
                vehicle,
            );
        }
        if previous_departure > vehicle.latest as f64 + tolerance.allowed(vehicle.latest as f64) {
            findings.push(route_finding(
                VerificationCode::VehicleTimeWindow,
                format!(
                    "vehicle {} completes at {previous_departure} after latest time {}",
                    vehicle.vehicle_id, vehicle.latest
                ),
                None,
                Some(vehicle.vehicle_id.clone()),
            ));
        }
        if vehicle
            .maximum_cost
            .is_some_and(|maximum| route_cost > maximum as f64 + tolerance.allowed(maximum as f64))
        {
            findings.push(route_finding(
                VerificationCode::VehicleMaximumCost,
                format!(
                    "vehicle {} route cost {route_cost} exceeds maximum {}",
                    vehicle.vehicle_id,
                    vehicle.maximum_cost.expect("checked maximum cost")
                ),
                None,
                Some(vehicle.vehicle_id.clone()),
            ));
        }
        if vehicle.maximum_time.is_some_and(|maximum| {
            let elapsed = previous_departure - vehicle.earliest as f64;
            elapsed > maximum as f64 + tolerance.allowed(maximum as f64)
        }) {
            findings.push(route_finding(
                VerificationCode::VehicleMaximumTime,
                format!(
                    "vehicle {} route duration exceeds maximum {}",
                    vehicle.vehicle_id,
                    vehicle.maximum_time.expect("checked maximum time")
                ),
                None,
                Some(vehicle.vehicle_id.clone()),
            ));
        }
        total_cost += route_cost + vehicle.fixed_cost as f64;
        routes.push(VehicleRoute {
            case_id: None,
            vehicle_id: vehicle.vehicle_id.clone(),
            stops: output_stops,
            objective: FiniteF64::new(route_cost + vehicle.fixed_cost as f64)
                .expect("route objective is finite"),
        });
    }

    check_pickup_delivery(problem, &seen_nodes, &mut findings);
    let served_orders = seen_nodes
        .keys()
        .filter_map(|node| problem.nodes.get(*node as usize))
        .map(|node| node.order_id.clone())
        .collect::<BTreeSet<_>>();
    let all_orders = problem
        .nodes
        .iter()
        .map(|node| node.order_id.clone())
        .collect::<BTreeSet<_>>();
    let dropped_orders = all_orders
        .difference(&served_orders)
        .cloned()
        .collect::<BTreeSet<_>>();
    for order in &dropped_orders {
        let nodes = problem
            .nodes
            .iter()
            .filter(|node| &node.order_id == order)
            .collect::<Vec<_>>();
        if nodes.iter().all(|node| node.prize == 0.0) {
            findings.push(route_finding(
                VerificationCode::MissingMandatoryOrder,
                format!("mandatory order {order} is not served"),
                Some(order.clone()),
                None,
            ));
        }
    }

    let independent_cost = FiniteF64::new(total_cost).expect("total route cost is finite");
    let report = report(findings, tolerance, None, None, None);
    RoutingVerification {
        report,
        routes,
        served_orders,
        dropped_orders,
        independently_calculated_cost: independent_cost,
    }
}

fn matrix_for_vehicle(
    problem: &CompiledRoutingProblem,
    vehicle_type: u8,
    transit_time: bool,
) -> Option<&CompiledDenseMatrix> {
    let matrices = if transit_time {
        &problem.transit_time_matrices
    } else {
        &problem.cost_matrices
    };
    matrices
        .iter()
        .find(|matrix| matrix.vehicle_type == vehicle_type)
}

fn matrix_value(
    matrix: Option<&CompiledDenseMatrix>,
    from: u32,
    to: u32,
    findings: &mut Vec<VerificationFinding>,
    vehicle: &crate::executor::CompiledVehicle,
) -> f64 {
    let Some(matrix) = matrix else {
        return 0.0;
    };
    let index = from as usize * matrix.dimension as usize + to as usize;
    if matrix.unavailable_cells.contains(&(index as u32)) {
        findings.push(route_finding(
            VerificationCode::UnavailableTravelArc,
            format!(
                "vehicle {} traverses unavailable matrix cell {index}",
                vehicle.vehicle_id
            ),
            None,
            Some(vehicle.vehicle_id.clone()),
        ));
    }
    matrix.values[index] as f64
}

fn check_order_vehicle_match(
    problem: &CompiledRoutingProblem,
    node: u32,
    vehicle: u32,
    order_id: OrderId,
    vehicle_id: VehicleId,
    findings: &mut Vec<VerificationFinding>,
) {
    if let Some(restriction) = problem
        .order_vehicle_matches
        .iter()
        .find(|restriction| restriction.node == node)
        && !restriction.vehicles.contains(&vehicle)
    {
        findings.push(route_finding(
            VerificationCode::VehicleOrderRestriction,
            format!("order {order_id} cannot be served by vehicle {vehicle_id}"),
            Some(order_id),
            Some(vehicle_id),
        ));
    }
}

fn check_pickup_delivery(
    problem: &CompiledRoutingProblem,
    seen_nodes: &BTreeMap<u32, (u32, usize)>,
    findings: &mut Vec<VerificationFinding>,
) {
    for pair in &problem.pickup_delivery_pairs {
        match (
            seen_nodes.get(&pair.pickup_node),
            seen_nodes.get(&pair.delivery_node),
        ) {
            (
                Some((pickup_vehicle, pickup_sequence)),
                Some((delivery_vehicle, delivery_sequence)),
            ) if pickup_vehicle == delivery_vehicle && pickup_sequence < delivery_sequence => {}
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => {
                let order = problem.nodes[pair.pickup_node as usize].order_id.clone();
                findings.push(route_finding(
                    VerificationCode::PartialPickupDelivery,
                    format!("pickup-delivery order {order} is only partially served"),
                    Some(order),
                    None,
                ));
            }
            _ => {
                let order = problem.nodes[pair.pickup_node as usize].order_id.clone();
                findings.push(route_finding(
                    VerificationCode::PickupDeliveryPrecedence,
                    format!(
                        "pickup-delivery order {order} is not served by one vehicle in pickup-before-delivery order"
                    ),
                    Some(order),
                    None,
                ));
            }
        }
    }
}

fn route_finding(
    code: VerificationCode,
    message: String,
    order_id: Option<OrderId>,
    vehicle_id: Option<VehicleId>,
) -> VerificationFinding {
    VerificationFinding {
        code,
        severity: VerificationSeverity::Error,
        message,
        variable_id: None,
        constraint_id: None,
        order_id,
        vehicle_id,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        domain::{LocationId, OrderId, RouteObjectiveMetric, VehicleId},
        executor::{
            CompiledRouteNode, CompiledRouteObjective, CompiledVehicle, ExecutorRouteVisit,
            ExecutorRoutingStatus, ExecutorVehicleRoute,
        },
    };

    use super::*;

    fn id<T>(value: &str) -> T
    where
        T: TryFrom<String>,
        T::Error: std::fmt::Debug,
    {
        value.to_owned().try_into().unwrap()
    }

    #[test]
    fn verifier_rejects_a_missing_mandatory_order() {
        let problem = CompiledRoutingProblem {
            location_ids: vec![id::<LocationId>("depot"), id::<LocationId>("customer")],
            nodes: vec![CompiledRouteNode {
                order_id: id::<OrderId>("order"),
                location_id: id("customer"),
                location_index: 1,
                kind: RouteNodeKind::Service,
                service_duration: 0,
                earliest: 0,
                latest: 100,
                prize: 0.0,
            }],
            vehicles: vec![CompiledVehicle {
                vehicle_id: id::<VehicleId>("vehicle"),
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
            vehicle_type_ids: vec!["van".to_owned()],
            cost_matrices: vec![CompiledDenseMatrix {
                vehicle_type: 0,
                dimension: 2,
                values: vec![0.0, 1.0, 1.0, 0.0],
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
        };
        let solution = ExecutorRoutingSolution {
            status: ExecutorRoutingStatus::Success,
            message: String::new(),
            objective: FiniteF64::default(),
            objective_components: BTreeMap::new(),
            vehicles_used: 1,
            routes: vec![ExecutorVehicleRoute {
                vehicle: 0,
                nodes: vec![ExecutorRouteVisit {
                    node: ExecutorRouteNode::Depot { location: 0 },
                    arrival: NonNegativeF64::default(),
                }],
            }],
            undeliverable_nodes: vec![0],
            solve_seconds: NonNegativeF64::default(),
        };

        let verified =
            verify_routing_solution(&problem, &solution, VerificationTolerance::default());
        assert!(!verified.report.verified);
        assert!(verified.dropped_orders.contains(&id("order")));
    }
}
