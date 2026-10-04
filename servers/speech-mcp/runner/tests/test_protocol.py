import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[4]))
from testing.python.protocol_schema import assert_peer_snapshot

"""Private decode tests; this suite performs no GPU inference."""
import unittest
from pydantic import ValidationError
from speech_runner.protocol import REQUEST_ADAPTER, EVENT_ADAPTER, Request, encode
from pathlib import Path


class ProtocolTests(unittest.TestCase):
    def test_invalid_discriminators_are_redacted_at_worker_boundaries(self):
        for action in [
            lambda: Request.parse(b'{"operation":"sentinel-private-value"}', Path("/tmp")),
            lambda: encode({"kind": "sentinel-private-value"}),
        ]:
            with self.assertRaises(ValueError) as raised:
                action()
            self.assertNotIn("sentinel-private-value", str(raised.exception))
            self.assertIn("union_tag_invalid", str(raised.exception))

    def test_worker_error_does_not_reflect_input(self):
        for adapter, value in [
            (REQUEST_ADAPTER, {"operation": "probe", "secret": "sentinel-private-value"}),
            (EVENT_ADAPTER, {"kind": "accepted", "secret": "sentinel-private-value"}),
            (REQUEST_ADAPTER, {"secret": "sentinel-private-value"}),
            (EVENT_ADAPTER, {"secret": "sentinel-private-value"}),
        ]:
            with self.assertRaises(ValidationError) as raised:
                adapter.validate_python(value)
            self.assertNotIn("sentinel-private-value", str(raised.exception))

    def test_empty_tagged_variants_are_closed(self):
        for adapter, value in [
            (REQUEST_ADAPTER, {"operation": "probe"}),
            (EVENT_ADAPTER, {"kind": "accepted"}),
        ]:
            adapter.validate_python(value)
            with self.assertRaises(ValidationError):
                adapter.validate_python({**value, "unexpected": True})

    def test_transcript_events_reject_nested_additions(self):
        value = {"kind": "transcript", "complete": True, "transcript": {
            "text": "hello", "duration_seconds": 1.0, "segments": [
                {"text": "hello", "start": 0.0, "end": 1.0, "words": [
                    {"word": "hello", "start": 0.0, "end": 1.0}
                ]}
            ]
        }}
        self.assertTrue(encode(value).endswith(b"\n"))
        value["transcript"]["segments"][0]["words"][0]["unexpected"] = True
        with self.assertRaises(ValueError):
            encode(value)

    def test_request_and_event_schemas_close_every_owned_object(self):
        for adapter in (REQUEST_ADAPTER, EVENT_ADAPTER):
            for definition in adapter.json_schema()["$defs"].values():
                if definition.get("type") == "object":
                    self.assertFalse(definition["additionalProperties"])


class PrivateProtocolSchemaTests(unittest.TestCase):
    def test_complete_reachable_private_protocol_schema(self):
        assert_peer_snapshot(
            Path(__file__).resolve().parents[2] / "testdata/private-protocol.schema.json",
            REQUEST_ADAPTER.json_schema(mode="validation"),
            EVENT_ADAPTER.json_schema(mode="serialization"),
        )


class NumericAdmissionTests(unittest.TestCase):
    def test_request_width_and_transcript_finiteness(self):
        for value in [-1, 2**32, True]:
            with self.assertRaises(ValidationError):
                REQUEST_ADAPTER.validate_python({"operation": "live", "sample_rate": value, "max_duration_seconds": 1})
        for value in [float("nan"), float("inf"), -float("inf")]:
            with self.assertRaises(ValueError):
                encode({"kind": "transcript", "complete": True, "transcript": {
                    "text": "observed", "duration_seconds": value, "segments": []}})
            with self.assertRaises(ValueError):
                encode({"kind": "transcript", "complete": True, "transcript": {
                    "text": "observed", "duration_seconds": 1.0, "segments": [{
                        "text": "observed", "start": 0.0, "end": 1.0, "words": [{
                            "word": "observed", "start": value, "end": 1.0}]}]}})
