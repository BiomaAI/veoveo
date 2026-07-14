use anyhow::{Context, Result, bail};
use geo::Intersects;

use crate::{
    analytics::MapAnalytics,
    catalog::{MapCatalog, MapScope},
    contract::{
        CorridorInspectionOutput, CorridorInspectionRequest, InspectLocationOutput,
        InspectLocationRequest, PublishRestrictionRequest, Restriction, Wgs84Position,
        WithdrawRestrictionRequest,
    },
};

#[derive(Clone, Debug)]
pub struct GeographyService {
    catalog: MapCatalog,
    analytics: MapAnalytics,
}

impl GeographyService {
    pub fn new(catalog: MapCatalog, analytics: MapAnalytics) -> Self {
        Self { catalog, analytics }
    }

    pub fn inspect_location(
        &self,
        scope: &MapScope,
        request: InspectLocationRequest,
    ) -> Result<InspectLocationOutput> {
        let tenant_key = scope.tenant_key();
        let location = self
            .analytics
            .location(&tenant_key, &request.location_id)?
            .context("unknown location")?;
        let nearby_facilities = self.analytics.nearby_facilities(
            &tenant_key,
            &location.position,
            request.nearby_radius,
            request.facility_limit,
        )?;
        Ok(InspectLocationOutput {
            containing_boundary_ids: self
                .analytics
                .containing_boundary_ids(&tenant_key, &location.position)?
                .into_iter()
                .map(|id| id.to_string())
                .collect(),
            location,
            nearby_facilities,
            data_gaps: Vec::new(),
        })
    }

    pub async fn inspect_corridor(
        &self,
        scope: &MapScope,
        request: CorridorInspectionRequest,
    ) -> Result<CorridorInspectionOutput> {
        request.corridor.validate()?;
        if request.width.get() <= 0.0 || request.width.get() > 100_000.0 {
            bail!("corridor width must be within (0, 100000] meters");
        }
        let line = request.corridor.to_geo()?;
        let restrictions = self
            .catalog
            .list_restrictions(scope)
            .await?
            .into_iter()
            .filter(|restriction| {
                restriction.cancelled_by.is_none()
                    && restriction.valid_from <= request.departure_time
                    && restriction
                        .valid_until
                        .is_none_or(|until| request.departure_time < until)
            })
            .filter(|restriction| {
                restriction.geometry.to_geo().is_ok_and(|polygon| {
                    polygon.intersects(&line)
                        || restriction.geometry.exterior.iter().any(|position| {
                            distance_to_corridor_m(position, &request.corridor.coordinates)
                                <= request.width.get()
                        })
                })
            })
            .collect();
        let facilities = self
            .analytics
            .list_facilities(&scope.tenant_key(), 10_000)?
            .into_iter()
            .filter(|facility| {
                distance_to_corridor_m(&facility.position, &request.corridor.coordinates)
                    <= request.width.get()
            })
            .take(500)
            .collect();
        Ok(CorridorInspectionOutput {
            restrictions,
            facilities,
            crossed_boundary_ids: self
                .analytics
                .intersecting_boundary_ids(&scope.tenant_key(), &request.corridor)?
                .into_iter()
                .map(|id| id.to_string())
                .collect(),
            data_gaps: Vec::new(),
        })
    }

    pub async fn publish_restriction(
        &self,
        scope: &MapScope,
        request: PublishRestrictionRequest,
    ) -> Result<Restriction> {
        self.catalog
            .create_restriction(scope, request.restriction)
            .await
    }

    pub async fn withdraw_restriction(
        &self,
        scope: &MapScope,
        request: WithdrawRestrictionRequest,
    ) -> Result<(Restriction, u64)> {
        let restriction = self
            .catalog
            .restriction(scope, &request.restriction_id)
            .await?
            .context("unknown restriction")?;
        let restriction = self
            .catalog
            .withdraw_restriction(
                scope,
                restriction,
                request.expected_record_version,
                request.effective_at,
                request.cancellation_restriction_id,
            )
            .await?;
        let invalidated = self
            .catalog
            .invalidate_routes_for_restriction(scope, &restriction.restriction_id)
            .await?;
        Ok((restriction, invalidated))
    }
}

fn distance_to_corridor_m(point: &Wgs84Position, corridor: &[Wgs84Position]) -> f64 {
    corridor
        .windows(2)
        .map(|segment| point_segment_distance_m(point, &segment[0], &segment[1]))
        .fold(f64::INFINITY, f64::min)
}

fn point_segment_distance_m(
    point: &Wgs84Position,
    start: &Wgs84Position,
    end: &Wgs84Position,
) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_008.8;
    let latitude_origin =
        ((start.latitude_deg + end.latitude_deg + point.latitude_deg) / 3.0).to_radians();
    let project = |position: &Wgs84Position| {
        let x = position.longitude_deg.to_radians() * latitude_origin.cos() * EARTH_RADIUS_M;
        let y = position.latitude_deg.to_radians() * EARTH_RADIUS_M;
        (x, y)
    };
    let (px, py) = project(point);
    let (ax, ay) = project(start);
    let (bx, by) = project(end);
    let dx = bx - ax;
    let dy = by - ay;
    let length_squared = dx * dx + dy * dy;
    if length_squared == 0.0 {
        return (px - ax).hypot(py - ay);
    }
    let t = (((px - ax) * dx + (py - ay) * dy) / length_squared).clamp(0.0, 1.0);
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(longitude_deg: f64, latitude_deg: f64) -> Wgs84Position {
        Wgs84Position::new(longitude_deg, latitude_deg, None).unwrap()
    }

    #[test]
    fn corridor_distance_uses_segment_not_only_vertices() {
        let distance = distance_to_corridor_m(
            &position(0.5, 0.01),
            &[position(0.0, 0.0), position(1.0, 0.0)],
        );
        assert!((1_000.0..1_200.0).contains(&distance));
    }
}
