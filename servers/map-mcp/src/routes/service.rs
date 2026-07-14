use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use geo::Intersects;
use sha2::{Digest, Sha256};

use crate::{
    analytics::MapAnalytics,
    catalog::{MapCatalog, MapScope},
    contract::{
        DatasetReleaseState, FacilityId, MapFamily, MobilityProfile, OperationalSnapshot,
        OperationalSnapshotId, ReachableArea, ReachableAreaId, ReachableAreaRequest, Restriction,
        RestrictionEffectKind, RouteEndpoint, RouteId, RouteMatrix, RouteMatrixCell, RouteMatrixId,
        RouteMatrixRequest, RoutePlan, RouteProvenance, RouteRequest, RouteStatus, RouteValidation,
        ValidateRouteRequest, ValidationId, Wgs84BoundingBox, Wgs84Position,
    },
    routes::{PlannerOutput, graph::GraphPlanner, valhalla::ValhallaPlanner, valhalla::sum_cost},
};

const PLANNER_VERSION: &str = "veoveo-map-route-v1";
const COST_MODEL_VERSION: &str = "veoveo-map-cost-v1";

#[derive(Clone, Debug)]
pub struct RouteService {
    catalog: MapCatalog,
    analytics: MapAnalytics,
    valhalla: ValhallaPlanner,
    graph: GraphPlanner,
}

impl RouteService {
    pub fn new(catalog: MapCatalog, analytics: MapAnalytics, valhalla: ValhallaPlanner) -> Self {
        let graph = GraphPlanner::new(analytics.clone());
        Self {
            catalog,
            analytics,
            valhalla,
            graph,
        }
    }

    pub async fn route(&self, scope: &MapScope, mut request: RouteRequest) -> Result<RoutePlan> {
        validate_request(&request)?;
        let tenant_key = scope.tenant_key();
        let profile = self
            .catalog
            .mobility_profile(
                scope,
                &request.mobility_profile_id,
                request.mobility_profile_version,
            )
            .await?
            .context("unknown mobility profile version")?;
        profile.validate()?;
        if request.departure_time < profile.metadata().valid_from
            || profile
                .metadata()
                .valid_until
                .is_some_and(|until| request.departure_time >= until)
        {
            bail!("mobility profile is not valid at route departure time");
        }
        let mut positions = Vec::with_capacity(request.waypoints.len() + 2);
        let mut facility_ids = BTreeSet::new();
        positions.push(self.resolve_endpoint(&tenant_key, &request.origin, &mut facility_ids)?);
        for waypoint in &request.waypoints {
            positions.push(self.resolve_endpoint(&tenant_key, waypoint, &mut facility_ids)?);
        }
        for facility_id in &request.constraints.required_facility_stops {
            positions.push(self.resolve_facility(&tenant_key, facility_id, &mut facility_ids)?);
        }
        positions.push(self.resolve_endpoint(
            &tenant_key,
            &request.destination,
            &mut facility_ids,
        )?);

        let coverage = coverage(&positions)?;
        let restrictions = self
            .catalog
            .list_restrictions(scope)
            .await?
            .into_iter()
            .filter(|restriction| {
                restriction_applies(restriction, &profile, request.departure_time)
            })
            .collect::<Vec<_>>();
        for restriction in &restrictions {
            if restriction.effect.kind == RestrictionEffectKind::Prohibit {
                request
                    .constraints
                    .avoided_areas
                    .push(restriction.geometry.clone());
            }
        }
        let (release_ids, map_families) = self
            .active_releases(scope, &profile, request.departure_time)
            .await?;
        if release_ids.is_empty() {
            bail!("coverage unavailable: no active release supports this mobility profile");
        }
        if !request
            .data_policy
            .required_map_families
            .is_subset(&map_families)
        {
            bail!("required map-family coverage is unavailable");
        }
        let snapshot = OperationalSnapshot {
            snapshot_id: OperationalSnapshotId::new(),
            captured_at: Utc::now(),
            departure_time: request.departure_time,
            coverage,
            restriction_ids: restrictions
                .iter()
                .map(|restriction| restriction.restriction_id.clone())
                .collect(),
            observation_release_ids: BTreeSet::new(),
        };
        self.catalog.persist_snapshot(scope, &snapshot).await?;
        let mut planned = match profile {
            MobilityProfile::Human(_) | MobilityProfile::RoadVehicle(_) => {
                self.valhalla
                    .plan(&request, &profile, &positions, &release_ids)
                    .await?
            }
            _ => self
                .graph
                .plan(&tenant_key, &request, &profile, &positions)?,
        };
        let restrictions_resolved = apply_restrictions(&mut planned, &restrictions)?;
        if planned.status == RouteStatus::PlanningAdvisory && restrictions_resolved {
            planned.status = RouteStatus::Validated;
        }
        if planned.status == RouteStatus::PlanningAdvisory
            && !request.data_policy.allow_planning_advisory
        {
            bail!("the selected route planner can only provide planning-advisory output");
        }
        let restriction_ids = planned
            .legs
            .iter()
            .flat_map(|leg| leg.restriction_ids.iter().cloned())
            .collect::<BTreeSet<_>>();
        let summary = sum_cost(&planned.legs)?;
        let route_id = RouteId::new();
        let plan = RoutePlan {
            route_uri: format!("map://route/{route_id}"),
            route_id,
            status: planned.status,
            mobility_profile_id: request.mobility_profile_id.clone(),
            mobility_profile_version: request.mobility_profile_version,
            departure_time: request.departure_time,
            arrival_time: planned.arrival_time,
            legs: planned.legs,
            alternatives: planned.alternatives,
            summary,
            crossed_boundary_ids: planned.crossed_boundary_ids,
            facility_ids,
            restriction_ids,
            validation_id: ValidationId::new(),
            provenance: RouteProvenance {
                base_release_ids: release_ids,
                operational_snapshot_id: snapshot.snapshot_id,
                planner_version: PLANNER_VERSION.to_owned(),
                cost_model_version: COST_MODEL_VERSION.to_owned(),
            },
            created_at: Utc::now(),
        };
        let digest = cache_digest(&request, &plan.provenance)?;
        self.catalog.persist_route(scope, &plan, digest).await?;
        Ok(plan)
    }

    pub async fn route_matrix(
        &self,
        scope: &MapScope,
        request: RouteMatrixRequest,
    ) -> Result<RouteMatrix> {
        if request.origins.is_empty() || request.destinations.is_empty() {
            bail!("route matrix requires at least one origin and destination");
        }
        if request.origins.len() > 20
            || request.destinations.len() > 20
            || request.origins.len() * request.destinations.len() > 400
        {
            bail!("route matrix is limited to 20 origins, 20 destinations, and 400 cells");
        }
        let mut cells = Vec::with_capacity(request.origins.len() * request.destinations.len());
        let mut provenance = None;
        for (origin_index, origin) in request.origins.iter().enumerate() {
            for (destination_index, destination) in request.destinations.iter().enumerate() {
                let route_request = RouteRequest {
                    mobility_profile_id: request.mobility_profile_id.clone(),
                    mobility_profile_version: request.mobility_profile_version,
                    origin: origin.clone(),
                    destination: destination.clone(),
                    waypoints: Vec::new(),
                    departure_time: request.departure_time,
                    objective: request.objective.clone(),
                    constraints: request.constraints.clone(),
                    alternatives: 0,
                    data_policy: request.data_policy.clone(),
                };
                match self.route(scope, route_request).await {
                    Ok(route) => {
                        provenance.get_or_insert_with(|| route.provenance.clone());
                        cells.push(RouteMatrixCell {
                            origin_index: origin_index as u32,
                            destination_index: destination_index as u32,
                            status: route.status,
                            cost: Some(route.summary),
                        });
                    }
                    Err(error) => {
                        tracing::debug!(
                            origin_index,
                            destination_index,
                            "route matrix cell unavailable: {error}"
                        );
                        cells.push(RouteMatrixCell {
                            origin_index: origin_index as u32,
                            destination_index: destination_index as u32,
                            status: RouteStatus::Unavailable,
                            cost: None,
                        });
                    }
                }
            }
        }
        let provenance = provenance.context(
            "route matrix unavailable: no origin/destination pair has supported coverage",
        )?;
        let matrix = RouteMatrix {
            matrix_id: RouteMatrixId::new(),
            cells,
            provenance,
            created_at: Utc::now(),
        };
        self.catalog
            .persist_matrix(
                scope,
                &matrix,
                request.mobility_profile_id.as_str(),
                request.mobility_profile_version,
            )
            .await?;
        Ok(matrix)
    }

    pub async fn reachable_area(
        &self,
        scope: &MapScope,
        request: ReachableAreaRequest,
    ) -> Result<ReachableArea> {
        let profile = self
            .catalog
            .mobility_profile(
                scope,
                &request.mobility_profile_id,
                request.mobility_profile_version,
            )
            .await?
            .context("unknown mobility profile version")?;
        profile.validate()?;
        if !matches!(
            profile,
            MobilityProfile::Human(_) | MobilityProfile::RoadVehicle(_)
        ) {
            bail!("reachable-area coverage is currently available for land profiles only");
        }
        let tenant_key = scope.tenant_key();
        let mut facility_ids = BTreeSet::new();
        let origin = self.resolve_endpoint(&tenant_key, &request.origin, &mut facility_ids)?;
        let (release_ids, map_families) = self
            .active_releases(scope, &profile, request.departure_time)
            .await?;
        if release_ids.is_empty()
            || !request
                .data_policy
                .required_map_families
                .is_subset(&map_families)
        {
            bail!("reachable-area coverage is unavailable for the requested data policy");
        }
        let restrictions = self
            .catalog
            .list_restrictions(scope)
            .await?
            .into_iter()
            .filter(|restriction| {
                restriction_applies(restriction, &profile, request.departure_time)
            })
            .collect::<Vec<_>>();
        let mut engine_request = request.clone();
        for restriction in &restrictions {
            if restriction.effect.kind == RestrictionEffectKind::Prohibit {
                engine_request
                    .constraints
                    .avoided_areas
                    .push(restriction.geometry.clone());
            }
        }
        let polygons = self
            .valhalla
            .reachable_area(&engine_request, &profile, &origin)
            .await?;
        let coverage = polygon_coverage(&polygons)?;
        let snapshot = OperationalSnapshot {
            snapshot_id: OperationalSnapshotId::new(),
            captured_at: Utc::now(),
            departure_time: request.departure_time,
            coverage,
            restriction_ids: restrictions
                .iter()
                .map(|restriction| restriction.restriction_id.clone())
                .collect(),
            observation_release_ids: BTreeSet::new(),
        };
        self.catalog.persist_snapshot(scope, &snapshot).await?;
        let reachable_area_id = ReachableAreaId::new();
        Ok(ReachableArea {
            reachable_area_uri: format!("map://reachable-area/{reachable_area_id}"),
            reachable_area_id,
            mobility_profile_id: request.mobility_profile_id,
            mobility_profile_version: request.mobility_profile_version,
            origin,
            departure_time: request.departure_time,
            budget: request.budget,
            polygons,
            provenance: RouteProvenance {
                base_release_ids: release_ids,
                operational_snapshot_id: snapshot.snapshot_id,
                planner_version: PLANNER_VERSION.to_owned(),
                cost_model_version: COST_MODEL_VERSION.to_owned(),
            },
            created_at: Utc::now(),
        })
    }

    pub async fn validate_route(
        &self,
        scope: &MapScope,
        request: ValidateRouteRequest,
    ) -> Result<RouteValidation> {
        let mut findings = Vec::new();
        if request.route.legs.is_empty() {
            findings.push("route has no legs".to_owned());
        }
        for (index, leg) in request.route.legs.iter().enumerate() {
            if let Err(error) = leg.geometry.validate() {
                findings.push(format!("leg {index} geometry is invalid: {error}"));
            }
        }
        let releases = self.catalog.list_releases(scope).await?;
        for release_id in &request.route.provenance.base_release_ids {
            match releases
                .iter()
                .find(|release| &release.release_id == release_id)
            {
                None => findings.push(format!("source release {release_id} is unavailable")),
                Some(release) if release.state == DatasetReleaseState::Quarantined => {
                    findings.push(format!("source release {release_id} is quarantined"))
                }
                Some(_) => {}
            }
        }
        let profile = self
            .catalog
            .mobility_profile(
                scope,
                &request.route.mobility_profile_id,
                request.route.mobility_profile_version,
            )
            .await?
            .context("route mobility profile version is unavailable")?;
        let restrictions = self.catalog.list_restrictions(scope).await?;
        for restriction in restrictions.iter().filter(|restriction| {
            restriction_applies(restriction, &profile, request.route.departure_time)
        }) {
            if restriction.effect.kind != RestrictionEffectKind::Prohibit {
                continue;
            }
            let area = restriction.geometry.to_geo()?;
            if request.route.legs.iter().any(|leg| {
                leg.geometry
                    .to_geo()
                    .is_ok_and(|line| area.intersects(&line))
            }) {
                findings.push(format!(
                    "route intersects active prohibition {}",
                    restriction.restriction_id
                ));
            }
        }
        Ok(RouteValidation {
            validation_id: ValidationId::new(),
            valid: findings.is_empty(),
            findings,
            validated_at: Utc::now(),
        })
    }

    fn resolve_endpoint(
        &self,
        tenant_key: &str,
        endpoint: &RouteEndpoint,
        facilities: &mut BTreeSet<FacilityId>,
    ) -> Result<Wgs84Position> {
        match endpoint {
            RouteEndpoint::Position { position } => {
                position.validate()?;
                Ok(position.clone())
            }
            RouteEndpoint::Location { location_id } => self
                .analytics
                .location(tenant_key, location_id)?
                .map(|location| location.position)
                .context("unknown location"),
            RouteEndpoint::Facility { facility_id } => {
                self.resolve_facility(tenant_key, facility_id, facilities)
            }
        }
    }

    fn resolve_facility(
        &self,
        tenant_key: &str,
        facility_id: &FacilityId,
        facilities: &mut BTreeSet<FacilityId>,
    ) -> Result<Wgs84Position> {
        let facility = self
            .analytics
            .facility(tenant_key, facility_id)?
            .context("unknown facility")?;
        facilities.insert(facility_id.clone());
        Ok(facility.position)
    }

    async fn active_releases(
        &self,
        scope: &MapScope,
        profile: &MobilityProfile,
        departure_time: chrono::DateTime<Utc>,
    ) -> Result<(
        BTreeSet<crate::contract::DatasetReleaseId>,
        BTreeSet<MapFamily>,
    )> {
        let sources = self.catalog.list_sources(scope).await?;
        let sources = sources
            .into_iter()
            .filter(|source| source.enabled)
            .map(|source| (source.source_id, source.map_families))
            .collect::<std::collections::BTreeMap<_, _>>();
        let compatible = profile.compatible_map_families();
        let mut releases = BTreeSet::new();
        let mut families = BTreeSet::new();
        for release in self.catalog.list_releases(scope).await? {
            if self
                .catalog
                .active_release_id(scope, &release.dataset_id)
                .await?
                .as_ref()
                != Some(&release.release_id)
                || release.valid_from > departure_time
                || release
                    .valid_until
                    .is_some_and(|until| until <= departure_time)
            {
                continue;
            }
            let Some(source_families) = sources.get(&release.source_id) else {
                continue;
            };
            let selected = source_families
                .intersection(&compatible)
                .copied()
                .collect::<BTreeSet<_>>();
            if !selected.is_empty() {
                releases.insert(release.release_id);
                families.extend(selected);
            }
        }
        Ok((releases, families))
    }
}

fn polygon_coverage(polygons: &[crate::contract::Wgs84Polygon]) -> Result<Wgs84BoundingBox> {
    let mut positions = polygons.iter().flat_map(|polygon| polygon.exterior.iter());
    let first = positions
        .next()
        .context("reachable area contains no coordinates")?;
    let mut west = first.longitude_deg;
    let mut east = first.longitude_deg;
    let mut south = first.latitude_deg;
    let mut north = first.latitude_deg;
    for position in positions {
        west = west.min(position.longitude_deg);
        east = east.max(position.longitude_deg);
        south = south.min(position.latitude_deg);
        north = north.max(position.latitude_deg);
    }
    let coverage = Wgs84BoundingBox {
        west,
        south,
        east,
        north,
    };
    coverage.validate()?;
    Ok(coverage)
}

fn validate_request(request: &RouteRequest) -> Result<()> {
    if request.mobility_profile_version == 0 {
        bail!("mobility_profile_version must be positive");
    }
    if request.waypoints.len() > 32 {
        bail!("route accepts at most 32 waypoints");
    }
    if request.alternatives > 3 {
        bail!("route accepts at most 3 alternatives");
    }
    if request.objective.kind == crate::contract::RouteObjectiveKind::Weighted
        && request.objective.weights.is_none()
    {
        bail!("weighted objective requires weights");
    }
    Ok(())
}

fn restriction_applies(
    restriction: &Restriction,
    profile: &MobilityProfile,
    departure_time: chrono::DateTime<Utc>,
) -> bool {
    restriction.cancelled_by.is_none()
        && restriction.valid_from <= departure_time
        && restriction
            .valid_until
            .is_none_or(|until| departure_time < until)
        && restriction
            .affected_mobility_families
            .contains(&profile.family())
}

fn apply_restrictions(output: &mut PlannerOutput, restrictions: &[Restriction]) -> Result<bool> {
    let mut fully_resolved = true;
    for leg in &mut output.legs {
        let line = leg.geometry.to_geo()?;
        for restriction in restrictions {
            let area = restriction.geometry.to_geo()?;
            if !area.intersects(&line) {
                continue;
            }
            leg.restriction_ids
                .insert(restriction.restriction_id.clone());
            match restriction.effect.kind {
                RestrictionEffectKind::Prohibit => {
                    output.status = RouteStatus::Unavailable;
                }
                RestrictionEffectKind::Require | RestrictionEffectKind::Limit => {
                    output.status = RouteStatus::PlanningAdvisory;
                    fully_resolved = false;
                }
                RestrictionEffectKind::Penalize => {
                    leg.cost.risk =
                        crate::contract::Ratio::new((leg.cost.risk.get() + 0.1).min(1.0))?;
                }
                RestrictionEffectKind::Advise => {}
            }
        }
    }
    if output.status == RouteStatus::Unavailable {
        bail!("no feasible route remains after applying active prohibitions");
    }
    Ok(fully_resolved)
}

fn coverage(positions: &[Wgs84Position]) -> Result<Wgs84BoundingBox> {
    let first = positions
        .first()
        .context("route has no resolved positions")?;
    let mut bounds = Wgs84BoundingBox {
        west: first.longitude_deg,
        south: first.latitude_deg,
        east: first.longitude_deg,
        north: first.latitude_deg,
    };
    for position in positions {
        position.validate()?;
        bounds.west = bounds.west.min(position.longitude_deg);
        bounds.east = bounds.east.max(position.longitude_deg);
        bounds.south = bounds.south.min(position.latitude_deg);
        bounds.north = bounds.north.max(position.latitude_deg);
    }
    bounds.validate()?;
    Ok(bounds)
}

fn cache_digest(request: &RouteRequest, provenance: &RouteProvenance) -> Result<String> {
    let bytes = serde_json::to_vec(&(request, provenance))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
