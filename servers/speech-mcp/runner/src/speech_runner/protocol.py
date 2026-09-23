"""Closed private request and response validation, independent of inference."""
from __future__ import annotations

import json
import math
from dataclasses import dataclass
from pathlib import Path

PROTOCOL = "veoveo.speech-worker/v1"
MODEL = "moondream/parakeet-ultra"
REVISION = "510e6f5a1c4619f39c72b083c091476935734e65"
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


@dataclass(frozen=True)
class Request:
    operation: str
    path: Path | None = None
    sample_rate: int | None = None
    max_duration_seconds: int = 0

    @classmethod
    def parse(cls, line: bytes, work: Path) -> Request:
        value = json.loads(line)
        if not isinstance(value, dict):
            raise ValueError("invalid request")
        operation = value.get("operation")
        if operation == "probe" and set(value) == {"operation"}:
            return cls(operation)
        if operation == "file" and set(value) == {"operation", "path", "max_duration_seconds"}:
            duration = integer(value["max_duration_seconds"], 1, 7200)
            if not isinstance(value["path"], str):
                raise ValueError("invalid path")
            path = Path(value["path"]).resolve(strict=True)
            if not path.is_relative_to(work) or not path.is_file():
                raise ValueError("source outside private workspace")
            if not 0 < path.stat().st_size <= 2 * 1024**3:
                raise ValueError("source size exceeds limit")
            return cls(operation, path=path, max_duration_seconds=duration)
        if operation == "live" and set(value) == {"operation", "sample_rate", "max_duration_seconds"}:
            return cls(operation, sample_rate=integer(value["sample_rate"], 8000, 48000),
                       max_duration_seconds=integer(value["max_duration_seconds"], 1, 120))
        raise ValueError("invalid operation")


def transcript(value: dict[str, object], duration_limit: int) -> dict[str, object]:
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
    return {"text": text(value["text"]), "duration_seconds": duration, "segments": output}


def encode(value: dict[str, object]) -> bytes:
    encoded = json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode() + b"\n"
    if len(encoded) > MAX_RESPONSE_BYTES:
        raise ValueError("response exceeds limit")
    return encoded
