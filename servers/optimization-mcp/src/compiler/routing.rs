use std::collections::{BTreeMap, BTreeSet};

use crate::{
    domain::{
        DenseTravelMatrix, LocationId, OptimizationSolution, RouteNodeKind, RouteObjectiveMetric,
        RouteOrderKind, RouteServicePolicy, RoutingProblem, SolutionDetail, TravelModelSource,
        VehicleTypeId,
    },
    executor::{
        CompiledCapacityDimension, CompiledDenseMatrix, CompiledInitialRouteNodeKind,
        CompiledInitialRoutingSolution, CompiledOrderVehicleMatch, CompiledPickupDeliveryPair,
        CompiledRouteNode, CompiledRouteObjective, CompiledRoutingProblem, CompiledVehicle,
        CompiledVehicleBreak,
    },
};

use super::CompileError;

const UNBOUNDED_TIME: u32 = i32::MAX as u32;

pub fn compile_routing_problem(
    problem: &RoutingProblem,
) -> Result<CompiledRoutingProblem, CompileError> {
    problem
        .validate()
        .map_err(|error| CompileError::InvalidProblem(error.to_string()))?;
    let TravelModelSource::Inline { model } = &problem.travel_model else {
        return Err(CompileError::UnmaterializedInput("routing travel model"));
    };

    let location_indices = index_locations(&model.location_ids)?;
    let (vehicle_type_ids, vehicle_type_indices) = index_vehicle_types(problem)?;
    let vehicles = compile_vehicles(problem, &location_indices, &vehicle_type_indices)?;
    let (nodes, pairs, order_vehicle_matches) =
        compile_nodes(problem, &location_indices, &vehicles)?;
    let capacity_dimensions = compile_capacity_dimensions(problem, &nodes, &vehicles);

    Ok(CompiledRoutingProblem {
        location_ids: model.location_ids.clone(),
        nodes,
        vehicles,
        vehicle_type_ids,
        cost_matrices: compile_matrices(
            &model.cost_matrices,
            &vehicle_type_indices,
            "cost matrix",
        )?,
        transit_time_matrices: compile_matrices(
            &model.transit_time_matrices,
            &vehicle_type_indices,
            "transit-time matrix",
        )?,
        capacity_dimensions,
        pickup_delivery_pairs: pairs,
        order_vehicle_matches,
        objectives: problem
            .objectives
            .iter()
            .map(|objective| {
                Ok(CompiledRouteObjective {
                    metric: objective.metric,
                    weight: f32_value(objective.weight.get(), "objective weight")?,
                })
            })
            .collect::<Result<_, CompileError>>()?,
        minimum_vehicles: problem
            .fleet
            .exact_vehicles
            .unwrap_or(problem.fleet.minimum_vehicles),
        initial_solution: None,
    })
}

pub fn compile_routing_initial_solution(
    problem: &CompiledRoutingProblem,
    solution: &OptimizationSolution,
) -> Result<CompiledInitialRoutingSolution, CompileError> {
    let SolutionDetail::Routing { routes, .. } = &solution.detail else {
        return Err(CompileError::InvalidProblem(
            "routing warm start must reference a routing solution".to_owned(),
        ));
    };
    if routes.iter().any(|route| route.case_id.is_some()) {
        return Err(CompileError::InvalidProblem(
            "route-scenario solutions cannot seed a single routing case".to_owned(),
        ));
    }
    let vehicle_indices = problem
        .vehicles
        .iter()
        .enumerate()
        .map(|(index, vehicle)| (vehicle.vehicle_id.clone(), index as u32))
        .collect::<BTreeMap<_, _>>();
    let mut vehicles = Vec::new();
    let mut route_nodes = Vec::new();
    let mut node_kinds = Vec::new();
    for route in routes {
        let vehicle = vehicle_indices
            .get(&route.vehicle_id)
            .copied()
            .ok_or_else(|| {
                CompileError::InvalidProblem(format!(
                    "warm start references unknown vehicle {}",
                    route.vehicle_id
                ))
            })?;
        for stop in &route.stops {
            let (node, kind) = match stop.node_kind {
                RouteNodeKind::Depot => (0, CompiledInitialRouteNodeKind::Depot),
                RouteNodeKind::Break => (0, CompiledInitialRouteNodeKind::Break),
                RouteNodeKind::Service | RouteNodeKind::Delivery | RouteNodeKind::Pickup => {
                    let order_id = stop.order_id.as_ref().ok_or_else(|| {
                        CompileError::InvalidProblem(
                            "warm-start order stop omits order_id".to_owned(),
                        )
                    })?;
                    let expected_kind = stop.node_kind;
                    let node = problem
                        .nodes
                        .iter()
                        .position(|candidate| {
                            &candidate.order_id == order_id && candidate.kind == expected_kind
                        })
                        .or_else(|| {
                            (expected_kind == RouteNodeKind::Service).then(|| {
                                problem
                                    .nodes
                                    .iter()
                                    .position(|candidate| &candidate.order_id == order_id)
                            })?
                        })
                        .ok_or_else(|| {
                            CompileError::InvalidProblem(format!(
                                "warm start references unknown order stop {order_id}"
                            ))
                        })? as u32;
                    let kind = match stop.node_kind {
                        RouteNodeKind::Pickup => CompiledInitialRouteNodeKind::Pickup,
                        RouteNodeKind::Service | RouteNodeKind::Delivery => {
                            CompiledInitialRouteNodeKind::Delivery
                        }
                        RouteNodeKind::Depot | RouteNodeKind::Break => unreachable!(),
                    };
                    (node, kind)
                }
            };
            vehicles.push(vehicle);
            route_nodes.push(node);
            node_kinds.push(kind);
        }
    }
    if route_nodes.is_empty() {
        return Err(CompileError::InvalidProblem(
            "routing warm start contains no route stops".to_owned(),
        ));
    }
    let solution_length = route_nodes.len() as u32;
    Ok(CompiledInitialRoutingSolution {
        vehicle_indices: vehicles,
        route_nodes,
        node_kinds,
        solution_offsets: vec![0, solution_length],
    })
}

fn index_locations(locations: &[LocationId]) -> Result<BTreeMap<LocationId, u32>, CompileError> {
    locations
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let index = u32::try_from(index).map_err(|_| CompileError::NumericRange {
                field: "location index",
                value: index.to_string(),
                target: "uint32",
            })?;
            Ok((id.clone(), index))
        })
        .collect()
}

fn index_vehicle_types(
    problem: &RoutingProblem,
) -> Result<(Vec<String>, BTreeMap<VehicleTypeId, u8>), CompileError> {
    let distinct = problem
        .fleet
        .vehicles
        .iter()
        .map(|vehicle| vehicle.vehicle_type_id.clone())
        .collect::<BTreeSet<_>>();
    if distinct.len() > u8::MAX as usize + 1 {
        return Err(CompileError::InvalidProblem(
            "cuOpt supports at most 256 routing vehicle types".to_owned(),
        ));
    }
    let mut ids = Vec::with_capacity(distinct.len());
    let mut indices = BTreeMap::new();
    for (index, id) in distinct.into_iter().enumerate() {
        let index = index as u8;
        ids.push(id.to_string());
        indices.insert(id, index);
    }
    Ok((ids, indices))
}

fn compile_vehicles(
    problem: &RoutingProblem,
    locations: &BTreeMap<LocationId, u32>,
    vehicle_types: &BTreeMap<VehicleTypeId, u8>,
) -> Result<Vec<CompiledVehicle>, CompileError> {
    problem
        .fleet
        .vehicles
        .iter()
        .map(|vehicle| {
            let time_window = vehicle.time_window.unwrap_or(crate::domain::TimeWindow {
                earliest: 0,
                latest: UNBOUNDED_TIME,
            });
            Ok(CompiledVehicle {
                vehicle_id: vehicle.vehicle_id.clone(),
                vehicle_type: vehicle_types[&vehicle.vehicle_type_id],
                start_location: locations[&vehicle.start_location_id],
                end_location: locations[&vehicle.end_location_id],
                earliest: time_window.earliest,
                latest: time_window.latest,
                fixed_cost: f32_value(vehicle.fixed_cost.get(), "vehicle fixed cost")?,
                maximum_cost: vehicle
                    .maximum_cost
                    .map(|value| f32_value(value.get(), "vehicle maximum cost"))
                    .transpose()?,
                maximum_time: vehicle
                    .maximum_time
                    .map(|value| f32_value(value.get(), "vehicle maximum time"))
                    .transpose()?,
                omit_first_trip: vehicle.omit_first_trip,
                omit_last_trip: vehicle.omit_last_trip,
                breaks: vehicle
                    .breaks
                    .iter()
                    .map(|vehicle_break| {
                        Ok(CompiledVehicleBreak {
                            earliest: vehicle_break.time_window.earliest,
                            latest: vehicle_break.time_window.latest,
                            duration: vehicle_break.duration,
                            allowed_locations: vehicle_break
                                .allowed_location_ids
                                .iter()
                                .map(|location| locations[location])
                                .collect(),
                        })
                    })
                    .collect::<Result<_, CompileError>>()?,
            })
        })
        .collect()
}

fn compile_nodes(
    problem: &RoutingProblem,
    locations: &BTreeMap<LocationId, u32>,
    vehicles: &[CompiledVehicle],
) -> Result<
    (
        Vec<CompiledRouteNode>,
        Vec<CompiledPickupDeliveryPair>,
        Vec<CompiledOrderVehicleMatch>,
    ),
    CompileError,
> {
    let vehicle_indices = vehicles
        .iter()
        .enumerate()
        .map(|(index, vehicle)| (vehicle.vehicle_id.clone(), index as u32))
        .collect::<BTreeMap<_, _>>();
    let prize_weight = problem
        .objectives
        .iter()
        .find(|objective| objective.metric == RouteObjectiveMetric::Prize)
        .map(|objective| objective.weight.get())
        .unwrap_or(0.0);

    let mut nodes = Vec::new();
    let mut pairs = Vec::new();
    let mut matches = Vec::new();
    for order in &problem.orders {
        let prize = match order.service_policy {
            RouteServicePolicy::Mandatory => 0.0,
            RouteServicePolicy::Optional => {
                if prize_weight <= 0.0 {
                    return Err(CompileError::InvalidProblem(format!(
                        "optional order {} requires a positive prize objective weight",
                        order.order_id
                    )));
                }
                f32_value(
                    order
                        .drop_penalty
                        .expect("validated optional order has a penalty")
                        .get()
                        / prize_weight,
                    "order prize",
                )?
            }
        };
        let first_node = nodes.len() as u32;
        match &order.order {
            RouteOrderKind::Service { stop } => nodes.push(compile_node(
                order.order_id.clone(),
                stop,
                RouteNodeKind::Service,
                prize,
                locations,
            )),
            RouteOrderKind::PickupDelivery { pickup, delivery } => {
                nodes.push(compile_node(
                    order.order_id.clone(),
                    pickup,
                    RouteNodeKind::Pickup,
                    prize,
                    locations,
                ));
                let delivery_node = nodes.len() as u32;
                nodes.push(compile_node(
                    order.order_id.clone(),
                    delivery,
                    RouteNodeKind::Delivery,
                    0.0,
                    locations,
                ));
                pairs.push(CompiledPickupDeliveryPair {
                    pickup_node: first_node,
                    delivery_node,
                });
            }
        }
        if !order.allowed_vehicle_ids.is_empty() {
            let allowed = order
                .allowed_vehicle_ids
                .iter()
                .map(|vehicle| vehicle_indices[vehicle])
                .collect::<Vec<_>>();
            let last_node = nodes.len() as u32;
            for node in first_node..last_node {
                matches.push(CompiledOrderVehicleMatch {
                    node,
                    vehicles: allowed.clone(),
                });
            }
        }
    }
    Ok((nodes, pairs, matches))
}

fn compile_node(
    order_id: crate::domain::OrderId,
    stop: &crate::domain::RouteStop,
    kind: RouteNodeKind,
    prize: f32,
    locations: &BTreeMap<LocationId, u32>,
) -> CompiledRouteNode {
    let window = stop.time_window.unwrap_or(crate::domain::TimeWindow {
        earliest: 0,
        latest: UNBOUNDED_TIME,
    });
    CompiledRouteNode {
        order_id,
        location_id: stop.location_id.clone(),
        location_index: locations[&stop.location_id],
        kind,
        service_duration: stop.service_duration,
        earliest: window.earliest,
        latest: window.latest,
        prize,
    }
}

fn compile_capacity_dimensions(
    problem: &RoutingProblem,
    nodes: &[CompiledRouteNode],
    vehicles: &[CompiledVehicle],
) -> Vec<CompiledCapacityDimension> {
    let dimensions = problem
        .orders
        .iter()
        .flat_map(|order| order.demand.keys().cloned())
        .chain(
            problem
                .fleet
                .vehicles
                .iter()
                .flat_map(|vehicle| vehicle.capacity.keys().cloned()),
        )
        .collect::<BTreeSet<_>>();
    dimensions
        .into_iter()
        .map(|dimension_id| {
            let demand = nodes
                .iter()
                .map(|node| {
                    let order = problem
                        .orders
                        .iter()
                        .find(|order| order.order_id == node.order_id)
                        .expect("compiled node references a source order");
                    let magnitude = order.demand.get(&dimension_id).copied().unwrap_or(0);
                    match node.kind {
                        RouteNodeKind::Delivery
                            if matches!(order.order, RouteOrderKind::PickupDelivery { .. }) =>
                        {
                            -magnitude
                        }
                        _ => magnitude,
                    }
                })
                .collect();
            let capacity = vehicles
                .iter()
                .map(|vehicle| {
                    let source = problem
                        .fleet
                        .vehicles
                        .iter()
                        .find(|candidate| candidate.vehicle_id == vehicle.vehicle_id)
                        .expect("compiled vehicle references a source vehicle");
                    source.capacity.get(&dimension_id).copied().unwrap_or(0)
                })
                .collect();
            CompiledCapacityDimension {
                dimension_id,
                demand,
                capacity,
            }
        })
        .collect()
}

fn compile_matrices(
    matrices: &[DenseTravelMatrix],
    vehicle_types: &BTreeMap<VehicleTypeId, u8>,
    field: &'static str,
) -> Result<Vec<CompiledDenseMatrix>, CompileError> {
    matrices
        .iter()
        .map(|matrix| {
            let vehicle_type = vehicle_types
                .get(&matrix.vehicle_type_id)
                .copied()
                .ok_or_else(|| {
                    CompileError::InvalidProblem(format!(
                        "{field} references unused vehicle type {}",
                        matrix.vehicle_type_id
                    ))
                })?;
            Ok(CompiledDenseMatrix {
                vehicle_type,
                dimension: matrix.dimension,
                values: matrix.values.clone(),
                unavailable_cells: matrix.unavailable_cells.clone(),
            })
        })
        .collect()
}

fn f32_value(value: f64, field: &'static str) -> Result<f32, CompileError> {
    if value > f32::MAX as f64 {
        return Err(CompileError::NumericRange {
            field,
            value: value.to_string(),
            target: "float32",
        });
    }
    Ok(value as f32)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use chrono::Utc;

    use crate::domain::{
        DenseTravelMatrix, FiniteF64, InlineTravelModel, LocationId, NonNegativeF64,
        ROUTING_PROBLEM_VERSION, RouteFleet, RouteLocation, RouteObjective, RouteOrder, RouteStop,
        RouteVehicle, TimeBasis, TimeUnit, TravelModelSource, VehicleTypeId,
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
    fn compiler_indexes_locations_and_vehicle_types_deterministically() {
        let depot: LocationId = id("depot");
        let customer: LocationId = id("customer");
        let van: VehicleTypeId = id("van");
        let problem = RoutingProblem {
            version: ROUTING_PROBLEM_VERSION.to_owned(),
            time_basis: TimeBasis {
                origin: Utc::now(),
                unit: TimeUnit::Second,
            },
            locations: vec![
                RouteLocation {
                    location_id: depot.clone(),
                    longitude_deg: Some(FiniteF64::new(0.0).unwrap()),
                    latitude_deg: Some(FiniteF64::new(0.0).unwrap()),
                },
                RouteLocation {
                    location_id: customer.clone(),
                    longitude_deg: Some(FiniteF64::new(1.0).unwrap()),
                    latitude_deg: Some(FiniteF64::new(1.0).unwrap()),
                },
            ],
            orders: vec![RouteOrder {
                order_id: id("order"),
                order: RouteOrderKind::Service {
                    stop: RouteStop {
                        location_id: customer,
                        time_window: None,
                        service_duration: 3,
                    },
                },
                demand: BTreeMap::new(),
                service_policy: RouteServicePolicy::Mandatory,
                drop_penalty: None,
                allowed_vehicle_ids: BTreeSet::new(),
            }],
            fleet: RouteFleet {
                vehicles: vec![RouteVehicle {
                    vehicle_id: id("vehicle"),
                    vehicle_type_id: van.clone(),
                    start_location_id: depot.clone(),
                    end_location_id: depot,
                    time_window: None,
                    breaks: vec![],
                    capacity: BTreeMap::new(),
                    fixed_cost: NonNegativeF64::default(),
                    maximum_cost: None,
                    maximum_time: None,
                    omit_first_trip: false,
                    omit_last_trip: false,
                }],
                minimum_vehicles: 0,
                exact_vehicles: None,
            },
            travel_model: TravelModelSource::Inline {
                model: InlineTravelModel {
                    location_ids: vec![id("depot"), id("customer")],
                    cost_matrices: vec![DenseTravelMatrix {
                        vehicle_type_id: van,
                        dimension: 2,
                        values: vec![0.0, 1.0, 1.0, 0.0],
                        unavailable_cells: vec![],
                    }],
                    transit_time_matrices: vec![],
                },
            },
            objectives: vec![RouteObjective {
                metric: RouteObjectiveMetric::Cost,
                weight: NonNegativeF64::new(1.0).unwrap(),
            }],
        };

        let compiled = compile_routing_problem(&problem).unwrap();
        assert_eq!(compiled.nodes[0].location_index, 1);
        assert_eq!(compiled.vehicles[0].vehicle_type, 0);
        assert_eq!(compiled.cost_matrices[0].values.len(), 4);
    }
}
