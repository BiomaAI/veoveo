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
serving, checkpoint verification and namespace isolation. The current image pin is
vLLM 0.31.0. The CUDA FP16 profile on an RTX 4090 passed vector comparison and
ranking. Scheduling, capacity and the full Knowledge rebuild/readback also passed
with the embedding serving process limited to 4 CPUs and 12 GiB of memory.
Installed profile selection, namespace isolation, API-key isolation and the
co-resident GPU budget still require qualification. Generic BF16 and 8 GiB profiles
are unqualified. The reference installation keeps Knowledge and Embedding enabled
as core services and budgets their
GPU allocation together. Reason runs in a separate acceptance batch.

## Standards And Protocols

| Standard or protocol | Profile |
|---|---|
| [vLLM 0.31.0](https://github.com/vllm-project/vllm/releases/tag/v0.31.0) | The official `vllm/vllm-openai:v0.31.0` image, pinned by the same OCI index digest as `reason-mcp`, run with the pooling runner; one vLLM pin serves both workloads |
| [OpenAI Embeddings API](https://platform.openai.com/docs/api-reference/embeddings), as implemented by vLLM | Internal protocol: `POST /v1/embeddings` and `GET /v1/models` over cluster-internal HTTP with a bearer API key; not a public contract |
| [`Qwen/Qwen3-Embedding-0.6B`](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B), revision `97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3` | Apache-2.0 embedding model: 1024-dimension output, 32,768-token context, last-token pooling, L2 normalization |
| NVIDIA CUDA | The runtime runs on a hardware GPU and fails at startup without one |
| Prometheus exposition format | vLLM's `/metrics` endpoint |
| `veoveo.ai/embedding-candidate-capture/v1` | Internal Knowledge-to-reference collector JSON; current closed typed members, 512 MiB raw-byte aggregate cap, original byte digest |
| RFC 3986 / foundation `ResourceUri` | Collector syntax uses hash-pinned rfc3986 2.0.0 raw component matching and rfc3986-validator 0.1.1; lowercase owner scheme, explicit authority, empty authority requires a path; original spelling is the identity |

The reference collector admits the complete current Knowledge capture through the focused `verification/capture.py` peer models before CUDA inspection, tokenizer/model loading, tensors or report creation. It checks profile identity, vector dimension and normalization, member indexes, unique selections and relevance membership. Rust serializes capture output through a 512 MiB aggregate byte cap before opening its output file. Python reads at most that cap plus one byte and refuses excess bytes before decode. This aggregate cap restricts the supported cross-product of the separate count and dimension maxima. The collector hashes the original admitted raw bytes for `captureDigest`. Capture and measurement format versions are unchanged. Pure receiver fixtures carry synthetic execution facts and cannot qualify NVIDIA inference.

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
The Helm Deployment starts without profiler instrumentation. The local diagnostic
procedure in the [verification guide](verification/README.md) may start a separate,
temporary vLLM process with Proton graph attribution enabled. vLLM's `/start_profile`
and `/stop_profile` routes bypass its API-key middleware, so that diagnostic process
must use an isolated network with no external peers. It sends one authenticated
embedding request through an in-container loopback client, then inspects the Hatchet
tree for a graph-attributed kernel in the request phase. This reports dispatch for
that batch shape; it does not count low-level graph replays or measure performance or
vector quality. The diagnostic profiler does not change the embedding serving
configuration or deployment template.

The qualified local FP16 profile uses the v0.31 image and its measured CUDA/Triton
versions. Qualification binds that execution profile and the supplied workload;
other runtime images or serving settings require their own checks.

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
`app.kubernetes.io/component` is not `computer-host`. Agent kernels
run in the separate `veoveo-agents` namespace, so the policy excludes them as well.
Requests carry the installation's embedding API key, which Helm mounts from one Secret
into the platform workloads that use embeddings. The key guards against a misconfigured
network policy; it does not identify callers.

Computers run arbitrary code, and the compute host's own egress policy already keeps
them away from cluster services. Agent kernels, Computers, and external MCP hosts embed
through the `knowledge__embed` MCP tool, which the gateway authorizes and audits like
any other tool and an agent's episode budget counts.

An explicitly reviewed first-empty Knowledge qualification overlay may admit one
additional trusted namespace to the existing GPU runtime. Ops binds that namespace's
name and UID, applies the owner-controlled label
`veoveo.ai/embedding-consumer=<namespace UID>`, and selects only its
`knowledge-mcp` component under one release label. A separate NetworkPolicy combines
that namespace selector and Pod selector in the same ingress peer on TCP 8000;
Knowledge egress selects the shared runtime namespace and workload on that port.
The fixture checks the namespace UID separately because NetworkPolicy selectors
cannot bind a UID. Computers, agent namespaces and unrelated callers gain no access.

NetworkPolicy grants are additive; Ops reviews their union with existing policies.
The native fixture binds the existing runtime namespace, Deployment and Service
identities, selected endpoint and current qualified runtime profile. Ops binds the
API-key reference and the reviewed NetworkPolicy union. It may omit a duplicate embedding
GPU workload and point Knowledge at the shared Service FQDN. The base chart and its
same-namespace access profile stay unchanged. Ops owns these temporary policies and
retires them with the isolated qualification release; it never retires the shared
runtime, model cache or API key as fixture cleanup.

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

Every vector carries its complete `EmbeddingSpace` and producer execution profile ID.
The space covers model, checkpoint revision, dimension, pooling, normalization,
effective precision and maximum input tokens. The immutable execution profile
separately covers runtime image, checkpoint manifest, measured NVIDIA environment
and effective serving configuration. Installation-selected qualification bundles bind
passing reference, retrieval, scheduling and capacity report identities to directional
query/producer compatibility. `/v1/models` checks an advertised model name only.
A consumer can reuse same-space vectors under a new execution profile only when that
profile has qualification for every retained producer. Different space semantics
require a new generation. The current installation must continue using its qualified
inputs until measured replacement bundles and the fresh-state transition are ready;
synthetic fixtures and historical reports cannot qualify newly declared profiles.

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

The qualified FP16 profile uses a 45% memory fraction on the reference GPU.
Qualification records the actual KV cache and graph dispatch for a real authenticated
request, then checks the selected profile through the
[verification guide](verification/README.md). The 32,768-token context needs at
least 3.5 GiB of KV cache; co-resident GPU workloads count against the installation's
memory budget. Reason runs in its separate acceptance batch.

Earlier local reference, scheduling and refusal results are preserved in the
[Phase 8 progress record](../../../docs/PLATFORM_FOUNDATIONS_PROGRESS.md#phase-8-knowledge-service).
The [October 2 retrieval comparison](verification/retrieval-2026-10-02.md) preserves
the corpus and model selection results; it used a fixed 4.625 GiB KV cache with a
0.90 memory fraction. Those measurements describe their recorded profiles and do
not qualify the current 45% candidate or its co-resident workload. Installed
namespace-isolation results also belong to their recorded run and do not establish
current profile capacity.

The verification guide owns regeneration and execution of current qualification
artifacts. Only a passing full production workload permits installation selection.

- On a hardware GPU the runtime becomes ready. It refuses readiness without a CUDA
  device or with a checkpoint whose digests differ from the pin.
- A reference test embeds a fixed set of queries and documents through the runtime and
  compares each vector with one produced by the model card's `transformers` recipe at
  the pinned revision. Each pair reaches cosine similarity of at least 0.999. The
  committed reference contains the actual current v2 four-input CUDA producer output
  for its selected space. Regeneration precedes complete-byte promotion.
- Client tests cover instruction formatting, request bounds, response validation, and
  the reported embedding space.
- A load test records throughput and shows interactive requests completing ahead of a
  concurrent bulk load.
- The compute host and an unadmitted namespace cannot connect. The explicit
  qualification overlay admits only its selected Knowledge release on TCP 8000;
  a request without the key is rejected.

## Implementation Map

| Path | Responsibility |
|---|---|
| `platform/runtimes/embedding/contract` | model/revision identities, checked dimensions, embedding-space equality and normalized vector admission shared below the client and Store |
| `platform/runtimes/embedding/client` | `veoveo-embedding-client`: typed requests, query formatting, priorities, bounds, validation, and `EmbeddingSpace` |
| `deploy/helm/veoveo` | runtime Deployment, model cache, init-container digest check, GPU request, NetworkPolicy, and API key Secret |
| `platform/runtimes/embedding/checkpoint.sha256` | pinned file digests for the checkpoint revision |

## Collector URI Dependencies

The collector uses the upstream whole RFC 3986 grammar before the raw component
matcher. It rejects ASCII controls and Unicode before grammar inspection. Matching
uses ASCII case-insensitivity because the upstream IPvFuture literal `v` must also
admit RFC spelling `V`. The scheme is checked separately against the foundation's
lowercase admission. The collector never repairs escapes, normalizes ports or
rewrites paths, queries or fragments. Empty-authority paths remain supported.
The upstream grammar admits zero-prefixed IPv4 octets inside IPv6 literals while
ResourceUri refuses them. After the complete grammar admits an authority, the
collector extracts only its bracketed IPv6 literal and checks it with Python's
stdlib IPv6Address. IPvFuture uses the declared URI grammar; unbracketed digit/dot
registered names are not subject to network-address validation.

The exact pure-Python wheels are declared in `verification/requirements.txt` using
the release hashes published by [rfc3986](https://pypi.org/project/rfc3986/2.0.0/)
and [rfc3986-validator](https://pypi.org/project/rfc3986-validator/0.1.1/).
The latter supplies the complete syntax predicate because rfc3986's high-level
value collapses empty authority and its authority validator narrows digit ports.
The qualification command mounts these verified wheels as a read-only dependency
overlay. The official GPU image and its inference packages keep their pins.
