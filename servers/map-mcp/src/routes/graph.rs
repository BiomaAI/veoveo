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

type RouteGraph = Graph<String, EdgeWeight, Directed>;
type RouteNodeIndex = HashMap<String, NodeIndex>;

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
        let (graph, _nodes) = build_graph(edges, request.objective.kind)?;
        let node_positions = node_positions(&graph)?;
        let planned = plan_path(&graph, &node_positions, positions, profile, &blocked_areas)?;
        let source_release_ids = planned
            .edges
            .iter()
            .map(|edge| edge.source_release_id.clone())
            .collect::<BTreeSet<_>>();
        let source_release_ids = if source_release_ids.is_empty() {
            releases_for_nodes(&graph, &planned.snapped_nodes)
        } else {
            source_release_ids
        };
        let distance = planned
            .edges
            .iter()
            .map(|edge| edge.distance_m)
            .sum::<f64>()
            + planned.connector_distance_m;
        let duration = planned
            .edges
            .iter()
            .map(|edge| edge.nominal_duration_s)
            .sum::<f64>()
            + planned.connector_distance_m / profile.routing_speed().get();
        let geometry = crate::spatial::resample_route_line(
            &planned.geometry,
            profile.planning().maximum_segment_length,
            profile.planning().maximum_route_points,
        )?;
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
        Ok(PlannerOutput {
            status: RouteStatus::PlanningAdvisory,
            legs: vec![leg],
            alternatives: Vec::new(),
            arrival_time,
            crossed_boundary_ids: BTreeSet::new(),
        })
    }
}

#[derive(Debug)]
struct PlannedPath {
    edges: Vec<NetworkEdge>,
    geometry: Wgs84LineString,
    connector_distance_m: f64,
    snapped_nodes: BTreeSet<NodeIndex>,
}

fn plan_path(
    graph: &RouteGraph,
    node_positions: &HashMap<NodeIndex, Wgs84Position>,
    positions: &[Wgs84Position],
    profile: &MobilityProfile,
    blocked_areas: &[geo::Polygon],
) -> Result<PlannedPath> {
    let mut snapped_nodes = BTreeSet::new();
    let mut edges = Vec::new();
    let mut coordinates = Vec::new();
    let mut connector_distance_m = 0.0;
    for exact in positions.windows(2) {
        let start = snap_node(
            &exact[0],
            node_positions,
            profile,
            ConnectorDirection::ToNetwork,
            blocked_areas,
        )?;
        let goal = snap_node(
            &exact[1],
            node_positions,
            profile,
            ConnectorDirection::FromNetwork,
            blocked_areas,
        )?;
        snapped_nodes.extend([start, goal]);
        let snapped_start = node_positions
            .get(&start)
            .context("snapped start node has no position")?;
        let snapped_goal = node_positions
            .get(&goal)
            .context("snapped goal node has no position")?;
        push_coordinate(&mut coordinates, &exact[0]);
        push_coordinate(&mut coordinates, snapped_start);
        connector_distance_m += distance(&exact[0], snapped_start);

        let (_, path) = astar(
            graph,
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
            append_geometry(&mut coordinates, &edge.weight().edge.geometry)?;
            edges.push(edge.weight().edge.clone());
        }

        push_coordinate(&mut coordinates, snapped_goal);
        push_coordinate(&mut coordinates, &exact[1]);
        connector_distance_m += distance(snapped_goal, &exact[1]);
    }
    let geometry = Wgs84LineString { coordinates };
    geometry.validate()?;
    Ok(PlannedPath {
        edges,
        geometry,
        connector_distance_m,
        snapped_nodes,
    })
}

fn releases_for_nodes(
    graph: &RouteGraph,
    nodes: &BTreeSet<NodeIndex>,
) -> BTreeSet<crate::contract::DatasetReleaseId> {
    graph
        .edge_references()
        .filter(|edge| nodes.contains(&edge.source()) || nodes.contains(&edge.target()))
        .map(|edge| edge.weight().edge.source_release_id.clone())
        .collect()
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
) -> Result<(RouteGraph, RouteNodeIndex)> {
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

#[derive(Clone, Copy)]
enum ConnectorDirection {
    ToNetwork,
    FromNetwork,
}

fn snap_node(
    position: &Wgs84Position,
    nodes: &HashMap<NodeIndex, Wgs84Position>,
    profile: &MobilityProfile,
    direction: ConnectorDirection,
    blocked_areas: &[geo::Polygon],
) -> Result<NodeIndex> {
    position.validate()?;
    let mut candidates = nodes
        .iter()
        .map(|(node, candidate)| (*node, distance(position, candidate)))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.1.total_cmp(&right.1).then(left.0.cmp(&right.0)));
    let (_, nearest_distance) = candidates
        .first()
        .context("network contains no snappable nodes")?;
    if *nearest_distance > MAX_SNAP_DISTANCE_M {
        bail!("route endpoint is {nearest_distance:.0} meters from the supported network");
    }
    let mut nearest_rejection = None;
    for (node, _) in candidates
        .into_iter()
        .take_while(|(_, distance)| *distance <= MAX_SNAP_DISTANCE_M)
    {
        let candidate = &nodes[&node];
        let coordinates = match direction {
            ConnectorDirection::ToNetwork => vec![position.clone(), candidate.clone()],
            ConnectorDirection::FromNetwork => vec![candidate.clone(), position.clone()],
        };
        let admitted = (|| -> Result<()> {
            let connector = crate::spatial::resample_route_line(
                &Wgs84LineString { coordinates },
                profile.planning().maximum_segment_length,
                profile.planning().maximum_route_points,
            )?;
            let line = connector.to_geo()?;
            if blocked_areas.iter().any(|area| area.intersects(&line)) {
                bail!("network connector intersects an avoided area");
            }
            crate::spatial::require_valid_route_lines(profile, &[connector], &[])
        })();
        match admitted {
            Ok(()) => return Ok(node),
            Err(error) => {
                nearest_rejection.get_or_insert(error);
            }
        }
    }
    Err(nearest_rejection.context("network contains no connector candidates")?).context(
        "no network connector within 10000 meters satisfies the mobility profile and avoided areas",
    )
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

fn append_geometry(coordinates: &mut Vec<Wgs84Position>, geometry: &Wgs84LineString) -> Result<()> {
    geometry.validate()?;
    for coordinate in &geometry.coordinates {
        push_coordinate(coordinates, coordinate);
    }
    Ok(())
}

fn push_coordinate(coordinates: &mut Vec<Wgs84Position>, position: &Wgs84Position) {
    if coordinates.last() != Some(position) {
        coordinates.push(position.clone());
    }
}

#[cfg(test)]
mod tests {
    use geo::polygon;

    use super::*;
    use crate::contract::{DatasetReleaseId, Degrees};

    fn profile() -> MobilityProfile {
        let mut profile: MobilityProfile =
            serde_json::from_str(include_str!("../../tests/fixtures/mobility.json")).unwrap();
        let MobilityProfile::Human(human) = &mut profile else {
            unreachable!()
        };
        human.planning.maximum_climb_angle = Some(Degrees::new(45.0).unwrap());
        human.planning.maximum_descent_angle = Some(Degrees::new(45.0).unwrap());
        human.planning.maximum_segment_length = Meters::new(5_000.0).unwrap();
        profile
    }

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

    #[test]
    fn same_snapped_node_retains_distinct_exact_endpoints() {
        let (graph, nodes) =
            build_graph(vec![edge("a", "b", 100.0)], RouteObjectiveKind::Shortest).unwrap();
        let node_positions = node_positions(&graph).unwrap();
        let exact_start = position(0.0001, 0.0);
        let exact_goal = position(0.0002, 0.0);

        let planned = plan_path(
            &graph,
            &node_positions,
            &[exact_start.clone(), exact_goal.clone()],
            &profile(),
            &[],
        )
        .unwrap();

        assert!(planned.edges.is_empty());
        assert_eq!(planned.geometry.coordinates.first(), Some(&exact_start));
        assert_eq!(planned.geometry.coordinates.last(), Some(&exact_goal));
        assert!(planned.connector_distance_m > 0.0);
        assert_eq!(releases_for_nodes(&graph, &planned.snapped_nodes).len(), 1);
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn path_retains_exact_endpoints_around_governed_edges() {
        let (graph, _) =
            build_graph(vec![edge("a", "b", 100.0)], RouteObjectiveKind::Shortest).unwrap();
        let node_positions = node_positions(&graph).unwrap();
        let exact_start = position(-0.001, 0.0);
        let exact_goal = position(0.011, 0.0);

        let planned = plan_path(
            &graph,
            &node_positions,
            &[exact_start.clone(), exact_goal.clone()],
            &profile(),
            &[],
        )
        .unwrap();

        assert_eq!(planned.edges.len(), 1);
        assert_eq!(planned.geometry.coordinates.first(), Some(&exact_start));
        assert_eq!(planned.geometry.coordinates.last(), Some(&exact_goal));
        assert_eq!(planned.geometry.coordinates.len(), 4);
        assert!(planned.connector_distance_m > 200.0);
    }

    #[test]
    fn flight_connector_uses_a_feasible_node_without_changing_endpoint_height() {
        let mut edge = edge("a", "b", 84.0);
        edge.bidirectional = true;
        edge.geometry.coordinates = vec![
            Wgs84Position::new(-73.9855, 40.758, Some(180.0)).unwrap(),
            Wgs84Position::new(-73.9845, 40.758, Some(180.0)).unwrap(),
        ];
        let (graph, nodes) = build_graph(vec![edge], RouteObjectiveKind::Shortest).unwrap();
        let node_positions = node_positions(&graph).unwrap();
        let start = Wgs84Position::new(
            -73.9855017697308,
            40.75800248937957,
            Some(179.65162659529597),
        )
        .unwrap();
        let goal = Wgs84Position::new(
            -73.9853017697308,
            40.75800248937957,
            Some(179.65162659529597),
        )
        .unwrap();
        let planned = plan_path(
            &graph,
            &node_positions,
            &[start.clone(), goal.clone()],
            &profile(),
            &[],
        )
        .unwrap();

        assert_eq!(planned.geometry.coordinates.first(), Some(&start));
        assert_eq!(planned.geometry.coordinates.last(), Some(&goal));
        assert_eq!(planned.geometry.coordinates[1], node_positions[&nodes["b"]]);
        assert_eq!(planned.edges.len(), 1);
        crate::spatial::require_valid_route_lines(&profile(), &[planned.geometry], &[]).unwrap();
    }

    #[test]
    fn connectors_apply_climb_and_descent_in_the_direction_of_travel() {
        let mut profile = profile();
        let MobilityProfile::Human(human) = &mut profile else {
            unreachable!()
        };
        human.planning.maximum_climb_angle = Some(Degrees::new(30.0).unwrap());
        human.planning.maximum_descent_angle = Some(Degrees::new(60.0).unwrap());
        let close = NodeIndex::new(0);
        let far = NodeIndex::new(1);
        let nodes = HashMap::from([
            (close, Wgs84Position::new(0.00001, 0.0, Some(1.0)).unwrap()),
            (far, Wgs84Position::new(0.001, 0.0, Some(1.0)).unwrap()),
        ]);
        let point = Wgs84Position::new(0.0, 0.0, Some(0.0)).unwrap();
        assert_eq!(
            snap_node(&point, &nodes, &profile, ConnectorDirection::ToNetwork, &[]).unwrap(),
            far
        );
        assert_eq!(
            snap_node(
                &point,
                &nodes,
                &profile,
                ConnectorDirection::FromNetwork,
                &[]
            )
            .unwrap(),
            close
        );
        let only_close = HashMap::from([(close, nodes[&close].clone())]);
        let error = snap_node(
            &point,
            &only_close,
            &profile,
            ConnectorDirection::ToNetwork,
            &[],
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("ClimbLimitExceeded"));
    }

    #[test]
    fn connectors_avoid_areas_and_stay_within_the_snap_distance() {
        let near = NodeIndex::new(0);
        let far = NodeIndex::new(1);
        let nodes = HashMap::from([(near, position(0.001, 0.0)), (far, position(0.0, 0.002))]);
        let blocked = polygon![
            (x: 0.0004, y: -0.0001), (x: 0.0006, y: -0.0001),
            (x: 0.0006, y: 0.0001), (x: 0.0004, y: 0.0001),
        ];
        assert_eq!(
            snap_node(
                &position(0.0, 0.0),
                &nodes,
                &profile(),
                ConnectorDirection::ToNetwork,
                &[blocked]
            )
            .unwrap(),
            far
        );
        assert!(
            snap_node(
                &position(0.2, 0.2),
                &nodes,
                &profile(),
                ConnectorDirection::ToNetwork,
                &[]
            )
            .is_err()
        );
    }
}
