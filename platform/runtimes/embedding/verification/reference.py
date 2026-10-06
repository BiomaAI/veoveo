"""Generate the CUDA reference fixture through the pinned runtime's Transformers.

Run with uv inside the qualified vLLM image, using its installed Python packages.
The checkpoint is local and all model loading is offline.
"""

import argparse
import hashlib
import importlib.metadata
import json
import math
from pathlib import Path

import torch
import torch.nn.functional as functional
from torch.nn.attention import SDPBackend, sdpa_kernel
from transformers import AutoModel, AutoTokenizer

REVISION = "97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3"
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
    parser.add_argument("--space-file", type=Path, required=True, help="explicit target vector space from the measured runtime configuration")
    parser.add_argument("--candidate-capture", type=Path, help="typed Knowledge candidate capture; CUDA comparison and ranking only")
    parser.add_argument("--retrieval-report", type=Path, help="new runtime-only retrieval report output")
    args = parser.parse_args()
    space = json.loads(args.space_file.read_text())
    if set(space) != {"model", "revision", "dimension", "pooling", "normalization", "precision", "maxInputTokens"}:
        raise RuntimeError("space requires the complete current identity profile")
    if (space["model"] != "qwen3-embedding-0.6b" or space["revision"] != REVISION
        or space["dimension"] != 1024 or space["pooling"] != "last_token"
        or space["normalization"] != "l2" or space["precision"] not in {"float32", "float16", "bfloat16", "int8", "int4"}
        or type(space["maxInputTokens"]) is not int or not 1 <= space["maxInputTokens"] <= 131072):
        raise RuntimeError("unsupported reference target space")
    if args.candidate_capture:
        measure_candidate(args, space)
        return
    if args.retrieval_report:
        raise RuntimeError("retrieval report requires candidate capture")
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
    if int(batch["attention_mask"].sum(dim=1).max()) > space["maxInputTokens"]:
        raise RuntimeError("reference input exceeds the target token ceiling")
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
        "schema": "veoveo.ai/embedding-reference/v2",
        "space": space,
        "checkpointManifest": "sha256:" + hashlib.sha256(args.manifest.read_bytes()).hexdigest(),
        "generator": {"torch": torch.__version__, "transformers": importlib.metadata.version("transformers"), "cuda": torch.version.cuda, "gpu": torch.cuda.get_device_name(), "attention": "cudnn_attention", "dtype": "bfloat16"},
        "task": TASK,
        "queries": [{"text": text, "values": values[index]} for index, text in enumerate(QUERIES)],
        "documents": [{"text": text, "values": values[len(QUERIES) + index]} for index, text in enumerate(DOCUMENTS)],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(document, indent=2) + "\n")
    print(json.dumps({"result": "pass", "vectors": len(values), "dimension": 1024, "gpu": document["generator"]["gpu"]}))


def write_new(path: Path, document: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as output:
        output.write(json.dumps(document, indent=2, allow_nan=False) + "\n")


def measure_candidate(args: argparse.Namespace, space: dict) -> None:
    """CUDA tensors own comparison/ranking; Rust owns corpus, chunk and case admission."""
    if not torch.cuda.is_available() or not args.retrieval_report:
        raise RuntimeError("candidate comparison requires CUDA and retrieval report output")
    if args.output.exists() or args.retrieval_report.exists():
        raise RuntimeError("measurement reports already exist")
    raw = args.candidate_capture.read_bytes()
    capture = json.loads(raw)
    if capture["format"] != "veoveo.ai/embedding-candidate-capture/v1":
        raise RuntimeError("unsupported candidate capture")
    profile = capture["profile"]
    contents = profile["contents"]
    manifest_digest = "sha256:" + hashlib.sha256(args.manifest.read_bytes()).hexdigest()
    if contents["space"] != space or contents["checkpointManifest"] != manifest_digest:
        raise RuntimeError("candidate space/checkpoint does not match reference inputs")
    for line in args.manifest.read_text().splitlines():
        expected, name = line.split("  ", 1)
        with (args.checkpoint / name).open("rb") as source:
            if hashlib.file_digest(source, "sha256").hexdigest() != expected:
                raise RuntimeError("checkpoint digest mismatch")
    chunks, queries = capture["chunks"], capture["queries"]
    member_count = len(capture["members"])
    if not 1 <= len(chunks) <= 262144 or not 1 <= len(queries) <= 1024 or not 1 <= member_count <= 65536:
        raise RuntimeError("invalid measurement input counts")
    threshold = capture["minimumRecallAtTen"]
    if isinstance(threshold, bool) or not isinstance(threshold, (int, float)) or not math.isfinite(threshold) or not 0 < threshold <= 1:
        raise RuntimeError("invalid declared recall threshold")
    tokenizer = AutoTokenizer.from_pretrained(args.checkpoint, padding_side="left", local_files_only=True, trust_remote_code=False)
    model = AutoModel.from_pretrained(args.checkpoint, dtype=torch.bfloat16, attn_implementation="sdpa", local_files_only=True, trust_remote_code=False).eval().cuda()

    def reference(texts: list[str]) -> torch.Tensor:
        result = []
        for start in range(0, len(texts), 16):
            batch = tokenizer(texts[start:start + 16], padding=True, truncation=False, return_tensors="pt").to("cuda")
            if int(batch["attention_mask"].sum(dim=1).max()) > space["maxInputTokens"]:
                raise RuntimeError("candidate reference exceeds token ceiling")
            with torch.inference_mode(), sdpa_kernel(SDPBackend.CUDNN_ATTENTION):
                hidden = model(**batch).last_hidden_state
                if not hidden.is_cuda or not bool(torch.all(batch["attention_mask"][:, -1])):
                    raise RuntimeError("reference did not execute CUDA last-token pooling")
                result.append(functional.normalize(hidden[:, -1].float(), p=2, dim=1))
        return torch.cat(result)

    def measured(rows: list[dict]) -> torch.Tensor:
        values = torch.tensor([row["values"] for row in rows], device="cuda", dtype=torch.float32)
        if values.shape != (len(rows), space["dimension"]) or not bool(torch.isfinite(values).all()):
            raise RuntimeError("invalid measured vector values")
        if not bool((torch.abs((values * values).sum(dim=1) - 1) <= .002).all()):
            raise RuntimeError("measured vectors are not normalized")
        return values

    actual_docs, actual_queries = measured(chunks), measured(queries)
    reference_docs = reference([chunk["text"] for chunk in chunks])
    reference_queries = reference([f"Instruct: {capture['queryTask']}\nQuery:{query['text']}" for query in queries])
    minimum = min(float(functional.cosine_similarity(actual_docs, reference_docs).min()), float(functional.cosine_similarity(actual_queries, reference_queries).min()))
    minimum_millionths = min(1000000, math.floor(minimum * 1000000))
    indexes = [chunk["member"] for chunk in chunks]
    if any(type(index) is not int or not 0 <= index < member_count for index in indexes):
        raise RuntimeError("invalid admitted member indexes")
    member_index = torch.tensor(indexes, device="cuda", dtype=torch.int64)
    actual_scores = actual_queries @ actual_docs.T
    reference_scores = reference_queries @ reference_docs.T
    case_results = []
    for index, query in enumerate(queries):
        selected, relevant = query["selectedMembers"], query["relevantMembers"]
        if not selected or not relevant or any(type(v) is not int or not 0 <= v < member_count for v in selected + relevant) or not set(relevant) <= set(selected):
            raise RuntimeError("invalid admitted case indexes")
        selected_tensor = torch.tensor(selected, device="cuda", dtype=torch.int64)
        def rank(scores: torch.Tensor) -> list[int]:
            members = torch.full((member_count,), -torch.inf, device="cuda")
            members.scatter_reduce_(0, member_index, scores, reduce="amax", include_self=True)
            order = torch.argsort(members[selected_tensor], descending=True, stable=True)[:10]
            # Transfer only the final member indexes for JSON report/judgment output.
            return selected_tensor[order].cpu().tolist()
        candidate_top, reference_top = rank(actual_scores[index]), rank(reference_scores[index])
        case_results.append({"id": query["id"], "candidateHits": len(set(candidate_top) & set(relevant)), "referenceHits": len(set(reference_top) & set(relevant)), "relevant": len(relevant)})
    candidate_recall = sum(case["candidateHits"] / case["relevant"] for case in case_results) / len(case_results)
    reference_recall = sum(case["referenceHits"] / case["relevant"] for case in case_results) / len(case_results)
    retrieval_passed = candidate_recall >= threshold and candidate_recall >= reference_recall
    shared = {"profileId": profile["id"], "space": space, "captureDigest": "sha256:" + hashlib.sha256(raw).hexdigest(), "datasetRevision": capture["datasetRevision"], "sourceCorpusRevision": capture["sourceCorpusRevision"], "queryTaskRevision": capture["queryTaskRevision"], "chunkingRevision": capture["chunkingRevision"], "checkpointManifest": manifest_digest}
    write_new(args.output, {"format": "veoveo.ai/embedding-candidate-reference/v1", "measurement": shared, "minimumCosineMillionths": minimum_millionths, "passed": minimum_millionths >= 999000, "vectors": len(chunks) + len(queries), "cudaDevice": torch.cuda.get_device_name(), "referencePrecision": "bfloat16", "attentionBackend": "cudnn_attention"})
    write_new(args.retrieval_report, {"format": "veoveo.ai/embedding-candidate-retrieval/v1", "measurement": shared, "candidateRecallAtTen": candidate_recall, "referenceRecallAtTen": reference_recall, "minimumRecallAtTen": threshold, "maximumReferenceRecallLoss": 0.0, "passed": retrieval_passed, "cases": case_results, "scope": "candidate_vector_ranking"})
    if minimum_millionths < 999000 or not retrieval_passed:
        raise RuntimeError("candidate reference/retrieval acceptance failed; reports retained")
    print(json.dumps({"result": "pass", "scope": "candidate_vector_ranking", "minimumCosineMillionths": minimum_millionths, "recallAtTen": candidate_recall}))


if __name__ == "__main__":
    main()
