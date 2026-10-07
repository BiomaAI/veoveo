"""Generate the CUDA reference fixture through the pinned runtime's Transformers.

Run with uv inside the qualified vLLM image, using its installed Python packages.
The checkpoint is local and all model loading is offline.
"""

import argparse
from dataclasses import asdict, dataclass
import hashlib
import importlib.metadata
import json
import math
import platform
from pathlib import Path
from typing import TYPE_CHECKING, Literal

from capture import Capture, Chunk, Query, Space, read_capture

import torch
import torch.nn.functional as functional
from torch.nn.attention import SDPBackend, sdpa_kernel
from transformers import AutoModel, AutoTokenizer

if TYPE_CHECKING:
    from transformers import PreTrainedModel, PreTrainedTokenizerBase

SupportedReferencePrecision = Literal["bfloat16", "float16"]

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
    parser.add_argument("--document-indices", type=int, nargs="+", help="diagnostic only: one or two captured document indices; --output receives diagnostic facts")
    parser.add_argument("--repeat-singleton", action="store_true", help="diagnostic only: repeat each selected singleton once")
    args = parser.parse_args()
    space = Space.model_validate_json(args.space_file.read_bytes())
    admit_reference_space(space)
    if args.document_indices is not None or args.repeat_singleton:
        diagnose_documents(args, space)
        return
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
        dtype=reference_dtype(space),
        attn_implementation="sdpa",
        local_files_only=True,
        trust_remote_code=False,
    ).eval().cuda()
    precision = require_model_dtype(model, space)
    texts = [f"Instruct: {TASK}\nQuery:{text}" for text in QUERIES] + DOCUMENTS
    batch = tokenizer(texts, padding=True, truncation=False, return_tensors="pt").to("cuda")
    if int(batch["attention_mask"].sum(dim=1).max()) > space.maxInputTokens:
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
        "space": space.model_dump(),
        "checkpointManifest": "sha256:" + hashlib.sha256(args.manifest.read_bytes()).hexdigest(),
        "generator": {"torch": torch.__version__, "transformers": importlib.metadata.version("transformers"), "cuda": torch.version.cuda, "gpu": torch.cuda.get_device_name(), "attention": "cudnn_attention", "dtype": precision},
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


@dataclass(frozen=True)
class ReferenceBatch:
    vectors: torch.Tensor
    token_lengths: torch.Tensor
    padding_widths: tuple[int, ...]


@dataclass(frozen=True)
class CosineDiagnostic:
    kind: Literal["document", "query"]
    count: int
    minimum: float
    belowThresholdCount: int


@dataclass(frozen=True)
class VectorDiagnostic:
    vectorIndex: int
    kind: Literal["document", "query"]
    kindIndex: int
    cosine: float
    inputTextSha256: str
    tokenLength: int
    paddingWidth: int
    paddingCount: int
    memberIndex: int | None
    collection: str | None
    queryId: str | None


@dataclass(frozen=True)
class RuntimeDiagnostic:
    python: str
    torch: str
    transformers: str
    vllm: str | None
    cuda: str | None
    cudnn: int | None
    gpu: str
    candidateRuntimeImage: str
    candidateVllm: str
    candidateDriver: str
    candidateCuda: str


@dataclass(frozen=True)
class ReferenceDiagnostic:
    event: Literal["candidate_reference_diagnostics"]
    captureDigest: str
    cosineThreshold: float
    byKind: tuple[CosineDiagnostic, ...]
    worstVectors: tuple[VectorDiagnostic, ...]
    runtime: RuntimeDiagnostic


def vector_diagnostics(
    capture: Capture, indices: list[int], cosines: list[float],
    token_lengths: list[int], padding_widths: list[int],
) -> tuple[VectorDiagnostic, ...]:
    """Project at most eight admitted identities and input digests, never bodies."""
    if not (len(indices) <= 8 and len(indices) == len(cosines)
            == len(token_lengths) == len(padding_widths)):
        raise RuntimeError("invalid bounded vector diagnostics")
    result = []
    for index, cosine, length, width in zip(indices, cosines, token_lengths, padding_widths):
        if (type(index) is not int or not 0 <= index < len(capture.chunks) + len(capture.queries)
            or not math.isfinite(cosine) or type(length) is not int or type(width) is not int
            or not 0 < length <= width):
            raise RuntimeError("invalid admitted vector diagnostic metadata")
        if index < len(capture.chunks):
            row = capture.chunks[index]
            text = row.text
            kind, kind_index = "document", index
            member, collection = row.member, capture.members[row.member].member.collection
            query_id = None
        else:
            kind_index = index - len(capture.chunks)
            query = capture.queries[kind_index]
            text = f"Instruct: {capture.queryTask}\nQuery:{query.text}"
            kind, member, collection, query_id = "query", None, None, query.id
        result.append(VectorDiagnostic(
            index, kind, kind_index, cosine,
            "sha256:" + hashlib.sha256(text.encode()).hexdigest(), length, width, width - length,
            member, collection, query_id,
        ))
    return tuple(result)


def runtime_diagnostic(capture: Capture) -> RuntimeDiagnostic:
    try:
        vllm = importlib.metadata.version("vllm")
    except importlib.metadata.PackageNotFoundError:
        vllm = None
    contents = capture.profile.contents
    return RuntimeDiagnostic(
        platform.python_version(), str(torch.__version__),
        importlib.metadata.version("transformers"), vllm, torch.version.cuda,
        torch.backends.cudnn.version(), torch.cuda.get_device_name(),
        contents.runtimeImage, contents.vllmVersion,
        contents.environment.driver, contents.environment.cuda,
    )


def reference_precision(space: Space) -> SupportedReferencePrecision:
    if space.precision == "bfloat16":
        return "bfloat16"
    if space.precision == "float16":
        return "float16"
    raise RuntimeError("unsupported reference precision; use bfloat16 or float16")


def reference_dtype(space: Space) -> torch.dtype:
    precision = reference_precision(space)
    return torch.bfloat16 if precision == "bfloat16" else torch.float16


def require_model_dtype(model: "PreTrainedModel", space: Space) -> SupportedReferencePrecision:
    if model.dtype != reference_dtype(space):
        raise RuntimeError("loaded reference model dtype differs from admitted precision")
    return reference_precision(space)


def admit_reference_space(space: Space) -> None:
    if (space.model != "qwen3-embedding-0.6b" or space.revision != REVISION
        or space.dimension != 1024 or space.pooling != "last_token"):
        raise RuntimeError("unsupported reference target space")
    reference_precision(space)


def candidate_inputs(args: argparse.Namespace, space: Space) -> tuple[bytes, Capture, str]:
    raw, capture = read_capture(args.candidate_capture)
    admit_reference_space(space)
    profile = capture.profile
    contents = profile.contents
    manifest_digest = "sha256:" + hashlib.sha256(args.manifest.read_bytes()).hexdigest()
    if contents.space != space or contents.checkpointManifest != manifest_digest:
        raise RuntimeError("candidate space/checkpoint does not match reference inputs")
    return raw, capture, manifest_digest


def verify_candidate_checkpoint(args: argparse.Namespace) -> None:
    for line in args.manifest.read_text().splitlines():
        expected, name = line.split("  ", 1)
        with (args.checkpoint / name).open("rb") as source:
            if hashlib.file_digest(source, "sha256").hexdigest() != expected:
                raise RuntimeError("checkpoint digest mismatch")

def candidate_model(args: argparse.Namespace, space: Space) -> tuple["PreTrainedTokenizerBase", "PreTrainedModel", SupportedReferencePrecision]:
    dtype = reference_dtype(space)
    tokenizer = AutoTokenizer.from_pretrained(args.checkpoint, padding_side="left", local_files_only=True, trust_remote_code=False)
    model = AutoModel.from_pretrained(args.checkpoint, dtype=dtype, attn_implementation="sdpa", local_files_only=True, trust_remote_code=False).eval().cuda()
    precision = require_model_dtype(model, space)
    return tokenizer, model, precision


def reference_batch(
    texts: list[str], tokenizer: "PreTrainedTokenizerBase", model: "PreTrainedModel", space: Space,
) -> ReferenceBatch:
    batch = tokenizer(texts, padding=True, truncation=False, return_tensors="pt").to("cuda")
    lengths = batch["attention_mask"].sum(dim=1)
    if int(lengths.max()) > space.maxInputTokens:
        raise RuntimeError("candidate reference exceeds token ceiling")
    with torch.inference_mode(), sdpa_kernel(SDPBackend.CUDNN_ATTENTION):
        hidden = model(**batch).last_hidden_state
        if not hidden.is_cuda or not bool(torch.all(batch["attention_mask"][:, -1])):
            raise RuntimeError("reference did not execute CUDA last-token pooling")
        vectors = functional.normalize(hidden[:, -1].float(), p=2, dim=1)
    return ReferenceBatch(vectors, lengths, (batch["input_ids"].shape[1],) * lengths.shape[0])


def measured_vectors(rows: list[Chunk] | list[Query], space: Space) -> torch.Tensor:
    values = torch.tensor([row.values for row in rows], device="cuda", dtype=torch.float32)
    if values.shape != (len(rows), space.dimension) or not bool(torch.isfinite(values).all()):
        raise RuntimeError("invalid measured vector values")
    if not bool((torch.abs((values * values).sum(dim=1) - 1) <= .002).all()):
        raise RuntimeError("measured vectors are not normalized")
    return values


@dataclass(frozen=True)
class TokenShape:
    tokenLength: int
    paddingWidth: int
    paddingCount: int


@dataclass(frozen=True)
class BatchCosines:
    candidateVsOriginalBatch: float
    candidateVsSingleton: float
    originalBatchVsSingleton: float


@dataclass(frozen=True)
class RepeatCosines:
    candidateVsRepeat: float
    originalBatchVsRepeat: float
    singletonVsRepeat: float


@dataclass(frozen=True)
class DocumentBatchDiagnostic:
    vectorIndex: int
    inputTextSha256: str
    memberIndex: int
    collection: str
    sourceBatchStart: int
    sourceBatchCount: int
    originalBatch: TokenShape
    singleton: TokenShape
    cosines: BatchCosines
    repeatSingleton: TokenShape | None
    repeatCosines: RepeatCosines | None


@dataclass(frozen=True)
class BatchDiagnostic:
    format: Literal["veoveo.ai/embedding-batch-diagnostic/v1"]
    scope: Literal["selected_document_batch_comparison"]
    captureDigest: str
    profileId: str
    checkpointManifest: str
    space: Space
    referencePrecision: SupportedReferencePrecision
    attentionBackend: Literal["cudnn_attention"]
    positions: Literal["model_default"]
    runtime: RuntimeDiagnostic
    documents: tuple[DocumentBatchDiagnostic, ...]


def selected_document_indices(capture: Capture, indices: list[int] | None) -> tuple[int, ...]:
    if (indices is None or not 1 <= len(indices) <= 2
        or any(type(index) is not int or not 0 <= index < len(capture.chunks) for index in indices)
        or len(set(indices)) != len(indices)):
        raise RuntimeError("diagnostic requires one or two distinct admitted document indices")
    return tuple(indices)


def original_document_batch(capture: Capture, index: int) -> tuple[int, tuple[Chunk, ...]]:
    selected_document_indices(capture, [index])
    start = index // 16 * 16
    return start, tuple(capture.chunks[start:start + 16])


def token_shape(batch: ReferenceBatch, index: int) -> TokenShape:
    length, width = int(batch.token_lengths[index]), batch.padding_widths[index]
    if not 0 < length <= width:
        raise RuntimeError("invalid diagnostic tokenizer shape")
    return TokenShape(length, width, width - length)


def cuda_cosine(left: torch.Tensor, right: torch.Tensor) -> float:
    if not left.is_cuda or not right.is_cuda:
        raise RuntimeError("diagnostic comparison requires CUDA vectors")
    cosine = float(functional.cosine_similarity(left, right)[0])
    if not math.isfinite(cosine):
        raise RuntimeError("invalid diagnostic cosine")
    return cosine


def batch_diagnostic_document(record: BatchDiagnostic) -> dict:
    """Serialize only typed diagnostic facts; acceptance formats stay separate."""
    if (not 1 <= len(record.documents) <= 2
        or len({row.vectorIndex for row in record.documents}) != len(record.documents)):
        raise RuntimeError("invalid bounded batch diagnostic records")
    if record.referencePrecision != reference_precision(record.space):
        raise RuntimeError("batch diagnostic precision differs from admitted space")
    document = asdict(record)
    document["space"] = record.space.model_dump()
    return document


def diagnose_documents(args: argparse.Namespace, space: Space) -> None:
    admit_reference_space(space)
    if not args.candidate_capture or args.retrieval_report:
        raise RuntimeError("batch diagnostic requires candidate capture and no retrieval report")
    raw, capture, manifest_digest = candidate_inputs(args, space)
    indices = selected_document_indices(capture, args.document_indices)
    if args.output.exists():
        raise RuntimeError("diagnostic output already exists")
    # Verify every pinned checkpoint byte before any CUDA admission/model call.
    verify_candidate_checkpoint(args)
    if not torch.cuda.is_available():
        raise RuntimeError("batch diagnostic requires an NVIDIA CUDA device")
    tokenizer, model, precision = candidate_model(args, space)
    originals: dict[int, ReferenceBatch] = {}
    documents: list[DocumentBatchDiagnostic] = []
    for index in indices:
        start, rows = original_document_batch(capture, index)
        if start not in originals:
            originals[start] = reference_batch([row.text for row in rows], tokenizer, model, space)
        original = originals[start]
        position = index - start
        chunk = capture.chunks[index]
        candidate = measured_vectors([chunk], space)
        singleton = reference_batch([chunk.text], tokenizer, model, space)
        original_vector = original.vectors[position:position + 1]
        cosines = BatchCosines(cuda_cosine(candidate, original_vector),
            cuda_cosine(candidate, singleton.vectors), cuda_cosine(original_vector, singleton.vectors))
        repeat_shape, repeat_cosines = None, None
        if args.repeat_singleton:
            repeat = reference_batch([chunk.text], tokenizer, model, space)
            repeat_shape = token_shape(repeat, 0)
            repeat_cosines = RepeatCosines(cuda_cosine(candidate, repeat.vectors),
                cuda_cosine(original_vector, repeat.vectors), cuda_cosine(singleton.vectors, repeat.vectors))
        documents.append(DocumentBatchDiagnostic(index,
            "sha256:" + hashlib.sha256(chunk.text.encode()).hexdigest(), chunk.member,
            capture.members[chunk.member].member.collection, start, len(rows),
            token_shape(original, position), token_shape(singleton, 0), cosines,
            repeat_shape, repeat_cosines))
    record = BatchDiagnostic("veoveo.ai/embedding-batch-diagnostic/v1",
        "selected_document_batch_comparison", "sha256:" + hashlib.sha256(raw).hexdigest(),
        capture.profile.id, manifest_digest, space, precision, "cudnn_attention",
        "model_default", runtime_diagnostic(capture), tuple(documents))
    document = batch_diagnostic_document(record)
    write_new(args.output, document)
    print(json.dumps(document, allow_nan=False), flush=True)


def measure_candidate(args: argparse.Namespace, space: Space) -> None:
    """CUDA tensors own comparison/ranking; Rust owns corpus, chunk and case admission."""
    raw, capture, manifest_digest = candidate_inputs(args, space)
    profile = capture.profile
    contents = profile.contents
    if not args.retrieval_report:
        raise RuntimeError("candidate comparison requires retrieval report output")
    if args.output.exists() or args.retrieval_report.exists():
        raise RuntimeError("measurement reports already exist")
    if not torch.cuda.is_available():
        raise RuntimeError("candidate comparison requires an NVIDIA CUDA device")
    verify_candidate_checkpoint(args)
    chunks, queries = capture.chunks, capture.queries
    member_count = len(capture.members)
    if not 1 <= len(chunks) <= 262144 or not 1 <= len(queries) <= 1024 or not 1 <= member_count <= 65536:
        raise RuntimeError("invalid measurement input counts")
    threshold = capture.minimumRecallAtTen
    if isinstance(threshold, bool) or not isinstance(threshold, (int, float)) or not math.isfinite(threshold) or not 0 < threshold <= 1:
        raise RuntimeError("invalid declared recall threshold")
    tokenizer, model, precision = candidate_model(args, space)

    def reference(texts: list[str]) -> ReferenceBatch:
        result = []
        token_lengths = []
        padding_widths = []
        for start in range(0, len(texts), 16):
            batch = reference_batch(texts[start:start + 16], tokenizer, model, space)
            result.append(batch.vectors)
            token_lengths.append(batch.token_lengths)
            padding_widths.extend(batch.padding_widths)
        return ReferenceBatch(torch.cat(result), torch.cat(token_lengths), tuple(padding_widths))

    def measured(rows: list[Chunk] | list[Query]) -> torch.Tensor:
        return measured_vectors(rows, space)

    actual_docs, actual_queries = measured(chunks), measured(queries)
    reference_docs = reference([chunk.text for chunk in chunks])
    reference_queries = reference([f"Instruct: {capture.queryTask}\nQuery:{query.text}" for query in queries])
    document_cosines = functional.cosine_similarity(actual_docs, reference_docs.vectors)
    query_cosines = functional.cosine_similarity(actual_queries, reference_queries.vectors)
    minimum = min(float(document_cosines.min()), float(query_cosines.min()))
    minimum_millionths = min(1000000, math.floor(minimum * 1000000))
    indexes = [chunk.member for chunk in chunks]
    if any(type(index) is not int or not 0 <= index < member_count for index in indexes):
        raise RuntimeError("invalid admitted member indexes")
    member_index = torch.tensor(indexes, device="cuda", dtype=torch.int64)
    actual_scores = actual_queries @ actual_docs.T
    reference_scores = reference_queries.vectors @ reference_docs.vectors.T
    case_results = []
    for index, query in enumerate(queries):
        selected, relevant = query.selectedMembers, query.relevantMembers
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
        case_results.append({"id": query.id, "candidateHits": len(set(candidate_top) & set(relevant)), "referenceHits": len(set(reference_top) & set(relevant)), "relevant": len(relevant)})
    candidate_recall = sum(case["candidateHits"] / case["relevant"] for case in case_results) / len(case_results)
    reference_recall = sum(case["referenceHits"] / case["relevant"] for case in case_results) / len(case_results)
    retrieval_passed = candidate_recall >= threshold and candidate_recall >= reference_recall
    shared = {"profileId": profile.id, "space": space.model_dump(), "captureDigest": "sha256:" + hashlib.sha256(raw).hexdigest(), "datasetRevision": capture.datasetRevision, "sourceCorpusRevision": capture.sourceCorpusRevision, "queryTaskRevision": capture.queryTaskRevision, "chunkingRevision": capture.chunkingRevision, "checkpointManifest": manifest_digest}
    write_new(args.output, {"format": "veoveo.ai/embedding-candidate-reference/v1", "measurement": shared, "minimumCosineMillionths": minimum_millionths, "passed": minimum_millionths >= 999000, "vectors": len(chunks) + len(queries), "cudaDevice": torch.cuda.get_device_name(), "referencePrecision": precision, "attentionBackend": "cudnn_attention"})
    write_new(args.retrieval_report, {"format": "veoveo.ai/embedding-candidate-retrieval/v1", "measurement": shared, "candidateRecallAtTen": candidate_recall, "referenceRecallAtTen": reference_recall, "minimumRecallAtTen": threshold, "maximumReferenceRecallLoss": 0.0, "passed": retrieval_passed, "cases": case_results, "scope": "candidate_vector_ranking"})
    # Reduce and select on CUDA. Only bounded diagnostic metadata crosses to host;
    # embedding components, token IDs and corpus bodies never enter this log.
    summaries = tuple(CosineDiagnostic(kind, len(rows), float(cosines.min()),
        int((cosines.to(torch.float64) < .999).sum()))
        for kind, rows, cosines in (("document", chunks, document_cosines),
                                   ("query", queries, query_cosines)))
    cosines = torch.cat((document_cosines, query_cosines))
    worst = torch.argsort(cosines, stable=True)[:8]
    lengths = torch.cat((reference_docs.token_lengths, reference_queries.token_lengths))
    widths = reference_docs.padding_widths + reference_queries.padding_widths
    worst_indices = worst.cpu().tolist()
    diagnostics = vector_diagnostics(capture, worst_indices, cosines[worst].cpu().tolist(),
        lengths[worst].cpu().tolist(), [widths[index] for index in worst_indices])
    diagnostic = ReferenceDiagnostic("candidate_reference_diagnostics",
        shared["captureDigest"], .999, summaries, diagnostics, runtime_diagnostic(capture))
    print(json.dumps(asdict(diagnostic), allow_nan=False), flush=True)
    if minimum_millionths < 999000 or not retrieval_passed:
        raise RuntimeError("candidate reference/retrieval acceptance failed; reports retained")
    print(json.dumps({"result": "pass", "scope": "candidate_vector_ranking", "minimumCosineMillionths": minimum_millionths, "recallAtTen": candidate_recall}))


if __name__ == "__main__":
    main()
