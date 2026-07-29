use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    ArtifactUri, CapacityDimensionId, FiniteF64, LocationId, MAX_CAPACITY_DIMENSIONS,
    MAX_INLINE_MATRIX_CELLS, MAX_OBJECTIVES, MAX_ROUTE_CASES, MapTravelModelUri, NonNegativeF64,
    OptimizationContractError, OptimizationProblemUri, OptimizationSolutionUri, OrderId,
    PositiveF64, ROUTING_PROBLEM_VERSION, RouteCaseId, SolverPolicyRef, TimeBasis, TimeWindow,
    VehicleId, VehicleTypeId, require_collection,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteLocation {
    pub location_id: LocationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longitude_deg: Option<FiniteF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latitude_deg: Option<FiniteF64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RouteStop {
    pub location_id: LocationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_window: Option<TimeWindow>,
    #[serde(default)]
    pub service_duration: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RouteOrderKind {
    Service {
        stop: RouteStop,
    },
    PickupDelivery {
        pickup: RouteStop,
        delivery: RouteStop,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RouteServicePolicy {
    Mandatory,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteOrder {
    pub order_id: OrderId,
    pub order: RouteOrderKind,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub demand: BTreeMap<CapacityDimensionId, i32>,
    pub service_policy: RouteServicePolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drop_penalty: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_vehicle_ids: BTreeSet<VehicleId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VehicleBreak {
    pub time_window: TimeWindow,
    pub duration: u32,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_location_ids: BTreeSet<LocationId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteVehicle {
    pub vehicle_id: VehicleId,
    pub vehicle_type_id: VehicleTypeId,
    pub start_location_id: LocationId,
    pub end_location_id: LocationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_window: Option<TimeWindow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<VehicleBreak>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capacity: BTreeMap<CapacityDimensionId, u32>,
    #[serde(default)]
    pub fixed_cost: NonNegativeF64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_cost: Option<PositiveF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_time: Option<PositiveF64>,
    #[serde(default)]
    pub omit_first_trip: bool,
    #[serde(default)]
    pub omit_last_trip: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteFleet {
    pub vehicles: Vec<RouteVehicle>,
    #[serde(default)]
    pub minimum_vehicles: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_vehicles: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DenseTravelMatrix {
    pub vehicle_type_id: VehicleTypeId,
    pub dimension: u32,
    pub values: Vec<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unavailable_cells: Vec<u32>,
}

impl DenseTravelMatrix {
    pub fn validate(&self, field: &'static str) -> Result<(), OptimizationContractError> {
        let dimension = usize::try_from(self.dimension).map_err(|_| {
            OptimizationContractError::InvalidProblem(format!("{field} dimension is too large"))
        })?;
        let cells = dimension.checked_mul(dimension).ok_or_else(|| {
            OptimizationContractError::InvalidProblem(format!("{field} dimension overflow"))
        })?;
        if cells == 0 || cells > MAX_INLINE_MATRIX_CELLS || self.values.len() != cells {
            return Err(OptimizationContractError::InvalidProblem(format!(
                "{field} must contain dimension² values and at most {MAX_INLINE_MATRIX_CELLS} cells"
            )));
        }
        if self
            .values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(OptimizationContractError::InvalidProblem(format!(
                "{field} values must be finite and non-negative"
            )));
        }
        if self
            .unavailable_cells
            .iter()
            .any(|index| *index as usize >= cells)
        {
            return Err(OptimizationContractError::InvalidProblem(format!(
                "{field} unavailable-cell index is outside the matrix"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InlineTravelModel {
    pub location_ids: Vec<LocationId>,
    pub cost_matrices: Vec<DenseTravelMatrix>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transit_time_matrices: Vec<DenseTravelMatrix>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TravelModelArtifact {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_resource_uri: Option<MapTravelModelUri>,
    pub model: InlineTravelModel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum TravelModelSource {
    MapResource {
        uri: MapTravelModelUri,
        manifest_uri: ArtifactUri,
    },
    Artifact {
        manifest_uri: ArtifactUri,
    },
    Inline {
        model: InlineTravelModel,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RouteObjectiveMetric {
    Cost,
    TravelTime,
    RouteSizeVariance,
    RouteServiceTimeVariance,
    Prize,
    VehicleFixedCost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteObjective {
    pub metric: RouteObjectiveMetric,
    pub weight: NonNegativeF64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoutingProblem {
    pub version: String,
    pub time_basis: TimeBasis,
    pub locations: Vec<RouteLocation>,
    pub orders: Vec<RouteOrder>,
    pub fleet: RouteFleet,
    pub travel_model: TravelModelSource,
    pub objectives: Vec<RouteObjective>,
}

impl RoutingProblem {
    pub fn validate(&self) -> Result<(), OptimizationContractError> {
        if self.version != ROUTING_PROBLEM_VERSION {
            return Err(OptimizationContractError::InvalidProblem(format!(
                "routing problem version must be {ROUTING_PROBLEM_VERSION}"
            )));
        }
        require_collection("locations", self.locations.len(), 1, u16::MAX as usize)?;
        require_collection("orders", self.orders.len(), 1, u16::MAX as usize)?;
        require_collection("vehicles", self.fleet.vehicles.len(), 1, u16::MAX as usize)?;
        require_collection("objectives", self.objectives.len(), 1, MAX_OBJECTIVES)?;

        let location_ids: BTreeSet<_> = self
            .locations
            .iter()
            .map(|location| &location.location_id)
            .collect();
        if location_ids.len() != self.locations.len() {
            return Err(OptimizationContractError::InvalidProblem(
                "location ids must be unique".to_owned(),
            ));
        }
        let order_ids: BTreeSet<_> = self.orders.iter().map(|order| &order.order_id).collect();
        if order_ids.len() != self.orders.len() {
            return Err(OptimizationContractError::InvalidProblem(
                "order ids must be unique".to_owned(),
            ));
        }
        let vehicle_ids: BTreeSet<_> = self
            .fleet
            .vehicles
            .iter()
            .map(|vehicle| &vehicle.vehicle_id)
            .collect();
        if vehicle_ids.len() != self.fleet.vehicles.len() {
            return Err(OptimizationContractError::InvalidProblem(
                "vehicle ids must be unique".to_owned(),
            ));
        }
        let objective_metrics: BTreeSet<_> = self
            .objectives
            .iter()
            .map(|objective| objective.metric)
            .collect();
        if objective_metrics.len() != self.objectives.len() {
            return Err(OptimizationContractError::InvalidProblem(
                "routing objective metrics must be unique".to_owned(),
            ));
        }
        if self
            .objectives
            .iter()
            .all(|objective| objective.weight.get() == 0.0)
        {
            return Err(OptimizationContractError::InvalidProblem(
                "at least one routing objective weight must be positive".to_owned(),
            ));
        }
        let has_service_orders = self
            .orders
            .iter()
            .any(|order| matches!(order.order, RouteOrderKind::Service { .. }));
        let has_pickup_delivery_orders = self
            .orders
            .iter()
            .any(|order| matches!(order.order, RouteOrderKind::PickupDelivery { .. }));
        if has_service_orders && has_pickup_delivery_orders {
            return Err(OptimizationContractError::InvalidProblem(
                "cuOpt 26.06 routing does not support mixing service and pickup-delivery orders"
                    .to_owned(),
            ));
        }

        let mut capacity_dimensions = BTreeSet::new();
        for vehicle in &self.fleet.vehicles {
            if !location_ids.contains(&vehicle.start_location_id)
                || !location_ids.contains(&vehicle.end_location_id)
            {
                return Err(OptimizationContractError::InvalidProblem(format!(
                    "vehicle {} references an unknown start or end location",
                    vehicle.vehicle_id
                )));
            }
            if let Some(window) = vehicle.time_window {
                window.validate("vehicle time window")?;
            }
            for vehicle_break in &vehicle.breaks {
                vehicle_break
                    .time_window
                    .validate("vehicle break time window")?;
                if vehicle_break.duration == 0 {
                    return Err(OptimizationContractError::InvalidProblem(
                        "vehicle break duration must be positive".to_owned(),
                    ));
                }
                if !vehicle_break
                    .allowed_location_ids
                    .iter()
                    .all(|location| location_ids.contains(location))
                {
                    return Err(OptimizationContractError::InvalidProblem(
                        "vehicle break references an unknown location".to_owned(),
                    ));
                }
            }
            capacity_dimensions.extend(vehicle.capacity.keys().cloned());
        }
        for order in &self.orders {
            if !order
                .allowed_vehicle_ids
                .iter()
                .all(|vehicle| vehicle_ids.contains(vehicle))
            {
                return Err(OptimizationContractError::InvalidProblem(format!(
                    "order {} references an unknown allowed vehicle",
                    order.order_id
                )));
            }
            if order.service_policy == RouteServicePolicy::Optional {
                let Some(drop_penalty) = order.drop_penalty else {
                    return Err(OptimizationContractError::InvalidProblem(format!(
                        "optional order {} requires a drop penalty",
                        order.order_id
                    )));
                };
                if drop_penalty.get() == 0.0 {
                    return Err(OptimizationContractError::InvalidProblem(format!(
                        "optional order {} drop penalty must be positive",
                        order.order_id
                    )));
                }
            }
            if order.demand.values().any(|demand| *demand < 0) {
                return Err(OptimizationContractError::InvalidProblem(format!(
                    "order {} demand values must be non-negative; pickup-delivery signs are derived",
                    order.order_id
                )));
            }
            capacity_dimensions.extend(order.demand.keys().cloned());
            validate_order_stops(order, &location_ids)?;
        }
        require_collection(
            "capacity dimensions",
            capacity_dimensions.len(),
            0,
            MAX_CAPACITY_DIMENSIONS,
        )?;

        if self.fleet.minimum_vehicles as usize > self.fleet.vehicles.len()
            || self
                .fleet
                .exact_vehicles
                .is_some_and(|value| value as usize > self.fleet.vehicles.len())
        {
            return Err(OptimizationContractError::InvalidProblem(
                "fleet vehicle bounds exceed the available fleet".to_owned(),
            ));
        }
        if let Some(exact) = self.fleet.exact_vehicles
            && exact < self.fleet.minimum_vehicles
        {
            return Err(OptimizationContractError::InvalidProblem(
                "exact vehicle count must not be below minimum vehicles".to_owned(),
            ));
        }

        if let TravelModelSource::Inline { model } = &self.travel_model {
            if model.location_ids.len() != self.locations.len()
                || model.location_ids.iter().collect::<BTreeSet<_>>()
                    != self
                        .locations
                        .iter()
                        .map(|location| &location.location_id)
                        .collect()
            {
                return Err(OptimizationContractError::InvalidProblem(
                    "inline travel-model locations must exactly match problem locations".to_owned(),
                ));
            }
            require_collection(
                "cost matrices",
                model.cost_matrices.len(),
                1,
                u8::MAX as usize + 1,
            )?;
            let used_vehicle_types = self
                .fleet
                .vehicles
                .iter()
                .map(|vehicle| &vehicle.vehicle_type_id)
                .collect::<BTreeSet<_>>();
            let cost_vehicle_types = model
                .cost_matrices
                .iter()
                .map(|matrix| &matrix.vehicle_type_id)
                .collect::<BTreeSet<_>>();
            if cost_vehicle_types.len() != model.cost_matrices.len()
                || cost_vehicle_types != used_vehicle_types
            {
                return Err(OptimizationContractError::InvalidProblem(
                    "inline cost matrices must contain exactly one matrix per used vehicle type"
                        .to_owned(),
                ));
            }
            for matrix in &model.cost_matrices {
                matrix.validate("cost matrix")?;
                if matrix.dimension as usize != model.location_ids.len() {
                    return Err(OptimizationContractError::InvalidProblem(
                        "cost matrix dimension must match travel-model locations".to_owned(),
                    ));
                }
            }
            let transit_vehicle_types = model
                .transit_time_matrices
                .iter()
                .map(|matrix| &matrix.vehicle_type_id)
                .collect::<BTreeSet<_>>();
            if !model.transit_time_matrices.is_empty()
                && (transit_vehicle_types.len() != model.transit_time_matrices.len()
                    || transit_vehicle_types != used_vehicle_types)
            {
                return Err(OptimizationContractError::InvalidProblem(
                    "inline transit-time matrices must be empty or contain exactly one matrix per used vehicle type"
                        .to_owned(),
                ));
            }
            for matrix in &model.transit_time_matrices {
                matrix.validate("transit-time matrix")?;
                if matrix.dimension as usize != model.location_ids.len() {
                    return Err(OptimizationContractError::InvalidProblem(
                        "transit-time matrix dimension must match travel-model locations"
                            .to_owned(),
                    ));
                }
            }
        }
        Ok(())
    }
}

fn validate_order_stops(
    order: &RouteOrder,
    location_ids: &BTreeSet<&LocationId>,
) -> Result<(), OptimizationContractError> {
    let validate = |stop: &RouteStop, label: &'static str| {
        if !location_ids.contains(&stop.location_id) {
            return Err(OptimizationContractError::InvalidProblem(format!(
                "order {} references an unknown {label} location",
                order.order_id
            )));
        }
        if let Some(window) = stop.time_window {
            window.validate(label)?;
        }
        Ok(())
    };
    match &order.order {
        RouteOrderKind::Service { stop } => validate(stop, "service stop"),
        RouteOrderKind::PickupDelivery { pickup, delivery } => {
            validate(pickup, "pickup stop")?;
            validate(delivery, "delivery stop")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum RoutingProblemSource {
    Inline { problem: RoutingProblem },
    Resource { uri: OptimizationProblemUri },
    Artifact { manifest_uri: ArtifactUri },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct RouteOutputPolicy {
    #[serde(default)]
    pub include_route_table_artifact: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptimizeRoutesRequest {
    pub problem: RoutingProblemSource,
    pub policy: SolverPolicyRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_solution: Option<OptimizationSolutionUri>,
    #[serde(default)]
    pub output: RouteOutputPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteScenario {
    pub case_id: RouteCaseId,
    pub problem: RoutingProblemSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_solution: Option<OptimizationSolutionUri>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptimizeRouteScenariosRequest {
    pub cases: Vec<RouteScenario>,
    pub policy: SolverPolicyRef,
    #[serde(default)]
    pub output: RouteOutputPolicy,
}

impl OptimizeRouteScenariosRequest {
    pub fn validate(&self) -> Result<(), OptimizationContractError> {
        require_collection("route cases", self.cases.len(), 2, MAX_ROUTE_CASES)?;
        if self
            .cases
            .iter()
            .map(|case| &case.case_id)
            .collect::<BTreeSet<_>>()
            .len()
            != self.cases.len()
        {
            return Err(OptimizationContractError::InvalidProblem(
                "route case ids must be unique".to_owned(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn id<T>(value: &str) -> T
    where
        T: TryFrom<String>,
        T::Error: std::fmt::Debug,
    {
        value.to_owned().try_into().unwrap()
    }

    fn problem() -> RoutingProblem {
        let location_a: LocationId = id("a");
        let location_b: LocationId = id("b");
        RoutingProblem {
            version: ROUTING_PROBLEM_VERSION.to_owned(),
            time_basis: TimeBasis {
                origin: Utc::now(),
                unit: super::super::TimeUnit::Second,
            },
            locations: vec![
                RouteLocation {
                    location_id: location_a.clone(),
                    longitude_deg: None,
                    latitude_deg: None,
                },
                RouteLocation {
                    location_id: location_b.clone(),
                    longitude_deg: None,
                    latitude_deg: None,
                },
            ],
            orders: vec![RouteOrder {
                order_id: id("order-1"),
                order: RouteOrderKind::Service {
                    stop: RouteStop {
                        location_id: location_b.clone(),
                        time_window: None,
                        service_duration: 0,
                    },
                },
                demand: BTreeMap::new(),
                service_policy: RouteServicePolicy::Mandatory,
                drop_penalty: None,
                allowed_vehicle_ids: BTreeSet::new(),
            }],
            fleet: RouteFleet {
                vehicles: vec![RouteVehicle {
                    vehicle_id: id("vehicle-1"),
                    vehicle_type_id: id("van"),
                    start_location_id: location_a.clone(),
                    end_location_id: location_a.clone(),
                    time_window: None,
                    breaks: Vec::new(),
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
                    location_ids: vec![location_a, location_b],
                    cost_matrices: vec![DenseTravelMatrix {
                        vehicle_type_id: id("van"),
                        dimension: 2,
                        values: vec![0.0, 1.0, 1.0, 0.0],
                        unavailable_cells: Vec::new(),
                    }],
                    transit_time_matrices: Vec::new(),
                },
            },
            objectives: vec![RouteObjective {
                metric: RouteObjectiveMetric::Cost,
                weight: NonNegativeF64::new(1.0).unwrap(),
            }],
        }
    }

    #[test]
    fn minimal_routing_problem_is_valid() {
        problem().validate().unwrap();
    }

    #[test]
    fn optional_order_requires_penalty() {
        let mut problem = problem();
        problem.orders[0].service_policy = RouteServicePolicy::Optional;
        assert!(problem.validate().is_err());
    }

    #[test]
    fn inline_matrix_shape_is_checked() {
        let mut problem = problem();
        let TravelModelSource::Inline { model } = &mut problem.travel_model else {
            unreachable!()
        };
        model.cost_matrices[0].values.pop();
        assert!(problem.validate().is_err());
    }
}
