from __future__ import annotations

import copy
import json
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
    def test_shared_output_schemas_satisfy_rust_field_enum_and_numeric_contracts(self) -> None:
        from veoveo_uav_sim import outbound
        snapshot = json.loads((Path(__file__).resolve().parents[4] / "servers/uav-sim-mcp/testdata/contract.schema.json").read_text())
        pairs = [
            (outbound.SimulationState, "SimulationState"),
            (outbound.CommandAcknowledgement, "CommandAcknowledgement"),
            (outbound.WorldAcknowledgement, "ConfigureWorldOutput"),
            (outbound.EnuVector, "EnuVector"), (outbound.NedVector, "NedVector"),
            (outbound.Quaternion, "QuaternionXyzw"), (outbound.Direction, "EnuDirection"),
            (outbound.RenderPose, "CameraRenderPoseState"),
            (outbound.TileFailure, "TileFailureState"), (outbound.TileState, "TileState"),
            (outbound.CameraState, "CameraState"), (outbound.VehicleState, "VehicleState"),
            (outbound.RuntimeTiming, "RuntimeTimingState"),
            (outbound.Vector3, "LiveVector3"), (outbound.Pose, "LivePose"),
            (outbound.Smoothing, "LiveCameraSmoothing"),
            (outbound.LiveCamera, "LiveCameraDescriptor"),
            (outbound.CameraRegion, "LiveCameraRegion"),
            (outbound.StreamProduct, "LiveStreamProductState"),
        ]
        rig_schemas = {
            variant["properties"]["kind"]["const"]: {**variant, "$defs": snapshot["LiveCameraRig"]["$defs"]}
            for variant in snapshot["LiveCameraRig"]["oneOf"]
        }
        rig_models = [outbound.FixedRig, outbound.LookAtRig, outbound.OrbitRig, outbound.FollowRig,
                      outbound.ChaseRig, outbound.MountedRig, outbound.FormationRig]
        for model in rig_models:
            kind = model.model_json_schema()["properties"]["kind"]["const"]
            name = "LiveCameraRig/" + kind
            snapshot[name] = rig_schemas[kind]
            pairs.append((model, name))
        state_defs = snapshot["SimulationState"]["$defs"]
        pairs.extend([(outbound.Position, "Wgs84Position"), (outbound.WorldBinding, "SimulationWorldBinding")])

        def resolve(field: dict, schema: dict) -> dict:
            if "$ref" in field:
                return schema["$defs"][field["$ref"].removeprefix("#/$defs/")]
            return field

        def kinds(field: dict, schema: dict) -> set:
            field = resolve(field, schema)
            for union in ("anyOf", "oneOf"):
                if union in field:
                    return set().union(*(kinds(branch, schema) for branch in field[union]))
            declared = field.get("type")
            return set(declared) if isinstance(declared, list) else {declared}

        def alternatives(field: dict, schema: dict) -> list[dict]:
            field = resolve(field, schema)
            for union in ("anyOf", "oneOf"):
                if union in field:
                    return [variant for branch in field[union] for variant in alternatives(branch, schema)]
            return [] if kinds(field, schema) == {"null"} else [field]

        for model, name in pairs:
            source = model.model_json_schema(mode="serialization")
            target = snapshot.get(name, {**state_defs[name], "$defs": state_defs}) if name in state_defs else snapshot[name]
            with self.subTest(model=name):
                self.assertEqual(set(source["properties"]), set(target["properties"]))
                self.assertEqual(set(source.get("required", [])), set(target.get("required", [])))
                self.assertFalse(source["additionalProperties"])
                if "additionalProperties" in target:
                    self.assertFalse(target["additionalProperties"])
                for key, target_field in target["properties"].items():
                    source_field = source["properties"][key]
                    self.assertEqual(kinds(source_field, source), kinds(target_field, target), key)
                    source_field, target_field = resolve(source_field, source), resolve(target_field, target)
                    if "enum" in target_field:
                        self.assertEqual(set(source_field.get("enum", [source_field.get("const")])), set(target_field["enum"]), key)
                    # Bounds can live on nullable scalar schemas or their anyOf
                    # branches. Check every matching source alternative.
                    for target_variant in alternatives(target_field, target):
                        for source_variant in alternatives(source_field, source):
                            if not (kinds(source_variant, source) & kinds(target_variant, target) - {"null"}):
                                continue
                            for constraint in ["minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"]:
                                if constraint in target_variant:
                                    if constraint in {"minimum", "exclusiveMinimum"}:
                                        self.assertGreaterEqual(source_variant[constraint], target_variant[constraint], key)
                                    else:
                                        self.assertLessEqual(source_variant[constraint], target_variant[constraint], key)
                            if target_variant.get("format", "").startswith("uint"):
                                bits = int(target_variant["format"].removeprefix("uint"))
                                self.assertLessEqual(source_variant["maximum"], 2**bits - 1, key)


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
        preconfig = object.__new__(PreconfigurationApplication)
        preconfig._config = SimpleNamespace(
            session_id="session-alpha", physics_hz=60, rendering_hz=30,
            cesium_ion_asset_id=2275207,
            operator_live_view=SimpleNamespace(cameras=[]),
        )
        preconfig._world_slot = WorldConfigurationSlot()
        # The existing layout requires configured streamable cameras. Reuse its
        # admitted product fixture without constructing a renderer.
        from unittest.mock import patch
        with patch("veoveo_uav_sim.server.initial_operator_atlas_state", return_value=fixture["state"]["stream_products"][0]):
            response = await preconfig._get_state(None)
        body = json.loads(response.body)
        self.assertEqual(body["lifecycle"], "unconfigured")
        self.assertIsNone(body["world"])
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
