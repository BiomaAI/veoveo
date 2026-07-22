"""One bounded world-model inference pass through the image's vLLM runtime.

vLLM is imported lazily so every other module — protocol parsing, frame
sampling, prompt assembly, normalization — stays testable without a GPU or
the runtime installed.
"""

from __future__ import annotations

import base64
import io
import time

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
    frames = sample_frames(
        Path(request.input_mp4),
        observation_frame_limit(request),
        request.pipeline.observation.width,
        request.pipeline.observation.height,
        request.decode_start_index,
    )
    prompt = build_prompt(request, [frame.index for frame in frames])
    raw_text = _generate(request, prompt, frames)
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


def _generate(request: RunnerRequest, prompt: str, frames: list[ObservedFrame]) -> str:
    from vllm import LLM
    from vllm.sampling_params import SamplingParams, StructuredOutputsParams

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
    model = LLM(
        model=request.model.model_path,
        trust_remote_code=True,
        limit_mm_per_prompt={"image": len(frames)},
        gpu_memory_utilization=request.model.engine.gpu_memory_utilization,
        max_model_len=request.model.engine.max_model_len,
    )
    content: list[dict] = [
        {"type": "image_url", "image_url": {"url": _data_url(frame)}} for frame in frames
    ]
    content.append({"type": "text", "text": prompt})
    outputs = model.chat(
        [{"role": "user", "content": content}],
        sampling_params=SamplingParams(**parameters),
    )
    return outputs[0].outputs[0].text


def _data_url(frame: ObservedFrame) -> str:
    buffer = io.BytesIO()
    frame.image.save(buffer, format="PNG")
    encoded = base64.b64encode(buffer.getvalue()).decode("ascii")
    return f"data:image/png;base64,{encoded}"
