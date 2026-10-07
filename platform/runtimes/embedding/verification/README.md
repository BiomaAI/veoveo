# Embedding Runtime Qualification

The runtime design owns [acceptance](../DESIGN.md#verification). Native client and Helm
tests do not establish CUDA execution or installed network isolation.


## Initial Qualification

A first qualification starts with actual effective execution contents, rather than
a bundle. Record the immutable image/checkpoint manifest, vLLM version, observed
NVIDIA GPU/CUDA/driver, precision, token/sequence budgets, scheduling policy,
attention backend, configured graph allowance and runtime dispatch from a real batch.
Record an explicit KV-cache override as bytes, or explicit null when none is set.
These values come from the serving process and its configuration. An omitted
`--enforce-eager` flag alone cannot establish CUDA graph execution.
The chart pins vLLM 0.31.0 and starts the production Deployment without profiler
instrumentation. A temporary isolated local process may use vLLM's [maintained Proton
graph-attribution profiler](https://docs.vllm.ai/en/v0.31.0/contributing/profiling/#profile-with-triton-proton)
for this dispatch observation. vLLM 0.31's `/start_profile` and `/stop_profile`
handlers bypass API-key middleware. Run the temporary process in a no-network
container, bind vLLM to container loopback, and issue the profile controls and the
authenticated embedding request from inside that container. Never expose the profile
routes through a service, host port, or production Deployment.
The `--cudagraph-metrics` table does not establish pooling-request dispatch:
vLLM's pooling output constructors omit graph statistics before the logger. The same
omission exists in 0.31.0. Graph capture and startup warmup alone do not establish
execution. Before profiling, verify that the live process reports Proton graph
attribution enabled and uses Triton 3.7 or newer; otherwise stop this observation.

After readiness, start the provider profile, send one ordinary authenticated short
embedding request, and stop the profile:

```text
POST /start_profile
POST /v1/embeddings
POST /stop_profile
```

The startup option is:

```text
--profiler-config '{"profiler":"proton","proton_profiler_dir":"/tmp/vllm-proton","proton_data":"tree","proton_output_format":"hatchet","proton_hook":"triton","proton_graph_attribution":true}'
```

Inspect the resulting Hatchet tree for CUDA graph-attributed kernels in the request
profile phase. Proton adds `<captured_at>` between the call path that captured a
kernel and the call path that replayed it. The `graph_launch` and `graph` names shown
in Proton's CUDA-graph example are scopes chosen by that example; vLLM need not emit
those names. A qualifying trace therefore records its actual replay ancestry, the
automatic `<captured_at>` boundary, the captured kernel ancestry, and a kernel name
with positive `time/ns` in `proton-viewer -m time/ns`. The exact image's pinned
[Triton 3.7.1 Proton documentation](https://github.com/triton-lang/triton/blob/v3.7.1/third_party/proton/README.md#cuda-graph)
defines this path composition, and [vLLM 0.31.0's CUDA-graph replay code](https://github.com/vllm-project/vllm/blob/v0.31.0/vllm/compilation/cuda_graph.py#L359-L363)
shows the replay boundary in the served runtime.

The request profile phase separates this path from startup graph capture. A capture
phase alone, an ordinary kernel path, or generic positive CUDA time does not prove
graph dispatch. A missing path is inconclusive, not evidence of eager execution; a
zero-time, missing-boundary, or capture-only path does not qualify. Record the actual
scope labels and request phase. This qualifies only the observed request shape; it
neither counts low-level replay calls nor attests performance or vector quality.
The embedding request uses its private API key; the two profile controls are
unauthenticated and rely on container network isolation. The profile configuration is
fixed at process startup; each interval starts and stops without restarting the engine.
Do not profile capacity or vector-quality measurements because profiling adds
instrumentation. The production chart does not enable this launch option.

The exact serving image includes Triton and `proton-viewer` but omits the viewer's
analysis packages. Run the viewer in an isolated analysis environment with the exact
image's Triton package and the trace mounted read-only. Hatchet 2026.1.0 requires
`pandas<3`, so the current locked analysis environment uses pandas 2.3.3, the latest
compatible stable release, and NumPy 2.2.6 to match the image. PyPI's pandas 3.0.6 is
newer but does not satisfy Hatchet's declared constraint. This environment is only for
trace rendering and does not change the serving image or its runtime dependencies.
Embedding qualification owns this temporary constraint and will review it at the next
vLLM qualification, no later than 2026-10-21, by adopting upstream Hatchet support for
pandas 3 and rerunning the trace analysis.

The existing profile writer computes the content identity and refuses incomplete
or contradictory contents. It does not measure or attest the process:

```sh
cargo test -p veoveo-embedding-client --features verification --test gpu \
  candidate::write_measured_execution_profile -- --ignored --exact --nocapture
```

Set `VEOVEO_EMBEDDING_PROFILE_CONTENTS_FILE` to the measured contents JSON and
`VEOVEO_EMBEDDING_PROFILE_OUTPUT` to a new absolute output path. Then set
`VEOVEO_EMBEDDING_PROFILE_FILE` to that output, `VEOVEO_EMBEDDING_URL` to the
corresponding candidate origin and `VEOVEO_EMBEDDING_API_KEY_FILE` to its private
key file. The verification client uses the same transport and vector validators
as production. It does not implement the production embedding port and cannot
publish or activate a generation.

Set absolute `VEOVEO_RETRIEVAL_INPUT` and `VEOVEO_RETRIEVAL_OUTPUT` paths, and an
explicit installation acceptance `VEOVEO_EMBEDDING_MINIMUM_RECALL_AT_TEN` in
(0,1]. Capture the existing owner-typed corpus, query task and chunking settings:

```sh
cargo test -p veoveo-knowledge-mcp --test gpu_retrieval \
  candidate_measurement::write_candidate_domain_configuration -- --ignored --exact --nocapture
cargo test -p veoveo-knowledge-mcp --test gpu_retrieval \
  candidate_measurement::capture_candidate_domain_vectors -- --ignored --exact --nocapture
```

The Rust capture writer and Python collector enforce a 512 MiB raw-byte cap. The
collector admits the complete profile, nested values and member selections before
CUDA or model access. `captureDigest` covers the original bytes. The count and
dimension ceilings apply together with this aggregate cap.

Pure receiver controls use the current Rust-produced structural fixture:

```sh
python -m unittest test_capture
```

Run that command from this verification directory in the qualified Python runtime
with Pydantic 2.13. The synthetic fixture verifies receiving admission and never
establishes model or GPU qualification.

The capture includes original source and judgment fingerprints, member identities,
selected collections, chunk text and vectors. This target makes no Store connection.
Run `reference.py` in the pinned NVIDIA image as described below, adding
`--candidate-capture <capture>` and `--retrieval-report <new-report>`.
`--space-file` must contain the candidate profile's complete `contents.space`.
`--output` is the new vector-comparison report. The checkpoint manifest must match
the measured profile. The current reference supports the pinned 0.6B checkpoint.

The reference path executes model inference, cosine comparison and member ranking
on CUDA. It requires every captured vector to reach cosine 0.999. Ranking aggregates
chunks by member and applies the captured case collection selection. Candidate
recall must meet the declared threshold and cannot lose judged recall against the CUDA
reference. Its report scope is `candidate_vector_ranking`; it grants no SQL,
source-authorization or production Knowledge acceptance.

Each completed comparison emits one `candidate_reference_diagnostics` JSON log
before the acceptance decision. The log reports document and query counts, minimum
cosines and counts below 0.999. It selects at most eight vectors in ascending cosine
order on CUDA; equal cosines keep the capture's document-then-query order. Each
entry identifies its capture vector index, SHA-256 of the exact UTF-8 model input,
token length, padded width and inferred left-padding count from its actual tokenizer
batch. Documents name their
member index and collection; queries name their query ID. Query input digests include
the instruction prefix. The log includes reference Python, Torch, Transformers,
vLLM, CUDA and cuDNN versions, plus the captured candidate runtime versions.
It exports no corpus bodies, resource URIs, tokens or embedding components.
The two acceptance reports keep their existing formats and refuse occupied output
paths. A failed comparison preserves those reports and fails the command.

For a causal comparison of one or two admitted documents, select their zero-based
chunk indices with `--document-indices`. This explicit mode writes a separate
`veoveo.ai/embedding-batch-diagnostic/v1` document to a new `--output` path and
rejects `--retrieval-report`. It verifies the strict capture, target space and local
checkpoint before CUDA admission. The model loads once with the full reference's
admitted precision and cuDNN settings. Each selected document runs in its original group of 16
chunks, preserving source order and left padding, then as a singleton with the same
model-default positions. The final source group can contain fewer than 16 chunks.
The optional `--repeat-singleton` runs that singleton once more.

```sh
python3 /qualification/verification/reference.py \
  --checkpoint <local-checkpoint> --manifest <checkpoint-manifest> \
  --space-file <candidate-space.json> --candidate-capture <original-capture.json> \
  --document-indices 952 704 --repeat-singleton \
  --output <new-batch-diagnostic.json>
```

CUDA computes candidate-versus-original-batch, candidate-versus-singleton and
original-batch-versus-singleton cosines. A repeat adds the corresponding three
cosines for that run. The output identifies the selected indices, exact input text
digests and original batch start/count, with actual token lengths and padding widths
for each reference run. It includes the original capture and checkpoint digests,
vector space, execution profile and runtime versions. These bounded facts contain
no input text, resource URIs, tokens or embedding components. The mode evaluates no
ranking or full-corpus acceptance and emits no passing report or space-reuse claim.
The default reference still compares every captured vector and applies its existing
acceptance conditions.

Set `VEOVEO_EMBEDDING_MINIMUM_INPUTS_PER_SECOND` and
`VEOVEO_EMBEDDING_MAXIMUM_QUERY_LATENCY_MICROS` to positive installation acceptance
limits, plus new absolute `VEOVEO_EMBEDDING_SCHEDULING_REPORT` and
`VEOVEO_EMBEDDING_CAPACITY_REPORT` outputs. The existing scheduling fixture queues
six batches of 32 documents. The interactive query must finish before at least
four batches, and measured throughput and query latency must meet those limits:

```sh
cargo test -p veoveo-embedding-client --features verification --test gpu \
  candidate::measure_candidate_priority_and_capacity_on_cuda -- --ignored --exact --nocapture
```

Set `VEOVEO_EMBEDDING_REFERENCE_REPORT` and
`VEOVEO_EMBEDDING_RETRIEVAL_REPORT` to the generated CUDA reports, retaining the
scheduling/capacity report variables. Set `VEOVEO_EMBEDDING_RUNTIME_OUTPUT` to a
new absolute path. Report admission checks matching identities, report scope,
passed results, recalculated recall, and capacity's scheduling-report digest:

```sh
cargo test -p veoveo-embedding-client --features verification --test gpu \
  candidate::write_initial_runtime_from_hardware_reports -- --ignored --exact --nocapture
```

This produces a self-compatible candidate bundle containing the admitted original
report identities. Set `VEOVEO_EMBEDDING_RUNTIME_FILE` to it and run the existing
full production Knowledge workload below, including hybrid retrieval, rebuild and
concurrent searches. Its configuration must carry the same explicit minimum recall,
corpus, query task and chunker as the report-bound candidate capture. The writer and
production entrypoint check those fingerprints against the bundle's report bytes.
The full workload persists measured results and fails below its declared minimum
recall. Candidate vector compatibility and production SQL quality have separate gates. Preserve its corpus fingerprints and source/policy/native
checks. **Only a passing production workload permits installation selection.**
Neither a synthetic fixture nor historical measurements can qualify the new profile.
Existing installed inputs stay unchanged until real NVIDIA acceptance and the
required fresh-state/drain transition are complete.

## Reference Vectors

`reference.py` follows the pinned model card's last-token pooling and L2 normalization.
It runs Transformers on CUDA and admits two weight precisions. The loaded model's
dtype must match the target space before inference. The fixed reference, full candidate
comparison and selected-document diagnostic share this admission.

| `space.precision` | Loaded Torch dtype | Normalization dtype |
|---|---|---|
| `bfloat16` | `torch.bfloat16` | `torch.float32` |
| `float16` | `torch.float16` | `torch.float32` |

The reference rejects `float32`, `int8` and `int4` before CUDA or model loading.
The padded reference batch requires cuDNN fused attention because PyTorch Flash
Attention rejects its non-null padding mask. The script requires that backend and
records the matched weight precision, installed library versions and GPU. The runtime
itself selects its supported fused attention implementation. The
[official model card](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B#transformers-usage)
includes a CUDA FP16 example.

Weight precision forms part of vector-space identity. An FP16 evaluation needs a
distinct FP16 space, an actual FP16 serving profile and a fresh production capture
from that profile. Capture admission requires `serving.precision` to equal
`space.precision`; changing labels cannot qualify retained BF16 vectors. Each precision
requires its own full-corpus comparison and retrieval reports, scheduling and capacity
reports, and passing production Knowledge workload. Every report uses a new exclusive
output path. The all-vector cosine threshold stays 0.999 and the full qualification
gates apply independently to both precisions. Implemented reference admission and
pure mock controls establish no GPU or space-reuse qualification.

The collector imports `rfc3986==2.0.0` and `rfc3986-validator==0.1.1`
from the hash-pinned [verification requirements](requirements.txt). Preparation
stages their two pure-Python wheels in `EMBEDDING_COLLECTOR_DEPENDENCIES` and
verifies the pinned wheel hashes. The supported host test environment uses the
same inputs with its qualified Pydantic 2.13 installation. Native and the
installation owner prepare those dependencies before qualification.

The GPU invocation mounts the wheels read-only and imports them through
`PYTHONPATH`. This overlay adds URI admission without installing into the official
image or changing its CUDA, PyTorch, Transformers or vLLM packages. An absent or
wrong dependency input refuses qualification. No download occurs in the GPU
invocation.

The committed `reference.json` contains the actual current v2 four-input CUDA producer
output for the selected qualified space. Generate it in the official pinned vLLM image
with that image's Transformers and PyTorch packages. This keeps the qualification
dependency set equal to the runtime image. Stage the checkpoint's ten files and verify
`checkpoint.sha256` first. From the repository root:

```sh
embedding_image=vllm/vllm-openai@sha256:c1c9f6fd5c109ba7f0546a59f5b2f15fb87f64c77782e90a27b648b42a8e67c3
timeout 240s docker run --rm --gpus all --network none \
  --user "$(id -u):$(id -g)" --shm-size 1g \
  -e HF_HUB_OFFLINE=1 -e UV_CACHE_DIR=/tmp/uv \
  -e PYTHONPATH=/qualification-deps/rfc3986-2.0.0-py2.py3-none-any.whl:/qualification-deps/rfc3986_validator-0.1.1-py2.py3-none-any.whl \
  -v "$EMBEDDING_COLLECTOR_DEPENDENCIES:/qualification-deps:ro" \
  -v "$(command -v uv):/usr/local/bin/veoveo-uv:ro" \
  -v "$EMBEDDING_CHECKPOINT:/models/checkpoint:ro" \
  -v "$PWD/platform/runtimes/embedding:/qualification" \
  --entrypoint /usr/local/bin/veoveo-uv "$embedding_image" \
  run --offline --no-project --no-managed-python -- \
  python3 /qualification/verification/reference.py \
  --checkpoint /models/checkpoint --manifest /qualification/checkpoint.sha256 \
  --space-file /qualification/verification/space.measured.json \
  --output /qualification/verification/reference.generated.json
```

The script refuses an existing output. Supply `space.measured.json` from the selected
qualified space and its effective deployment precision. Verify the complete generated
current v2 output against that space, then promote its original bytes to `reference.json`
and use it as `VEOVEO_EMBEDDING_REFERENCE_FILE`. Keep historical experiment measurements
in their separate records. Promotion changes no format marker or retained vector labels.
Reference generation refuses a missing GPU or invalid checkpoint.
No package installation or model download occurs in the container.

## Served Vectors And Scheduling

Start the chart's pinned runtime with a verified checkpoint and installation key. Prove
CUDA access in its container before running these checks. Set `VEOVEO_EMBEDDING_URL` to
that runtime's HTTP origin and `VEOVEO_EMBEDDING_API_KEY_FILE` to a private file containing
the key without a trailing newline. Set `VEOVEO_EMBEDDING_RUNTIME_FILE` to the installation-admitted
immutable profile and qualification bundle. The bundle must identify actual effective
precision, checkpoint manifest, measured GPU/CUDA/driver and passing report digests.
These files are prerequisites; model discovery cannot attest them. Then run:

```sh
cargo test -p veoveo-embedding-client --test gpu -- --ignored --test-threads=1 --nocapture
```

The reference case checks all four vectors at cosine similarity of at least 0.999.
The scheduling case queues six bulk batches through one shared client and requires an
interactive query to finish ahead of at least four queued batches. It reports query
latency, bulk completion times and total input throughput as JSON. Each case has a
120-second deadline. These small fixtures qualify correctness and client scheduling;
Knowledge's recall and model-size comparison need its own representative evaluation set.

## Retrieval Model Comparison

The [Knowledge evaluation workload](../../../../servers/knowledge-mcp/evaluation/README.md)
builds a generation from owner-typed domain fixtures, rebuilds it under concurrent
search, and retains per-query relevance judgments and results. Generate one configuration from each measured runtime bundle and preserve both corpus
fingerprints across runs. The writer refuses a missing bundle and never invents model
precision or a passing qualification.

| Checkpoint | Revision | File verification |
|---|---|---|
| [Qwen3-Embedding-0.6B](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B) | `97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3` | [SHA-256 manifest](../checkpoint.sha256) |
| [Qwen3-Embedding-4B](https://huggingface.co/Qwen/Qwen3-Embedding-4B) | `5cf2132abc99cad020ac570b19d031efec650f2b` | [SHA-256 manifest](qwen3-embedding-4b.sha256) |
| [Qwen3-Embedding-8B](https://huggingface.co/Qwen/Qwen3-Embedding-8B) | `1d8ad4ca9b3dd8059ad90a75d4983776a23d44af` | [SHA-256 manifest](qwen3-embedding-8b.sha256) |

The manifests bind the runtime files to the pinned upstream Git/LFS objects. Stage
each checkpoint before timing; the containers use `HF_HUB_OFFLINE=1`. Run one model
at a time with the same official image, BF16 weights, pooling runner and priority
scheduler. Keep `--max-model-len 32768`, `--max-num-batched-tokens 8192` and
`--max-num-seqs 32` constant. The comparison uses
`--kv-cache-memory-bytes 4966055936` (4.625 GiB) for all three models and
`--gpu-memory-utilization 0.90`. The explicit cache allocation allows observed memory
to reflect the checkpoint cost rather than a different cache reservation. This is a
comparison setting; the deployed 0.6B runtime uses its installation memory fraction.
The allocation includes room for the cache block reserved beyond the larger models'
nominal 4.5 GiB requirement at the full context length.

Require CUDA at startup and record the selected attention backend. Sample device
memory and the runtime's compute-process memory once per second through the workload.
Device totals include the desktop and browser; report those separately from the model
process. Stop other GPU workloads, BuildKit and checkpoint downloads for the comparison.
This isolated run does not qualify concurrent simulation and reasoning workloads.

The [retrieval comparison](retrieval-2026-10-02.md) records the fixed corpus,
runtime settings, measured retrieval and indexing results, and model-selection limits.

## Refusal And Network Checks

Run the chart's init command against a fixture with one altered checkpoint file and
require failure before vLLM starts. Run its container startup command without a CUDA
device and require refusal. Preserve the accepted checkpoint while creating corruption
fixtures, and remove every owned test container after it stops.

An unauthenticated `/v1/models` request must return 401. In Kubernetes, check the
authenticated route from a platform pod, then try it from a `computer-host` pod and
a pod in a different namespace. Both denied connections must fail before the probe's
deadline, by rejection or timeout according to the installed CNI. Use the same Service
IP and port for allowed and denied probes, and require an allowed request to succeed
in the same run. Confirm that the denied pods ran the client and that the network
policy caused the failure; missing binaries, DNS failures and unavailable endpoints
do not qualify. Remove owned probe pods and namespaces afterward. Test the real CNI
policy; Docker networking and Helm rendering cannot qualify namespace isolation.
