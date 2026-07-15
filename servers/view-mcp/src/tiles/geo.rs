use glam::DVec3;

use crate::{
    contract::Wgs84Position3d,
    geodesy::{WGS84_EQUATORIAL_RADIUS_M, geodetic_to_ecef},
};

use super::traversal::WorldVolume;

const REGION_OBB_MAX_SPAN_RAD: f64 = 0.5;

pub fn region_to_ecef_volume(region: &[f64; 6]) -> WorldVolume {
    let [west, south, east, north, min_h, max_h] = *region;
    let span_lon = east - west;
    let span_lat = north - south;
    if !span_lon.is_finite()
        || !span_lat.is_finite()
        || span_lon <= 0.0
        || span_lat <= 0.0
        || span_lon > REGION_OBB_MAX_SPAN_RAD
        || span_lat > REGION_OBB_MAX_SPAN_RAD
    {
        return WorldVolume::Sphere {
            center: DVec3::ZERO,
            radius: WGS84_EQUATORIAL_RADIUS_M + max_h.max(0.0),
        };
    }

    let center_lat = (south + north) * 0.5;
    let center_lon = (west + east) * 0.5;
    let center_position = Wgs84Position3d {
        latitude_degrees: center_lat.to_degrees(),
        longitude_degrees: center_lon.to_degrees(),
        ellipsoidal_height_meters: 0.0,
    };
    let center = geodetic_to_ecef(center_position);
    let (sin_lat, cos_lat) = center_lat.sin_cos();
    let (sin_lon, cos_lon) = center_lon.sin_cos();
    let east_axis = DVec3::new(-sin_lon, cos_lon, 0.0);
    let north_axis = DVec3::new(-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat);
    let up_axis = DVec3::new(cos_lat * cos_lon, cos_lat * sin_lon, sin_lat);

    let mut low = DVec3::splat(f64::INFINITY);
    let mut high = DVec3::splat(f64::NEG_INFINITY);
    for latitude_step in 0..=2 {
        for longitude_step in 0..=2 {
            let latitude = south + span_lat * f64::from(latitude_step) / 2.0;
            let longitude = west + span_lon * f64::from(longitude_step) / 2.0;
            for height in [min_h, max_h] {
                let point = geodetic_to_ecef(Wgs84Position3d {
                    latitude_degrees: latitude.to_degrees(),
                    longitude_degrees: longitude.to_degrees(),
                    ellipsoidal_height_meters: height,
                });
                let delta = point - center;
                let local = DVec3::new(
                    delta.dot(east_axis),
                    delta.dot(north_axis),
                    delta.dot(up_axis),
                );
                low = low.min(local);
                high = high.max(local);
            }
        }
    }
    let half = (high - low) * 0.5;
    let offset = (high + low) * 0.5;
    WorldVolume::Obb {
        center: center + east_axis * offset.x + north_axis * offset.y + up_axis * offset.z,
        half_axes: [east_axis * half.x, north_axis * half.y, up_axis * half.z],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antimeridian_region_is_conservative() {
        let region = [
            179_f64.to_radians(),
            -1_f64.to_radians(),
            (-179_f64).to_radians(),
            1_f64.to_radians(),
            0.0,
            100.0,
        ];
        let (center, radius) = region_to_ecef_volume(&region).bounding_sphere();
        assert_eq!(center, DVec3::ZERO);
        assert!(radius >= WGS84_EQUATORIAL_RADIUS_M);
    }

    #[test]
    fn continent_scale_region_is_conservative() {
        let region = [
            (-130_f64).to_radians(),
            10_f64.to_radians(),
            (-55_f64).to_radians(),
            75_f64.to_radians(),
            -100.0,
            10_000.0,
        ];
        let (center, radius) = region_to_ecef_volume(&region).bounding_sphere();
        assert_eq!(center, DVec3::ZERO);
        assert!(radius >= WGS84_EQUATORIAL_RADIUS_M);
    }
}
