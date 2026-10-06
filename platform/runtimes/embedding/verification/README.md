# Embedding Runtime Qualification

The runtime design owns [acceptance](../DESIGN.md#verification). Native client and Helm
tests do not establish CUDA execution or installed network isolation.


## Initial Qualification

A first qualification starts with actual effective execution contents, rather than
a bundle. Record the immutable image/checkpoint manifest, vLLM version, observed
NVIDIA GPU/CUDA/driver, precision, token/sequence budgets, scheduling policy,
attention backend, configured graph allowance and observed graph execution.
Record an explicit KV-cache override as bytes, or explicit null when none is set.
These values come from the serving process and its configuration. An omitted
`--enforce-eager` flag alone cannot establish CUDA graph execution.
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
It runs Transformers on CUDA with bfloat16 weights. The padded reference batch requires
cuDNN fused attention because PyTorch Flash Attention rejects its non-null padding
mask. The script requires that backend and records the installed library versions and
GPU. The runtime itself selects its supported fused attention implementation.

The committed `reference.json` is generated in the official pinned vLLM image, using
that image's Transformers and PyTorch packages. This keeps the qualification dependency
set equal to the runtime image. Stage the checkpoint's ten files and verify
`checkpoint.sha256` first. From the repository root:

```sh
embedding_image=vllm/vllm-openai@sha256:8a69ffad015f138d7170c4ddc429e230a3bc1c1719f67e14324749df200a4b90
timeout 240s docker run --rm --gpus all --network none \
  --user "$(id -u):$(id -g)" --shm-size 1g \
  -e HOME=/tmp -e HF_HUB_OFFLINE=1 -e UV_CACHE_DIR=/tmp/uv \
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

The script refuses an existing output. Supply `space.measured.json` from the explicit
target deployment settings, including effective precision. Preserve historical
`reference.json`; use the generated current-format file as `VEOVEO_EMBEDDING_REFERENCE_FILE`. Reference generation refuses a missing GPU or invalid checkpoint.
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
