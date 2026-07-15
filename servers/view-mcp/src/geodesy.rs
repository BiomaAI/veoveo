use glam::{DMat3, DMat4, DQuat, DVec3, DVec4, Vec3Swizzles};

use crate::contract::{
    CameraDefinition, ContractError, GeodeticCameraPose, HeadingPitchRoll, Wgs84Position3d,
};

pub const WGS84_EQUATORIAL_RADIUS_M: f64 = 6_378_137.0;
pub const WGS84_FLATTENING: f64 = 1.0 / 298.257_223_563;
pub const WGS84_ECC_SQ: f64 = WGS84_FLATTENING * (2.0 - WGS84_FLATTENING);

pub fn geodetic_to_ecef(position: Wgs84Position3d) -> DVec3 {
    let (sin_lat, cos_lat) = position.latitude_degrees.to_radians().sin_cos();
    let (sin_lon, cos_lon) = position.longitude_degrees.to_radians().sin_cos();
    let n = WGS84_EQUATORIAL_RADIUS_M / (1.0 - WGS84_ECC_SQ * sin_lat * sin_lat).sqrt();
    DVec3::new(
        (n + position.ellipsoidal_height_meters) * cos_lat * cos_lon,
        (n + position.ellipsoidal_height_meters) * cos_lat * sin_lon,
        (n * (1.0 - WGS84_ECC_SQ) + position.ellipsoidal_height_meters) * sin_lat,
    )
}

pub fn ecef_to_geodetic(ecef: DVec3) -> Wgs84Position3d {
    let a = WGS84_EQUATORIAL_RADIUS_M;
    let b = a * (1.0 - WGS84_FLATTENING);
    let ep_sq = (a * a - b * b) / (b * b);
    let p = ecef.xy().length();
    let theta = (ecef.z * a).atan2(p * b);
    let (sin_theta, cos_theta) = theta.sin_cos();
    let longitude = ecef.y.atan2(ecef.x);
    let latitude =
        (ecef.z + ep_sq * b * sin_theta.powi(3)).atan2(p - WGS84_ECC_SQ * a * cos_theta.powi(3));
    let sin_lat = latitude.sin();
    let n = a / (1.0 - WGS84_ECC_SQ * sin_lat * sin_lat).sqrt();
    let height = p / latitude.cos() - n;
    Wgs84Position3d {
        latitude_degrees: latitude.to_degrees(),
        longitude_degrees: longitude.to_degrees(),
        ellipsoidal_height_meters: height,
    }
}

pub fn enu_basis(position: Wgs84Position3d) -> (DVec3, DVec3, DVec3) {
    let (sin_lat, cos_lat) = position.latitude_degrees.to_radians().sin_cos();
    let (sin_lon, cos_lon) = position.longitude_degrees.to_radians().sin_cos();
    let east = DVec3::new(-sin_lon, cos_lon, 0.0);
    let north = DVec3::new(-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat);
    let up = DVec3::new(cos_lat * cos_lon, cos_lat * sin_lon, sin_lat);
    (east, north, up)
}

pub fn world_from_ecef(origin: Wgs84Position3d) -> DMat4 {
    let (east, north, up) = enu_basis(origin);
    let basis = DMat3::from_cols(east, up, -north).transpose();
    let translation = -(basis * geodetic_to_ecef(origin));
    DMat4::from_cols(
        DVec4::from((basis.x_axis, 0.0)),
        DVec4::from((basis.y_axis, 0.0)),
        DVec4::from((basis.z_axis, 0.0)),
        DVec4::from((translation, 1.0)),
    )
}

pub fn resolve_camera(camera: &CameraDefinition) -> Result<GeodeticCameraPose, ContractError> {
    camera.clone().validate()?;
    match camera {
        CameraDefinition::Pose(pose) => Ok(pose.clone()),
        CameraDefinition::LookAt(camera) => {
            let hpr = orientation_toward(camera.eye, camera.target, 0.0)?;
            Ok(GeodeticCameraPose {
                position: camera.eye,
                orientation: hpr,
                vertical_fov_degrees: camera.vertical_fov_degrees,
            })
        }
        CameraDefinition::OrbitTarget(camera) => {
            let target_ecef = geodetic_to_ecef(camera.target);
            let (east, north, up) = enu_basis(camera.target);
            let azimuth = camera.azimuth_degrees.to_radians();
            let elevation = camera.elevation_degrees.to_radians();
            let horizontal = camera.distance_meters * elevation.cos();
            let offset = east * (horizontal * azimuth.sin())
                + north * (horizontal * azimuth.cos())
                + up * (camera.distance_meters * elevation.sin());
            let eye = ecef_to_geodetic(target_ecef + offset);
            let hpr = orientation_toward(eye, camera.target, 0.0)?;
            Ok(GeodeticCameraPose {
                position: eye,
                orientation: hpr,
                vertical_fov_degrees: camera.vertical_fov_degrees,
            })
        }
    }
}

fn orientation_toward(
    eye: Wgs84Position3d,
    target: Wgs84Position3d,
    roll_degrees: f64,
) -> Result<HeadingPitchRoll, ContractError> {
    let delta_ecef = geodetic_to_ecef(target) - geodetic_to_ecef(eye);
    let (east, north, up) = enu_basis(eye);
    let local = DVec3::new(
        delta_ecef.dot(east),
        delta_ecef.dot(north),
        delta_ecef.dot(up),
    );
    if local.length_squared() < 1e-12 {
        return Err(ContractError::CoincidentEyeAndTarget);
    }
    let horizontal = local.xy().length();
    Ok(HeadingPitchRoll {
        heading_degrees: local.x.atan2(local.y).to_degrees().rem_euclid(360.0),
        pitch_degrees: local.z.atan2(horizontal).to_degrees(),
        roll_degrees,
    })
}

pub fn camera_world_transform(pose: &GeodeticCameraPose, origin: Wgs84Position3d) -> DMat4 {
    let world_from_ecef = world_from_ecef(origin);
    let position = world_from_ecef.transform_point3(geodetic_to_ecef(pose.position));
    let h = pose.orientation.heading_degrees.to_radians();
    let p = pose.orientation.pitch_degrees.to_radians();
    let r = pose.orientation.roll_degrees.to_radians();

    let forward = DVec3::new(h.sin() * p.cos(), p.sin(), -h.cos() * p.cos()).normalize();
    let mut right = forward.cross(DVec3::Y).normalize_or(DVec3::X);
    let mut up = right.cross(forward).normalize();
    if r != 0.0 {
        let roll = DQuat::from_axis_angle(forward, r);
        right = roll * right;
        up = roll * up;
    }
    DMat4::from_cols(
        DVec4::from((right, 0.0)),
        DVec4::from((up, 0.0)),
        DVec4::from((-forward, 0.0)),
        DVec4::from((position, 1.0)),
    )
}

pub fn camera_ecef_basis(pose: &GeodeticCameraPose) -> (DVec3, DVec3, DVec3) {
    let heading = pose.orientation.heading_degrees.to_radians();
    let pitch = pose.orientation.pitch_degrees.to_radians();
    let roll = pose.orientation.roll_degrees.to_radians();
    let forward_local = DVec3::new(
        heading.sin() * pitch.cos(),
        heading.cos() * pitch.cos(),
        pitch.sin(),
    )
    .normalize();
    let mut right_local = DVec3::new(heading.cos(), -heading.sin(), 0.0).normalize();
    let mut up_local = right_local.cross(forward_local).normalize();
    if roll != 0.0 {
        let rotation = DQuat::from_axis_angle(forward_local, roll);
        right_local = rotation * right_local;
        up_local = rotation * up_local;
    }
    let (east, north, geodetic_up) = enu_basis(pose.position);
    let to_ecef =
        |local: DVec3| (east * local.x + north * local.y + geodetic_up * local.z).normalize();
    (
        to_ecef(forward_local),
        to_ecef(right_local),
        to_ecef(up_local),
    )
}

#[cfg(test)]
mod tests {
    use approx::assert_abs_diff_eq;

    use super::*;
    use crate::contract::{LookAtCamera, OrbitTargetCamera};

    fn stockholm(height: f64) -> Wgs84Position3d {
        Wgs84Position3d {
            latitude_degrees: 59.3293,
            longitude_degrees: 18.0686,
            ellipsoidal_height_meters: height,
        }
    }

    #[test]
    fn local_origin_cancels_planetary_translation() {
        let origin = stockholm(10.0);
        let local = world_from_ecef(origin).transform_point3(geodetic_to_ecef(origin));
        assert!(local.length() < 1e-6, "{local:?}");
    }

    #[test]
    fn orbit_resolves_to_requested_distance_and_target_heading() {
        let target = stockholm(0.0);
        let resolved = resolve_camera(&CameraDefinition::OrbitTarget(OrbitTargetCamera {
            target,
            distance_meters: 1_000.0,
            azimuth_degrees: 180.0,
            elevation_degrees: 30.0,
            vertical_fov_degrees: 45.0,
        }))
        .unwrap();
        let distance = (geodetic_to_ecef(resolved.position) - geodetic_to_ecef(target)).length();
        assert_abs_diff_eq!(distance, 1_000.0, epsilon = 0.1);
    }

    #[test]
    fn look_at_points_down_from_above() {
        let pose = resolve_camera(&CameraDefinition::LookAt(LookAtCamera {
            eye: stockholm(1_000.0),
            target: stockholm(0.0),
            vertical_fov_degrees: 45.0,
        }))
        .unwrap();
        assert!(pose.orientation.pitch_degrees < -89.0);
    }
}
