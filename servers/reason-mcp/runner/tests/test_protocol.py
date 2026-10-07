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
        "taskId": "01983da0-0000-7000-8000-000000000001",
        "inputMp4": "/tmp/input.mp4",
        "inputWidth": 1920,
        "inputHeight": 1080,
        "responseJson": "/tmp/response.json",
        "pipeline": {
            "pipelineId": "video-reasoning",
            "promptTemplatePath": "/etc/veoveo/reason/prompt-template.txt",
            "promptRevision": "v1",
            "observation": {"width": 640, "height": 360, "maximumFrames": 6},
        },
        "model": {
            "modelId": "world-model",
            "modelPath": "/models/world-model",
            "format": "local_checkpoint",
            "modelDigest": "sha256:test",
            "engine": {
                "kind": "vllm",
                "gpuMemoryUtilization": 0.7,
                "maxModelLen": 8_192,
            },
        },
        "task": {"kind": "detect_events", "prompt": "vehicles entering the frame"},
        "grounding": {
            "schema": "veoveo.ai/reason-grounding/v2",
            "sourceArtifactUri": "stream://artifact/test",
            "frames": [
                {"index": 10, "detections": [{"label": "car", "trackId": 7}]},
            ],
        },
        "requestedRange": {"start": 0, "end": 3_000_000_000},
        "decodeStartIndex": 0,
        "sampling": {"maxFrames": 16},
        "decode": {"mode": "greedy"},
        "maxEvents": 100,
        "maxAnswerBytes": 10_000,
        "maxResponseBytes": 1_000_000,
    }


def test_request_roundtrip_preserves_wire_names() -> None:
    request = protocol.parse_request(json.dumps(request_document()).encode())
    assert request.schema_ == protocol.REQUEST_SCHEMA
    assert request.task.kind == "detect_events"
    assert request.decode.mode == "greedy"
    assert request.grounding is not None
    assert request.grounding.track_ids() == {7}
    assert request.model.engine.gpuMemoryUtilization == 0.7
    assert request.model.engine.maxModelLen == 8_192
    assert request.pipeline.observation.maximumFrames == 6


def test_unsupported_schema_is_rejected() -> None:
    document = request_document()
    document["schema"] = "something-else/v9"
    with pytest.raises(ValueError):
        protocol.parse_request(json.dumps(document).encode())


def test_unknown_fields_are_rejected() -> None:
    document = request_document()
    document["surprise"] = True
    with pytest.raises(ValueError):
        protocol.parse_request(json.dumps(document).encode())


def test_invalid_engine_budget_is_rejected() -> None:
    document = request_document()
    document["model"]["engine"]["gpuMemoryUtilization"] = 0.0
    with pytest.raises(ValueError):
        protocol.parse_request(json.dumps(document).encode())


def test_pipeline_observation_budget_bounds_the_request() -> None:
    request = protocol.parse_request(json.dumps(request_document()).encode())
    assert request.sampling.maxFrames == 16
    assert observation_frame_limit(request) == 6

    request.sampling.maxFrames = 4
    assert observation_frame_limit(request) == 4


def test_response_serializes_the_tagged_answer() -> None:
    response = protocol.RunnerResponse(
        answer=protocol.EventsAnswer(
            events=[
                protocol.ReasonedEvent(
                    range=protocol.IndexRange(start=1, end=2),
                    label="car passes",
                    description="a car crosses the frame",
                    trackIds=[7],
                )
            ]
        ),
        observedFrames=3,
        elapsedMs=25,
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
        ((), "inputWidth", 2**16), ((), "decodeStartIndex", 2**63),
        ((), "maxEvents", 0), ((), "maxAnswerBytes", 0), ((), "maxResponseBytes", 0),
        (("sampling",), "maxFrames", 2**32),
        (("model",), "format", "uncontrolled"),
        (("grounding",), "schema", "uncontrolled"),
        (("grounding", "frames", 0, "detections", 0), "trackId", -1),
        (("model", "engine"), "gpuMemoryUtilization", float("nan")),
    ]:
        changed = copy.deepcopy(request_document())
        target = changed
        for segment in location:
            target = target[segment]
        target[key] = value
        with pytest.raises(ValueError):
            protocol.parse_request(json.dumps(changed).encode())
    for key in ("observedFrames", "elapsedMs"):
        response = {"answer": {"kind": "description", "text": "observed"}, "observedFrames": 1, "elapsedMs": 0}
        response[key] = 2**64
        with pytest.raises(ValueError):
            protocol.RunnerResponse.model_validate(response)


def test_signed_ranges_share_owner_admission_before_runner_work() -> None:
    assert protocol.IndexRange(start=-10, end=-1).model_dump() == {"start": -10, "end": -1}
    with pytest.raises(ValueError, match="range must be ordered"):
        protocol.IndexRange(start=2, end=1)


@pytest.mark.parametrize("path,current,retired", [
    ((), "taskId", "task_id"), ((), "inputMp4", "input_mp4"),
    ((), "inputWidth", "input_width"), ((), "inputHeight", "input_height"),
    ((), "responseJson", "response_json"), ((), "requestedRange", "requested_range"),
    ((), "decodeStartIndex", "decode_start_index"), ((), "maxEvents", "max_events"),
    ((), "maxAnswerBytes", "max_answer_bytes"), ((), "maxResponseBytes", "max_response_bytes"),
    (("pipeline",), "pipelineId", "pipeline_id"),
    (("pipeline",), "promptTemplatePath", "prompt_template_path"),
    (("pipeline",), "promptRevision", "prompt_revision"),
    (("pipeline", "observation"), "maximumFrames", "maximum_frames"),
    (("model",), "modelId", "model_id"), (("model",), "modelPath", "model_path"),
    (("model",), "modelDigest", "model_digest"),
    (("model", "engine"), "gpuMemoryUtilization", "gpu_memory_utilization"),
    (("model", "engine"), "maxModelLen", "max_model_len"),
    (("grounding",), "sourceArtifactUri", "source_artifact_uri"),
    (("grounding", "frames", 0, "detections", 0), "trackId", "track_id"),
    (("sampling",), "maxFrames", "max_frames"),
])
def test_request_admits_one_spelling_on_both_decoder_paths(path, current, retired):
    import copy
    current_document = request_document()
    for decode in (protocol.RunnerRequest.model_validate,
                   lambda value: protocol.RunnerRequest.model_validate_json(json.dumps(value))):
        admitted = decode(current_document)
        assert admitted.taskId == current_document["taskId"]
        for mode in ("replacement", "mixed", "conflicting"):
            changed = copy.deepcopy(current_document)
            target = changed
            for part in path:
                target = target[part]
            value = target[current]
            if mode == "replacement":
                del target[current]
            target[retired] = None if mode == "conflicting" else value
            with pytest.raises(ValueError):
                decode(changed)


def test_response_and_sampled_decode_refuse_retired_mixed_fields():
    for decode in (protocol.RunnerResponse.model_validate,
                   lambda value: protocol.RunnerResponse.model_validate_json(json.dumps(value))):
        current = {"schema": protocol.RESPONSE_SCHEMA,
                   "answer": {"kind": "events", "events": [{"range": {"start": 1, "end": 2},
                       "label": "entry", "description": "car enters", "trackIds": [7]}]},
                   "observedFrames": 3, "elapsedMs": 2}
        assert decode(current).observedFrames == 3
        import copy
        for path, key, retired in [((), "observedFrames", "observed_frames"),
                                  ((), "elapsedMs", "elapsed_ms"),
                                  (("answer", "events", 0), "trackIds", "track_ids")]:
            for mode in ("replacement", "mixed", "conflicting"):
                bad = copy.deepcopy(current)
                target = bad
                for part in path:
                    target = target[part]
                value = target[key]
                if mode == "replacement":
                    del target[key]
                target[retired] = None if mode == "conflicting" else value
                with pytest.raises(ValueError):
                    decode(bad)
    for decode in (protocol.RunnerRequest.model_validate,
                   lambda value: protocol.RunnerRequest.model_validate_json(json.dumps(value))):
        value = request_document()
        value["decode"] = {"mode": "sampled", "temperature": 0.5, "topP": 0.9, "seed": 1}
        assert decode(value).decode.topP == 0.9
        value["decode"]["top_p"] = 0.9
        with pytest.raises(ValueError):
            decode(value)


def test_actual_entrypoint_refuses_retired_request_before_inference_or_output(tmp_path, monkeypatch):
    from reason_runner import main, inference
    calls = []
    monkeypatch.setattr(inference, "run", lambda request: calls.append(request))
    # The entrypoint's stdout redirection is process hygiene, not a test-side fd effect.
    monkeypatch.setattr(main.os, "dup2", lambda *_: None)
    request_path = tmp_path / "request.json"
    response_path = tmp_path / "response.json"
    document = request_document()
    document["responseJson"] = str(response_path)
    document["response_json"] = str(response_path)
    request_path.write_text(json.dumps(document))
    monkeypatch.setattr(sys, "argv", ["reason-runner", "--request-json", str(request_path),
                                     "--response-json", str(response_path)])
    with pytest.raises(ValueError):
        main.main()
    assert calls == []
    assert not response_path.exists()
    assert not response_path.with_suffix(".tmp").exists()


def test_current_protocol_markers_refuse_retired_versions_and_internal_schema_name():
    for decode in (protocol.RunnerRequest.model_validate,
                   lambda value: protocol.RunnerRequest.model_validate_json(json.dumps(value))):
        for marker in ("veoveo.reason-runner-request/v3", "veoveo.ai/reason-runner-request/v3"):
            value = request_document()
            value["schema"] = marker
            with pytest.raises(ValueError):
                decode(value)
        value = request_document()
        value["schema_"] = value["schema"]
        with pytest.raises(ValueError):
            decode(value)
    for decode in (protocol.RunnerResponse.model_validate,
                   lambda value: protocol.RunnerResponse.model_validate_json(json.dumps(value))):
        value = {"schema": "veoveo.reason-runner-response/v1", "answer": {"kind": "description", "text": "scene"},
                 "observedFrames": 1, "elapsedMs": 0}
        with pytest.raises(ValueError):
            decode(value)
