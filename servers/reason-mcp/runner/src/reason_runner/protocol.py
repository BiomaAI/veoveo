"""Wire-exact mirror of the Rust runner request/response contract."""

from __future__ import annotations

from typing import Annotated, Literal, Union

from pydantic import BaseModel, ConfigDict, Field

REQUEST_SCHEMA = "veoveo.reason-runner-request/v3"
RESPONSE_SCHEMA = "veoveo.reason-runner-response/v1"


I64 = Annotated[int, Field(ge=-(2**63), le=2**63 - 1)]
U16 = Annotated[int, Field(ge=0, le=2**16 - 1)]
U32 = Annotated[int, Field(ge=0, le=2**32 - 1)]
U64 = Annotated[int, Field(ge=0, le=2**64 - 1)]
PositiveU64 = Annotated[int, Field(ge=1, le=2**64 - 1)]


class _Model(BaseModel):
    model_config = ConfigDict(populate_by_name=True, extra="forbid", strict=True, allow_inf_nan=False, json_schema_serialization_defaults_required=True)


class IndexRange(_Model):
    start: I64
    end: I64


class Observation(_Model):
    width: U32
    height: U32
    maximum_frames: int = Field(ge=1, le=1_024)


class RunnerPipeline(_Model):
    pipeline_id: str
    prompt_template_path: str
    prompt_revision: str
    observation: Observation


class VllmEngine(_Model):
    kind: Literal["vllm"]
    gpu_memory_utilization: float = Field(ge=0.1, le=1.0)
    max_model_len: int = Field(ge=1_024, le=1_048_576)


class RunnerModel(_Model):
    model_config = ConfigDict(populate_by_name=True, extra="forbid", strict=True, allow_inf_nan=False, json_schema_serialization_defaults_required=True, protected_namespaces=())

    model_id: str
    model_path: str
    format: Literal["local_checkpoint"]
    model_digest: str | None = None
    engine: VllmEngine


class DescribeSegment(_Model):
    kind: Literal["describe_segment"]
    prompt: str | None = None


class DetectEvents(_Model):
    kind: Literal["detect_events"]
    prompt: str


class AnswerQuestion(_Model):
    kind: Literal["answer_question"]
    question: str


ReasoningTask = Annotated[
    Union[DescribeSegment, DetectEvents, AnswerQuestion], Field(discriminator="kind")
]


class GroundingDetection(_Model):
    label: str
    track_id: U64 | None = None


class GroundingFrame(_Model):
    index: I64
    detections: list[GroundingDetection]


class GroundingDetections(_Model):
    schema_: Literal["veoveo.reason-grounding/v1"] = Field(alias="schema")
    source_artifact_uri: str
    frames: list[GroundingFrame]

    def track_ids(self) -> set[int]:
        return {
            detection.track_id
            for frame in self.frames
            for detection in frame.detections
            if detection.track_id is not None
        }


class GreedyDecode(_Model):
    mode: Literal["greedy"]


class SampledDecode(_Model):
    mode: Literal["sampled"]
    temperature: float
    top_p: float
    seed: U64


DecodePolicy = Annotated[Union[GreedyDecode, SampledDecode], Field(discriminator="mode")]


class ObservationSampling(_Model):
    max_frames: U32


class RunnerRequest(_Model):
    model_config = ConfigDict(populate_by_name=True, extra="forbid", strict=True, allow_inf_nan=False, json_schema_serialization_defaults_required=True, protected_namespaces=())

    schema_: str = Field(alias="schema")
    task_id: str
    input_mp4: str
    input_width: U16
    input_height: U16
    response_json: str
    pipeline: RunnerPipeline
    model: RunnerModel
    task: ReasoningTask
    grounding: GroundingDetections | None = None
    requested_range: IndexRange
    decode_start_index: I64
    sampling: ObservationSampling
    decode: DecodePolicy
    max_events: PositiveU64
    max_answer_bytes: PositiveU64
    max_response_bytes: PositiveU64


class ReasonedEvent(_Model):
    range: IndexRange
    label: str
    description: str
    track_ids: list[U64] = Field(default_factory=list)


class DescriptionAnswer(_Model):
    kind: Literal["description"] = "description"
    text: str


class EventsAnswer(_Model):
    kind: Literal["events"] = "events"
    events: list[ReasonedEvent]


class TextAnswer(_Model):
    kind: Literal["answer"] = "answer"
    text: str


ReasoningAnswer = Annotated[
    Union[DescriptionAnswer, EventsAnswer, TextAnswer], Field(discriminator="kind")
]


class RunnerResponse(_Model):
    schema_: str = Field(alias="schema", default=RESPONSE_SCHEMA)
    answer: ReasoningAnswer
    observed_frames: U64
    elapsed_ms: U64

    def to_json(self) -> str:
        return self.model_dump_json(by_alias=True)


def parse_request(raw: bytes) -> RunnerRequest:
    request = RunnerRequest.model_validate_json(raw)
    if request.schema_ != REQUEST_SCHEMA:
        raise ValueError(f"unsupported runner request schema `{request.schema_}`")
    return request


def answer_kind_for(task: ReasoningTask) -> str:
    return {
        "describe_segment": "description",
        "detect_events": "events",
        "answer_question": "answer",
    }[task.kind]
