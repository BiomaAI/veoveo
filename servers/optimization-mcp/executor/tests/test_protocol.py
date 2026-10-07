import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[4]))
from testing.python.protocol_schema import assert_peer_snapshot

import asyncio
import json
import unittest
import copy
from pathlib import Path
from pydantic import ValidationError

from veoveo_cuopt_executor.protocol import (
    PROTOCOL_VERSION,
    ProtocolError,
    error_response,
    read_frame,
    ExecutorRequest,
    ExecutorResponse,
)


class ProtocolTests(unittest.IsolatedAsyncioTestCase):
    def test_every_request_result_and_route_node_variant_round_trips(self) -> None:
        fixture = json.loads((Path(__file__).parents[2] / "testdata/executor-protocol.json").read_text())
        for model, key in [(ExecutorRequest, "requests"), (ExecutorResponse, "responses")]:
            for value in fixture[key]:
                decoded = model.model_validate(value)
                self.assertEqual(model.model_validate_json(decoded.model_dump_json()), decoded)
        successful = ExecutorResponse.model_validate(fixture["responses"][1])
        self.assertEqual([visit.node.kind for visit in successful.result.solution.routes[0].nodes], ["depot", "order", "break"])

    def test_all_current_private_members_refuse_retired_and_mixed_on_both_receivers(self) -> None:
        from pydantic import TypeAdapter
        import re
        fixture = json.loads((Path(__file__).parents[2] / "testdata/executor-protocol.json").read_text())

        def positions(value, path=()):
            if isinstance(value, dict):
                for key, child in value.items():
                    if any(character.isupper() for character in key):
                        yield path, key, re.sub(r"([A-Z])", lambda match: "_" + match[1].lower(), key)
                    yield from positions(child, (*path, key))
            elif isinstance(value, list):
                for index, child in enumerate(value):
                    yield from positions(child, (*path, index))

        count = 0
        for model, name in [(ExecutorRequest, "requests"), (ExecutorResponse, "responses")]:
            adapter = TypeAdapter(model)
            for current in fixture[name]:
                admitted = model.model_validate(current)
                self.assertEqual(model.model_validate_json(json.dumps(current)), admitted)
                self.assertEqual(adapter.validate_python(current), admitted)
                self.assertEqual(adapter.validate_json(json.dumps(current)), admitted)
                for path, key, retired in positions(current):
                    for keep_current in [False, True]:
                        changed = copy.deepcopy(current)
                        target = changed
                        for component in path:
                            target = target[component]
                        target[retired] = target[key] if keep_current else target.pop(key)
                        with self.subTest(model=name, path=path, current=key, mixed=keep_current):
                            for decode in [model.model_validate, adapter.validate_python]:
                                with self.assertRaises(ValidationError):
                                    decode(changed)
                            for decode in [model.model_validate_json, adapter.validate_json]:
                                with self.assertRaises(ValidationError):
                                    decode(json.dumps(changed))
                        count += 1
        self.assertGreater(count, 20)

    async def test_retired_private_request_refuses_before_gpu_or_solver_dispatch(self) -> None:
        from unittest.mock import patch, AsyncMock
        from veoveo_cuopt_executor.server import ExecutorServer
        from veoveo_cuopt_executor.gpu import GpuHealth

        class Writer:
            def __init__(self):
                self.body = bytearray()
                self.closed = False
            def write(self, body):
                self.body.extend(body)
            async def drain(self):
                pass
            def is_closing(self):
                return self.closed
            def close(self):
                self.closed = True
            async def wait_closed(self):
                pass

        current = {"protocol": PROTOCOL_VERSION, "runId": "run-fixture", "operation": {"operation": "health"}}
        for mutation in ["replacement", "mixed", "revision"]:
            invalid = copy.deepcopy(current)
            if mutation == "revision":
                invalid["protocol"] = "veoveo.ai/cuopt-executor/v1"
            else:
                invalid["run_id"] = invalid["runId"] if mutation == "mixed" else invalid.pop("runId")
            body = json.dumps(invalid).encode()
            reader = asyncio.StreamReader()
            reader.feed_data(len(body).to_bytes(8, "big") + body)
            reader.feed_eof()
            writer = Writer()
            server = ExecutorServer(Path("/unused/socket"), Path("/unused/staging"), 4096,
                                    GpuHealth(True, "26.08", "13.3", "fixture", "fixture", "9.0"))
            server.solve = AsyncMock(side_effect=AssertionError("solver must not dispatch"))
            with patch("veoveo_cuopt_executor.server.assert_gpu_available") as gpu:
                await server.handle_connection(reader, writer)
                gpu.assert_not_called()
                server.solve.assert_not_called()
            result = json.loads(writer.body[8:])
            self.assertEqual(result["result"]["error"]["code"], "protocol_failure")
            self.assertTrue(writer.closed)

    async def test_invalid_discriminator_is_redacted_in_protocol_error(self) -> None:
        from veoveo_cuopt_executor.protocol import require_protocol, write_frame
        request = {"protocol": PROTOCOL_VERSION, 'runId': "run-fixture", "operation": {"operation": "sentinel-private-value"}}
        with self.assertRaises(ProtocolError) as raised:
            require_protocol(request)
        self.assertNotIn("sentinel-private-value", str(raised.exception))
        response_value = {"protocol": PROTOCOL_VERSION, 'runId': "run-fixture", "result": {"result": "sentinel-private-value"}}
        with self.assertRaises(ProtocolError) as raised:
            await write_frame(None, response_value, 4096)
        self.assertNotIn("sentinel-private-value", str(raised.exception))
        self.assertIn("union_tag_invalid", str(raised.exception))

    def test_decode_diagnostics_do_not_reflect_input(self) -> None:
        from veoveo_cuopt_executor.protocol import require_protocol
        value = {"protocol": PROTOCOL_VERSION, 'runId': "run-fixture", "operation": {"operation": "health", "secret": "sentinel-private-value"}}
        with self.assertRaises(ProtocolError) as raised:
            require_protocol(value)
        self.assertNotIn("sentinel-private-value", str(raised.exception))

    def test_shared_rust_peer_fixture_is_closed(self) -> None:
        fixture = json.loads((Path(__file__).parents[2] / "testdata/executor-protocol.json").read_text())
        for model, name, paths in [
            (ExecutorRequest, "request", [(), ("operation",)]),
            (ExecutorResponse, "response", [(), ("result",), ("result", "error")]),
        ]:
            model.model_validate(fixture[name])
            for path in paths:
                changed = copy.deepcopy(fixture[name])
                target = changed
                for key in path:
                    target = target[key]
                target["unexpected"] = True
                with self.assertRaises(ValidationError):
                    model.model_validate(changed)
            schema = model.model_json_schema()
            self.assertFalse(schema["additionalProperties"])
            for definition in schema["$defs"].values():
                if definition.get("type") == "object" and "properties" in definition:
                    self.assertFalse(definition["additionalProperties"])

    async def test_reads_a_bounded_big_endian_frame(self) -> None:
        request = {
            "protocol": PROTOCOL_VERSION,
            'runId': "run-018f0000-0000-7000-8000-000000000000",
            "operation": {"operation": "health"},
        }
        body = json.dumps(request).encode()
        reader = asyncio.StreamReader()
        reader.feed_data(len(body).to_bytes(8, "big") + body)
        reader.feed_eof()
        self.assertEqual(await read_frame(reader, 4096), request)

    async def test_rejects_an_oversized_frame_before_body_read(self) -> None:
        reader = asyncio.StreamReader()
        reader.feed_data((4097).to_bytes(8, "big"))
        reader.feed_eof()
        with self.assertRaises(ProtocolError):
            await read_frame(reader, 4096)

    def test_errors_are_typed_protocol_results(self) -> None:
        result = error_response(
            "run-018f0000-0000-7000-8000-000000000000",
            "invalid_request",
            "bad request",
        )
        self.assertEqual(result["result"]["result"], "error")
        self.assertEqual(
            result["result"]["error"]["code"], "invalid_request"
        )


if __name__ == "__main__":
    unittest.main()


class PrivateProtocolSchemaTests(unittest.TestCase):
    def test_complete_reachable_private_protocol_schema(self):
        assert_peer_snapshot(
            Path(__file__).resolve().parents[2] / "testdata/private-protocol.schema.json",
            ExecutorRequest.model_json_schema(mode="validation"),
            ExecutorResponse.model_json_schema(mode="serialization"),
        )


class NumericAdmissionTests(unittest.TestCase):
    def test_nested_width_and_nonnegative_admission(self):
        from veoveo_cuopt_executor.protocol import (
            CompiledCapacityDimension, CompiledDenseMatrix, ExecutorIncumbent,
            RoutingSolverSettings,
        )
        valid = {'vehicleType': 255, "dimension": 1, "values": [0.0]}
        CompiledDenseMatrix.model_validate(valid)
        for changed in [{**valid, 'vehicleType': 256}, {**valid, "dimension": -1}, {**valid, "dimension": 2**32}, {**valid, "dimension": True}]:
            with self.assertRaises(ValidationError):
                CompiledDenseMatrix.model_validate(changed)
        CompiledCapacityDimension.model_validate({'dimensionId': "capacity-fixture", "demand": [-(2**31)], "capacity": [2**32 - 1]})
        with self.assertRaises(ValidationError):
            CompiledCapacityDimension.model_validate({'dimensionId': "capacity-fixture", "demand": [2**31], "capacity": [0]})
        for value in [-1.0, float("nan"), float("inf")]:
            with self.assertRaises(ValidationError):
                RoutingSolverSettings(timeLimitSeconds=value)
        with self.assertRaises(ValidationError):
            ExecutorIncumbent(sequence=2**64, values=[], objective=0.0, bound=0.0, foundAtSeconds=0.0)
