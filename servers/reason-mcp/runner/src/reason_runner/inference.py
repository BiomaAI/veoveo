"""One bounded world-model inference pass through the image's vLLM runtime.

vLLM is imported lazily so every other module — protocol parsing, frame
sampling, prompt assembly, normalization — stays testable without a GPU or
the runtime installed.
"""

from __future__ import annotations

import time
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from vllm import LLM

from .prompting import (
    build_prompt,
    events_json_schema,
    normalize_events,
    truncate_text,
)
from .protocol import (
    DescriptionAnswer,
    RunnerRequest,
    RunnerResponse,
    TextAnswer,
    answer_kind_for,
)
from .video import ObservedFrame, sample_frames


def observation_frame_limit(request: RunnerRequest) -> int:
    return min(request.sampling.max_frames, request.pipeline.observation.maximum_frames)


def run(request: RunnerRequest) -> RunnerResponse:
    from pathlib import Path

    started = time.monotonic()
    from .gpu_model import create_model

    model = create_model(request, observation_frame_limit(request))
    frames = sample_frames(
        Path(request.input_mp4),
        observation_frame_limit(request),
        request.pipeline.observation.width,
        request.pipeline.observation.height,
        request.decode_start_index,
        request.input_width,
        request.input_height,
    )
    prompt = build_prompt(request, [frame.index for frame in frames])
    raw_text = _generate(model, request, prompt, frames)
    answer_kind = answer_kind_for(request.task)
    if answer_kind == "events":
        grounded = request.grounding.track_ids() if request.grounding else set()
        answer = normalize_events(raw_text, request.requested_range, request.max_events, grounded)
    elif answer_kind == "description":
        answer = DescriptionAnswer(text=truncate_text(raw_text, request.max_answer_bytes))
    else:
        answer = TextAnswer(text=truncate_text(raw_text, request.max_answer_bytes))
    elapsed_ms = int((time.monotonic() - started) * 1_000)
    return RunnerResponse(answer=answer, observed_frames=len(frames), elapsed_ms=elapsed_ms)


def _generate(model: LLM, request: RunnerRequest, prompt: str, frames: list[ObservedFrame]) -> str:
    from vllm.sampling_params import SamplingParams, StructuredOutputsParams
    from .gpu_model import generate

    decode = request.decode
    parameters = {
        "max_tokens": min(4_096, max(256, request.max_answer_bytes // 4)),
    }
    if decode.mode == "greedy":
        parameters["temperature"] = 0.0
    else:
        parameters["temperature"] = decode.temperature
        parameters["top_p"] = decode.top_p
        parameters["seed"] = decode.seed
    if answer_kind_for(request.task) == "events":
        parameters["structured_outputs"] = StructuredOutputsParams(
            json=events_json_schema(request.requested_range, request.max_events)
        )
    return generate(model, request, prompt, frames, SamplingParams(**parameters))
