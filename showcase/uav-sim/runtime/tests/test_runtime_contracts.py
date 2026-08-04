from __future__ import annotations

import os
import sys
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

import numpy as np
from pymavlink import mavutil

from veoveo_mcp.simulation_pose import (
    POSE_PROTOCOL_SCHEMA,
    PosePublisherStatus,
    entity_table_digest,
)
from veoveo_uav_sim.camera_quality import (
    measure_camera_frame,
    normalize_rgb_frame,
    should_record_camera_frame,
)
from veoveo_uav_sim.config import FleetLoopConfig, RuntimeConfig
from veoveo_uav_sim.contracts import ContractError, parse_command, parse_operation
from veoveo_uav_sim.fleet_loop import FleetLoopController, vehicle_loop_route
from veoveo_uav_sim.event_queue import NonBlockingEventQueue
from veoveo_uav_sim.geo import enu_to_geodetic, horizontal_distance_m
from veoveo_uav_sim.physics_batch import (
    FleetPhysicsLifecycle,
    IsaacFleetPhysicsBatch,
    RigidBodyBatchAccumulator,
)
from veoveo_uav_sim.pose import PhysicsCadenceGate, PoseProducer, entity_ids
from veoveo_uav_sim.px4 import Px4Commander, Px4CommandRejected
from veoveo_uav_sim.realtime import PeriodicDeadline, RealtimePhysicsClock
from veoveo_uav_sim.state import (
    RuntimeState,
    VehicleTelemetry,
    initial_runtime_timing,
)
from veoveo_uav_sim.vehicle_model import (
    PX4_IRIS_MOMENT_CONSTANT,
    PX4_IRIS_MOTOR_CONSTANT,
    PX4_IRIS_YAW_MOMENT_COEFFICIENT,
    Px4IrisThrustCurve,
)
from veoveo_uav_sim.stream_output import _annex_b_nals, _packetize_nal
from veoveo_uav_sim.world_config import (
    GeoreferenceOrigin,
    WorldConfiguration,
    WorldConfigurationError,
    WorldConfigurationSlot,
)


VALID_ENVIRONMENT = {
    "CESIUM_ION_ACCESS_TOKEN": "test-token",
    "UAV_SIM_CESIUM_ION_ASSET_ID": "2275207",
    "UAV_SIM_RECORDING_KEY": "019f7122-3d89-7d21-8312-8940d1e0f510",
    "UAV_SIM_SESSION_ID": "uav-showcase",
    "UAV_SIM_TILE_CACHE_POLICY": "persistent",
    "UAV_SIM_WORLD_SOURCE": "google_photorealistic_3d_tiles",
    "UAV_SIM_POSE_PRODUCER_ID": "uav-sim",
    "UAV_SIM_POSE_PRODUCER_SPIFFE_ID": ("spiffe://veoveo.local/simulation/uav-sim"),
    "UAV_SIM_POSE_EPOCH_ID": "epoch-1",
    "UAV_SIM_POSE_INGRESS_HOST": "simulation-view-pose",
    "UAV_SIM_POSE_INGRESS_PORT": "7443",
    "UAV_SIM_POSE_SERVER_HOSTNAME": "simulation-view-pose.veoveo.svc",
    "UAV_SIM_POSE_CA_CERTIFICATE": "/run/secrets/simulation-view-pose/ca.crt",
    "UAV_SIM_POSE_CLIENT_CERTIFICATE": ("/run/secrets/simulation-view-pose/tls.crt"),
    "UAV_SIM_POSE_CLIENT_PRIVATE_KEY": ("/run/secrets/simulation-view-pose/tls.key"),
}

WORLD = WorldConfiguration(
    revision_uri="frames://world/uav-showcase-new-york/revision/revision-1",
    spec_sha256="1" * 64,
    simulation_frame_uri=(
        "frames://world/uav-showcase-new-york/revision/revision-1/frame/isaac-world"
    ),
    georeference_origin=GeoreferenceOrigin(
        latitude_degrees=40.758,
        longitude_degrees=-73.9855,
        ellipsoid_height_m=-17.0,
    ),
)


class RuntimeConfigTests(unittest.TestCase):
    def test_google_tiles_are_mandatory_and_exact(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
        self.assertEqual(config.cesium_ion_asset_id, 2_275_207)
        self.assertEqual(config.tile_cache_policy.value, "persistent")

        invalid = {**VALID_ENVIRONMENT, "UAV_SIM_CESIUM_ION_ASSET_ID": "1"}
        with patch.dict(os.environ, invalid, clear=True):
            with self.assertRaisesRegex(ValueError, "Google Photorealistic 3D Tiles"):
                RuntimeConfig.from_environment()

    def test_direct_google_key_is_not_a_runtime_input(self) -> None:
        environment = {**VALID_ENVIRONMENT, "GOOGLE_MAPS_API_KEY": "not-used"}
        with patch.dict(os.environ, environment, clear=True):
            config = RuntimeConfig.from_environment()
        self.assertEqual(config.cesium_ion_access_token, "test-token")

    def test_default_render_cadence_matches_the_camera(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
        self.assertEqual(config.physics_hz, 60)
        self.assertEqual(config.rendering_hz, 2)
        self.assertEqual(config.rendering_hz, config.camera.fps)
        self.assertEqual(config.pose_cadence_hz, 20)
        self.assertEqual(config.pose_buffer_duration_ms, 500)
        self.assertEqual(config.px4_connect_timeout_seconds, 180.0)
        self.assertEqual(config.camera.vehicle_id, "uav-1")
        self.assertEqual(config.camera.bit_rate_bps, 750_000)
        self.assertEqual(config.recording.telemetry_hz, 5)
        self.assertEqual(config.recording.queue_capacity, 256)
        self.assertEqual(config.recording.map_provider.value, "openStreetMap")

        app_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "app.py"
        ).read_text()
        self.assertNotIn("omni.replicator", app_source)
        self.assertNotIn("import CameraSensor", app_source)
        self.assertIn("HydraRgbCameraSensor", app_source)
        self.assertIn("PoseProducer", app_source)
        self.assertNotIn("livestream", app_source.lower())
        self.assertNotIn("follow_camera", app_source)

    def test_recording_policy_is_typed_and_bounded(self) -> None:
        environment = {
            **VALID_ENVIRONMENT,
            "UAV_SIM_RECORDING_MAP_PROVIDER": "mapboxSatellite",
            "UAV_SIM_RECORDING_TELEMETRY_HZ": "4",
            "UAV_SIM_RECORDING_QUEUE_CAPACITY": "128",
        }
        with patch.dict(os.environ, environment, clear=True):
            config = RuntimeConfig.from_environment()
        self.assertEqual(config.recording.map_provider.value, "mapboxSatellite")
        self.assertEqual(config.recording.telemetry_hz, 4)
        self.assertEqual(config.recording.queue_capacity, 128)

        invalid = {
            **VALID_ENVIRONMENT,
            "UAV_SIM_RECORDING_MAP_PROVIDER": "automatic",
        }
        with patch.dict(os.environ, invalid, clear=True):
            with self.assertRaisesRegex(ValueError, "openStreetMap or mapboxSatellite"):
                RuntimeConfig.from_environment()

    def test_preconfiguration_state_uses_the_pose_timing_contract(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()

        timing = initial_runtime_timing(config)
        self.assertEqual(timing["physics_hz"], 60)
        self.assertEqual(timing["native_rendering_hz"], 2)
        self.assertEqual(timing["pose_cadence_hz"], 20)
        self.assertEqual(timing["pose_buffer_target_snapshots"], 10)

        server_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "server.py"
        ).read_text()
        self.assertIn('"timing": initial_runtime_timing(self._config)', server_source)
        self.assertIn("self._config.pose_cadence_hz", server_source)
        self.assertNotIn(
            "self._config.vehicle_count,\n                    self._config.rendering_hz",
            server_source,
        )

    def test_multi_instance_px4_has_distinct_gcs_ports(self) -> None:
        runtime_root = Path(__file__).parents[1]
        dockerfile = (runtime_root / "Dockerfile").read_text()
        px4_patch = (
            runtime_root / "patches" / "px4-1.17.0-multi-instance-gcs.patch"
        ).read_text()
        self.assertIn("git -C px4 apply --check /tmp/px4.patch", dockerfile)
        self.assertIn("udp_gcs_port_remote=$((14550+px4_instance))", px4_patch)

        pegasus_patch = (
            runtime_root / "patches" / "pegasus-5.1.0-isaac-6.0.1.patch"
        ).read_text()
        self.assertIn("if self._enable_lockstep:", pegasus_patch)
        self.assertIn("for _ in range(256):", pegasus_patch)
        self.assertIn("recv_match(blocking=False)", pegasus_patch)
        self.assertIn("-o $udp_gcs_port_remote", px4_patch)
        self.assertIn("param set-default SDLOG_BACKEND 0", px4_patch)

    def test_default_fleet_loop_encloses_manhattan(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            loop = RuntimeConfig.from_environment().fleet_loop
        self.assertEqual(loop.center_east_m, 1_700.0)
        self.assertEqual(loop.center_north_m, 3_000.0)
        self.assertEqual(loop.east_radius_m, 2_500.0)
        self.assertEqual(loop.north_radius_m, 9_000.0)
        self.assertEqual(loop.relative_altitude_m, 450.0)
        self.assertEqual(loop.takeoff_timeout_seconds, 420.0)

    def test_fleet_loop_rejects_an_excessive_separated_altitude(self) -> None:
        environment = {
            **VALID_ENVIRONMENT,
            "UAV_SIM_VEHICLE_COUNT": "4",
            "UAV_SIM_FLEET_LOOP_RELATIVE_ALTITUDE_M": "490",
            "UAV_SIM_FLEET_LOOP_VERTICAL_SEPARATION_M": "10",
        }
        with patch.dict(os.environ, environment, clear=True):
            with self.assertRaisesRegex(ValueError, "must not exceed 500"):
                RuntimeConfig.from_environment()

    def test_stream_publication_is_explicit_and_typed(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            self.assertIsNone(RuntimeConfig.from_environment().stream_publication)
        environment = {
            **VALID_ENVIRONMENT,
            "UAV_SIM_STREAM_HOST": "stream-mcp",
            "UAV_SIM_STREAM_PORT": "9000",
            "UAV_SIM_STREAM_PAYLOAD_TYPE": "96",
            "UAV_SIM_STREAM_SOURCE_VEHICLE_ID": "uav-1",
        }
        with patch.dict(os.environ, environment, clear=True):
            publication = RuntimeConfig.from_environment().stream_publication
        self.assertIsNotNone(publication)
        assert publication is not None
        self.assertEqual(publication.host, "stream-mcp")
        self.assertEqual(publication.port, 9000)

        with patch.dict(
            os.environ,
            {**VALID_ENVIRONMENT, "UAV_SIM_STREAM_PORT": "9000"},
            clear=True,
        ):
            with self.assertRaisesRegex(ValueError, "requires UAV_SIM_STREAM_HOST"):
                RuntimeConfig.from_environment()

    def test_pose_publication_is_mandatory_and_strongly_identified(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
            state = RuntimeState(config, WORLD).snapshot()
        publication = state["pose_publication"]
        self.assertEqual(publication["protocol_schema"], POSE_PROTOCOL_SCHEMA)
        self.assertEqual(publication["producer_id"], "uav-sim")
        self.assertEqual(
            publication["producer_spiffe_id"],
            "spiffe://veoveo.local/simulation/uav-sim",
        )
        self.assertEqual(publication["epoch_id"], "epoch-1")
        self.assertEqual(publication["cadence_hz"], config.pose_cadence_hz)
        self.assertEqual(state["timing"]["physics_hz"], 60)
        self.assertEqual(state["timing"]["native_rendering_hz"], 2)
        self.assertEqual(state["timing"]["pose_cadence_hz"], 20)
        self.assertEqual(
            publication["entity_table_digest"],
            str(entity_table_digest(1, entity_ids(config.vehicle_count))),
        )

    def test_pose_publication_rejects_invalid_spiffe_or_secret_paths(self) -> None:
        for override, message in (
            ({"UAV_SIM_POSE_PRODUCER_SPIFFE_ID": "https://example.test"}, "SPIFFE"),
            ({"UAV_SIM_POSE_CLIENT_PRIVATE_KEY": "tls.key"}, "absolute"),
        ):
            with self.subTest(override=override):
                with patch.dict(
                    os.environ,
                    {**VALID_ENVIRONMENT, **override},
                    clear=True,
                ):
                    with self.assertRaisesRegex(ValueError, message):
                        RuntimeConfig.from_environment()

    def test_nadir_camera_is_the_only_canonical_stream(self) -> None:
        with patch.dict(
            os.environ,
            {**VALID_ENVIRONMENT, "UAV_SIM_VEHICLE_COUNT": "4"},
            clear=True,
        ):
            state = RuntimeState(RuntimeConfig.from_environment(), WORLD).snapshot()
        self.assertEqual(len(state["cameras"]), 1)
        camera = state["cameras"][0]
        camera_path = camera["entity_path"]
        recording_path = state["recordings"][0]["camera_streams"][0]
        self.assertTrue(camera_path.endswith("/camera/down"))
        self.assertEqual(recording_path, camera_path)
        self.assertNotIn("front", camera_path)
        self.assertEqual(camera["codec"], "h264")
        self.assertEqual(camera["encoder"], "nvidia_nvenc")
        self.assertEqual(state["recordings"][0]["camera_streams"], [camera_path])

        recording_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "recording.py"
        ).read_text()
        self.assertIn('add_stream("h264_nvenc"', recording_source)
        self.assertNotIn("libx264", recording_source)
        self.assertIn("rr.send_blueprint(", recording_source)
        self.assertIn("make_active=True", recording_source)
        self.assertIn("make_default=True", recording_source)
        self.assertNotIn("default_blueprint=", recording_source)
        self.assertIn("rrb.MapView", recording_source)

    def test_recording_degradation_is_visible_without_blocking_readiness(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
        state = RuntimeState(config, WORLD)
        state.update_recording_publisher("degraded", 17, 9, "network unavailable")
        recording = state.snapshot()["recordings"][0]
        self.assertTrue(recording["active"])
        self.assertEqual(recording["publisher_lifecycle"], "degraded")
        self.assertEqual(recording["queue_capacity"], 256)
        self.assertEqual(recording["queued_events"], 17)
        self.assertEqual(recording["dropped_events"], 9)
        self.assertEqual(recording["diagnostic"], "network unavailable")

    def test_camera_optics_and_mount_are_typed_runtime_inputs(self) -> None:
        environment = {
            **VALID_ENVIRONMENT,
            "UAV_SIM_CAMERA_FOCAL_LENGTH_MM": "12.5",
            "UAV_SIM_CAMERA_CLIPPING_NEAR_M": "0.1",
            "UAV_SIM_CAMERA_CLIPPING_FAR_M": "50000",
            "UAV_SIM_CAMERA_TRANSLATION_X_M": "0.75",
        }
        with patch.dict(os.environ, environment, clear=True):
            camera = RuntimeConfig.from_environment().camera
        self.assertEqual(camera.focal_length_mm, 12.5)
        self.assertEqual(camera.clipping_near_m, 0.1)
        self.assertEqual(camera.clipping_far_m, 50_000.0)
        self.assertEqual(camera.mount.translation_xyz_m, (0.75, 0.0, 0.05))

    def test_camera_vehicle_must_be_admitted_and_own_direct_stream(self) -> None:
        with patch.dict(
            os.environ,
            {
                **VALID_ENVIRONMENT,
                "UAV_SIM_VEHICLE_COUNT": "4",
                "UAV_SIM_CAMERA_VEHICLE_ID": "uav-5",
            },
            clear=True,
        ):
            with self.assertRaisesRegex(ValueError, "admitted fleet vehicle"):
                RuntimeConfig.from_environment()
        with patch.dict(
            os.environ,
            {
                **VALID_ENVIRONMENT,
                "UAV_SIM_VEHICLE_COUNT": "4",
                "UAV_SIM_CAMERA_VEHICLE_ID": "uav-1",
                "UAV_SIM_STREAM_HOST": "stream-mcp",
                "UAV_SIM_STREAM_SOURCE_VEHICLE_ID": "uav-2",
            },
            clear=True,
        ):
            with self.assertRaisesRegex(ValueError, "must match"):
                RuntimeConfig.from_environment()

    def test_camera_mount_rejects_a_non_unit_quaternion(self) -> None:
        environment = {
            **VALID_ENVIRONMENT,
            "UAV_SIM_CAMERA_ORIENTATION_W": "1",
            "UAV_SIM_CAMERA_ORIENTATION_X": "1",
            "UAV_SIM_CAMERA_ORIENTATION_Y": "0",
            "UAV_SIM_CAMERA_ORIENTATION_Z": "0",
        }
        with patch.dict(os.environ, environment, clear=True):
            with self.assertRaisesRegex(ValueError, "unit quaternion"):
                RuntimeConfig.from_environment()

    def test_camera_clipping_range_must_be_ordered(self) -> None:
        environment = {
            **VALID_ENVIRONMENT,
            "UAV_SIM_CAMERA_CLIPPING_NEAR_M": "10",
            "UAV_SIM_CAMERA_CLIPPING_FAR_M": "1",
        }
        with patch.dict(os.environ, environment, clear=True):
            with self.assertRaisesRegex(ValueError, "must be less than"):
                RuntimeConfig.from_environment()


class StreamOutputTests(unittest.TestCase):
    def test_annex_b_access_units_preserve_each_nal(self) -> None:
        access_unit = (
            b"\x00\x00\x00\x01\x67\x01\x02"
            b"\x00\x00\x01\x68\x03"
            b"\x00\x00\x00\x01\x65\x04\x05"
        )
        self.assertEqual(
            _annex_b_nals(access_unit),
            [b"\x67\x01\x02", b"\x68\x03", b"\x65\x04\x05"],
        )

    def test_large_nal_uses_rfc_6184_fu_a_boundaries(self) -> None:
        nal = bytes([0x65]) + bytes(range(1, 16))
        fragments = _packetize_nal(nal, 8)
        self.assertGreater(len(fragments), 1)
        self.assertEqual(fragments[0][0], 0x7C)
        self.assertEqual(fragments[0][1], 0x85)
        self.assertEqual(fragments[-1][1], 0x45)
        reconstructed = bytes([nal[0]]) + b"".join(
            fragment[2:] for fragment in fragments
        )
        self.assertEqual(reconstructed, nal)


class RecordingIsolationTests(unittest.TestCase):
    def test_bounded_queue_replaces_oldest_without_waiting(self) -> None:
        events = NonBlockingEventQueue[int](2)
        events.offer(1)
        events.offer(2)
        events.offer(3)

        self.assertEqual(events.depth(), 2)
        self.assertEqual(events.dropped(), 1)
        self.assertEqual(events.take(0.01), 2)
        self.assertEqual(events.take(0.01), 3)

    def test_recording_work_never_runs_in_physics_callback(self) -> None:
        app_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "app.py"
        ).read_text()
        callback = app_source.split("def advance_physics", maxsplit=1)[1].split(
            "physics_lifecycle =", maxsplit=1
        )[0]
        self.assertIn("recording.offer_frame", callback)
        self.assertNotIn("recording.log_frame", callback)
        self.assertNotIn("recording.log_imu", callback)
        self.assertNotIn(".encode(", callback)


class FleetLoopTests(unittest.TestCase):
    def test_routes_are_closed_and_separate_fleet_vehicles(self) -> None:
        config = FleetLoopConfig(
            relative_altitude_m=300.0,
            vertical_separation_m=10.0,
            takeoff_timeout_seconds=420.0,
            center_east_m=1_700.0,
            center_north_m=3_000.0,
            east_radius_m=2_500.0,
            north_radius_m=9_000.0,
            radial_separation_m=15.0,
            waypoint_count=12,
            speed_mps=12.0,
            hold_seconds=0.0,
        )
        first = vehicle_loop_route(config, WORLD.georeference_origin, 0, 4)
        second = vehicle_loop_route(config, WORLD.georeference_origin, 1, 4)

        self.assertEqual(len(first), 12)
        self.assertEqual(len(second), 12)
        self.assertAlmostEqual(
            first[0].ellipsoid_height_m,
            WORLD.georeference_origin.ellipsoid_height_m + 300.0,
            delta=5.0,
        )
        self.assertGreater(
            second[0].ellipsoid_height_m,
            first[0].ellipsoid_height_m + 5.0,
        )
        self.assertNotEqual(
            (first[0].latitude_degrees, first[0].longitude_degrees),
            (second[0].latitude_degrees, second[0].longitude_degrees),
        )

    def test_explicit_control_interrupts_and_relinquishes_default_loop(self) -> None:
        commander = _FleetLoopCommander()
        controller = FleetLoopController(
            FleetLoopConfig(
                relative_altitude_m=300.0,
                vertical_separation_m=10.0,
                takeoff_timeout_seconds=420.0,
                center_east_m=1_700.0,
                center_north_m=3_000.0,
                east_radius_m=2_500.0,
                north_radius_m=9_000.0,
                radial_separation_m=15.0,
                waypoint_count=4,
                speed_mps=12.0,
                hold_seconds=0.0,
            ),
            WORLD.georeference_origin,
            {"uav-1": commander},
        )
        controller.start()
        self.assertTrue(commander.mission_started.wait(2.0))

        controller.take_control(("uav-1",), timeout_seconds=2.0)
        self.assertTrue(commander.mission_interrupted)
        self.assertIsNone(commander.last_timeout_seconds)
        self.assertFalse(commander.interrupt.is_set())
        controller.close()


class RigidBodyBatchTests(unittest.TestCase):
    def test_fleet_batch_is_bound_only_after_reset_and_rebinds_atomically(self) -> None:
        events: list[str] = []
        prefix = "/World/uav_1"

        class FakeWorld:
            def __init__(self) -> None:
                self.physics_sim_view = object()
                self.callbacks = {
                    prefix + suffix: object()
                    for suffix in ("/state", "/update", "/Sensors", "/mav_state")
                }

            def physics_callback_exists(self, name: str) -> bool:
                return name in self.callbacks

            def remove_physics_callback(self, name: str) -> None:
                events.append(f"remove:{name}")
                del self.callbacks[name]

            def reset(self) -> None:
                self.assert_no_callbacks_during_reset()
                events.append("reset")

            def assert_no_callbacks_during_reset(self) -> None:
                if self.callbacks:
                    raise AssertionError("physics callback survived reset boundary")

            def add_physics_callback(self, name: str, callback: object) -> None:
                events.append(f"add:{name}")
                self.callbacks[name] = callback

        class FakeBatch:
            def rebind(self, _simulation_view: object) -> None:
                events.append("rebind")

            def refresh_states(self) -> None:
                events.append("refresh")

            def flush_forces(self) -> None:
                events.append("flush")

        class FakeVehicle:
            def bind_physics_batch(self, _batch: FakeBatch) -> None:
                events.append("bind")

            def update_state(self, _dt: float) -> None:
                events.append("state")

            def update(self, _dt: float) -> None:
                events.append("dynamics")

            def update_sensors(self, _dt: float) -> None:
                events.append("sensors")

            def update_sim_state(self, _dt: float) -> None:
                events.append("backend")

        world = FakeWorld()
        batch = FakeBatch()
        lifecycle = FleetPhysicsLifecycle(
            world,
            {"uav-1": FakeVehicle()},
            {"uav-1": prefix},
            (prefix + "/body",),
            batch_factory=lambda _paths, _simulation_view: (
                events.append("create") or batch
            ),
            after_step=lambda _dt: events.append("after"),
        )

        self.assertIs(lifecycle.reset(), batch)
        self.assertEqual(
            events[-4:],
            [
                "reset",
                "create",
                "bind",
                "add:/World/veoveo_uav_fleet/physics_batch",
            ],
        )

        events.clear()
        self.assertIs(lifecycle.reset(), batch)
        self.assertEqual(
            events,
            [
                "remove:/World/veoveo_uav_fleet/physics_batch",
                "reset",
                "rebind",
                "bind",
                "add:/World/veoveo_uav_fleet/physics_batch",
            ],
        )

        events.clear()
        callback = world.callbacks["/World/veoveo_uav_fleet/physics_batch"]
        callback(0.004)  # type: ignore[operator]
        self.assertEqual(
            events,
            [
                "refresh",
                "state",
                "dynamics",
                "sensors",
                "backend",
                "flush",
                "after",
            ],
        )

    def test_force_at_position_is_reduced_to_force_and_torque(self) -> None:
        batch = RigidBodyBatchAccumulator(("/World/uav_1/body",))
        batch.queue_force(
            "/World/uav_1/body",
            (0.0, 0.0, 4.0),
            (0.0, 2.0, 0.0),
        )
        batch.queue_torque("/World/uav_1/body", (0.0, 0.0, 3.0))

        np.testing.assert_array_equal(batch.forces, [[0.0, 0.0, 4.0]])
        np.testing.assert_array_equal(batch.torques, [[8.0, 0.0, 3.0]])

        forces = batch.forces
        torques = batch.torques
        batch.clear_forces()
        self.assertIs(batch.forces, forces)
        self.assertIs(batch.torques, torques)
        np.testing.assert_array_equal(batch.forces, np.zeros((1, 3)))
        np.testing.assert_array_equal(batch.torques, np.zeros((1, 3)))

    def test_submitted_tensor_uses_the_live_warp_owned_accumulator(self) -> None:
        class FakeArray:
            def __init__(self, value: np.ndarray) -> None:
                self.value = value

            def numpy(self) -> np.ndarray:
                return self.value

        class FakeDevice:
            is_cuda = True

            def __str__(self) -> str:
                return "cuda:0"

        class FakeWarp:
            float32 = np.float32
            uint32 = np.uint32

            def __init__(self) -> None:
                self.synchronized = False

            def get_device(self, _device: object) -> FakeDevice:
                return FakeDevice()

            def zeros(
                self,
                shape: int | tuple[int, ...],
                *,
                dtype: object,
                device: object,
            ) -> FakeArray:
                return FakeArray(np.zeros(shape, dtype=dtype))

            def array(
                self,
                value: np.ndarray,
                *,
                dtype: object,
                device: object,
            ) -> FakeArray:
                return FakeArray(np.array(value, dtype=dtype, copy=True))

            def copy(self, target: FakeArray, source: FakeArray) -> None:
                np.copyto(target.value, source.value)

            def synchronize_stream(self, _device: object) -> None:
                self.synchronized = True

        class FakeRigidBodyView:
            prim_paths = ("/World/uav_1/body",)

            def __init__(self) -> None:
                self.submitted_forces: np.ndarray | None = None

            def apply_forces_and_torques_at_position(
                self,
                forces: FakeArray,
                _torques: FakeArray,
                _positions: object,
                _indices: FakeArray,
                _is_global: bool,
            ) -> None:
                self.submitted_forces = forces.value.copy()

        class FakeSimulationView:
            device = "cuda:0"

            def __init__(self) -> None:
                self.rigid_body_view = FakeRigidBodyView()

            def set_subspace_roots(self, _root: str) -> None:
                pass

            def create_rigid_body_view(
                self, _paths: list[str]
            ) -> FakeRigidBodyView:
                return self.rigid_body_view

        fake_warp = FakeWarp()
        simulation_view = FakeSimulationView()
        with patch.dict(sys.modules, {"warp": fake_warp}):
            batch = IsaacFleetPhysicsBatch(
                ("/World/uav_1/body",), simulation_view
            )
            batch.queue_force(
                "/World/uav_1/body", (0.0, 0.0, 4.0), (0.0, 0.0, 0.0)
            )
            batch.flush_forces()

        self.assertTrue(fake_warp.synchronized)
        np.testing.assert_array_equal(
            simulation_view.rigid_body_view.submitted_forces,
            [0.0, 0.0, 4.0],
        )

    def test_one_state_batch_serves_distinct_vehicle_bodies(self) -> None:
        paths = ("/World/uav_1/body", "/World/uav_2/body")
        batch = RigidBodyBatchAccumulator(paths)
        transforms = np.array(
            [
                [1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 1.0],
                [4.0, 5.0, 6.0, 0.0, 0.0, 1.0, 0.0],
            ],
            dtype=np.float32,
        )
        velocities = np.array(
            [
                [7.0, 8.0, 9.0, 0.1, 0.2, 0.3],
                [10.0, 11.0, 12.0, 0.4, 0.5, 0.6],
            ],
            dtype=np.float32,
        )
        batch.update_states(transforms, velocities)

        first = batch.state(paths[0])
        second = batch.state(paths[1])
        np.testing.assert_array_equal(first.position_xyz, [1.0, 2.0, 3.0])
        np.testing.assert_array_equal(
            first.orientation_xyzw, [0.0, 0.0, 0.0, 1.0]
        )
        np.testing.assert_array_equal(
            second.linear_velocity_xyz, [10.0, 11.0, 12.0]
        )
        np.testing.assert_allclose(
            second.angular_velocity_xyz, [0.4, 0.5, 0.6]
        )

    def test_unknown_body_and_non_finite_input_fail_closed(self) -> None:
        batch = RigidBodyBatchAccumulator(("/World/uav_1/body",))
        with self.assertRaisesRegex(RuntimeError, "outside the admitted fleet"):
            batch.queue_torque("/World/uav_2/body", (0.0, 0.0, 0.0))
        with self.assertRaisesRegex(RuntimeError, "non-finite"):
            batch.queue_force(
                "/World/uav_1/body", (float("nan"), 0.0, 0.0), (0.0, 0.0, 0.0)
            )


class _FleetLoopStatus:
    def __init__(self, flight_state: str) -> None:
        self.flight_state = flight_state


class _FleetLoopCommander:
    def __init__(self) -> None:
        self.flight_state = "standby"
        self.interrupt = threading.Event()
        self.mission_started = threading.Event()
        self.mission_interrupted = False
        self.last_timeout_seconds: float | None = 1_800.0

    def status(self) -> _FleetLoopStatus:
        return _FleetLoopStatus(self.flight_state)

    def arm(self) -> None:
        self.flight_state = "armed"

    def takeoff(self, _relative_altitude_m: float) -> None:
        self.flight_state = "flying"

    def execute_mission(
        self,
        _waypoints: tuple[object, ...],
        timeout_seconds: float | None = 1_800.0,
    ) -> int:
        self.last_timeout_seconds = timeout_seconds
        self.mission_started.set()
        if not self.interrupt.wait(2.0):
            return 1
        self.mission_interrupted = True
        raise RuntimeError("mission interrupted")

    def interrupt_mission(self) -> None:
        self.interrupt.set()

    def clear_mission_interrupt(self) -> None:
        self.interrupt.clear()


class PoseProducerTests(unittest.TestCase):
    def test_pose_cadence_is_exact_and_independent_of_render_steps(self) -> None:
        gate = PhysicsCadenceGate(physics_hz=250, output_hz=20)
        due_steps = [step for step in range(1, 251) if gate.due(step)]
        self.assertEqual(len(due_steps), 20)
        self.assertEqual(due_steps[0], 13)
        self.assertEqual(due_steps[-1], 250)
        self.assertEqual(set(np.diff(due_steps)), {12, 13})

        gate.reset()
        self.assertFalse(gate.due(1))
        with self.assertRaisesRegex(RuntimeError, "increase monotonically"):
            gate.due(1)

    def test_complete_snapshots_keep_a_monotonic_renderer_timeline(self) -> None:
        publishers: list[_FakePosePublisher] = []

        def create_publisher(*args: object, **kwargs: object) -> _FakePosePublisher:
            publisher = _FakePosePublisher(*args, **kwargs)
            publishers.append(publisher)
            return publisher

        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
        updates: list[dict[str, object]] = []
        with patch(
            "veoveo_uav_sim.pose.OrderedPosePublisher",
            side_effect=create_publisher,
        ):
            producer = PoseProducer(
                config=config.pose_publication,
                session_id=config.session_id,
                world=WORLD,
                vehicle_count=1,
                cadence_hz=20,
                buffer_duration_ms=50,
                update_state=updates.append,
            )
            telemetry = VehicleTelemetry(
                vehicle_id="uav-1",
                position_enu=(1.0, 2.0, 3.0),
                attitude_xyzw=(0.0, 0.0, 0.0, 1.0),
                linear_velocity_enu_mps=(4.0, 5.0, 6.0),
                flight_state="flying",
                battery_percent=90.0,
                px4_connected=True,
            )
            producer.offer([telemetry])
            producer.offer([telemetry])
            deadline = time.monotonic() + 1.0
            while len(publishers[0].snapshots) < 2 and time.monotonic() < deadline:
                time.sleep(0.005)
            producer.close()

        snapshots = publishers[0].snapshots
        self.assertEqual([snapshot.sequence for snapshot in snapshots], [1, 2])
        self.assertEqual(
            [snapshot.simulation_timestamp_ns for snapshot in snapshots],
            [50_000_000, 100_000_000],
        )
        self.assertEqual(len(snapshots[0].entities), 1)
        self.assertIsNone(snapshots[0].entities[0].velocity)
        self.assertEqual(
            snapshots[0].entity_table_digest,
            entity_table_digest(1, entity_ids(1)),
        )
        self.assertTrue(any(update["lifecycle"] == "ready" for update in updates))
        self.assertEqual(updates[-1]["lifecycle"], "stopped")

    def test_incomplete_entity_snapshots_are_rejected(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
        with patch(
            "veoveo_uav_sim.pose.OrderedPosePublisher",
            _FakePosePublisher,
        ):
            producer = PoseProducer(
                config=config.pose_publication,
                session_id=config.session_id,
                world=WORLD,
                vehicle_count=1,
                cadence_hz=20,
                buffer_duration_ms=50,
                update_state=lambda _publication: None,
            )
            with self.assertRaisesRegex(RuntimeError, "complete snapshot"):
                producer.offer([])
            producer.close()

    def test_lazy_tls_startup_does_not_replace_buffered_poses(self) -> None:
        publishers: list[_LazyFakePosePublisher] = []

        def create_publisher(*args: object, **kwargs: object) -> _LazyFakePosePublisher:
            publisher = _LazyFakePosePublisher(*args, **kwargs)
            publishers.append(publisher)
            return publisher

        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
        with patch(
            "veoveo_uav_sim.pose.OrderedPosePublisher",
            side_effect=create_publisher,
        ):
            producer = PoseProducer(
                config=config.pose_publication,
                session_id=config.session_id,
                world=WORLD,
                vehicle_count=1,
                cadence_hz=20,
                buffer_duration_ms=50,
                update_state=lambda _publication: None,
            )
            telemetry = VehicleTelemetry(
                vehicle_id="uav-1",
                position_enu=(1.0, 2.0, 3.0),
                attitude_xyzw=(0.0, 0.0, 0.0, 1.0),
                linear_velocity_enu_mps=(4.0, 5.0, 6.0),
                flight_state="flying",
                battery_percent=90.0,
                px4_connected=True,
            )
            producer.offer([telemetry])
            producer.offer([telemetry])
            deadline = time.monotonic() + 1.0
            while publishers[0].sent_snapshots < 2 and time.monotonic() < deadline:
                time.sleep(0.005)
            producer.close()

        self.assertEqual(publishers[0].offered_snapshots, 2)
        self.assertEqual(publishers[0].sent_snapshots, 2)
        self.assertEqual(publishers[0].replaced_snapshots, 0)


class RealtimeClockTests(unittest.TestCase):
    def test_physics_clock_never_batches_missed_actuator_intervals(self) -> None:
        now = [100.0]
        clock = RealtimePhysicsClock(60, clock=lambda: now[0])

        self.assertEqual(clock.due_steps(0), 0)
        now[0] += 0.09
        self.assertEqual(clock.due_steps(0), 1)
        self.assertEqual(clock.due_steps(1), 0)
        self.assertGreater(clock.seconds_until_next_step(1), 0.0)

    def test_physics_clock_discards_wall_lag_instead_of_replaying_it(self) -> None:
        now = [10.0]
        clock = RealtimePhysicsClock(60, clock=lambda: now[0])
        now[0] += 2.0

        self.assertEqual(clock.due_steps(0), 1)
        status = clock.status()
        self.assertEqual(status.rebases, 1)
        self.assertAlmostEqual(status.discarded_wall_seconds, 119 / 60)

    def test_render_deadline_skips_missed_periods(self) -> None:
        now = [20.0]
        deadline = PeriodicDeadline(2, clock=lambda: now[0])
        self.assertTrue(deadline.due())
        self.assertFalse(deadline.due())
        now[0] += 1.6
        self.assertTrue(deadline.due())
        self.assertGreater(deadline.seconds_until_due(), 0.0)


class Px4IrisVehicleModelTests(unittest.TestCase):
    def test_yaw_coefficient_matches_pinned_px4_iris_contract(self) -> None:
        self.assertEqual(PX4_IRIS_MOTOR_CONSTANT, 5.84e-6)
        self.assertEqual(PX4_IRIS_MOMENT_CONSTANT, 0.06)
        self.assertAlmostEqual(
            PX4_IRIS_YAW_MOMENT_COEFFICIENT,
            PX4_IRIS_MOTOR_CONSTANT * PX4_IRIS_MOMENT_CONSTANT,
        )

    def test_motor_response_uses_bounded_asymmetric_first_order_dynamics(self) -> None:
        model = Px4IrisThrustCurve()
        model.set_input_reference([1100.0] * 4)
        force, rising_velocity, moment = model.update(None, 1.0 / 60.0)
        self.assertTrue(all(0.0 < value < 1100.0 for value in rising_velocity))
        self.assertTrue(all(value > 0.0 for value in force))
        self.assertAlmostEqual(moment, 0.0)

        model.set_input_reference([0.0] * 4)
        _, falling_velocity, _ = model.update(None, 1.0 / 60.0)
        self.assertTrue(
            all(0.0 < after < before for after, before in zip(falling_velocity, rising_velocity))
        )

    def test_motor_model_preserves_px4_rotor_yaw_directions(self) -> None:
        model = Px4IrisThrustCurve()
        model.set_input_reference([900.0, 900.0, 300.0, 300.0])
        _, _, moment = model.update(None, 1.0)
        self.assertLess(moment, 0.0)


class _FakePosePublisher:
    def __init__(self, *_args: object, **_kwargs: object) -> None:
        self.snapshots: list[object] = []
        self.closed = False

    def offer(self, snapshot: object) -> None:
        self.snapshots.append(snapshot)

    def status(self) -> PosePublisherStatus:
        sent = len(self.snapshots)
        last_sequence = (
            getattr(self.snapshots[-1], "sequence") if self.snapshots else None
        )
        return PosePublisherStatus(
            running=not self.closed,
            connected=not self.closed,
            offered_snapshots=sent,
            sent_snapshots=sent,
            replaced_snapshots=0,
            last_sent_sequence=last_sequence,
            last_error=None,
        )

    def close(self) -> None:
        self.closed = True


class _LazyFakePosePublisher:
    def __init__(self, *_args: object, **_kwargs: object) -> None:
        self.closed = False
        self.offered_snapshots = 0
        self.sent_snapshots = 0
        self.replaced_snapshots = 0
        self._pending_sequence: int | None = None
        self._pending_since = 0.0
        self._last_sent_sequence: int | None = None

    def offer(self, snapshot: object) -> None:
        self.offered_snapshots += 1
        if self._pending_sequence is not None:
            self.replaced_snapshots += 1
        self._pending_sequence = int(getattr(snapshot, "sequence"))
        self._pending_since = time.monotonic()

    def status(self) -> PosePublisherStatus:
        if (
            self._pending_sequence is not None
            and time.monotonic() - self._pending_since >= 0.05
        ):
            self.sent_snapshots += 1
            self._last_sent_sequence = self._pending_sequence
            self._pending_sequence = None
        return PosePublisherStatus(
            running=not self.closed,
            connected=self.sent_snapshots > 0 and not self.closed,
            offered_snapshots=self.offered_snapshots,
            sent_snapshots=self.sent_snapshots,
            replaced_snapshots=self.replaced_snapshots,
            last_sent_sequence=self._last_sent_sequence,
            last_error=None,
        )

    def close(self) -> None:
        self.closed = True


class AdapterContractTests(unittest.TestCase):
    def test_commands_reject_unknown_fields(self) -> None:
        with self.assertRaises(ContractError):
            parse_command(
                {
                    "command": "arm",
                    "session_id": "uav-showcase",
                    "vehicle_id": "uav-1",
                    "legacy_vehicle": "one",
                }
            )

    def test_missions_require_the_expected_world_revision(self) -> None:
        mission = parse_operation(
            {
                "operation": "execute_mission",
                "input": {
                    "session_id": "uav-showcase",
                    "mission_id": "mission-1",
                    "expected_world_revision_uri": WORLD.revision_uri,
                    "vehicles": [
                        {
                            "vehicle_id": "uav-1",
                            "waypoints": [
                                {
                                    "position": {
                                        "latitude_degrees": 13.6929,
                                        "longitude_degrees": -89.2182,
                                        "ellipsoid_height_m": 705.0,
                                    },
                                    "speed_mps": 3.0,
                                    "hold_seconds": 0.0,
                                }
                            ],
                        }
                    ],
                },
            }
        )
        self.assertEqual(
            mission.expected_world_revision_uri,
            WORLD.revision_uri,
        )

    def test_enu_origin_round_trips_to_wgs84(self) -> None:
        latitude, longitude, height = enu_to_geodetic(
            0.0, 0.0, 0.0, 13.6929, -89.2182, 700.0
        )
        self.assertAlmostEqual(latitude, 13.6929, places=8)
        self.assertAlmostEqual(longitude, -89.2182, places=8)
        self.assertAlmostEqual(height, 700.0, places=4)

    def test_horizontal_distance_resolves_short_uav_waypoints(self) -> None:
        distance = horizontal_distance_m(13.6929, -89.2182, 13.6929, -89.21818)
        self.assertGreater(distance, 2.0)
        self.assertLess(distance, 2.3)


class _MavlinkMessage:
    def __init__(self, message_type: str, **fields: object) -> None:
        self._message_type = message_type
        for name, value in fields.items():
            setattr(self, name, value)

    def get_type(self) -> str:
        return self._message_type

    def get_srcSystem(self) -> int:
        return 1


class _MavlinkSender:
    def __init__(self, events: list[str]) -> None:
        self.commands: list[tuple[int, tuple[float, ...]]] = []
        self._events = events

    def heartbeat_send(self, *_args: object) -> None:
        pass

    def command_long_send(
        self,
        _target_system: int,
        _target_component: int,
        command: int,
        _confirmation: int,
        *parameters: float,
    ) -> None:
        self.commands.append((command, parameters))
        self._events.append(f"send:{command}")


class _MavlinkConnection:
    def __init__(self, acknowledgements: list[int]) -> None:
        self.events: list[str] = []
        self.mav = _MavlinkSender(self.events)
        self._messages: list[_MavlinkMessage] = []
        for command in acknowledgements:
            self._messages.append(
                _MavlinkMessage(
                    "COMMAND_ACK",
                    command=command,
                    result=mavutil.mavlink.MAV_RESULT_ACCEPTED,
                )
            )
            if command == mavutil.mavlink.MAV_CMD_DO_SET_MODE:
                _base_mode, custom_mode, custom_sub_mode = mavutil.px4_map["LOITER"]
                self._messages.append(
                    _MavlinkMessage(
                        "HEARTBEAT",
                        base_mode=(mavutil.mavlink.MAV_MODE_FLAG_CUSTOM_MODE_ENABLED),
                        custom_mode=(custom_mode << 16 | custom_sub_mode << 24),
                    )
                )
        self._messages.append(
            _MavlinkMessage(
                "HEARTBEAT",
                base_mode=mavutil.mavlink.MAV_MODE_FLAG_SAFETY_ARMED,
                custom_mode=0,
            )
        )

    def recv_match(self, **_kwargs: object) -> _MavlinkMessage:
        message = self._messages.pop(0)
        self.events.append(f"receive:{message.get_type()}")
        return message


class Px4CommanderTests(unittest.TestCase):
    def test_initial_arm_retries_temporary_preflight_rejection(self) -> None:
        connection = _MavlinkConnection([mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM])
        connection._messages.insert(
            0,
            _MavlinkMessage(
                "COMMAND_ACK",
                command=mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM,
                result=mavutil.mavlink.MAV_RESULT_TEMPORARILY_REJECTED,
            ),
        )
        commander = Px4Commander(instance=0, origin_height_m=-17.0)
        commander._connection = connection
        commander._connected = True

        with patch("veoveo_uav_sim.px4.ARM_RETRY_INTERVAL_SECONDS", 0.0):
            commander.arm()

        self.assertEqual(
            [command for command, _parameters in connection.mav.commands],
            [
                mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM,
                mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM,
            ],
        )

    def test_initial_arm_fails_immediately_on_permanent_rejection(self) -> None:
        connection = _MavlinkConnection([])
        connection._messages.insert(
            0,
            _MavlinkMessage(
                "COMMAND_ACK",
                command=mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM,
                result=mavutil.mavlink.MAV_RESULT_DENIED,
            ),
        )
        commander = Px4Commander(instance=0, origin_height_m=-17.0)
        commander._connection = connection
        commander._connected = True

        with self.assertRaises(Px4CommandRejected) as rejected:
            commander.arm()
        self.assertEqual(
            rejected.exception.result,
            mavutil.mavlink.MAV_RESULT_DENIED,
        )

    def test_rearm_exits_land_mode_before_arming(self) -> None:
        connection = _MavlinkConnection(
            [
                mavutil.mavlink.MAV_CMD_DO_SET_MODE,
                mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM,
            ]
        )
        commander = Px4Commander(instance=0, origin_height_m=-17.0)
        commander._connection = connection
        commander._connected = True
        commander._has_flown = True
        commander._landed_state = mavutil.mavlink.MAV_LANDED_STATE_ON_GROUND

        commander.arm()

        self.assertEqual(
            [command for command, _parameters in connection.mav.commands],
            [
                mavutil.mavlink.MAV_CMD_DO_SET_MODE,
                mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM,
            ],
        )
        loiter_mode = mavutil.px4_map["LOITER"]
        self.assertEqual(
            connection.mav.commands[0][1][:3],
            loiter_mode,
        )
        self.assertLess(
            connection.events.index("receive:HEARTBEAT"),
            connection.events.index(
                f"send:{mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM}"
            ),
        )

    def test_initial_arm_does_not_change_flight_mode(self) -> None:
        connection = _MavlinkConnection([mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM])
        commander = Px4Commander(instance=0, origin_height_m=-17.0)
        commander._connection = connection
        commander._connected = True

        commander.arm()

        self.assertEqual(
            [command for command, _parameters in connection.mav.commands],
            [mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM],
        )


class WorldConfigurationTests(unittest.TestCase):
    def test_world_binding_is_strict_and_typed(self) -> None:
        world = WorldConfiguration.from_request(
            {"session_id": "uav-showcase", "world": WORLD.as_dict()},
            "uav-showcase",
        )
        self.assertEqual(world, WORLD)

    def test_world_binding_rejects_a_frame_from_another_revision(self) -> None:
        payload = WORLD.as_dict()
        payload["simulation_frame_uri"] = (
            "frames://world/other/revision/revision-2/frame/isaac-world"
        )
        with self.assertRaisesRegex(WorldConfigurationError, "frame in revision_uri"):
            WorldConfiguration.from_request(
                {"session_id": "uav-showcase", "world": payload},
                "uav-showcase",
            )

    def test_world_slot_is_idempotent_and_immutable(self) -> None:
        slot = WorldConfigurationSlot()
        self.assertEqual(slot.configure(WORLD), WORLD)
        self.assertEqual(slot.configure(WORLD), WORLD)
        other = WorldConfiguration(
            revision_uri=("frames://world/uav-showcase-new-york/revision/revision-2"),
            spec_sha256="2" * 64,
            simulation_frame_uri=(
                "frames://world/uav-showcase-new-york/revision/revision-2/"
                "frame/isaac-world"
            ),
            georeference_origin=WORLD.georeference_origin,
        )
        with self.assertRaisesRegex(WorldConfigurationError, "different world"):
            slot.configure(other)


class CameraQualityTests(unittest.TestCase):
    def test_black_camera_frame_is_not_visible(self) -> None:
        quality = measure_camera_frame(np.zeros((48, 64, 3), dtype=np.uint8))
        self.assertFalse(quality.operational)
        self.assertFalse(quality.visible)
        self.assertEqual(quality.mean_luma, 0.0)
        self.assertEqual(quality.dynamic_range, 0)
        self.assertEqual(quality.non_black_fraction, 0.0)

    def test_visible_camera_frame_is_accepted(self) -> None:
        frame = np.zeros((48, 64, 3), dtype=np.uint8)
        frame[8:40, 8:56] = (32, 128, 224)
        quality = measure_camera_frame(frame)
        self.assertTrue(quality.operational)
        self.assertTrue(quality.visible)
        self.assertGreater(quality.mean_luma, 2.0)
        self.assertGreater(quality.dynamic_range, 8)
        self.assertGreater(quality.non_black_fraction, 0.02)

    def test_uniform_bright_frame_is_not_visible_content(self) -> None:
        frame = np.full((48, 64, 3), 128, dtype=np.uint8)
        quality = measure_camera_frame(frame)
        self.assertTrue(quality.operational)
        self.assertFalse(quality.visible)
        self.assertEqual(quality.dynamic_range, 0)

    def test_ready_world_keeps_uniform_camera_frames_in_recording(self) -> None:
        quality = measure_camera_frame(np.full((48, 64, 3), 128, dtype=np.uint8))
        self.assertFalse(quality.visible)
        self.assertTrue(should_record_camera_frame(quality, tiles_ready=True))

    def test_warming_world_withholds_non_visible_camera_frames(self) -> None:
        quality = measure_camera_frame(np.zeros((48, 64, 3), dtype=np.uint8))
        self.assertFalse(should_record_camera_frame(quality, tiles_ready=False))

    def test_normalized_float_rgb_is_scaled_before_encoding(self) -> None:
        frame = np.full((4, 4, 3), 0.5, dtype=np.float32)
        normalized = normalize_rgb_frame(frame)
        self.assertEqual(normalized.dtype, np.uint8)
        self.assertEqual(int(normalized[0, 0, 0]), 128)


if __name__ == "__main__":
    unittest.main()
