mod graph;
mod service;

pub mod valhalla;

pub use service::RouteService;

use chrono::{DateTime, Utc};

use crate::contract::{RouteAlternative, RouteLeg, RouteStatus};

#[derive(Clone, Debug)]
struct PlannerOutput {
    status: RouteStatus,
    legs: Vec<RouteLeg>,
    alternatives: Vec<RouteAlternative>,
    arrival_time: Option<DateTime<Utc>>,
    crossed_boundary_ids: std::collections::BTreeSet<String>,
}
