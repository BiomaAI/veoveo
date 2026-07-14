use anyhow::{Context, Result, bail};
use geo::{Covers, Intersects, Validation};
use geographiclib_rs::{DirectGeodesic, Geodesic, InverseGeodesic};
use proj::Proj;

use crate::contract::{
    Degrees, GeodesicDirectOutput, GeodesicDirectRequest, GeodesicInverseOutput,
    GeodesicInverseRequest, GeofenceRule, GeofenceViolation, Meters, ProjectedPosition,
    TransformCrsOutput, TransformCrsRequest, ValidateGeofenceOutput, ValidateGeofenceRequest,
    Wgs84Position,
};

pub fn transform_crs(request: TransformCrsRequest) -> Result<TransformCrsOutput> {
    if request.positions.is_empty() {
        bail!("transform_crs requires at least one position");
    }
    if request.source_crs.as_str() == "EPSG:4978" || request.target_crs.as_str() == "EPSG:4978" {
        bail!("transform_crs is two-dimensional and does not accept geocentric EPSG:4978");
    }
    let transformer = Proj::new_known_crs(
        request.source_crs.as_str(),
        request.target_crs.as_str(),
        None,
    )
    .with_context(|| {
        format!(
            "creating PROJ transform {} to {}",
            request.source_crs, request.target_crs
        )
    })?;
    let info = transformer.proj_info();
    let approximation_used = info
        .description
        .as_deref()
        .is_some_and(|description| description.to_ascii_lowercase().contains("ballpark"));
    if approximation_used && !request.allow_approximation {
        bail!("PROJ selected a ballpark transformation; explicit acceptance is required");
    }
    let positions = request
        .positions
        .into_iter()
        .map(|position| {
            position.validate()?;
            if position.crs != request.source_crs {
                bail!(
                    "position CRS {} does not match request source CRS {}",
                    position.crs,
                    request.source_crs
                );
            }
            if position.z.is_some() {
                bail!("transform_crs does not copy or transform vertical coordinates");
            }
            let (x, y) = transformer
                .convert((position.x, position.y))
                .with_context(|| {
                    format!(
                        "transforming position from {} to {}",
                        request.source_crs, request.target_crs
                    )
                })?;
            Ok(ProjectedPosition {
                crs: request.target_crs.clone(),
                x,
                y,
                z: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(TransformCrsOutput {
        positions,
        engine: info.description.unwrap_or_else(|| "PROJ".to_owned()),
        approximation_used,
        accuracy: (info.accuracy >= 0.0)
            .then(|| Meters::new(info.accuracy))
            .transpose()?,
    })
}

pub fn geodesic_inverse(request: GeodesicInverseRequest) -> Result<GeodesicInverseOutput> {
    request.start.validate()?;
    request.end.validate()?;
    let (distance, initial, final_azimuth, _arc): (f64, f64, f64, f64) = Geodesic::wgs84().inverse(
        request.start.latitude_deg,
        request.start.longitude_deg,
        request.end.latitude_deg,
        request.end.longitude_deg,
    );
    Ok(GeodesicInverseOutput {
        distance: Meters::new(distance)?,
        initial_azimuth: Degrees::new(normalize_bearing(initial))?,
        final_azimuth: Degrees::new(normalize_bearing(final_azimuth))?,
        engine: "geographiclib-rs:wgs84".to_owned(),
    })
}

pub fn geodesic_direct(request: GeodesicDirectRequest) -> Result<GeodesicDirectOutput> {
    request.start.validate()?;
    if request.initial_azimuth.get() > 360.0 {
        bail!("initial azimuth must be within [0, 360]");
    }
    let (latitude_deg, longitude_deg, final_azimuth): (f64, f64, f64) = Geodesic::wgs84().direct(
        request.start.latitude_deg,
        request.start.longitude_deg,
        request.initial_azimuth.get(),
        request.distance.get(),
    );
    Ok(GeodesicDirectOutput {
        end: Wgs84Position::new(
            longitude_deg,
            latitude_deg,
            request.start.ellipsoidal_height_m,
        )?,
        final_azimuth: Degrees::new(normalize_bearing(final_azimuth))?,
        engine: "geographiclib-rs:wgs84".to_owned(),
    })
}

pub fn validate_geofence(request: ValidateGeofenceRequest) -> Result<ValidateGeofenceOutput> {
    let polygon = request.geofence.to_geo()?;
    let path = request.path.to_geo()?;
    if !polygon.is_valid() {
        bail!("geofence polygon is not topologically valid");
    }
    let valid = match request.rule {
        GeofenceRule::MustRemainInside => polygon.covers(&path),
        GeofenceRule::MustRemainOutside => !polygon.intersects(&path),
        GeofenceRule::MustNotCrossBoundary => {
            !polygon.exterior().intersects(&path)
                && polygon
                    .interiors()
                    .iter()
                    .all(|interior| !interior.intersects(&path))
        }
    };
    let violations = if valid {
        Vec::new()
    } else {
        request
            .path
            .coordinates
            .windows(2)
            .enumerate()
            .filter_map(|(index, segment)| {
                let line = geo_types::LineString::new(vec![
                    segment[0].coordinate(),
                    segment[1].coordinate(),
                ]);
                let violates = match request.rule {
                    GeofenceRule::MustRemainInside => !polygon.covers(&line),
                    GeofenceRule::MustRemainOutside => polygon.intersects(&line),
                    GeofenceRule::MustNotCrossBoundary => {
                        polygon.exterior().intersects(&line)
                            || polygon
                                .interiors()
                                .iter()
                                .any(|interior| interior.intersects(&line))
                    }
                };
                violates.then(|| GeofenceViolation {
                    segment_index: index as u32,
                    position: segment[0].clone(),
                    reason: match request.rule {
                        GeofenceRule::MustRemainInside => {
                            "segment is not fully covered by the geofence".to_owned()
                        }
                        GeofenceRule::MustRemainOutside => {
                            "segment intersects the geofence".to_owned()
                        }
                        GeofenceRule::MustNotCrossBoundary => {
                            "segment crosses a geofence boundary".to_owned()
                        }
                    },
                })
            })
            .collect()
    };
    Ok(ValidateGeofenceOutput { valid, violations })
}

fn normalize_bearing(value: f64) -> f64 {
    value.rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{Wgs84LineString, Wgs84Polygon};

    fn position(longitude_deg: f64, latitude_deg: f64) -> Wgs84Position {
        Wgs84Position::new(longitude_deg, latitude_deg, None).unwrap()
    }

    #[test]
    fn geodesic_inverse_uses_ellipsoidal_distance() {
        let output = geodesic_inverse(GeodesicInverseRequest {
            start: position(-89.2, 13.69),
            end: position(-89.19, 13.7),
        })
        .unwrap();
        assert!(output.distance.get() > 1_000.0);
        assert!(output.distance.get() < 2_000.0);
    }

    #[test]
    fn geofence_validation_checks_segments_not_only_vertices() {
        let polygon = Wgs84Polygon {
            exterior: vec![
                position(0.0, 0.0),
                position(2.0, 0.0),
                position(2.0, 2.0),
                position(0.0, 2.0),
                position(0.0, 0.0),
            ],
            interiors: Vec::new(),
        };
        let output = validate_geofence(ValidateGeofenceRequest {
            geofence: polygon,
            path: Wgs84LineString {
                coordinates: vec![position(-1.0, 1.0), position(3.0, 1.0)],
            },
            rule: GeofenceRule::MustRemainInside,
        })
        .unwrap();
        assert!(!output.valid);
        assert_eq!(output.violations.len(), 1);
    }
}
