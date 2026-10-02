# Embedding Runtime

The embedding runtime turns text into vectors for any trusted Veoveo component. It
serves one embedding model with vLLM on a hardware GPU, and a typed Rust client in
this directory gives every service the same request shape, query formatting, and
vector identity. The knowledge service is its first consumer. Agent memory, search,
deduplication, and similarity features can call it without new infrastructure.

## Status

The [shared contract](contract/DESIGN.md) implements typed inputs, embedding-space
identities and vector admission without runtime dependencies. The
[HTTP client](client/DESIGN.md) implements authenticated requests, shared request limits,
priorities, deadlines and response validation. The Helm component implements GPU
serving, checkpoint verification and namespace isolation. Local CUDA reference,
scheduling and refusal checks pass. Installed namespace isolation and authenticated
model access pass on the reference cluster. The controlled domain-corpus comparison
qualifies 0.6B, 4B and 8B through Knowledge retrieval and selects 0.6B. Composed
GPU-memory qualification with the selected concurrent workloads remains open in the
[implementation plan](../../../docs/PLATFORM_FOUNDATIONS_PLAN.md#phase-8-knowledge-service).
The reference installation qualifies Reason separately and keeps it stopped during
the composed flight check.

## Standards And Protocols

| Standard or protocol | Profile |
|---|---|
| [vLLM 0.30.0](https://github.com/vllm-project/vllm/releases/tag/v0.30.0) | The official `vllm/vllm-openai:v0.30.0` image, pinned by the same OCI digest as `reason-mcp`, run with the pooling runner; one vLLM pin serves both workloads |
| [OpenAI Embeddings API](https://platform.openai.com/docs/api-reference/embeddings), as implemented by vLLM | Internal protocol: `POST /v1/embeddings` and `GET /v1/models` over cluster-internal HTTP with a bearer API key; not a public contract |
| [`Qwen/Qwen3-Embedding-0.6B`](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B), revision `97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3` | Apache-2.0 embedding model: 1024-dimension output, 32,768-token context, last-token pooling, L2 normalization |
| NVIDIA CUDA | The runtime runs on a hardware GPU and fails at startup without one |
| Prometheus exposition format | vLLM's `/metrics` endpoint |

## Serving

The runtime is a Deployment of the official vLLM image with no Veoveo code in it:

```text
VLLM_API_KEY=<from Secret> HF_HUB_OFFLINE=1 \
vllm serve /models/qwen3-embedding-0.6b-97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3 --runner pooling \
  --served-model-name qwen3-embedding-0.6b --scheduling-policy priority \
  --gpu-memory-utilization <installation value>
```

`--runner pooling` is required because the checkpoint declares `Qwen3ForCausalLM`;
without it vLLM loads a generative model and mounts no `/v1/embeddings` route. vLLM
owns tokenization, batching, padding, pooling, and GPU scheduling. The pooler uses
last-token pooling with L2 normalization, which the checkpoint's sentence-transformers
configuration declares. The served-vector comparison against the CUDA Transformers
fixture confirms that vLLM applies this configuration without a pooler override.

The runtime shares GPUs with the installation's other workloads, so it claims only the
memory fraction the installation sets, as `reason.engine.gpuMemoryUtilization` does
for Reason. The installation's GPU budget counts its share.

The checkpoint follows the `reason-mcp` model-cache pattern. The installation stages
the Hugging Face layout on a model-cache volume the same way it stages Reason's
world-model checkpoint. An init container checks every file
against [its pinned SHA-256](checkpoint.sha256) with `sha256sum -c` before vLLM starts, and
`HF_HUB_OFFLINE=1` forbids downloads. The Pod uses the `nvidia` runtime class and
requests `nvidia.com/gpu`, so the runtime has no CPU path. `/health` backs readiness.

The runtime is a separate deployment because it is a GPU workload with its own image,
scaling, and failure behavior, shared by every consumer.
The chart's `embedding-runtime` component belongs to the full installation. Its
dedicated NetworkPolicy applies independently of the general policy switch.

## Access

Every workload in the installation's platform namespace may call the runtime, except
the Computers compute host. Its NetworkPolicy admits pods in that namespace whose
`app.kubernetes.io/component` is not `computer-host`, and nothing else. Agent kernels
run in the separate `veoveo-agents` namespace, so the policy excludes them as well.
Requests carry the installation's embedding API key, which Helm mounts from one Secret
into the platform workloads that use embeddings. The key guards against a misconfigured
network policy; it does not identify callers.

Computers run arbitrary code, and the compute host's own egress policy already keeps
them away from cluster services. Agent kernels, Computers, and external MCP hosts embed
through the `knowledge__embed` MCP tool, which the gateway authorizes and audits like
any other tool and an agent's episode budget counts.

## Client

`veoveo-embedding-client` in this directory is the typed Rust client every service
uses. Python services call the same endpoint with the OpenAI client and follow the
same rules.

- `embed_documents(texts)` sends plain text.
- `embed_query(task, text)` formats `Instruct: {task}\nQuery:{text}`, with no space
  after `Query:`, as the model card specifies. Callers never format instructions
  themselves.
- Requests are bounded in input count and total size. The client validates the vector
  count and dimension of every response.
- Each request declares interactive or bulk priority, mapped to vLLM values 0 and 10.
  Client clones share request permits and reserve capacity from bulk admission. Runtime
  qualification measures whether interactive work completes ahead of queued indexing.

Every response carries the shared contract's `EmbeddingSpace`: model name, checkpoint revision,
dimension, and the vLLM image digest. The client verifies the served model through
`/v1/models`; deployment configuration supplies the revision, dimension and image digest.
A consumer stores the space with its vectors and compares
vectors only within one space. A model change therefore creates a new space, and each
consumer rebuilds its vectors deliberately.

## Model Selection

The installation serves one model at a time. Qualification compares
`Qwen3-Embedding-0.6B`, `4B`, and `8B` through the same runtime by changing only the
checkpoint. Their outputs have 1024, 2560, and 4096 dimensions. The comparison records
retrieval recall at 10 on the knowledge evaluation set, GPU memory, and throughput in
inputs per second. The runtime ships 0.6B unless a larger model shows a retrieval gain
that justifies its memory on the installation's shared GPUs.

The [domain-corpus comparison](verification/retrieval-2026-10-02.md) retains 0.6B.
All three models achieve recall at ten of 1.000 on 78 fixed queries over 153 members.
The 0.6B runtime rebuilds 1,197 chunks at 98.34 chunks/second with concurrent search,
compared with 50.50 for 4B and 29.81 for 8B. Observed model-process memory peaks at
6,962, 14,456 and 20,964 MiB respectively with a common 4.625 GiB cache. The larger
checkpoints provide no recall gain on this corpus. Its constructed judgments and
single runs do not establish production-wide recall, quality equivalence or capacity
under the installation's simultaneous GPU workloads.

## Verification

The local RTX 4090 check uses the pinned official image with its PyTorch 2.13.0,
Transformers 5.17.0 and CUDA 13.0 packages. All four reference comparisons exceed
0.999 cosine similarity; the smallest is 0.9997335. The runtime selects FlashAttention
2, while the padded reference batch uses cuDNN attention. Both execute on CUDA.
The 25% memory fraction supports the full 32,768-token context on this 24 GiB device.
The full context requires 3.5 GiB of KV cache.

With six queued 32-input bulk requests, an interactive query completed in 81 ms,
before the first bulk batch completed at 126 ms. The 192-input fixture processed about
300 inputs/second. This checks the shipped combination of vLLM priority and shared
client admission. It does not isolate scheduler priority from the client's bulk cap,
or establish Knowledge retrieval throughput. The runtime rejects a corrupted checkpoint,
startup without CUDA and unauthenticated model discovery. Installed probes reach model
discovery from a platform pod with the API key and receive HTTP 401 without it.
The same image cannot connect from a `computer-host`-labelled pod or another namespace.
The reference CNI rejects these connections through its Pod firewall; a connection
timeout is not required. Memory coexistence under load still requires qualification.

The [verification guide](verification/README.md) owns regeneration and execution.

- On a hardware GPU the runtime becomes ready. It refuses readiness without a CUDA
  device or with a checkpoint whose digests differ from the pin.
- A reference test embeds a fixed set of queries and documents through the runtime and
  compares each vector with one produced by the model card's `transformers` recipe at
  the pinned revision. Each pair reaches cosine similarity of at least 0.999. The
  reference vectors are a committed fixture, generated once with `uv run`.
- Client tests cover instruction formatting, request bounds, response validation, and
  the reported embedding space.
- A load test records throughput and shows interactive requests completing ahead of a
  concurrent bulk load.
- A pod outside the platform namespace and the compute host cannot connect, and a
  request without the key is rejected.

## Implementation Map

| Path | Responsibility |
|---|---|
| `platform/runtimes/embedding/contract` | model/revision identities, checked dimensions, embedding-space equality and normalized vector admission shared below the client and Store |
| `platform/runtimes/embedding/client` | `veoveo-embedding-client`: typed requests, query formatting, priorities, bounds, validation, and `EmbeddingSpace` |
| `deploy/helm/veoveo` | runtime Deployment, model cache, init-container digest check, GPU request, NetworkPolicy, and API key Secret |
| `platform/runtimes/embedding/checkpoint.sha256` | pinned file digests for the checkpoint revision |
