from __future__ import annotations

import json
import math
import os
import socket
import struct
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np
from pymavlink import mavutil
from veoveo_uav_sim.app import kit_live_render_arguments
from veoveo_uav_sim.config import (
    FleetLoopConfig,
    RuntimeConfig,
    StreamPublicationConfig,
)
from veoveo_uav_sim.contracts import ContractError, parse_command, parse_operation
from veoveo_uav_sim.event_queue import NonBlockingEventQueue
from veoveo_uav_sim.fleet_loop import FleetLoopController, vehicle_loop_route
from veoveo_uav_sim.geo import enu_to_geodetic, horizontal_distance_m
from veoveo_uav_sim.h264 import (
    annex_b_nals,
    make_decoder_reentrant,
    parse_native_h264_access_unit,
)
from veoveo_uav_sim.hydra_camera import (
    native_sensor_aov_arguments,
    native_sensor_aov_signal_port,
)
from veoveo_uav_sim.physical_camera import (
    physical_camera_path,
    physical_camera_product_name,
)
from veoveo_uav_sim.physics_batch import (
    FleetPhysicsLifecycle,
    FleetPhysicsTiming,
    IsaacFleetPhysicsBatch,
    RigidBodyBatchAccumulator,
)
from veoveo_uav_sim.px4 import Px4Commander, Px4CommandRejected
from veoveo_uav_sim.realtime import (
    FixedStepCadenceGate,
    MonotonicPhysicsClock,
)
from veoveo_uav_sim.rtsp_h264 import (
    H264RtpDepacketizer,
    RtpPacket,
    parse_rtp_packet,
)
from veoveo_uav_sim.runtime_events import (
    RUNTIME_EVENT_SCHEMA,
    notify_adapter_ready,
    notify_runtime_ready,
)
from veoveo_uav_sim.state import (
    RuntimeState,
    initial_runtime_timing,
)
from veoveo_uav_sim.stream_output import (
    StreamPublicationWorker,
    _packetize_nal,
    _rtp_timestamp,
)
from veoveo_uav_sim.tile_lifecycle import (
    NativeTileEvent,
    NativeTileEventBridge,
    TileLifecycleController,
)
from veoveo_uav_sim.vehicle_model import (
    PX4_IRIS_MOMENT_CONSTANT,
    PX4_IRIS_MOTOR_CONSTANT,
    PX4_IRIS_SENSOR_CADENCE,
    PX4_IRIS_YAW_MOMENT_COEFFICIENT,
    Px4IrisSensorCadence,
    Px4IrisThrustCurve,
    attitude_enu_flu_to_ned_frd,
    enu_to_ned_vector,
    flu_to_frd_vector,
    inverse_rotate_vector_xyzw,
    quaternion_multiply_xyzw,
)
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
    "UAV_SIM_RENDERING_HZ": "30",
    "UAV_SIM_LIVE_VIEWER_SLOTS": "2",
    "UAV_SIM_LIVE_ACTIVATION_TIMEOUT_SECONDS": "7.5",
    "UAV_SIM_LIVE_PUBLIC_MEDIA_IP": "127.0.0.1",
    "UAV_SIM_OPERATOR_CAMERAS_JSON": json.dumps(
        [
            {
                "cameraId": "follow",
                "revision": 1,
                "rig": {
                    "kind": "follow_entity",
                    "targetEntityId": "uav-1",
                    "eyeOffsetFluM": {"x": -12.0, "y": 2.0, "z": 4.0},
                    "targetOffsetFluM": {"x": 0.0, "y": 0.0, "z": 0.2},
                    "smoothing": {
                        "translationHalfLifeMs": 150,
                        "rotationHalfLifeMs": 120,
                        "teleportDistanceM": 100.0,
                        "resetAfterGapMs": 1000,
                    },
                },
                "optics": {
                    "widthPx": 1280,
                    "heightPx": 720,
                    "frameRateHz": 30,
                    "verticalFovDegrees": 60.0,
                    "nearClipM": 0.1,
                    "farClipM": 100000.0,
                },
                "streamPolicy": "continuous",
            }
        ]
    ),
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
    def test_rtp_timestamp_has_a_per_source_epoch_and_wraps(self) -> None:
        self.assertEqual(_rtp_timestamp(0x12345678, 0.0), 0x12345678)
        self.assertEqual(_rtp_timestamp(0xFFFF_FFF0, 1.0), 89_984)
        with self.assertRaisesRegex(ValueError, "finite and non-negative"):
            _rtp_timestamp(1, -1.0)

    def test_physical_camera_uses_a_stable_usd_identifier(self) -> None:
        self.assertEqual(
            physical_camera_path("uav-1"),
            "/World/PhysicalCameras/uav_1_down",
        )
        self.assertEqual(
            physical_camera_product_name("uav-1"),
            "physical_uav_1_down",
        )
        with self.assertRaisesRegex(ValueError, "vehicle identity is invalid"):
            physical_camera_path("uav/1")

    def test_px4_frame_transforms_do_not_require_scipy_objects(self) -> None:
        np.testing.assert_allclose(
            enu_to_ned_vector([1.0, 2.0, 3.0]), [2.0, 1.0, -3.0]
        )
        np.testing.assert_allclose(
            flu_to_frd_vector([1.0, 2.0, 3.0]), [1.0, -2.0, -3.0]
        )
        np.testing.assert_allclose(
            inverse_rotate_vector_xyzw([0.0, 0.0, 0.0, 1.0], [1.0, 2.0, 3.0]),
            [1.0, 2.0, 3.0],
        )
        quarter_turn_z = [0.0, 0.0, math.sqrt(0.5), math.sqrt(0.5)]
        np.testing.assert_allclose(
            inverse_rotate_vector_xyzw(quarter_turn_z, [0.0, 1.0, 0.0]),
            [1.0, 0.0, 0.0],
            atol=1.0e-12,
        )
        np.testing.assert_allclose(
            quaternion_multiply_xyzw(
                quarter_turn_z, [0.0, 0.0, 0.0, 1.0]
            ),
            quarter_turn_z,
        )
        converted = attitude_enu_flu_to_ned_frd([0.0, 0.0, 0.0, 1.0])
        self.assertAlmostEqual(float(np.linalg.norm(converted)), 1.0)
        np.testing.assert_allclose(
            converted, [0.0, 0.0, -math.sqrt(0.5), -math.sqrt(0.5)]
        )

    def test_px4_sensor_cadence_is_bounded_by_the_physics_clock(self) -> None:
        PX4_IRIS_SENSOR_CADENCE.validate_for_physics(60)
        self.assertEqual(PX4_IRIS_SENSOR_CADENCE.imu_hz, 60)
        self.assertEqual(PX4_IRIS_SENSOR_CADENCE.barometer_hz, 30)
        self.assertEqual(PX4_IRIS_SENSOR_CADENCE.magnetometer_hz, 30)
        self.assertEqual(PX4_IRIS_SENSOR_CADENCE.gps_hz, 10)

        with self.assertRaisesRegex(ValueError, "exceeds physics cadence"):
            Px4IrisSensorCadence(imu_hz=120).validate_for_physics(60)
        with self.assertRaisesRegex(ValueError, "must divide physics cadence"):
            Px4IrisSensorCadence(gps_hz=11).validate_for_physics(60)

    def test_authoritative_tick_does_not_wait_for_present_threads(self) -> None:
        arguments = kit_live_render_arguments()
        self.assertEqual(
            arguments,
            [
                "--/app/runLoops/main/rateLimitEnabled=false",
                "--/app/player/useFixedTimeStepping=false",
                "--/app/runLoops/main/syncToPresent=false",
                "--/app/runLoops/rendering_0/syncToPresent=false",
                "--/app/runLoops/rendering_1/syncToPresent=false",
                "--/app/runLoopsGlobal/syncToPresent=false",
                "--/exts/omni.kit.renderer.core/present/presentAfterRendering=false",
            ],
        )

    def test_google_tiles_are_mandatory_and_exact(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
        self.assertEqual(config.cesium_ion_asset_id, 2_275_207)
        self.assertEqual(config.tile_cache_policy.value, "persistent")
        self.assertEqual(config.tile_streaming.maximum_screen_space_error, 16.0)
        self.assertEqual(config.tile_streaming.maximum_simultaneous_loads, 20)
        self.assertEqual(config.tile_streaming.maximum_cached_bytes, 2_147_483_648)
        self.assertTrue(config.tile_streaming.preload_ancestors)
        self.assertTrue(config.tile_streaming.preload_siblings)
        self.assertTrue(config.tile_streaming.forbid_holes)
        self.assertEqual(
            config.runtime_event_socket,
            Path("/var/run/veoveo-uav-sim/runtime-events.sock"),
        )

        invalid_holes = {
            **VALID_ENVIRONMENT,
            "UAV_SIM_TILE_FORBID_HOLES": "sometimes",
        }
        with patch.dict(os.environ, invalid_holes, clear=True):
            with self.assertRaisesRegex(ValueError, "must be true or false"):
                RuntimeConfig.from_environment()

        invalid = {**VALID_ENVIRONMENT, "UAV_SIM_CESIUM_ION_ASSET_ID": "1"}
        with patch.dict(os.environ, invalid, clear=True):
            with self.assertRaisesRegex(ValueError, "Google Photorealistic 3D Tiles"):
                RuntimeConfig.from_environment()

    def test_runtime_ready_event_is_one_nonblocking_datagram(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            event_socket = Path(directory) / "runtime-events.sock"
            with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as receiver:
                receiver.bind(str(event_socket))
                receiver.settimeout(1.0)
                self.assertTrue(
                    notify_runtime_ready(
                        event_socket,
                        session_id="uav-showcase",
                        generation=7,
                    )
                )
                payload = json.loads(receiver.recv(1024))
        self.assertEqual(
            payload,
            {
                "schema": RUNTIME_EVENT_SCHEMA,
                "event": "ready",
                "sessionId": "uav-showcase",
                "generation": 7,
            },
        )

    def test_adapter_ready_event_is_one_nonblocking_datagram(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            event_socket = Path(directory) / "runtime-events.sock"
            with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as receiver:
                receiver.bind(str(event_socket))
                receiver.settimeout(1.0)
                self.assertTrue(
                    notify_adapter_ready(
                        event_socket,
                        session_id="uav-showcase",
                        generation=7,
                    )
                )
                payload = json.loads(receiver.recv(1024))
        self.assertEqual(
            payload,
            {
                "schema": RUNTIME_EVENT_SCHEMA,
                "event": "adapter_ready",
                "sessionId": "uav-showcase",
                "generation": 7,
            },
        )

    def test_runtime_ready_event_does_not_wait_for_a_missing_consumer(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            missing = Path(directory) / "runtime-events.sock"
            self.assertFalse(
                notify_runtime_ready(
                    missing,
                    session_id="uav-showcase",
                    generation=1,
                )
            )
        self.assertFalse(
            notify_runtime_ready(
                Path("/does/not/matter/runtime-events.sock"),
                session_id="",
                generation=0,
            )
        )

    def test_direct_google_key_is_not_a_runtime_input(self) -> None:
        environment = {**VALID_ENVIRONMENT, "GOOGLE_MAPS_API_KEY": "not-used"}
        with patch.dict(os.environ, environment, clear=True):
            config = RuntimeConfig.from_environment()
        self.assertEqual(config.cesium_ion_access_token, "test-token")

    def test_operator_render_cadence_is_independent_of_sensor_cadence(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
        self.assertEqual(config.physics_hz, 60)
        self.assertEqual(config.rendering_hz, 30)
        self.assertEqual(config.camera.fps, 2)
        self.assertEqual(config.operator_live_view.cameras[0].optics.frame_rate_hz, 30)
        self.assertEqual(config.operator_live_view.activation_timeout_seconds, 7.5)
        self.assertEqual(config.px4_connect_timeout_seconds, 180.0)
        self.assertEqual(config.camera.vehicle_id, "uav-1")
        self.assertEqual(config.recording.telemetry_hz, 5)
        self.assertEqual(config.recording.queue_capacity, 256)
        self.assertEqual(config.recording.map_provider.value, "openStreetMap")

        app_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "app.py"
        ).read_text()
        self.assertIn('"omni.kit.livestream.rtsp"', app_source)
        self.assertNotIn('"omni.replicator.nv"', app_source)
        self.assertNotIn('"omni.replicator.core"', app_source)
        self.assertNotIn("import CameraSensor", app_source)
        self.assertNotIn("RtxCamera", app_source)
        self.assertNotIn("isaacsim.sensors.experimental.rtx", app_source)
        self.assertIn("NativeH264CameraSensor", app_source)
        self.assertIn("create_physical_rgb_camera", app_source)
        self.assertIn("omni.kit.livestream.aov", app_source)
        self.assertIn("AuthoritativeOperatorCameraCollection", app_source)
        self.assertIn('"sync_loads": False', app_source)
        self.assertIn('"disable_viewport_updates": True', app_source)
        self.assertIn(
            '"--/exts/cesium.omniverse/externallyManagedViewports=true"',
            app_source,
        )
        self.assertNotIn("get_active_viewport", app_source)
        self.assertIn("current_pose_cesium_viewport", app_source)
        self.assertNotIn("follow_camera", app_source)
        self.assertNotIn("operator_camera_cadence", app_source)
        self.assertIn("update_operator_cameras()", app_source)
        self.assertNotIn("physical_camera_cadence", app_source)
        self.assertIn("render_fps=config.camera.fps", app_source)
        self.assertIn("sensor.observe_simulation_time(", app_source)
        self.assertIn("simulation_time_s, physics_step", app_source)

        operator_camera_source = (
            Path(__file__).parents[1]
            / "veoveo_uav_sim"
            / "operator_camera.py"
        ).read_text()
        self.assertIn("CreateHorizontalApertureAttr", operator_camera_source)

        hydra_camera_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "hydra_camera.py"
        ).read_text()
        operator_product_source = (
            Path(__file__).parents[1]
            / "veoveo_uav_sim"
            / "operator_products.py"
        ).read_text()
        product_sources = "".join(
            (hydra_camera_source, operator_product_source)
        )
        self.assertEqual(product_sources.count("get_frame_info"), 1)
        self.assertIn("is_async_low_latency=False", hydra_camera_source)
        self.assertIn("is_async_low_latency=False", operator_product_source)
        self.assertNotIn("AnnotatorRegistry", hydra_camera_source)
        self.assertNotIn("omni.replicator", hydra_camera_source)
        self.assertIn('"streamType": "rtsp"', hydra_camera_source)
        self.assertIn("RtspH264Receiver", hydra_camera_source)

    def test_native_sensor_aov_uses_one_internal_nvenc_stream(self) -> None:
        arguments = native_sensor_aov_arguments(
            "physical_uav_1_down",
            rtsp_port=8554,
            target_fps=2,
        )
        self.assertEqual(len(arguments), 5)
        self.assertTrue(all("physical_uav_1_down.LdrColor" in value for value in arguments))
        self.assertTrue(any(value.endswith("/streamType=rtsp") for value in arguments))
        self.assertTrue(any(value.endswith("/signalPort=8555") for value in arguments))
        self.assertTrue(any(value.endswith("/streamPort=8554") for value in arguments))
        self.assertEqual(native_sensor_aov_signal_port(8554), 8555)
        with self.assertRaisesRegex(ValueError, "between 1 and 65534"):
            native_sensor_aov_signal_port(65_535)

    def test_native_sensor_aov_ports_cannot_overlap_operator_products(self) -> None:
        environment = {
            **VALID_ENVIRONMENT,
            "UAV_SIM_LIVE_SIGNALING_PORT_BASE": "8555",
        }
        with patch.dict(os.environ, environment, clear=True):
            with self.assertRaisesRegex(ValueError, "AOV port ranges overlap at 8555"):
                RuntimeConfig.from_environment()

    def test_physical_capture_cadence_is_exact(self) -> None:
        cadence = FixedStepCadenceGate(60, 2)
        self.assertEqual(
            [step for step in range(1, 121) if cadence.due(step)],
            [30, 60, 90, 120],
        )

    def test_headless_cesium_has_one_authoritative_viewport_writer(self) -> None:
        runtime_root = Path(__file__).parents[1]
        dockerfile = (runtime_root / "Dockerfile").read_text()
        patch = (
            runtime_root
            / "patches"
            / "cesium-0.29.0-external-viewports.patch"
        ).read_text()
        self.assertIn("cesium-0.29.0-external-viewports.patch", dockerfile)
        self.assertIn("externallyManagedViewports", patch)
        self.assertIn("externallyManagedViewports = false", patch)
        self.assertIn("if not settings.get_as_bool", patch)

        lifecycle_patch = (
            runtime_root
            / "patches"
            / "cesium-0.29.0-lifecycle-events.patch"
        ).read_text()
        native_patch = (
            runtime_root
            / "patches"
            / "cesium-native-ca0311f-tile-load-events.patch"
        ).read_text()
        self.assertIn("cesium-0.29.0-lifecycle-events.patch", dockerfile)
        self.assertIn("cesium-native-ca0311f-tile-load-events.patch", dockerfile)
        self.assertIn("TILESET_LOAD_FAILED", lifecycle_patch)
        self.assertIn("TileContent", native_patch)
        self.assertNotIn("releases/download", dockerfile)

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

    def test_preconfiguration_state_uses_authoritative_runtime_timing(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()

        timing = initial_runtime_timing(config)
        self.assertEqual(timing["physics_hz"], 60)
        self.assertEqual(timing["native_rendering_hz"], 30)
        self.assertEqual(timing["render_cycles"], 0)
        self.assertEqual(timing["physics_steps"], 0)
        self.assertEqual(timing["refresh_states_wall_seconds"], 0.0)
        self.assertEqual(timing["vehicle_update_wall_seconds"], 0.0)
        self.assertEqual(timing["state_update_wall_seconds"], 0.0)
        self.assertEqual(timing["dynamics_update_wall_seconds"], 0.0)
        self.assertEqual(timing["sensor_update_wall_seconds"], 0.0)
        self.assertEqual(timing["backend_state_wall_seconds"], 0.0)
        self.assertEqual(timing["flush_forces_wall_seconds"], 0.0)
        self.assertEqual(timing["after_step_wall_seconds"], 0.0)
        self.assertEqual(timing["native_update_wall_seconds"], 0.0)
        self.assertEqual(timing["render_cycle_wall_seconds"], 0.0)
        self.assertEqual(timing["maximum_physics_step_ms"], 0.0)
        self.assertEqual(timing["maximum_native_update_ms"], 0.0)
        self.assertEqual(timing["maximum_render_cycle_ms"], 0.0)

        server_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "server.py"
        ).read_text()
        self.assertIn('"timing": initial_runtime_timing(self._config)', server_source)
        self.assertNotIn(
            "self._config.vehicle_count,\n                    self._config.rendering_hz",
            server_source,
        )

    def test_preconfiguration_accepts_ephemeral_viewer_slot_reset(self) -> None:
        server_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "server.py"
        ).read_text()
        preconfiguration = server_source.split(
            "class PreconfigurationApplication:", maxsplit=1
        )[1].split("class AdapterApplication:", maxsplit=1)[0]
        self.assertIn(
            '"/v1/live-products/release-all"', preconfiguration
        )
        self.assertIn(
            'return web.json_response({"accepted": True})', preconfiguration
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
        self.assertEqual(publication.queue_capacity, 32)

        with patch.dict(
            os.environ,
            {**VALID_ENVIRONMENT, "UAV_SIM_STREAM_PORT": "9000"},
            clear=True,
        ):
            with self.assertRaisesRegex(ValueError, "requires UAV_SIM_STREAM_HOST"):
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
        self.assertEqual(camera["transport"], "rtsp_rtp")
        self.assertEqual(state["recordings"][0]["camera_streams"], [camera_path])

        recording_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "recording.py"
        ).read_text()
        self.assertNotIn("import av", recording_source)
        self.assertNotIn("VideoFrame", recording_source)
        self.assertNotIn("libx264", recording_source)
        self.assertIn("rr.send_blueprint(", recording_source)
        self.assertIn("make_active=True", recording_source)
        self.assertIn("make_default=True", recording_source)
        self.assertNotIn("default_blueprint=", recording_source)
        self.assertIn("rrb.MapView", recording_source)
        self.assertIn("rr.Radius.ui_points(8.0)", recording_source)
        self.assertIn(
            "batcher_config=rr.ChunkBatcherConfig.LOW_LATENCY()", recording_source
        )
        camera_stream_source = recording_source.split(
            "class RecordedH264CameraStream:", maxsplit=1
        )[1].split("class RecordingPublisher:", maxsplit=1)[0]
        self.assertEqual(camera_stream_source.count("rr.Pinhole("), 1)
        self.assertIn("static=True", camera_stream_source)
        self.assertEqual(
            camera_stream_source.count("rr.VideoStream(codec=rr.VideoCodec.H264)"),
            1,
        )
        video_packet_source = camera_stream_source.split(
            "def _video_packet(", maxsplit=1
        )[1].split("\n\n\nclass RecordingPublisher:", maxsplit=1)[0]
        self.assertNotIn('"codec"', video_packet_source)
        self.assertIn("rr.VideoStream.from_fields(sample=sample)", video_packet_source)
        self.assertNotIn("is_keyframe", video_packet_source)
        publish_source = camera_stream_source.split(
            "    def publish(", maxsplit=1
        )[1].split("    def _set_time(", maxsplit=1)[0]
        self.assertNotIn("rr.Pinhole(", publish_source)
        self.assertIn("access_unit.sample", publish_source)

    def test_stream_publication_owns_a_stable_independent_rtp_epoch(self) -> None:
        published = threading.Event()
        published_twice = threading.Event()
        instances: list[object] = []

        class FakePublisher:
            def __init__(self, _config: StreamPublicationConfig) -> None:
                self.closed = False
                self.samples: list[tuple[bytes, float]] = []
                instances.append(self)

            def publish(self, sample: bytes, simulation_time_s: float) -> None:
                self.samples.append((sample, simulation_time_s))
                published.set()
                if len(self.samples) == 2:
                    published_twice.set()

            def close(self) -> None:
                self.closed = True

        config = StreamPublicationConfig(
            host="stream-mcp",
            port=9000,
            payload_type=96,
            source_vehicle_id="uav-1",
            queue_capacity=4,
        )
        access_unit = parse_native_h264_access_unit(
            b"\x00\x00\x00\x01\x65\x88\x84"
        )
        with patch(
            "veoveo_uav_sim.stream_output.RtpH264Publisher",
            FakePublisher,
        ):
            worker = StreamPublicationWorker(config)
            worker.offer(access_unit, 1.0)
            self.assertTrue(published.wait(1.0))
            worker.offer(access_unit, 1.05)
            self.assertTrue(published_twice.wait(1.0))
            worker.close()
            status = worker.status()

        self.assertEqual(len(instances), 1)
        publisher = instances[0]
        assert isinstance(publisher, FakePublisher)
        self.assertEqual(len(publisher.samples), 2)
        self.assertTrue(publisher.closed)
        self.assertEqual(status.lifecycle, "stopped")
        self.assertEqual(status.dropped_access_units, 0)
        self.assertEqual(status.published_access_units, 2)

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

    def test_inactive_viewer_slots_do_not_require_camera_assignments(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            state = RuntimeState(RuntimeConfig.from_environment(), WORLD)
        inactive_slots = state.snapshot()["stream_products"]

        state.update_stream_products(inactive_slots)

        snapshot = state.snapshot()
        self.assertEqual(
            [product["capacitySlot"] for product in snapshot["stream_products"]],
            [0, 1],
        )
        self.assertTrue(
            all("cameraId" not in product for product in snapshot["stream_products"])
        )
        self.assertEqual(snapshot["live_cameras"][0]["health"], "healthy")

    def test_shared_logical_camera_aggregates_distinct_viewer_slots(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            state = RuntimeState(RuntimeConfig.from_environment(), WORLD)
        state.update_stream_products(
            [
                {
                    "streamProductId": "product-slot-0",
                    "capacitySlot": 0,
                    "cameraId": "follow",
                    "liveViewId": "view-a",
                    "lifecycle": "failed",
                    "lastFrameAt": "2026-08-07T18:00:00Z",
                },
                {
                    "streamProductId": "product-slot-1",
                    "capacitySlot": 1,
                    "cameraId": "follow",
                    "liveViewId": "view-b",
                    "lifecycle": "ready",
                    "lastFrameAt": "2026-08-07T18:00:01Z",
                },
            ]
        )

        camera = state.snapshot()["live_cameras"][0]
        self.assertEqual(camera["health"], "healthy")
        self.assertEqual(camera["lastFrameAt"], "2026-08-07T18:00:01Z")

    def test_render_timing_separates_native_update_from_complete_cycle(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            state = RuntimeState(RuntimeConfig.from_environment(), WORLD)
        state.observe_render_cycle(
            0.02,
            0.03,
            FleetPhysicsTiming(
                physics_steps=2,
                refresh_states_wall_seconds=0.002,
                vehicle_update_wall_seconds=0.004,
                state_update_wall_seconds=0.0005,
                dynamics_update_wall_seconds=0.002,
                sensor_update_wall_seconds=0.001,
                backend_state_wall_seconds=0.0005,
                flush_forces_wall_seconds=0.006,
                after_step_wall_seconds=0.008,
                maximum_physics_step_ms=11.0,
            ),
        )
        state.observe_render_cycle(
            0.04,
            0.05,
            FleetPhysicsTiming(
                physics_steps=5,
                refresh_states_wall_seconds=0.005,
                vehicle_update_wall_seconds=0.010,
                state_update_wall_seconds=0.001,
                dynamics_update_wall_seconds=0.005,
                sensor_update_wall_seconds=0.003,
                backend_state_wall_seconds=0.001,
                flush_forces_wall_seconds=0.015,
                after_step_wall_seconds=0.020,
                maximum_physics_step_ms=13.0,
            ),
        )

        timing = state.snapshot()["timing"]
        self.assertEqual(timing["render_cycles"], 2)
        self.assertEqual(timing["physics_steps"], 5)
        self.assertAlmostEqual(timing["refresh_states_wall_seconds"], 0.005)
        self.assertAlmostEqual(timing["vehicle_update_wall_seconds"], 0.010)
        self.assertAlmostEqual(timing["state_update_wall_seconds"], 0.001)
        self.assertAlmostEqual(timing["dynamics_update_wall_seconds"], 0.005)
        self.assertAlmostEqual(timing["sensor_update_wall_seconds"], 0.003)
        self.assertAlmostEqual(timing["backend_state_wall_seconds"], 0.001)
        self.assertAlmostEqual(timing["flush_forces_wall_seconds"], 0.015)
        self.assertAlmostEqual(timing["after_step_wall_seconds"], 0.020)
        self.assertAlmostEqual(timing["native_update_wall_seconds"], 0.06)
        self.assertAlmostEqual(timing["render_cycle_wall_seconds"], 0.08)
        self.assertAlmostEqual(timing["maximum_physics_step_ms"], 13.0)
        self.assertAlmostEqual(timing["maximum_native_update_ms"], 40.0)
        self.assertAlmostEqual(timing["maximum_render_cycle_ms"], 50.0)

        with self.assertRaisesRegex(ValueError, "cannot be negative"):
            state.observe_render_cycle(
                -0.01,
                0.01,
                FleetPhysicsTiming(5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            )
        with self.assertRaisesRegex(ValueError, "cannot be shorter"):
            state.observe_render_cycle(
                0.02,
                0.01,
                FleetPhysicsTiming(5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            )

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
            annex_b_nals(access_unit),
            (b"\x67\x01\x02", b"\x68\x03", b"\x65\x04\x05"),
        )
        parsed = parse_native_h264_access_unit(access_unit)
        self.assertTrue(parsed.is_keyframe)
        self.assertTrue(parsed.is_decoder_reentrant)
        self.assertEqual(parsed.nal_types, (7, 8, 5))

    def test_native_access_unit_accepts_inter_frame(self) -> None:
        parsed = parse_native_h264_access_unit(b"\x00\x00\x00\x01\x41\x01")
        self.assertFalse(parsed.is_keyframe)
        self.assertFalse(parsed.is_decoder_reentrant)

    def test_parameter_sets_make_native_idr_decoder_reentrant(self) -> None:
        parsed = parse_native_h264_access_unit(b"\x00\x00\x00\x01\x65\x01")
        qualified = make_decoder_reentrant(parsed, b"\x67\x01", b"\x68\x02")
        self.assertTrue(qualified.is_decoder_reentrant)
        self.assertEqual(qualified.nal_types, (7, 8, 5))

    def test_rtp_parser_honors_extension_and_padding(self) -> None:
        header = struct.pack("!BBHII", 0xB0, 0xE0, 7, 9, 11)
        extension = struct.pack("!HHI", 0xBEDE, 1, 0x01020304)
        packet = parse_rtp_packet(header + extension + b"\x65\x99" + b"\x00\x02")
        self.assertEqual(packet.sequence, 7)
        self.assertEqual(packet.timestamp, 9)
        self.assertTrue(packet.marker)
        self.assertEqual(packet.payload_type, 96)
        self.assertEqual(packet.payload, b"\x65\x99")

    def test_rtsp_rtp_depacketizer_qualifies_first_idr_and_retains_p_frames(self) -> None:
        depacketizer = H264RtpDepacketizer(
            96,
            sequence_parameter_set=b"\x67\x01",
            picture_parameter_set=b"\x68\x02",
        )
        idr = depacketizer.push(RtpPacket(1, 100, True, 96, b"\x65\x03"))
        self.assertIsNotNone(idr)
        assert idr is not None
        self.assertTrue(idr.is_decoder_reentrant)
        inter = depacketizer.push(RtpPacket(2, 200, True, 96, b"\x41\x04"))
        self.assertIsNotNone(inter)
        assert inter is not None
        self.assertFalse(inter.is_keyframe)

    def test_rtsp_rtp_depacketizer_reassembles_fu_a(self) -> None:
        depacketizer = H264RtpDepacketizer(
            96,
            sequence_parameter_set=b"\x67\x01",
            picture_parameter_set=b"\x68\x02",
        )
        self.assertIsNone(
            depacketizer.push(RtpPacket(1, 100, False, 96, b"\x7c\x85\x03"))
        )
        access_unit = depacketizer.push(
            RtpPacket(2, 100, True, 96, b"\x7c\x45\x04")
        )
        self.assertIsNotNone(access_unit)
        assert access_unit is not None
        self.assertEqual(annex_b_nals(access_unit.sample)[-1], b"\x65\x03\x04")

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
            def rebind(self, _physics_view: object) -> None:
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
            batch_factory=lambda _paths, _physics_view: (
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
        timing = lifecycle.timing()
        self.assertEqual(timing.physics_steps, 1)
        self.assertGreaterEqual(timing.refresh_states_wall_seconds, 0.0)
        self.assertGreaterEqual(timing.vehicle_update_wall_seconds, 0.0)
        self.assertGreaterEqual(timing.state_update_wall_seconds, 0.0)
        self.assertGreaterEqual(timing.dynamics_update_wall_seconds, 0.0)
        self.assertGreaterEqual(timing.sensor_update_wall_seconds, 0.0)
        self.assertGreaterEqual(timing.backend_state_wall_seconds, 0.0)
        self.assertGreaterEqual(timing.flush_forces_wall_seconds, 0.0)
        self.assertGreaterEqual(timing.after_step_wall_seconds, 0.0)
        self.assertGreaterEqual(timing.maximum_physics_step_ms, 0.0)

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

    def test_tensor_batch_uses_live_buffers_and_stream_local_sync(self) -> None:
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
                self.stream_syncs = 0

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
                self.stream_syncs += 1

            def synchronize_device(self, _device: object) -> None:
                raise AssertionError(
                    "fleet physics must not synchronize unrelated GPU streams"
                )

        class FakeRigidBodyView:
            prim_paths = ("/World/uav_1/body",)

            def __init__(self) -> None:
                self.submitted_forces: np.ndarray | None = None

            def get_transforms(self) -> FakeArray:
                return FakeArray(
                    np.array(
                        [[1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 1.0]],
                        dtype=np.float32,
                    )
                )

            def get_velocities(self) -> FakeArray:
                return FakeArray(
                    np.array(
                        [[4.0, 5.0, 6.0, 0.1, 0.2, 0.3]],
                        dtype=np.float32,
                    )
                )

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

            def create_rigid_body_view(self, _paths: list[str]) -> FakeRigidBodyView:
                return self.rigid_body_view

        fake_warp = FakeWarp()
        physics_view = FakeSimulationView()
        with patch.dict(sys.modules, {"warp": fake_warp}):
            batch = IsaacFleetPhysicsBatch(("/World/uav_1/body",), physics_view)
            batch.refresh_states()
            state = batch.state("/World/uav_1/body")
            batch.queue_force("/World/uav_1/body", (0.0, 0.0, 4.0), (0.0, 0.0, 0.0))
            batch.flush_forces()

        self.assertEqual(fake_warp.stream_syncs, 2)
        np.testing.assert_array_equal(state.position_xyz, [1.0, 2.0, 3.0])
        np.testing.assert_array_equal(state.linear_velocity_xyz, [4.0, 5.0, 6.0])
        np.testing.assert_array_equal(
            physics_view.rigid_body_view.submitted_forces,
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
        np.testing.assert_array_equal(first.orientation_xyzw, [0.0, 0.0, 0.0, 1.0])
        np.testing.assert_array_equal(second.linear_velocity_xyz, [10.0, 11.0, 12.0])
        np.testing.assert_allclose(second.angular_velocity_xyz, [0.4, 0.5, 0.6])

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


class NativeCadenceTests(unittest.TestCase):
    def test_runtime_coalesces_render_work_after_due_fixed_physics(self) -> None:
        app_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "app.py"
        ).read_text()
        self.assertIn("physics_clock.due_steps(physics_step)", app_source)
        self.assertIn("render_cadence.due(physics_step)", app_source)
        self.assertIn("world.step(render=False)", app_source)
        self.assertIn("world.render()", app_source)
        physics_index = app_source.rindex("world.step(render=False)")
        camera_index = app_source.rindex("update_operator_cameras()")
        render_index = app_source.rindex("world.render()")
        self.assertLess(physics_index, camera_index)
        self.assertLess(camera_index, render_index)
        self.assertNotIn("world.step(render=True)", app_source)
        self.assertIn("loop_runner.set_manual_mode(True)", app_source)
        self.assertNotIn("RealtimePhysicsClock", app_source)
        self.assertNotIn("PeriodicDeadline", app_source)

    def test_monotonic_clock_retains_bounded_physics_debt(self) -> None:
        now = [100.0]
        clock = MonotonicPhysicsClock(
            60,
            maximum_steps_per_pass=60,
            clock=lambda: now[0],
        )
        clock.reset(0)
        self.assertEqual(clock.due_steps(0), 0)

        now[0] += 0.11
        self.assertEqual(clock.due_steps(0), 6)
        self.assertEqual(clock.due_steps(6), 0)

        now[0] += 2.0
        self.assertEqual(clock.due_steps(6), 60)
        self.assertEqual(clock.due_steps(66), 60)
        self.assertEqual(clock.due_steps(126), 0)
        self.assertGreater(clock.seconds_until_next_step(126), 0.0)

        with self.assertRaisesRegex(ValueError, "maximum physics steps"):
            MonotonicPhysicsClock(60, maximum_steps_per_pass=0)

    def test_physics_step_gate_selects_exact_render_cadence(self) -> None:
        gate = FixedStepCadenceGate(60, 30)
        self.assertEqual(
            [step for step in range(1, 9) if gate.due(step)],
            [2, 4, 6, 8],
        )
        gate.reset(8)
        self.assertEqual(
            [step for step in range(9, 13) if gate.due(step)],
            [10, 12],
        )


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
            all(
                0.0 < after < before
                for after, before in zip(falling_velocity, rising_velocity)
            )
        )

    def test_motor_model_preserves_px4_rotor_yaw_directions(self) -> None:
        model = Px4IrisThrustCurve()
        model.set_input_reference([900.0, 900.0, 300.0, 300.0])
        _, _, moment = model.update(None, 1.0)
        self.assertLess(moment, 0.0)


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


class StreamedWorldHealthTests(unittest.TestCase):
    def test_native_event_payload_is_reduced_to_the_typed_safe_surface(self) -> None:
        event = SimpleNamespace(
            payload={
                "tilesetPath": "/World/Tileset",
                "generation": 7,
                "loadType": "tile_content",
                "statusCode": 400,
            }
        )
        parsed = NativeTileEventBridge._parse(event, kind="load_failed")
        self.assertEqual(
            parsed,
            NativeTileEvent(
                kind="load_failed",
                tileset_path="/World/Tileset",
                generation=7,
                load_type="tile_content",
                http_status=400,
            ),
        )

    def test_visibility_absence_never_infers_provider_failure(self) -> None:
        controller = TileLifecycleController(
            tileset_path="/World/Tileset", ready_frames=2
        )
        for _ in range(10_000):
            state = controller.observe_render(
                resident_tiles=30_000,
                visible_tiles=0,
                loading_tiles=0,
            )
        self.assertEqual(state.lifecycle, "streaming")
        self.assertEqual(state.refresh_count, 0)
        self.assertIsNone(state.last_failure)

    def test_session_rejection_requests_one_generation_refresh(self) -> None:
        controller = TileLifecycleController(
            tileset_path="/World/Tileset", ready_frames=2
        )
        event = NativeTileEvent(
            kind="load_failed",
            tileset_path="/World/Tileset",
            generation=1,
            load_type="tile_content",
            http_status=400,
        )
        self.assertTrue(controller.accept(event).reload_tileset)
        self.assertFalse(controller.accept(event).reload_tileset)
        state = controller.snapshot()
        self.assertEqual(state.lifecycle, "refreshing")
        self.assertEqual(state.provider_generation, 1)
        self.assertEqual(state.refresh_count, 1)
        self.assertEqual(
            state.last_failure.code if state.last_failure else None,
            "provider_session_rejected",
        )

    def test_matching_replacement_generation_recovers_deterministically(self) -> None:
        controller = TileLifecycleController(
            tileset_path="/World/Tileset", ready_frames=2
        )
        controller.accept(
            NativeTileEvent(
                kind="load_failed",
                tileset_path="/World/Tileset",
                generation=1,
                load_type="tile_content",
                http_status=400,
            )
        )
        controller.accept(
            NativeTileEvent(
                kind="loaded",
                tileset_path="/World/Tileset",
                generation=2,
            )
        )
        first = controller.observe_render(
            resident_tiles=20, visible_tiles=4, loading_tiles=2
        )
        recovered = controller.observe_render(
            resident_tiles=24, visible_tiles=6, loading_tiles=0
        )
        self.assertEqual(first.lifecycle, "streaming")
        self.assertEqual(recovered.lifecycle, "ready")
        self.assertIsNone(recovered.diagnostic)

    def test_rejected_replacement_generation_degrades_without_a_loop(self) -> None:
        controller = TileLifecycleController(
            tileset_path="/World/Tileset", ready_frames=1
        )
        first = NativeTileEvent(
            kind="load_failed",
            tileset_path="/World/Tileset",
            generation=1,
            load_type="tile_content",
            http_status=400,
        )
        second = NativeTileEvent(
            kind="load_failed",
            tileset_path="/World/Tileset",
            generation=2,
            load_type="tile_content",
            http_status=400,
        )
        self.assertTrue(controller.accept(first).reload_tileset)
        self.assertFalse(controller.accept(second).reload_tileset)
        self.assertFalse(controller.accept(second).reload_tileset)
        state = controller.snapshot()
        self.assertEqual(state.lifecycle, "degraded")
        self.assertEqual(state.refresh_count, 1)

    def test_credential_failure_is_typed_and_never_refreshed(self) -> None:
        controller = TileLifecycleController(
            tileset_path="/World/Tileset", ready_frames=1
        )
        action = controller.accept(
            NativeTileEvent(
                kind="load_failed",
                tileset_path="/World/Tileset",
                generation=1,
                load_type="ion_endpoint",
                http_status=401,
            )
        )
        state = controller.snapshot()
        self.assertFalse(action.reload_tileset)
        self.assertEqual(state.lifecycle, "degraded")
        self.assertEqual(
            state.last_failure.code if state.last_failure else None,
            "credentials_rejected",
        )

    def test_visual_health_is_not_simulation_authority(self) -> None:
        source = (Path(__file__).parents[1] / "veoveo_uav_sim" / "app.py").read_text()
        self.assertIn("TileLifecycleController(", source)
        self.assertIn('sensor_status.lifecycle == "degraded"', source)
        self.assertIn("statistics.tiles_rendered", source)
        self.assertIn("cesium_interface.reload_tileset(tileset_path)", source)
        self.assertNotIn("tile_absent_since", source)
        self.assertNotIn("assess_tile_health", source)
        self.assertNotIn('raise RuntimeError("Google Photorealistic', source)
        self.assertNotIn(
            'raise RuntimeError(\n                                f"down camera', source
        )

        sensor_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "hydra_camera.py"
        ).read_text()
        self.assertIn("simulation continues", sensor_source)
        self.assertNotIn("raise RuntimeError(\"native Isaac", sensor_source)

        server_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "server.py"
        ).read_text()
        simulation_ready = server_source.split(
            "        simulation_ready = (", maxsplit=1
        )[1].split("        )\n", maxsplit=1)[0]
        self.assertNotIn('snapshot["tiles"]', simulation_ready)
        self.assertNotIn('snapshot["cameras"]', simulation_ready)
        self.assertNotIn('snapshot["recordings"]', simulation_ready)
        self.assertIn("visual_ready", server_source)

    def test_native_camera_fanout_has_no_software_encoder_or_drain(self) -> None:
        recording_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "recording.py"
        ).read_text()
        app_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "app.py"
        ).read_text()
        self.assertNotIn("import av", recording_source)
        self.assertNotIn("h264_nvenc", recording_source)
        self.assertNotIn("encode(None)", recording_source)
        self.assertIn("class RecordedH264CameraStream", recording_source)
        self.assertNotIn("RtpH264Publisher", recording_source)
        self.assertNotIn("stream_output", recording_source)
        self.assertIn("recording.offer_camera_access_unit(", app_source)
        self.assertIn("stream_publication.offer(", app_source)


if __name__ == "__main__":
    unittest.main()
