from __future__ import annotations

import os
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
from veoveo_uav_sim.config import RuntimeConfig
from veoveo_uav_sim.contracts import ContractError, parse_command, parse_operation
from veoveo_uav_sim.geo import enu_to_geodetic, horizontal_distance_m
from veoveo_uav_sim.pose import PoseProducer, entity_ids
from veoveo_uav_sim.px4 import Px4Commander
from veoveo_uav_sim.state import RuntimeState, VehicleTelemetry
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
    "UAV_SIM_POSE_PRODUCER_SPIFFE_ID": (
        "spiffe://veoveo.local/simulation/uav-sim"
    ),
    "UAV_SIM_POSE_EPOCH_ID": "epoch-1",
    "UAV_SIM_POSE_INGRESS_HOST": "simulation-view-pose",
    "UAV_SIM_POSE_INGRESS_PORT": "7443",
    "UAV_SIM_POSE_SERVER_HOSTNAME": "simulation-view-pose.veoveo.svc",
    "UAV_SIM_POSE_CA_CERTIFICATE": "/run/secrets/simulation-view-pose/ca.crt",
    "UAV_SIM_POSE_CLIENT_CERTIFICATE": (
        "/run/secrets/simulation-view-pose/tls.crt"
    ),
    "UAV_SIM_POSE_CLIENT_PRIVATE_KEY": (
        "/run/secrets/simulation-view-pose/tls.key"
    ),
}

WORLD = WorldConfiguration(
    revision_uri="frames://world/uav-showcase-new-york/revision/revision-1",
    spec_sha256="1" * 64,
    simulation_frame_uri=(
        "frames://world/uav-showcase-new-york/revision/revision-1/"
        "frame/isaac-world"
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
        self.assertEqual(config.rendering_hz, 20)
        self.assertEqual(config.rendering_hz, config.camera.fps)

        app_source = (
            Path(__file__).parents[1] / "veoveo_uav_sim" / "app.py"
        ).read_text()
        self.assertNotIn("omni.replicator", app_source)
        self.assertNotIn("import CameraSensor", app_source)
        self.assertIn("HydraRgbCameraSensor", app_source)
        self.assertIn("PoseProducer", app_source)
        self.assertNotIn("livestream", app_source.lower())
        self.assertNotIn("follow_camera", app_source)

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
        self.assertEqual(publication["cadence_hz"], config.rendering_hz)
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
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            state = RuntimeState(RuntimeConfig.from_environment(), WORLD).snapshot()
        camera = state["cameras"][0]
        camera_path = camera["entity_path"]
        recording_path = state["recordings"][0]["camera_streams"][0]
        self.assertTrue(camera_path.endswith("/camera/down"))
        self.assertEqual(recording_path, camera_path)
        self.assertNotIn("front", camera_path)
        self.assertEqual(camera["codec"], "h264")
        self.assertEqual(camera["encoder"], "nvidia_nvenc")

        recording_source = (
            Path(__file__).parents[1]
            / "veoveo_uav_sim"
            / "recording.py"
        ).read_text()
        self.assertIn('add_stream("h264_nvenc"', recording_source)
        self.assertNotIn("libx264", recording_source)

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


class PoseProducerTests(unittest.TestCase):
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
            "veoveo_uav_sim.pose.LatestPosePublisher",
            side_effect=create_publisher,
        ):
            producer = PoseProducer(
                config=config.pose_publication,
                session_id=config.session_id,
                world=WORLD,
                vehicle_count=1,
                cadence_hz=20,
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
        self.assertTrue(
            any(update["lifecycle"] == "ready" for update in updates)
        )
        self.assertEqual(updates[-1]["lifecycle"], "stopped")

    def test_incomplete_entity_snapshots_are_rejected(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
        with patch(
            "veoveo_uav_sim.pose.LatestPosePublisher",
            _FakePosePublisher,
        ):
            producer = PoseProducer(
                config=config.pose_publication,
                session_id=config.session_id,
                world=WORLD,
                vehicle_count=1,
                cadence_hz=20,
                update_state=lambda _publication: None,
            )
            with self.assertRaisesRegex(RuntimeError, "complete snapshot"):
                producer.offer([])
            producer.close()


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
                _base_mode, custom_mode, custom_sub_mode = mavutil.px4_map[
                    "LOITER"
                ]
                self._messages.append(
                    _MavlinkMessage(
                        "HEARTBEAT",
                        base_mode=(
                            mavutil.mavlink.MAV_MODE_FLAG_CUSTOM_MODE_ENABLED
                        ),
                        custom_mode=(
                            custom_mode << 16 | custom_sub_mode << 24
                        ),
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
        commander._landed_state = (
            mavutil.mavlink.MAV_LANDED_STATE_ON_GROUND
        )

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
        connection = _MavlinkConnection(
            [mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM]
        )
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
        with self.assertRaisesRegex(
            WorldConfigurationError, "frame in revision_uri"
        ):
            WorldConfiguration.from_request(
                {"session_id": "uav-showcase", "world": payload},
                "uav-showcase",
            )

    def test_world_slot_is_idempotent_and_immutable(self) -> None:
        slot = WorldConfigurationSlot()
        self.assertEqual(slot.configure(WORLD), WORLD)
        self.assertEqual(slot.configure(WORLD), WORLD)
        other = WorldConfiguration(
            revision_uri=(
                "frames://world/uav-showcase-new-york/revision/revision-2"
            ),
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
