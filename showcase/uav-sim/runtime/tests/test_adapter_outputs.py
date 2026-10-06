from __future__ import annotations

import copy
import json
import math
import struct
from pathlib import Path
from types import SimpleNamespace
import unittest

from aiohttp.test_utils import TestClient, TestServer

from veoveo_uav_sim.outbound import (
    CommandAcknowledgement, OPERATION_RESULT_ADAPTER, OutputContractError,
    RuntimeEventWire, SimulationState, WorldAcknowledgement, admit_output,
)
from veoveo_uav_sim.runtime_events import RuntimeEvent, RuntimeEventPublisher
from veoveo_uav_sim.server import AdapterApplication, PreconfigurationApplication
from veoveo_uav_sim.world_config import WorldConfigurationSlot


FIXTURE = Path(__file__).with_name("fixtures") / "adapter_outputs.json"


def outputs() -> dict:
    return json.loads(FIXTURE.read_text(encoding="utf-8"))


class AdapterOutputTests(unittest.TestCase):
    def test_complete_private_endpoint_schemas_are_directionally_compatible(self) -> None:
        from testing.python.protocol_schema import assert_compatible
        from veoveo_uav_sim.contracts import COMMAND_ADAPTER, OPERATION_ADAPTER
        from veoveo_uav_sim.world_config import ConfigureWorldWire
        snapshot = json.loads((Path(__file__).resolve().parents[4] / "servers/uav-sim-mcp/testdata/private-protocol.schema.json").read_text())
        requests = {
            "POST /v1/world request": ConfigureWorldWire,
            "POST /v1/commands request": COMMAND_ADAPTER,
            "POST /v1/operations request": OPERATION_ADAPTER,
        }
        responses = {
            "GET /v1/state response": SimulationState,
            "POST /v1/world response": WorldAcknowledgement,
            "POST /v1/commands response": CommandAcknowledgement,
            "POST /v1/operations response": OPERATION_RESULT_ADAPTER,
            "GET /v1/events NDJSON": RuntimeEventWire,
        }
        self.assertEqual(set(snapshot), set(requests) | set(responses))
        for roots, direction, mode in [(requests, "rust_to_python", "validation"), (responses, "python_to_rust", "serialization")]:
            for endpoint, model in roots.items():
                with self.subTest(endpoint=endpoint):
                    self.assertEqual(snapshot[endpoint]["direction"], direction)
                    python = model.json_schema(mode=mode) if hasattr(model, "json_schema") else model.model_json_schema(mode=mode)
                    rust = snapshot[endpoint]["schema"]
                    producer, consumer = (rust, python) if direction == "rust_to_python" else (python, rust)
                    assert_compatible(producer, consumer, context=endpoint)

    def test_rust_consumer_fixture_preserves_every_output_shape(self) -> None:
        fixture = outputs()
        for model, payload in [
            (SimulationState, fixture["state"]),
            (CommandAcknowledgement, fixture["command"]),
            (WorldAcknowledgement, fixture["world"]),
            *((OPERATION_RESULT_ADAPTER, value) for value in fixture["results"]),
            *((RuntimeEventWire, value) for value in fixture["events"]),
        ]:
            with self.subTest(model=model):
                self.assertEqual(admit_output(model, payload), payload)

    def test_every_controlled_nested_object_rejects_extra_fields(self) -> None:
        def objects(value: object, path: tuple = ()):
            if isinstance(value, dict):
                yield path
                for key, child in value.items():
                    yield from objects(child, (*path, key))
            elif isinstance(value, list):
                for index, child in enumerate(value):
                    yield from objects(child, (*path, index))

        fixture = outputs()
        for model, payload in [
            (SimulationState, fixture["state"]),
            (WorldAcknowledgement, fixture["world"]),
            (CommandAcknowledgement, fixture["command"]),
            *((OPERATION_RESULT_ADAPTER, value) for value in fixture["results"]),
            *((RuntimeEventWire, value) for value in fixture["events"]),
        ]:
            for path in objects(payload):
                mutated = copy.deepcopy(payload)
                target = mutated
                for segment in path:
                    target = target[segment]
                target["distinctive-secret-field"] = "distinctive-secret-value"
                with self.subTest(path=path), self.assertRaises(OutputContractError) as failure:
                    admit_output(model, mutated)
                self.assertIn("extra_forbidden", str(failure.exception))
                self.assertNotIn("distinctive-secret", str(failure.exception))

    def test_battery_output_is_finite_and_survives_native_f32_rounding(self) -> None:
        preceding = struct.unpack("!f", struct.pack("!I", 0x42C7FFFF))[0]
        for value in [0.0, math.nextafter(0.0, 1.0), 0.1, preceding,
                      (preceding + 100.0) / 2.0, math.nextafter(100.0, 0.0), 100.0]:
            with self.subTest(value=value):
                payload = outputs()["state"]
                payload["vehicles"][0]["battery_percent"] = value
                admitted = admit_output(SimulationState, payload)
                emitted = admitted["vehicles"][0]["battery_percent"]
                self.assertEqual(emitted, value)
                rounded = struct.unpack("!f", struct.pack("!f", emitted))[0]
                self.assertTrue(math.isfinite(rounded))
                self.assertGreaterEqual(rounded, 0)
                self.assertLessEqual(rounded, 100)
        for value in [True, -0.1, math.nextafter(100.0, math.inf), float("nan"), float("inf")]:
            payload = outputs()["state"]
            payload["vehicles"][0]["battery_percent"] = value
            with self.subTest(value=value), self.assertRaises(OutputContractError):
                admit_output(SimulationState, payload)

    def test_numeric_widths_enums_finiteness_and_bool_separation(self) -> None:
        invalid = [
            (("physics_step",), True),
            (("physics_step",), -1),
            (("physics_step",), 2**64),
            (("simulation_time_s",), float("nan")),
            (("simulation_time_s",), float("inf")),
            (("lifecycle",), "distinctive-secret-enum"),
            (("session_id",), ".."),
            (("timing", "physics_hz"), 29),
            (("timing", "native_rendering_hz"), 121),
            (("cameras", 0, "encoder"), "software"),
            (("cameras", 0, "width"), 2**32),
            (("cameras", 0, "last_frame_keyframe"), 1),
            (("cameras", 0, "render_pose", "forward_error_degrees"), 181.0),
            (("tiles", "last_failure", "http_status"), 2**16),
            (("vehicles", 0, "battery_percent"), 100.1),
            (("vehicles", 0, "wgs84", "latitude_degrees"), 90.1),
            (("live_cameras", 0, "health"), "degraded"),
            (("live_cameras", 0, "rig", "pose", "orientationXyzw", "w"), 0.0),
            (("live_cameras", 1, "rig", "smoothing", "resetAfterGapMs"), 0),
            (("live_cameras", 2, "rig", "radiusM"), 0.1),
            (("live_cameras", 6, "rig", "targetEntityIds"), ["uav-2", "uav-1"]),
            (("live_cameras", 0, "farClipM"), 0.01),
            (("stream_products", 0, "cameraRegions", 0, "widthPx"), 1281),
            (("world", "georeference_origin", "latitude_degrees"), float("nan")),
            (("world", "georeference_origin", "latitude_degrees"), 90.1),
            (("world", "georeference_origin", "longitude_degrees"), -180.1),
            (("world", "georeference_origin", "ellipsoid_height_m"), float("inf")),
            (("updated_at",), "20261004T120000+0000"),
            (("updated_at",), "2026-W40-7T12:00:00+00:00"),
            (("updated_at",), "distinctive-secret-timestamp"),
        ]
        for path, value in invalid:
            state = outputs()["state"]
            parent = state
            for segment in path[:-1]:
                parent = parent[segment]
            parent[path[-1]] = value
            with self.subTest(path=path), self.assertRaises(OutputContractError) as failure:
                admit_output(SimulationState, state)
            self.assertNotIn("distinctive-secret", str(failure.exception))

    def test_world_and_state_child_relationships_are_admitted_before_encoding(self) -> None:
        for target in ["vehicles", "cameras", "live_cameras", "stream_products", "recordings"]:
            state = outputs()["state"]
            if not state[target]:
                continue
            state[target].append(copy.deepcopy(state[target][0]))
            with self.subTest(target=target), self.assertRaises(OutputContractError):
                admit_output(SimulationState, state)
        for path, value in [
            (("cameras", 0, "vehicle_id"), "foreign"),
            (("live_cameras", 0, "sessionId"), "foreign"),
            (("stream_products", 0, "cameraRegions", 0, "cameraId"), "foreign"),
            (("world", "spec_sha256"), "A" * 64),
            (("world", "spec_sha256"), "a" * 63),
            (("world", "spec_sha256"), "a" * 65),
            (("world", "spec_sha256"), "sha256:" + "a" * 64),
            (("world", "simulation_frame_uri"), "frames://world/foreign/revision/other/frame/isaac-world"),
        ]:
            state = outputs()["state"]
            parent = state
            for segment in path[:-1]:
                parent = parent[segment]
            parent[path[-1]] = value
            with self.subTest(path=path), self.assertRaises(OutputContractError):
                admit_output(SimulationState, state)
        for uri in ["uav-sim://session/../world", "uav-sim://session/session-alpha/world?query=1", "https://example.test/session", "uav-sim://session/session-alpha/world/extra"]:
            value = outputs()["world"]
            value["resource_uri"] = uri
            with self.subTest(uri=uri), self.assertRaises(OutputContractError):
                admit_output(WorldAcknowledgement, value)

    def test_timestamp_parser_preserves_rust_accepted_case_and_offset_spellings(self) -> None:
        for stamp in ["2026-10-04t12:00:00z", "2026-10-04T12:00:00+03:30"]:
            state = outputs()["state"]
            state["updated_at"] = stamp
            self.assertEqual(admit_output(SimulationState, state)["updated_at"], stamp)
        world = outputs()["world"]
        world["world"]["georeference_origin"]["latitude_degrees"] = float("nan")
        with self.assertRaises(OutputContractError):
            admit_output(WorldAcknowledgement, world)

    def test_optional_omission_and_null_follow_rust_decoders(self) -> None:
        state = outputs()["state"]
        state.pop("world")
        self.assertNotIn("world", admit_output(SimulationState, state))
        state["world"] = None
        state["cameras"][0]["diagnostic"] = None
        state["stream_products"][0]["sourceToRenderP95Microseconds"] = None
        encoded = admit_output(SimulationState, state)
        self.assertIsNone(encoded["world"])
        self.assertIsNone(encoded["cameras"][0]["diagnostic"])
        self.assertIsNone(encoded["stream_products"][0]["sourceToRenderP95Microseconds"])
        state["vehicles"] = None
        with self.assertRaises(OutputContractError):
            admit_output(SimulationState, state)

    def test_duplicate_and_overlapping_atlas_regions_are_rejected(self) -> None:
        for camera_id, x in [("camera-0", 1280), ("camera-1", 1)]:
            state = outputs()["state"]
            product = state["stream_products"][0]
            product["codedWidthPx"] = 2560
            region = copy.deepcopy(product["cameraRegions"][0])
            region.update(cameraId=camera_id, xPx=x)
            product["cameraRegions"].append(region)
            with self.assertRaises(OutputContractError):
                admit_output(SimulationState, state)

    def test_completion_keeps_recording_admission_after_physical_settlement(self) -> None:
        result = outputs()["results"][0]
        result["output"]["recording_keys"] = ["opaque producer key rejected at catalog projection"]
        self.assertEqual(admit_output(OPERATION_RESULT_ADAPTER, result), result)
        result["output"]["elapsed_seconds"] = -0.1
        with self.assertRaises(OutputContractError):
            admit_output(OPERATION_RESULT_ADAPTER, result)
        mission = outputs()["results"][2]
        mission["output"]["finished_at"] = "2026-08-07T17:59:59Z"
        with self.assertRaises(OutputContractError):
            admit_output(OPERATION_RESULT_ADAPTER, mission)

    def test_runtime_event_encoder_validates_even_without_subscribers(self) -> None:
        publisher = RuntimeEventPublisher()
        for event, session, generation in [
            ("ready", "..", 1), ("ready", "session-alpha", True),
            ("ready", "session-alpha", 2**64), ("unknown-secret", "session-alpha", 1),
        ]:
            with self.subTest(event=event), self.assertRaises(OutputContractError) as failure:
                RuntimeEvent(event, session, generation).encode()
            self.assertNotIn("unknown-secret", str(failure.exception))
        with self.assertRaises(OutputContractError):
            publisher.publish(event="ready", session_id="session-alpha", generation=True)
        self.assertIsNone(publisher._latest)


class AdapterOutputHttpTests(unittest.IsolatedAsyncioTestCase):
    async def test_preconfiguration_and_active_state_validate_before_wire(self) -> None:
        fixture = outputs()
        from veoveo_uav_sim.operator_camera_config import OperatorLiveViewRuntimeConfig
        camera = fixture["state"]["live_cameras"][0]
        operator_config = OperatorLiveViewRuntimeConfig.from_json(
            json.dumps([{
                "cameraId": camera["cameraId"],
                "revision": camera["revision"],
                "rig": camera["rig"],
                "optics": {
                    "widthPx": camera["widthPx"],
                    "heightPx": camera["heightPx"],
                    "frameRateHz": camera["frameRateMillihertz"] // 1000,
                    "verticalFovDegrees": camera["verticalFovDegrees"],
                    "nearClipM": camera["nearClipM"],
                    "farClipM": camera["farClipM"],
                },
                "streamPolicy": camera["streamPolicy"],
            }]),
            rtsp_port_base=8554,
        )
        preconfig = object.__new__(PreconfigurationApplication)
        preconfig._config = SimpleNamespace(
            session_id="session-alpha", physics_hz=60, rendering_hz=30,
            cesium_ion_asset_id=2275207, operator_live_view=operator_config,
        )
        preconfig._world_slot = WorldConfigurationSlot()
        # Real camera and atlas descriptors require no renderer or simulator.
        response = await preconfig._get_state(None)
        body = json.loads(response.body)
        self.assertEqual(body["lifecycle"], "unconfigured")
        self.assertIsNone(body["world"])
        self.assertEqual(body["live_cameras"][0]["cameraId"], camera["cameraId"])
        self.assertEqual(body["stream_products"][0]["cameraRegions"][0]["cameraId"], camera["cameraId"])
        self.assertEqual(admit_output(SimulationState, body), body)

        active = object.__new__(AdapterApplication)
        active._state = SimpleNamespace(snapshot=lambda: fixture["state"])
        response = await active._get_state(None)
        self.assertEqual(json.loads(response.body), fixture["state"])
        fixture["state"]["vehicles"][0]["unknown-secret"] = "hidden-input"
        with self.assertRaises(OutputContractError) as failure:
            await active._get_state(None)
        self.assertNotIn("hidden-input", str(failure.exception))

    async def test_command_and_operation_success_are_checked_at_http_boundary(self) -> None:
        from aiohttp import web
        fixture = outputs()
        application = object.__new__(AdapterApplication)
        application._execute_command = lambda _: fixture["command"]
        application._execute_operation = lambda _: fixture["results"][0]
        app = web.Application()
        app.router.add_post("/commands", application._command)
        app.router.add_post("/operations", application._operation)
        async with TestClient(TestServer(app)) as client:
            response = await client.post("/commands", json={"command": "pause", "session_id": "session-alpha"})
            self.assertEqual(response.status, 200)
            self.assertEqual(await response.json(), fixture["command"])
            fixture["command"]["distinctive-secret-field"] = "hidden-input"
            response = await client.post("/commands", json={"command": "pause", "session_id": "session-alpha"})
            self.assertEqual(response.status, 409)
            self.assertEqual(await response.json(), {"error": "invalid adapter output: extra_forbidden"})
            response = await client.post("/operations", json={"operation": "run_scenario", "input": {"session_id": "session-alpha", "duration_seconds": 1.0, "parameters": {}}})
            self.assertEqual(response.status, 200)
            fixture["results"][0]["output"]["elapsed_seconds"] = True
            response = await client.post("/operations", json={"operation": "run_scenario", "input": {"session_id": "session-alpha", "duration_seconds": 1.0, "parameters": {}}})
            self.assertEqual(response.status, 409)
            self.assertEqual(await response.json(), {"error": "invalid adapter output: float_type"})
