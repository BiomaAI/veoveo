use std::fmt;

use geo_types::{LineString, Polygon};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::CrsId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeometryError(&'static str);

impl fmt::Display for GeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for GeometryError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Wgs84Position {
    pub longitude_deg: f64,
    pub latitude_deg: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ellipsoidal_height_m: Option<f64>,
}

impl Wgs84Position {
    pub fn new(
        longitude_deg: f64,
        latitude_deg: f64,
        ellipsoidal_height_m: Option<f64>,
    ) -> Result<Self, GeometryError> {
        let value = Self {
            longitude_deg,
            latitude_deg,
            ellipsoidal_height_m,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), GeometryError> {
        if !self.longitude_deg.is_finite()
            || !self.latitude_deg.is_finite()
            || self
                .ellipsoidal_height_m
                .is_some_and(|height| !height.is_finite())
        {
            return Err(GeometryError("WGS84 coordinates must be finite"));
        }
        if !(-180.0..=180.0).contains(&self.longitude_deg) {
            return Err(GeometryError("longitude_deg must be within [-180, 180]"));
        }
        if !(-90.0..=90.0).contains(&self.latitude_deg) {
            return Err(GeometryError("latitude_deg must be within [-90, 90]"));
        }
        Ok(())
    }

    pub fn coordinate(&self) -> geo_types::Coord<f64> {
        geo_types::coord! { x: self.longitude_deg, y: self.latitude_deg }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectedPosition {
    pub crs: CrsId,
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z: Option<f64>,
}

impl ProjectedPosition {
    pub fn validate(&self) -> Result<(), GeometryError> {
        if !self.x.is_finite() || !self.y.is_finite() || self.z.is_some_and(|z| !z.is_finite()) {
            return Err(GeometryError("projected coordinates must be finite"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MapPosition {
    Wgs84(Wgs84Position),
    Projected(ProjectedPosition),
}

impl MapPosition {
    pub fn validate(&self) -> Result<(), GeometryError> {
        match self {
            Self::Wgs84(position) => position.validate(),
            Self::Projected(position) => position.validate(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Wgs84LineString {
    pub coordinates: Vec<Wgs84Position>,
}

impl Wgs84LineString {
    pub fn validate(&self) -> Result<(), GeometryError> {
        if self.coordinates.len() < 2 {
            return Err(GeometryError(
                "a line string requires at least two positions",
            ));
        }
        self.coordinates
            .iter()
            .try_for_each(Wgs84Position::validate)
    }

    pub fn to_geo(&self) -> Result<LineString<f64>, GeometryError> {
        self.validate()?;
        Ok(LineString::new(
            self.coordinates
                .iter()
                .map(Wgs84Position::coordinate)
                .collect(),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Wgs84Polygon {
    pub exterior: Vec<Wgs84Position>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interiors: Vec<Vec<Wgs84Position>>,
}

impl Wgs84Polygon {
    pub fn validate(&self) -> Result<(), GeometryError> {
        validate_ring(&self.exterior)?;
        self.interiors
            .iter()
            .try_for_each(|ring| validate_ring(ring))
    }

    pub fn to_geo(&self) -> Result<Polygon<f64>, GeometryError> {
        self.validate()?;
        Ok(Polygon::new(
            ring_to_geo(&self.exterior),
            self.interiors
                .iter()
                .map(|ring| ring_to_geo(ring))
                .collect(),
        ))
    }
}

fn validate_ring(ring: &[Wgs84Position]) -> Result<(), GeometryError> {
    if ring.len() < 4 {
        return Err(GeometryError(
            "a polygon ring requires at least four positions",
        ));
    }
    ring.iter().try_for_each(Wgs84Position::validate)?;
    let first = ring.first().expect("ring length checked");
    let last = ring.last().expect("ring length checked");
    if first.longitude_deg != last.longitude_deg || first.latitude_deg != last.latitude_deg {
        return Err(GeometryError("a polygon ring must be closed"));
    }
    Ok(())
}

fn ring_to_geo(ring: &[Wgs84Position]) -> LineString<f64> {
    LineString::new(ring.iter().map(Wgs84Position::coordinate).collect())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Wgs84BoundingBox {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl Wgs84BoundingBox {
    pub fn validate(&self) -> Result<(), GeometryError> {
        Wgs84Position::new(self.west, self.south, None)?;
        Wgs84Position::new(self.east, self.north, None)?;
        if self.south > self.north {
            return Err(GeometryError("bounding-box south must not exceed north"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wgs84_position_enforces_axis_ranges() {
        assert!(Wgs84Position::new(-89.0, 13.0, None).is_ok());
        assert!(Wgs84Position::new(181.0, 13.0, None).is_err());
        assert!(Wgs84Position::new(-89.0, 91.0, None).is_err());
    }

    #[test]
    fn polygon_requires_closed_rings() {
        let point = |longitude_deg, latitude_deg| Wgs84Position {
            longitude_deg,
            latitude_deg,
            ellipsoidal_height_m: None,
        };
        let polygon = Wgs84Polygon {
            exterior: vec![
                point(0.0, 0.0),
                point(1.0, 0.0),
                point(1.0, 1.0),
                point(0.0, 0.0),
            ],
            interiors: Vec::new(),
        };
        assert!(polygon.validate().is_ok());
    }
}
