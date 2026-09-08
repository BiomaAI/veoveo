"""Owned CUDA image embeddings for the single-process vLLM Qwen3-VL adapter.

The public reasoning contract stays model-neutral. This internal adapter admits
the Qwen3-VL architecture and reuses the engine's already loaded vision tower.
"""

from __future__ import annotations

import os
from typing import TYPE_CHECKING
from uuid import uuid4

from .protocol import RunnerRequest
from .video import ObservedFrame

if TYPE_CHECKING:
    import torch
    from vllm import LLM
    from vllm.sampling_params import SamplingParams


def decoder_reservation_gib(request: RunnerRequest) -> float:
    """Reserve source DPB, RGB output, observations, and CUDA decoder overhead.

    NVDEC emits one selected RGB surface at a time. Sixteen reference surfaces
    cover the H.264 DPB; RGB-sized accounting also covers its smaller NV12 form.
    vLLM separately profiles the vision tower at the admitted observation size.
    """
    count = min(request.sampling.max_frames, request.pipeline.observation.maximum_frames)
    observation = request.pipeline.observation
    if min(request.input_width, request.input_height, observation.width, observation.height, count) <= 0:
        raise ValueError("frame counts and dimensions must be positive")
    source_bytes = request.input_width * request.input_height * 3 * 20
    observation_bytes = observation.width * observation.height * 3 * count * 8
    return (source_bytes + observation_bytes + 512 * 1024**2) / 1024**3


def create_model(request: RunnerRequest, maximum_frames: int) -> LLM:
    # Set before importing vLLM. GPU tensors never enter its RPC serializer.
    os.environ["VLLM_ENABLE_V1_MULTIPROCESSING"] = "0"
    import torch
    from vllm import LLM
    from vllm.v1.engine.core_client import InprocClient
    from vllm.v1.executor.uniproc_executor import UniProcExecutor

    if not torch.cuda.is_available():
        raise RuntimeError("Reason inference requires an NVIDIA CUDA device")
    observation = request.pipeline.observation
    model = LLM(
        model=request.model.model_path,
        trust_remote_code=False,
        tensor_parallel_size=1,
        pipeline_parallel_size=1,
        distributed_executor_backend="uni",
        enable_mm_embeds=True,
        mm_processor_cache_gb=0,
        mm_ipc_gpu_memory_gb=decoder_reservation_gib(request),
        limit_mm_per_prompt={"image": {
            "count": maximum_frames,
            "width": observation.width,
            "height": observation.height,
        }},
        gpu_memory_utilization=request.model.engine.gpu_memory_utilization,
        max_model_len=request.model.engine.max_model_len,
        max_num_seqs=1,
    )
    core = model.llm_engine.engine_core
    if not isinstance(core, InprocClient) or type(core.engine_core.model_executor) is not UniProcExecutor:
        raise RuntimeError("Reason CUDA embeddings require an in-process single-worker engine")
    return model


def generate(
    model: LLM,
    request: RunnerRequest,
    prompt: str,
    frames: list[ObservedFrame],
    parameters: SamplingParams,
) -> str:
    import torch
    from transformers import AutoProcessor
    from transformers.image_processing_backends import TorchvisionBackend
    from vllm.model_executor.models.qwen3_vl import Qwen3VLForConditionalGeneration

    if not frames or any(not frame.image.is_cuda for frame in frames):
        raise RuntimeError("Reason accepts only owned CUDA observation tensors")
    processor = AutoProcessor.from_pretrained(request.model.model_path, trust_remote_code=False)
    if not isinstance(processor.image_processor, TorchvisionBackend):
        raise RuntimeError("Reason requires the CUDA-capable Transformers image processor")
    # CPU grid and token metadata never contain image pixels or embeddings.
    processed = processor.image_processor(
        images=[frame.image for frame in frames],
        input_data_format="channels_first",
        device=frames[0].image.device,
        return_tensors="pt",
    )
    pixels: torch.Tensor = processed["pixel_values"]
    grid: torch.Tensor = processed["image_grid_thw"]
    if not pixels.is_cuda or grid.is_cuda or tuple(grid.shape) != (len(frames), 3):
        raise RuntimeError("Reason image processor violated CUDA pixels / host grid ownership")
    owner_pid = os.getpid()

    def encode(engine_model: torch.nn.Module) -> torch.Tensor:
        if os.getpid() != owner_pid or not isinstance(engine_model, Qwen3VLForConditionalGeneration):
            raise RuntimeError("Reason admits Qwen3-VL in the owning CUDA process only")
        with torch.inference_mode():
            # visual.forward includes the base features AND every deepstack level.
            embeddings = engine_model.visual(pixels, grid_thw=grid)
        expected_width = engine_model.visual_dim * (1 + engine_model.deepstack_num_level)
        if not embeddings.is_cuda or embeddings.ndim != 2 or embeddings.shape[1] != expected_width:
            raise RuntimeError("Reason vision tower returned invalid CUDA deepstack embeddings")
        return embeddings

    encoded = model.apply_model(encode)
    if len(encoded) != 1:
        raise RuntimeError("Reason requires exactly one embedding owner")
    embeddings = encoded[0]
    content = [{"type": "image"} for _ in frames]
    content.append({"type": "text", "text": prompt})
    rendered = processor.apply_chat_template(
        [{"role": "user", "content": content}], tokenize=False, add_generation_prompt=True
    )
    outputs = model.generate(
        {
            "prompt": rendered,
            "multi_modal_data": {"image": {"image_embeds": embeddings, "image_grid_thw": grid}},
            # Unique IDs avoid the default tensor-content hash and its CPU readback.
            "multi_modal_uuids": {"image": [uuid4().hex for _ in frames]},
        },
        sampling_params=parameters,
        use_tqdm=False,
    )
    # All frame and embedding owners remain live until synchronous generation ends.
    return outputs[0].outputs[0].text
