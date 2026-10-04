import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[4]))
from testing.python.protocol_schema import assert_peer_snapshot

import json

import pytest

from reason_runner import protocol
from reason_runner.inference import observation_frame_limit


def request_document() -> dict:
    return {
        "schema": protocol.REQUEST_SCHEMA,
        "task_id": "01983da0-0000-7000-8000-000000000001",
        "input_mp4": "/tmp/input.mp4",
        "input_width": 1920,
        "input_height": 1080,
        "response_json": "/tmp/response.json",
        "pipeline": {
            "pipeline_id": "video-reasoning",
            "prompt_template_path": "/etc/veoveo/reason/prompt-template.txt",
            "prompt_revision": "v1",
            "observation": {"width": 640, "height": 360, "maximum_frames": 6},
        },
        "model": {
            "model_id": "world-model",
            "model_path": "/models/world-model",
            "format": "local_checkpoint",
            "model_digest": "sha256:test",
            "engine": {
                "kind": "vllm",
                "gpu_memory_utilization": 0.7,
                "max_model_len": 8_192,
            },
        },
        "task": {"kind": "detect_events", "prompt": "vehicles entering the frame"},
        "grounding": {
            "schema": "veoveo.reason-grounding/v1",
            "source_artifact_uri": "stream://artifact/test",
            "frames": [
                {"index": 10, "detections": [{"label": "car", "track_id": 7}]},
            ],
        },
        "requested_range": {"start": 0, "end": 3_000_000_000},
        "decode_start_index": 0,
        "sampling": {"max_frames": 16},
        "decode": {"mode": "greedy"},
        "max_events": 100,
        "max_answer_bytes": 10_000,
        "max_response_bytes": 1_000_000,
    }


def test_request_roundtrip_preserves_wire_names() -> None:
    request = protocol.parse_request(json.dumps(request_document()).encode())
    assert request.schema_ == protocol.REQUEST_SCHEMA
    assert request.task.kind == "detect_events"
    assert request.decode.mode == "greedy"
    assert request.grounding is not None
    assert request.grounding.track_ids() == {7}
    assert request.model.engine.gpu_memory_utilization == 0.7
    assert request.model.engine.max_model_len == 8_192
    assert request.pipeline.observation.maximum_frames == 6


def test_unsupported_schema_is_rejected() -> None:
    document = request_document()
    document["schema"] = "something-else/v9"
    with pytest.raises(ValueError, match="unsupported runner request schema"):
        protocol.parse_request(json.dumps(document).encode())


def test_unknown_fields_are_rejected() -> None:
    document = request_document()
    document["surprise"] = True
    with pytest.raises(ValueError):
        protocol.parse_request(json.dumps(document).encode())


def test_invalid_engine_budget_is_rejected() -> None:
    document = request_document()
    document["model"]["engine"]["gpu_memory_utilization"] = 0.0
    with pytest.raises(ValueError):
        protocol.parse_request(json.dumps(document).encode())


def test_pipeline_observation_budget_bounds_the_request() -> None:
    request = protocol.parse_request(json.dumps(request_document()).encode())
    assert request.sampling.max_frames == 16
    assert observation_frame_limit(request) == 6

    request.sampling.max_frames = 4
    assert observation_frame_limit(request) == 4


def test_response_serializes_the_tagged_answer() -> None:
    response = protocol.RunnerResponse(
        answer=protocol.EventsAnswer(
            events=[
                protocol.ReasonedEvent(
                    range=protocol.IndexRange(start=1, end=2),
                    label="car passes",
                    description="a car crosses the frame",
                    track_ids=[7],
                )
            ]
        ),
        observed_frames=3,
        elapsed_ms=25,
    )
    document = json.loads(response.to_json())
    assert document["schema"] == protocol.RESPONSE_SCHEMA
    assert document["answer"]["kind"] == "events"
    assert document["answer"]["events"][0]["range"] == {"start": 1, "end": 2}


def test_answer_kind_mapping_matches_the_rust_contract() -> None:
    assert protocol.answer_kind_for(protocol.DescribeSegment(kind="describe_segment")) == "description"
    assert (
        protocol.answer_kind_for(protocol.AnswerQuestion(kind="answer_question", question="what happened?")) == "answer"
    )

@pytest.mark.parametrize("path", [
    (), ("model",), ("model", "engine"), ("pipeline", "observation"),
    ("task",), ("decode",), ("grounding", "frames", 0, "detections", 0),
])
def test_owned_request_shapes_reject_nested_additions(path) -> None:
    document = request_document()
    target = document
    for key in path:
        target = target[key]
    target["unexpected"] = True
    with pytest.raises(ValueError):
        protocol.parse_request(json.dumps(document).encode())


def test_private_protocol_schema_compatibility():
    assert_peer_snapshot(
        Path(__file__).resolve().parents[2] / "testdata/private-protocol.schema.json",
        protocol.RunnerRequest.model_json_schema(mode="validation"),
        protocol.RunnerResponse.model_json_schema(mode="serialization"),
    )


def test_nested_integer_widths_and_closed_model_vocabulary():
    import copy
    for location, key, value in [
        ((), "input_width", 2**16), ((), "decode_start_index", 2**63),
        ((), "max_events", 0), ((), "max_answer_bytes", 0), ((), "max_response_bytes", 0),
        (("sampling",), "max_frames", 2**32),
        (("model",), "format", "uncontrolled"),
        (("grounding",), "schema", "uncontrolled"),
        (("grounding", "frames", 0, "detections", 0), "track_id", -1),
        (("model", "engine"), "gpu_memory_utilization", float("nan")),
    ]:
        changed = copy.deepcopy(request_document())
        target = changed
        for segment in location:
            target = target[segment]
        target[key] = value
        with pytest.raises(ValueError):
            protocol.parse_request(json.dumps(changed).encode())
    for key in ("observed_frames", "elapsed_ms"):
        response = {"answer": {"kind": "description", "text": "observed"}, "observed_frames": 1, "elapsed_ms": 0}
        response[key] = 2**64
        with pytest.raises(ValueError):
            protocol.RunnerResponse.model_validate(response)
