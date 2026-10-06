//! Admission of world observations through the UAV owner's contract.
use super::*;
use veoveo_uav_sim_mcp::contract::{
    CameraLifecycle, SimulationLifecycle, SimulationState, TileLifecycle,
};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum WorldReadiness {
    Warming(&'static str),
    Ready,
}

pub(super) fn world_readiness(
    state: &SimulationState,
    scenario: &UavAcceptanceScenario,
    revision: &FrameWorldRevisionUri,
    frame: &WorldFrameUri,
) -> Result<WorldReadiness> {
    ensure!(
        state.session_id == scenario.session_id,
        "UAV returned another session"
    );
    ensure!(
        !matches!(
            state.lifecycle,
            SimulationLifecycle::Failed
                | SimulationLifecycle::Stopping
                | SimulationLifecycle::Stopped
        ),
        "UAV runtime cannot become ready: {:?}",
        state.lifecycle
    );
    if let Some(world) = &state.world {
        ensure!(
            &world.revision_uri == revision && &world.simulation_frame_uri == frame,
            "UAV session uses the wrong immutable Frames world"
        );
    } else {
        ensure!(
            matches!(
                state.lifecycle,
                SimulationLifecycle::Unconfigured | SimulationLifecycle::Starting
            ),
            "running UAV session omitted its immutable world"
        );
    }
    ensure!(
        state.tiles.lifecycle != TileLifecycle::Degraded,
        "terrain failed: {:?}",
        state.tiles
    );
    ensure!(
        state.tiles.source == "google_photorealistic_3d_tiles"
            && state.tiles.ion_asset_id == GOOGLE_PHOTOREALISTIC_3D_TILES_ASSET_ID,
        "UAV returned the wrong terrain provider or asset"
    );
    for camera in &state.cameras {
        ensure!(
            !matches!(
                camera.lifecycle,
                CameraLifecycle::Failed | CameraLifecycle::Degraded
            ),
            "native NVENC camera failed: {camera:?}"
        );
    }
    for camera in &state.live_cameras {
        camera.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            !matches!(
                camera.health,
                LiveCameraHealth::Failed | LiveCameraHealth::Stale
            ),
            "operator camera failed: {camera:?}"
        );
    }
    if !state.stream_products.is_empty() {
        ensure!(
            camera_product_set_matches_contract(&state.stream_products),
            "tiled camera product violates its shared-stream contract"
        );
    }
    if state.world.is_none()
        || matches!(
            state.lifecycle,
            SimulationLifecycle::Unconfigured | SimulationLifecycle::Starting
        )
    {
        return Ok(WorldReadiness::Warming("runtime startup"));
    }
    if state.tiles.lifecycle != TileLifecycle::Ready
        || state.tiles.resident_tiles == 0
        || state.tiles.visible_tiles == 0
    {
        return Ok(WorldReadiness::Warming("terrain coverage"));
    }
    if !state
        .vehicles
        .iter()
        .any(|vehicle| vehicle.vehicle_id == scenario.vehicle_id && vehicle.px4_connected)
    {
        return Ok(WorldReadiness::Warming("selected PX4 vehicle connection"));
    }
    if !state.cameras.iter().any(|camera| {
        camera.vehicle_id == scenario.vehicle_id
            && camera.lifecycle == CameraLifecycle::Ready
            && camera.frames_observed >= 3
            && camera.last_access_unit_bytes > 0
    }) {
        return Ok(WorldReadiness::Warming("native NVENC access units"));
    }
    if state.live_cameras.is_empty()
        || state
            .live_cameras
            .iter()
            .any(|camera| camera.health != LiveCameraHealth::Healthy)
        || state.stream_products.is_empty()
    {
        return Ok(WorldReadiness::Warming("operator cameras"));
    }
    Ok(WorldReadiness::Ready)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed() -> (SimulationState, UavAcceptanceScenario) {
        let state =
            serde_json::from_str(include_str!("../../tests/fixtures/world-ready.json")).unwrap();
        let scenario = UavAcceptanceScenario::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../showcase/uav-sim/scenarios/new-york-aerial.json"),
        )
        .unwrap();
        (state, scenario)
    }

    #[test]
    fn running_session_waits_through_declared_warmup_then_admits() {
        let (mut state, scenario) = observed();
        let binding = state.world.clone().unwrap();
        let check = |state: &SimulationState| {
            world_readiness(
                state,
                &scenario,
                &binding.revision_uri,
                &binding.simulation_frame_uri,
            )
            .unwrap()
        };
        state.tiles.lifecycle = TileLifecycle::Streaming;
        assert_eq!(check(&state), WorldReadiness::Warming("terrain coverage"));
        state.tiles.lifecycle = TileLifecycle::Ready;
        state.cameras[0].frames_observed = 0;
        assert_eq!(
            check(&state),
            WorldReadiness::Warming("native NVENC access units")
        );
        state.cameras[0].frames_observed = 3;
        state.live_cameras[0].health = LiveCameraHealth::Warming;
        assert_eq!(check(&state), WorldReadiness::Warming("operator cameras"));
        state.live_cameras[0].health = LiveCameraHealth::Healthy;
        assert_eq!(check(&state), WorldReadiness::Ready);
    }

    #[test]
    fn warmup_does_not_hide_wrong_world_or_failed_hardware_products() {
        let (good, scenario) = observed();
        let binding = good.world.clone().unwrap();
        for index in 1..6 {
            let mut state = good.clone();
            state.tiles.lifecycle = TileLifecycle::Streaming;
            match index {
                1 => state.lifecycle = SimulationLifecycle::Failed,
                2 => state.tiles.lifecycle = TileLifecycle::Degraded,
                3 => state.cameras[0].lifecycle = CameraLifecycle::Failed,
                4 => state.live_cameras[0].health = LiveCameraHealth::Stale,
                _ => state.stream_products[0].lifecycle = LiveStreamProductLifecycle::Failed,
            }
            assert!(
                world_readiness(
                    &state,
                    &scenario,
                    &binding.revision_uri,
                    &binding.simulation_frame_uri
                )
                .is_err()
            );
        }
        let mut invalid_world = serde_json::to_value(&good).unwrap();
        invalid_world["world"]["spec_sha256"] = "not-a-digest".into();
        assert!(serde_json::from_value::<SimulationState>(invalid_world).is_err());
        let mut wire = serde_json::to_value(good).unwrap();
        wire["cameras"][0]["encoder"] = "software".into();
        assert!(serde_json::from_value::<SimulationState>(wire).is_err());
    }
}
