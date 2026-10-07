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
    return min(request.sampling.maxFrames, request.pipeline.observation.maximumFrames)


def run(request: RunnerRequest) -> RunnerResponse:
    from pathlib import Path

    started = time.monotonic()
    from .gpu_model import create_model

    model = create_model(request, observation_frame_limit(request))
    frames = sample_frames(
        Path(request.inputMp4),
        observation_frame_limit(request),
        request.pipeline.observation.width,
        request.pipeline.observation.height,
        request.decodeStartIndex,
        request.inputWidth,
        request.inputHeight,
    )
    prompt = build_prompt(request, [frame.index for frame in frames])
    raw_text = _generate(model, request, prompt, frames)
    answer_kind = answer_kind_for(request.task)
    if answer_kind == "events":
        grounded = request.grounding.track_ids() if request.grounding else set()
        answer = normalize_events(raw_text, request.requestedRange, request.maxEvents, grounded)
    elif answer_kind == "description":
        answer = DescriptionAnswer(text=truncate_text(raw_text, request.maxAnswerBytes))
    else:
        answer = TextAnswer(text=truncate_text(raw_text, request.maxAnswerBytes))
    elapsed_ms = int((time.monotonic() - started) * 1_000)
    return RunnerResponse(answer=answer, observedFrames=len(frames), elapsedMs=elapsed_ms)


def _generate(model: LLM, request: RunnerRequest, prompt: str, frames: list[ObservedFrame]) -> str:
    from vllm.sampling_params import SamplingParams, StructuredOutputsParams
    from .gpu_model import generate

    decode = request.decode
    parameters = {
        "max_tokens": min(4_096, max(256, request.maxAnswerBytes // 4)),
    }
    if decode.mode == "greedy":
        parameters["temperature"] = 0.0
    else:
        parameters["temperature"] = decode.temperature
        parameters["top_p"] = decode.topP
        parameters["seed"] = decode.seed
    if answer_kind_for(request.task) == "events":
        parameters["structured_outputs"] = StructuredOutputsParams(
            json=events_json_schema(request.requestedRange, request.maxEvents)
        )
    return generate(model, request, prompt, frames, SamplingParams(**parameters))
