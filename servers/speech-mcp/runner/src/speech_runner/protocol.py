"""Closed private request and response validation, independent of inference."""
from __future__ import annotations

import json
import math
from dataclasses import dataclass
from pathlib import Path
from typing import Annotated, Literal
from pydantic import BaseModel, ConfigDict, Field, TypeAdapter, ValidationError

PROTOCOL = "veoveo.ai/speech-worker/v2"
MAX_FRAME_BYTES = 192_000
MAX_TEXT_BYTES = 512 * 1024
MAX_RESPONSE_BYTES = 8 * 1024 * 1024


def integer(value: object, lower: int, upper: int) -> int:
    if type(value) is not int or not lower <= value <= upper:
        raise ValueError("invalid integer")
    return value


def number(value: object) -> float:
    if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
        raise ValueError("invalid timestamp")
    return float(value)


def text(value: object) -> str:
    if not isinstance(value, str) or len(value.encode("utf-8")) > MAX_TEXT_BYTES:
        raise ValueError("invalid text")
    return value


class _Wire(BaseModel):
    model_config = ConfigDict(hide_input_in_errors=True, extra="forbid", strict=True, allow_inf_nan=False)


class ProbeRequest(_Wire):
    operation: Literal["probe"]


class FileRequest(_Wire):
    operation: Literal["file"]
    path: str
    maxDurationSeconds: Annotated[int, Field(ge=0, le=2**32 - 1)]


class LiveRequest(_Wire):
    operation: Literal["live"]
    sampleRate: Annotated[int, Field(ge=0, le=2**32 - 1)]
    maxDurationSeconds: Annotated[int, Field(ge=0, le=2**32 - 1)]


RequestWire = Annotated[ProbeRequest | FileRequest | LiveRequest, Field(discriminator="operation")]
REQUEST_ADAPTER = TypeAdapter(RequestWire, config=ConfigDict(hide_input_in_errors=True))


class Word(_Wire):
    word: str
    start: float
    end: float


class Segment(_Wire):
    text: str
    start: float
    end: float
    words: list[Word]


class Transcript(_Wire):
    text: str
    durationSeconds: float
    segments: list[Segment]


class AcceptedEvent(_Wire):
    kind: Literal["accepted"]


class ReadyEvent(_Wire):
    kind: Literal["ready"]
    protocol: str
    device: str
    model: str
    revision: str


class TranscriptEvent(_Wire):
    kind: Literal["transcript"]
    complete: bool
    transcript: Transcript


class ErrorEvent(_Wire):
    kind: Literal["error"]
    code: Literal["invalid_input", "capacity", "inference_failed", "timed_out"]


EventWire = Annotated[AcceptedEvent | ReadyEvent | TranscriptEvent | ErrorEvent, Field(discriminator="kind")]
EVENT_ADAPTER = TypeAdapter(EventWire, config=ConfigDict(hide_input_in_errors=True))


def validate_wire(adapter: TypeAdapter, value: object, *, encoded: bool = False):
    """Worker errors name validation kinds without reflecting audio or input values."""
    try:
        return adapter.validate_json(value) if encoded else adapter.validate_python(value)
    except ValidationError as error:
        kinds = sorted({item["type"] for item in error.errors(include_input=False, include_context=False, include_url=False)})
        raise ValueError("invalid worker input: " + ", ".join(kinds)) from error


@dataclass(frozen=True)
class Request:
    operation: str
    path: Path | None = None
    sample_rate: Annotated[int, Field(ge=0, le=2**32 - 1)] | None = None
    max_duration_seconds: Annotated[int, Field(ge=0, le=2**32 - 1)] = 0

    @classmethod
    def parse(cls, line: bytes, work: Path) -> Request:
        value = validate_wire(REQUEST_ADAPTER, line, encoded=True)
        if isinstance(value, ProbeRequest):
            return cls(value.operation)
        if isinstance(value, FileRequest):
            duration = integer(value.maxDurationSeconds, 1, 7200)
            path = Path(value.path).resolve(strict=True)
            if not path.is_relative_to(work) or not path.is_file():
                raise ValueError("source outside private workspace")
            if not 0 < path.stat().st_size <= 2 * 1024**3:
                raise ValueError("source size exceeds limit")
            return cls(value.operation, path=path, max_duration_seconds=duration)
        if isinstance(value, LiveRequest):
            return cls(value.operation, sample_rate=integer(value.sampleRate, 8000, 48000),
                       max_duration_seconds=integer(value.maxDurationSeconds, 1, 120))
        raise ValueError("invalid operation")


def provider_transcript(value: dict[str, object], duration_limit: int) -> dict[str, object]:
    """Project provider output into the domain shape; never copy diagnostic fields."""
    duration = number(value["duration_seconds"])
    if duration > duration_limit + 0.1:
        raise ValueError("transcript exceeds source bound")
    segments = value.get("segments", [])
    if not isinstance(segments, list) or len(segments) > 10_000:
        raise ValueError("invalid segment count")
    word_count = 0
    output = []
    for segment in segments:
        words = segment.get("words", [])
        word_count += len(words)
        if word_count > 100_000:
            raise ValueError("invalid word count")
        output.append({"text": text(segment["text"]), "start": number(segment["start"]),
                       "end": number(segment["end"]), "words": [
                           {"word": text(word["word"]), "start": number(word["start"]),
                            "end": number(word["end"])} for word in words]})
    return Transcript(text=text(value["text"]), durationSeconds=duration, segments=output).model_dump(by_alias=True)


def encode(value: dict[str, object]) -> bytes:
    validate_wire(EVENT_ADAPTER, value)
    encoded = json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode() + b"\n"
    if len(encoded) > MAX_RESPONSE_BYTES:
        raise ValueError("response exceeds limit")
    return encoded
