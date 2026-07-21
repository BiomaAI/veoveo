"""Prompt assembly and answer normalization for one reasoning pass."""

from __future__ import annotations

import json
from pathlib import Path

from .protocol import (
    EventsAnswer,
    IndexRange,
    ReasonedEvent,
    RunnerRequest,
)

MAX_EVENT_LABEL_BYTES = 256
MAX_EVENT_DESCRIPTION_BYTES = 4_096
MAX_TRACK_CITATIONS_PER_EVENT = 64
MAX_GROUNDING_PROMPT_FRAMES = 200


def task_instruction(request: RunnerRequest) -> str:
    task = request.task
    span = request.requested_range
    if task.kind == "describe_segment":
        focus = f" Focus on: {task.prompt}" if task.prompt else ""
        return (
            "Describe what happens across these frames as one factual paragraph."
            f"{focus}"
        )
    if task.kind == "detect_events":
        return (
            f"Detect events matching: {task.prompt}. Report each event as JSON with an "
            f"inclusive `range` of source-timeline indices between {span.start} and "
            f"{span.end}, a short `label`, a factual `description`, and `track_ids` "
            "citing grounding tracks when they justify the event. Report only events "
            "the frames support."
        )
    return f"Answer this question from the frames alone: {task.question}"


def build_prompt(request: RunnerRequest, frame_indices: list[int]) -> str:
    template = Path(request.pipeline.prompt_template_path).read_text(encoding="utf-8").strip()
    sections = [template]
    sections.append(
        "Frames are ordered and each carries its source-timeline index: "
        + ", ".join(str(index) for index in frame_indices)
    )
    if request.grounding is not None:
        lines = []
        for frame in request.grounding.frames[:MAX_GROUNDING_PROMPT_FRAMES]:
            detections = ", ".join(
                detection.label
                + (f" (track {detection.track_id})" if detection.track_id is not None else "")
                for detection in frame.detections
            )
            if detections:
                lines.append(f"index {frame.index}: {detections}")
        if lines:
            sections.append(
                "Grounded detections from a completed perception analysis:\n" + "\n".join(lines)
            )
    sections.append(task_instruction(request))
    return "\n\n".join(sections)


def events_json_schema(span: IndexRange, max_events: int) -> dict:
    """Structured-output schema forcing the typed events shape."""
    return {
        "type": "object",
        "properties": {
            "events": {
                "type": "array",
                "maxItems": max_events,
                "items": {
                    "type": "object",
                    "properties": {
                        "range": {
                            "type": "object",
                            "properties": {
                                "start": {
                                    "type": "integer",
                                    "minimum": span.start,
                                    "maximum": span.end,
                                },
                                "end": {
                                    "type": "integer",
                                    "minimum": span.start,
                                    "maximum": span.end,
                                },
                            },
                            "required": ["start", "end"],
                        },
                        "label": {"type": "string", "minLength": 1, "maxLength": 200},
                        "description": {"type": "string", "minLength": 1, "maxLength": 2000},
                        "track_ids": {
                            "type": "array",
                            "items": {"type": "integer", "minimum": 0},
                            "maxItems": MAX_TRACK_CITATIONS_PER_EVENT,
                        },
                    },
                    "required": ["range", "label", "description"],
                },
            }
        },
        "required": ["events"],
    }


def normalize_events(
    raw_text: str,
    span: IndexRange,
    max_events: int,
    grounded_tracks: set[int],
) -> EventsAnswer:
    """Parse model output into the typed events answer the server will accept.

    The Rust executor rejects out-of-range, unordered, or ungrounded output
    outright, so normalization drops what the contract would reject instead of
    inventing repairs for it.
    """
    document = json.loads(raw_text)
    events: list[ReasonedEvent] = []
    for entry in document.get("events", []):
        try:
            event = ReasonedEvent.model_validate(entry)
        except ValueError:
            continue
        if event.range.start > event.range.end:
            continue
        if event.range.start < span.start or event.range.end > span.end:
            continue
        label = event.label.strip()
        description = event.description.strip()
        if not label or not description:
            continue
        events.append(
            ReasonedEvent(
                range=event.range,
                label=label[:MAX_EVENT_LABEL_BYTES],
                description=description[:MAX_EVENT_DESCRIPTION_BYTES],
                track_ids=sorted(
                    {
                        track
                        for track in event.track_ids[:MAX_TRACK_CITATIONS_PER_EVENT]
                        if track in grounded_tracks
                    }
                ),
            )
        )
    events.sort(key=lambda event: (event.range.start, event.range.end))
    return EventsAnswer(events=events[:max_events])


def truncate_text(text: str, max_bytes: int) -> str:
    stripped = text.strip()
    encoded = stripped.encode("utf-8")
    if len(encoded) <= max_bytes:
        return stripped
    return encoded[:max_bytes].decode("utf-8", errors="ignore").strip()
