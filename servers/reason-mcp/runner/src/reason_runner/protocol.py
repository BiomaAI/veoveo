"""Wire-exact mirror of the Rust runner request/response contract."""

from __future__ import annotations

from typing import Annotated, Literal, Union

from pydantic import BaseModel, ConfigDict, Field, model_validator

REQUEST_SCHEMA = "veoveo.ai/reason-runner-request/v4"
RESPONSE_SCHEMA = "veoveo.ai/reason-runner-response/v2"


I64 = Annotated[int, Field(ge=-(2**63), le=2**63 - 1)]
U16 = Annotated[int, Field(ge=0, le=2**16 - 1)]
U32 = Annotated[int, Field(ge=0, le=2**32 - 1)]
U64 = Annotated[int, Field(ge=0, le=2**64 - 1)]
PositiveU64 = Annotated[int, Field(ge=1, le=2**64 - 1)]


class _Model(BaseModel):
    model_config = ConfigDict(validate_by_alias=True, validate_by_name=False, hide_input_in_errors=True, extra="forbid", strict=True, allow_inf_nan=False, json_schema_serialization_defaults_required=True)

    @model_validator(mode="before")
    @classmethod
    def admit_declared_fields(cls, value):
        # Pydantic's JSON path can otherwise hide mixed alias/name inputs.
        if isinstance(value, dict):
            admitted = {field.alias or name for name, field in cls.model_fields.items()}
            if value.keys() - admitted:
                raise ValueError("undeclared Reason protocol field")
        return value


class IndexRange(_Model):
    model_config = ConfigDict(**(_Model.model_config | {"frozen": True}))

    start: I64
    end: I64

    @model_validator(mode="after")
    def ordered(self) -> "IndexRange":
        if self.start > self.end:
            raise ValueError("video range must be ordered")
        return self


class Observation(_Model):
    width: U32
    height: U32
    maximumFrames: int = Field(ge=1, le=1_024)


class RunnerPipeline(_Model):
    pipelineId: str
    promptTemplatePath: str
    promptRevision: str
    observation: Observation


class VllmEngine(_Model):
    kind: Literal["vllm"]
    gpuMemoryUtilization: float = Field(ge=0.1, le=1.0)
    maxModelLen: int = Field(ge=1_024, le=1_048_576)


class RunnerModel(_Model):
    model_config = ConfigDict(validate_by_alias=True, validate_by_name=False, hide_input_in_errors=True, extra="forbid", strict=True, allow_inf_nan=False, json_schema_serialization_defaults_required=True, protected_namespaces=())

    modelId: str
    modelPath: str
    format: Literal["local_checkpoint"]
    modelDigest: str | None = None
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
    trackId: U64 | None = None


class GroundingFrame(_Model):
    index: I64
    detections: list[GroundingDetection]


class GroundingDetections(_Model):
    schema_: Literal["veoveo.ai/reason-grounding/v2"] = Field(alias="schema")
    sourceArtifactUri: str
    frames: list[GroundingFrame]

    def track_ids(self) -> set[int]:
        return {
            detection.trackId
            for frame in self.frames
            for detection in frame.detections
            if detection.trackId is not None
        }


class GreedyDecode(_Model):
    mode: Literal["greedy"]


class SampledDecode(_Model):
    mode: Literal["sampled"]
    temperature: float
    topP: float
    seed: U64


DecodePolicy = Annotated[Union[GreedyDecode, SampledDecode], Field(discriminator="mode")]


class ObservationSampling(_Model):
    maxFrames: U32


class RunnerRequest(_Model):
    model_config = ConfigDict(validate_by_alias=True, validate_by_name=False, hide_input_in_errors=True, extra="forbid", strict=True, allow_inf_nan=False, json_schema_serialization_defaults_required=True, protected_namespaces=())

    schema_: Literal["veoveo.ai/reason-runner-request/v4"] = Field(alias="schema")
    taskId: str
    inputMp4: str
    inputWidth: U16
    inputHeight: U16
    responseJson: str
    pipeline: RunnerPipeline
    model: RunnerModel
    task: ReasoningTask
    grounding: GroundingDetections | None = None
    requestedRange: IndexRange
    decodeStartIndex: I64
    sampling: ObservationSampling
    decode: DecodePolicy
    maxEvents: PositiveU64
    maxAnswerBytes: PositiveU64
    maxResponseBytes: PositiveU64


class ReasonedEvent(_Model):
    range: IndexRange
    label: str
    description: str
    trackIds: list[U64] = Field(default_factory=list)


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
    schema_: Literal["veoveo.ai/reason-runner-response/v2"] = Field(alias="schema", default=RESPONSE_SCHEMA)
    answer: ReasoningAnswer
    observedFrames: U64
    elapsedMs: U64

    def to_json(self) -> str:
        return self.model_dump_json(by_alias=True)


def parse_request(raw: bytes) -> RunnerRequest:
    return RunnerRequest.model_validate_json(raw)


def answer_kind_for(task: ReasoningTask) -> str:
    return {
        "describe_segment": "description",
        "detect_events": "events",
        "answer_question": "answer",
    }[task.kind]
