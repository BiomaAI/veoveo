"""Generate the CUDA reference fixture through the pinned runtime's Transformers.

Run with uv inside the qualified vLLM image, using its installed Python packages.
The checkpoint is local and all model loading is offline.
"""

import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path

import torch
import torch.nn.functional as functional
from torch.nn.attention import SDPBackend, sdpa_kernel
from transformers import AutoModel, AutoTokenizer

REVISION = "97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3"
IMAGE = "sha256:8a69ffad015f138d7170c4ddc429e230a3bc1c1719f67e14324749df200a4b90"
TASK = "Retrieve relevant operational records."
QUERIES = ["Where is the evacuation shelter?", "Which roads need inspection after flooding?"]
DOCUMENTS = [
    "The municipal school is the designated evacuation shelter for the northern district.",
    "The completed flood analysis recommends inspecting the river bridge and its approach roads.",
]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise RuntimeError("reference output already exists")
    if not torch.cuda.is_available():
        raise RuntimeError("reference generation requires an NVIDIA CUDA device")
    for line in args.manifest.read_text().splitlines():
        expected, name = line.split("  ", 1)
        with (args.checkpoint / name).open("rb") as source:
            if hashlib.file_digest(source, "sha256").hexdigest() != expected:
                raise RuntimeError(f"checkpoint digest mismatch: {name}")

    tokenizer = AutoTokenizer.from_pretrained(
        args.checkpoint, padding_side="left", local_files_only=True, trust_remote_code=False
    )
    model = AutoModel.from_pretrained(
        args.checkpoint,
        dtype=torch.bfloat16,
        attn_implementation="sdpa",
        local_files_only=True,
        trust_remote_code=False,
    ).eval().cuda()
    texts = [f"Instruct: {TASK}\nQuery:{text}" for text in QUERIES] + DOCUMENTS
    batch = tokenizer(texts, padding=True, truncation=False, return_tensors="pt").to("cuda")
    # cuDNN supports the non-null padding mask in this reference batch. Require
    # its fused CUDA attention kernel rather than changing the padding semantics.
    with torch.inference_mode(), sdpa_kernel(SDPBackend.CUDNN_ATTENTION):
        output = model(**batch).last_hidden_state
        if not output.is_cuda or not bool(torch.all(batch["attention_mask"][:, -1])):
            raise RuntimeError("expected CUDA hidden states with left-padded inputs")
        vectors = functional.normalize(output[:, -1].float(), p=2, dim=1)
        if vectors.shape != (len(texts), 1024) or not bool(torch.isfinite(vectors).all()):
            raise RuntimeError("invalid reference vector shape or components")
        torch.cuda.synchronize()
        # Host transfer is required at the JSON fixture export boundary.
        values = vectors.cpu().tolist()

    document = {
        "schema": "veoveo.ai/embedding-reference/v1",
        "space": {"model": "qwen3-embedding-0.6b", "revision": REVISION, "dimension": 1024, "runtimeImage": IMAGE},
        "generator": {"torch": torch.__version__, "transformers": importlib.metadata.version("transformers"), "cuda": torch.version.cuda, "gpu": torch.cuda.get_device_name(), "attention": "cudnn_attention", "dtype": "bfloat16"},
        "task": TASK,
        "queries": [{"text": text, "values": values[index]} for index, text in enumerate(QUERIES)],
        "documents": [{"text": text, "values": values[len(QUERIES) + index]} for index, text in enumerate(DOCUMENTS)],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(document, indent=2) + "\n")
    print(json.dumps({"result": "pass", "vectors": len(values), "dimension": 1024, "gpu": document["generator"]["gpu"]}))


if __name__ == "__main__":
    main()
