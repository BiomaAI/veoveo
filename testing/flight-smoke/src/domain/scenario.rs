use super::*;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UavAcceptanceScenario {
    pub(super) schema: String,
    pub(super) session_id: String,
    pub(super) geospatial_layer_id: String,
    pub(super) world: FrameWorldScenario,
    pub(super) vehicle_id: String,
    pub(super) world_ready_timeout_seconds: u64,
    pub(super) takeoff: TakeoffScenario,
    pub(super) camera: CameraAcceptance,
    pub(super) map_mobility_profile_uri: String,
    pub(super) mission: MissionScenario,
    pub(super) recording: RecordingAcceptance,
    pub(super) stream: StreamScenario,
    pub(super) reason: ReasonScenario,
    pub(super) view: ViewAcceptance,
    pub(super) landing_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FrameWorldScenario {
    pub(super) world_id: FrameWorldId,
    pub(super) display_name: String,
    pub(super) description: String,
    pub(super) simulation_frame_id: FrameId,
    pub(super) tree: FrameWorldTree,
}

impl FrameWorldScenario {
    pub(super) fn origin(&self) -> Result<&Wgs84Position> {
        self.tree
            .frames
            .iter()
            .find_map(|frame| match &frame.parent_transform {
                Some(FrameParentTransform::GeodeticTangent { origin }) => Some(origin),
                _ => None,
            })
            .context("world tree omitted a geodetic tangent anchor")
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TakeoffScenario {
    pub(super) relative_altitude_m: f64,
    pub(super) minimum_reached_altitude_m: f64,
    pub(super) state_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CameraAcceptance {
    pub(super) stream_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MissionScenario {
    pub(super) longitude_offset_degrees: f64,
    pub(super) speed_mps: f64,
    pub(super) hold_seconds: f64,
    pub(super) task_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecordingAcceptance {
    pub(super) live_rows_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StreamScenario {
    pub(super) live_pipeline_id: String,
    pub(super) minimum_live_frames: u64,
    pub(super) maximum_result_age_ms: u64,
    pub(super) live_timeout_seconds: u64,
    pub(super) recording_replay: RecordingReplayAcceptance,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecordingReplayAcceptance {
    pub(super) range_lag_seconds: f64,
    pub(super) freshness_probe_duration_seconds: f64,
    pub(super) range_duration_seconds: f64,
    pub(super) maximum_frames: u64,
    pub(super) task_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReasonScenario {
    pub(super) prompt: String,
    pub(super) maximum_frames: u64,
    pub(super) task_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ViewAcceptance {
    pub(super) timeout_seconds: u64,
    pub(super) minimum_mission_sensor_frames: u64,
    pub(super) camera: ViewCameraAcceptance,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ViewCameraAcceptance {
    pub(super) width_px: u32,
    pub(super) height_px: u32,
    pub(super) frame_rate_millihertz: u32,
    pub(super) vertical_fov_degrees: f64,
    pub(super) near_clip_m: f64,
    pub(super) far_clip_m: f64,
    pub(super) offset_flu_m: ViewOffset,
    pub(super) smoothing_seconds: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ViewOffset {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) z: f64,
}
impl UavAcceptanceScenario {
    pub(super) fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)
            .with_context(|| format!("reading UAV acceptance scenario {}", path.display()))?;
        let scenario: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("decoding UAV acceptance scenario {}", path.display()))?;
        scenario.validate()?;
        Ok(scenario)
    }

    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "veoveo.uav-sim-acceptance/v11",
            "unsupported UAV acceptance scenario schema {:?}",
            self.schema
        );
        validate_identity("session_id", &self.session_id)?;
        validate_identity("geospatial_layer_id", &self.geospatial_layer_id)?;
        validate_identity("vehicle_id", &self.vehicle_id)?;
        parse_mobility_profile_uri(&self.map_mobility_profile_uri)?;
        ensure!(
            !self.world.display_name.trim().is_empty(),
            "world display name must not be blank"
        );
        ensure!(
            !self.world.description.trim().is_empty(),
            "world description must not be blank"
        );
        ensure!(
            self.world
                .tree
                .frames
                .iter()
                .any(|frame| frame.frame_id == self.world.simulation_frame_id),
            "simulation_frame_id must identify a frame in the world tree"
        );
        ensure!(
            self.world
                .tree
                .frames
                .iter()
                .filter(|frame| {
                    frame.parent_frame_id.is_none() && frame.basis == FrameBasis::EcefWgs84
                })
                .count()
                == 1,
            "world tree must contain one ECEF root"
        );
        let origin = self.world.origin()?;
        ensure!(
            origin.latitude_degrees.is_finite()
                && (-90.0..=90.0).contains(&origin.latitude_degrees)
                && origin.longitude_degrees.is_finite()
                && (-180.0..=180.0).contains(&origin.longitude_degrees)
                && origin.ellipsoid_height_m.is_finite()
                && (-100_000.0..=100_000.0).contains(&origin.ellipsoid_height_m),
            "origin must contain bounded WGS84 coordinates"
        );
        ensure!(
            self.takeoff.relative_altitude_m.is_finite()
                && (1.0..=10_000.0).contains(&self.takeoff.relative_altitude_m),
            "takeoff.relative_altitude_m must be between 1 and 10000"
        );
        ensure!(
            self.takeoff.minimum_reached_altitude_m.is_finite()
                && self.takeoff.minimum_reached_altitude_m > 0.0
                && self.takeoff.minimum_reached_altitude_m <= self.takeoff.relative_altitude_m,
            "takeoff.minimum_reached_altitude_m must be positive and no higher than takeoff"
        );
        ensure!(
            self.world_ready_timeout_seconds > 0
                && self.takeoff.state_timeout_seconds > 0
                && self.camera.stream_timeout_seconds > 0
                && self.mission.task_timeout_seconds > 0
                && self.recording.live_rows_timeout_seconds > 0
                && self.stream.live_timeout_seconds > 0
                && self.stream.recording_replay.task_timeout_seconds > 0
                && self.view.timeout_seconds > 0
                && self.landing_timeout_seconds > 0,
            "scenario timeouts must be positive"
        );
        ensure!(
            self.mission.longitude_offset_degrees.is_finite()
                && self.mission.longitude_offset_degrees.abs() <= 1.0
                && self.mission.longitude_offset_degrees != 0.0
                && self.mission.speed_mps.is_finite()
                && (0.1..=100.0).contains(&self.mission.speed_mps)
                && self.mission.hold_seconds.is_finite()
                && (0.0..=3_600.0).contains(&self.mission.hold_seconds),
            "mission parameters are outside the accepted flight envelope"
        );
        ensure!(
            !self.stream.live_pipeline_id.trim().is_empty()
                && self.stream.minimum_live_frames > 0
                && self.stream.maximum_result_age_ms > 0
                && self.stream.live_timeout_seconds > 0,
            "Stream live acceptance must require a pipeline, frames, freshness, and timeout"
        );
        let replay = &self.stream.recording_replay;
        ensure!(
            replay.range_lag_seconds.is_finite()
                && replay.range_lag_seconds >= 0.0
                && replay.range_lag_seconds <= 2.0
                && replay.freshness_probe_duration_seconds.is_finite()
                && replay.freshness_probe_duration_seconds > 0.0
                && replay.freshness_probe_duration_seconds <= replay.range_duration_seconds
                && replay.range_duration_seconds.is_finite()
                && replay.range_duration_seconds > 0.0
                && (1..=10_000).contains(&replay.maximum_frames)
                && replay.task_timeout_seconds > 0,
            "Stream replay must probe a positive range no more than two seconds behind the live edge"
        );
        ensure!(
            !self.reason.prompt.trim().is_empty()
                && self.reason.prompt.len() <= 8_192
                && (1..=1_024).contains(&self.reason.maximum_frames)
                && self.reason.task_timeout_seconds > 0,
            "reason parameters must define a bounded prompted observation"
        );
        let view = &self.view.camera;
        ensure!(
            (64..=7680).contains(&view.width_px)
                && (64..=4320).contains(&view.height_px)
                && (1_000..=240_000).contains(&view.frame_rate_millihertz)
                && view.vertical_fov_degrees.is_finite()
                && (1.0..179.0).contains(&view.vertical_fov_degrees)
                && view.near_clip_m.is_finite()
                && view.near_clip_m > 0.0
                && view.far_clip_m.is_finite()
                && view.far_clip_m > view.near_clip_m
                && [
                    view.offset_flu_m.x,
                    view.offset_flu_m.y,
                    view.offset_flu_m.z
                ]
                .into_iter()
                .all(f64::is_finite)
                && view.smoothing_seconds.is_finite()
                && (0.0..=60.0).contains(&view.smoothing_seconds)
                && self.view.minimum_mission_sensor_frames > 0,
            "view parameters must define one bounded follow camera and advancing mission checkpoint"
        );
        Ok(())
    }
}
pub(super) fn validate_identity(name: &str, value: &str) -> Result<()> {
    ensure!(
        (1..=128).contains(&value.len())
            && value
                .bytes()
                .all(|byte| { byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.') }),
        "{name} must contain 1-128 ASCII letters, digits, underscores, dashes, or dots"
    );
    Ok(())
}
