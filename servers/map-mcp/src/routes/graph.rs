use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result, bail};
use geo::Intersects;
use geographiclib_rs::{Geodesic, InverseGeodesic};
use petgraph::{Directed, Graph, algo::astar, graph::NodeIndex, visit::EdgeRef};

use crate::{
    analytics::{MapAnalytics, NetworkEdge},
    contract::{
        MapFamily, Meters, MobilityProfile, Ratio, RouteCost, RouteLeg, RouteObjectiveKind,
        RouteRequest, RouteStatus, Seconds, Wgs84LineString, Wgs84Position,
    },
    routes::PlannerOutput,
};

const MAX_SNAP_DISTANCE_M: f64 = 10_000.0;

#[derive(Clone, Debug)]
pub(super) struct GraphPlanner {
    analytics: MapAnalytics,
}

#[derive(Clone, Debug)]
struct EdgeWeight {
    edge: NetworkEdge,
    cost: f64,
}

impl GraphPlanner {
    pub fn new(analytics: MapAnalytics) -> Self {
        Self { analytics }
    }

    pub fn plan(
        &self,
        tenant_key: &str,
        request: &RouteRequest,
        profile: &MobilityProfile,
        positions: &[Wgs84Position],
    ) -> Result<PlannerOutput> {
        if request.alternatives > 0 {
            bail!("alternate routes are not available for the governed graph planner");
        }
        if !request.constraints.required_areas.is_empty() {
            bail!("required-area constraints are not available for the governed graph planner");
        }
        let map_family = graph_family(profile)?;
        let edges = self.analytics.network_edges(tenant_key, map_family)?;
        if edges.is_empty() {
            bail!("coverage unavailable for {map_family:?}");
        }
        let blocked_areas = request
            .constraints
            .avoided_areas
            .iter()
            .map(|polygon| polygon.to_geo())
            .collect::<Result<Vec<_>, _>>()?;
        let edges = edges
            .into_iter()
            .filter(|edge| {
                edge.geometry
                    .to_geo()
                    .is_ok_and(|line| blocked_areas.iter().all(|area| !area.intersects(&line)))
            })
            .collect::<Vec<_>>();
        if edges.is_empty() {
            bail!("no network edges remain after applying avoided areas");
        }
        let (graph, nodes) = build_graph(edges, request.objective.kind)?;
        let node_positions = node_positions(&graph)?;
        let snapped = positions
            .iter()
            .map(|position| snap_node(position, &node_positions))
            .collect::<Result<Vec<_>>>()?;
        let mut selected = Vec::new();
        for pair in snapped.windows(2) {
            let start = pair[0];
            let goal = pair[1];
            let (_, path) = astar(
                &graph,
                start,
                |node| node == goal,
                |edge| edge.weight().cost,
                |_| 0.0,
            )
            .context("no feasible path exists in the governed network")?;
            for nodes in path.windows(2) {
                let edge = graph
                    .edges_connecting(nodes[0], nodes[1])
                    .min_by(|left, right| left.weight().cost.total_cmp(&right.weight().cost))
                    .context("planned graph path omitted an edge")?;
                selected.push(edge.weight().edge.clone());
            }
        }
        let geometry = join_geometry(&selected)?;
        let source_release_ids = selected
            .iter()
            .map(|edge| edge.source_release_id.clone())
            .collect::<BTreeSet<_>>();
        let distance = selected.iter().map(|edge| edge.distance_m).sum();
        let duration = selected.iter().map(|edge| edge.nominal_duration_s).sum();
        let leg = RouteLeg {
            sequence: 0,
            map_family,
            geometry,
            cost: RouteCost {
                distance: Meters::new(distance)?,
                duration: Seconds::new(duration)?,
                energy: None,
                fuel: None,
                monetary_minor_units: None,
                risk: Ratio::new(0.0)?,
            },
            instructions: Vec::new(),
            source_release_ids,
            restriction_ids: BTreeSet::new(),
        };
        let arrival_time = chrono::TimeDelta::try_seconds(duration.round() as i64)
            .map(|duration| request.departure_time + duration);
        let _ = nodes;
        Ok(PlannerOutput {
            status: RouteStatus::PlanningAdvisory,
            legs: vec![leg],
            alternatives: Vec::new(),
            arrival_time,
            crossed_boundary_ids: BTreeSet::new(),
        })
    }
}

fn graph_family(profile: &MobilityProfile) -> Result<MapFamily> {
    Ok(match profile {
        MobilityProfile::OffRoadVehicle(_) => MapFamily::OffRoadTerrain,
        MobilityProfile::RailVehicle(_) => MapFamily::RailTransit,
        MobilityProfile::SurfaceVessel(_) | MobilityProfile::SubsurfaceVessel(_) => {
            MapFamily::Maritime
        }
        MobilityProfile::FixedWing(_)
        | MobilityProfile::Rotorcraft(_)
        | MobilityProfile::Uas(_) => MapFamily::Aviation,
        _ => bail!("mobility profile belongs to the land routing adapter"),
    })
}

fn build_graph(
    edges: Vec<NetworkEdge>,
    objective: RouteObjectiveKind,
) -> Result<(
    Graph<String, EdgeWeight, Directed>,
    HashMap<String, NodeIndex>,
)> {
    if !matches!(
        objective,
        RouteObjectiveKind::Fastest | RouteObjectiveKind::Shortest
    ) {
        bail!("the governed graph planner supports fastest and shortest objectives");
    }
    let mut graph = Graph::<String, EdgeWeight, Directed>::new();
    let mut nodes = HashMap::new();
    for edge in edges {
        let start = *nodes
            .entry(edge.from_node.clone())
            .or_insert_with(|| graph.add_node(edge.from_node.clone()));
        let end = *nodes
            .entry(edge.to_node.clone())
            .or_insert_with(|| graph.add_node(edge.to_node.clone()));
        let cost = match objective {
            RouteObjectiveKind::Fastest => edge.nominal_duration_s,
            RouteObjectiveKind::Shortest => edge.distance_m,
            _ => unreachable!(),
        };
        graph.add_edge(
            start,
            end,
            EdgeWeight {
                edge: edge.clone(),
                cost,
            },
        );
        if edge.bidirectional {
            let mut reverse = edge;
            std::mem::swap(&mut reverse.from_node, &mut reverse.to_node);
            reverse.geometry.coordinates.reverse();
            graph.add_edge(
                end,
                start,
                EdgeWeight {
                    edge: reverse,
                    cost,
                },
            );
        }
    }
    Ok((graph, nodes))
}

fn node_positions(
    graph: &Graph<String, EdgeWeight, Directed>,
) -> Result<HashMap<NodeIndex, Wgs84Position>> {
    let mut positions = HashMap::new();
    for edge in graph.edge_references() {
        let geometry = &edge.weight().edge.geometry.coordinates;
        let first = geometry.first().context("network edge geometry is empty")?;
        let last = geometry.last().context("network edge geometry is empty")?;
        insert_consistent(&mut positions, edge.source(), first)?;
        insert_consistent(&mut positions, edge.target(), last)?;
    }
    Ok(positions)
}

fn insert_consistent(
    positions: &mut HashMap<NodeIndex, Wgs84Position>,
    node: NodeIndex,
    position: &Wgs84Position,
) -> Result<()> {
    if let Some(existing) = positions.get(&node) {
        if distance(existing, position) > 1.0 {
            bail!("network node has inconsistent endpoint geometry");
        }
    } else {
        positions.insert(node, position.clone());
    }
    Ok(())
}

fn snap_node(
    position: &Wgs84Position,
    nodes: &HashMap<NodeIndex, Wgs84Position>,
) -> Result<NodeIndex> {
    position.validate()?;
    let (node, distance) = nodes
        .iter()
        .map(|(node, candidate)| (*node, distance(position, candidate)))
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .context("network contains no snappable nodes")?;
    if distance > MAX_SNAP_DISTANCE_M {
        bail!("route endpoint is {distance:.0} meters from the supported network");
    }
    Ok(node)
}

fn distance(left: &Wgs84Position, right: &Wgs84Position) -> f64 {
    let (meters, _, _, _): (f64, f64, f64, f64) = Geodesic::wgs84().inverse(
        left.latitude_deg,
        left.longitude_deg,
        right.latitude_deg,
        right.longitude_deg,
    );
    meters
}

fn join_geometry(edges: &[NetworkEdge]) -> Result<Wgs84LineString> {
    let mut coordinates = Vec::new();
    for edge in edges {
        edge.geometry.validate()?;
        let start = usize::from(!coordinates.is_empty());
        coordinates.extend(edge.geometry.coordinates.iter().skip(start).cloned());
    }
    let geometry = Wgs84LineString { coordinates };
    geometry.validate()?;
    Ok(geometry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::DatasetReleaseId;

    fn position(longitude_deg: f64, latitude_deg: f64) -> Wgs84Position {
        Wgs84Position::new(longitude_deg, latitude_deg, None).unwrap()
    }

    fn edge(from: &str, to: &str, distance_m: f64) -> NetworkEdge {
        NetworkEdge {
            edge_id: format!("{from}-{to}"),
            map_family: MapFamily::Maritime,
            from_node: from.to_owned(),
            to_node: to.to_owned(),
            geometry: Wgs84LineString {
                coordinates: vec![position(0.0, 0.0), position(0.01, 0.0)],
            },
            distance_m,
            nominal_duration_s: distance_m / 5.0,
            bidirectional: false,
            source_release_id: DatasetReleaseId::new(),
        }
    }

    #[test]
    fn graph_cost_uses_the_declared_objective() {
        let (graph, _) =
            build_graph(vec![edge("a", "b", 100.0)], RouteObjectiveKind::Shortest).unwrap();
        assert_eq!(graph.edge_weights().next().unwrap().cost, 100.0);
    }
}
