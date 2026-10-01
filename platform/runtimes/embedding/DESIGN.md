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
priorities, deadlines and response validation. GPU serving and installed qualification
remain open. Phase 8 of the
[implementation plan](../../../docs/PLATFORM_FOUNDATIONS_PLAN.md#phase-8-knowledge-service)
delivers them before the knowledge service consumes them.

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
vllm serve /models/qwen3-embedding-0.6b --runner pooling \
  --served-model-name qwen3-embedding-0.6b --scheduling-policy priority \
  --gpu-memory-utilization <installation value>
```

`--runner pooling` is required because the checkpoint declares `Qwen3ForCausalLM`;
without it vLLM loads a generative model and mounts no `/v1/embeddings` route. vLLM
owns tokenization, batching, padding, pooling, and GPU scheduling. The pooler uses
last-token pooling with L2 normalization, which the checkpoint's sentence-transformers
configuration declares. Qualification confirms that vLLM applies it, and the
deployment sets the pooler configuration explicitly if it does not.

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

## Verification

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
