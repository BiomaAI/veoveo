# Knowledge Sharing

Veoveo servers share knowledge through MCP resources. Each server stays the system
of record for its domain and publishes collections through the
[`ai.veoveo/knowledge-source`](../mcp/knowledge-extension/DESIGN.md) extension. The
gateway records what each caller observed. The `knowledge-mcp` server catalogs every
declared collection and indexes the approved ones for search.

`knowledge-mcp` answers two questions for agents and people: where does the
knowledge about a subject live, and how current is what Veoveo has seen of it.
Search returns links to the owning server's resources, and every result states the
freshness of the cached revision behind it.

## Status

Designed. Neither the extension crate nor `knowledge-mcp` exists. The
[implementation plan](PLATFORM_FOUNDATIONS_PLAN.md) sequences delivery. When the
`servers/knowledge-mcp` crate is created, the service sections of this document move
into its `DESIGN.md`, and this document keeps the cross-component flow.

```text
crate       veoveo-knowledge-mcp
folder      servers/knowledge-mcp
slug        knowledge
URI scheme  knowledge
MCP         /knowledge/mcp
admin REST  /knowledge/admin
health      /knowledge/healthz
```

## Standards And Protocols

| Standard or protocol | Profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/draft) `2026-07-28` | Hosted-server profile under [contract revision 3](../mcp/contract/DESIGN.md): tools, resources, templates, completion, `subscriptions/listen`, and official Tasks |
| `ai.veoveo/knowledge-source` | Consumed as a client on every source read; declared as a server for the `knowledge.docs` collection |
| [W3C DCAT 3](https://www.w3.org/TR/vocab-dcat-3/) | Catalog shape: the installation catalog is a `dcat:Catalog`, each source server a `dcat:DataService`, each collection a `dcat:Dataset`. Resources return JSON with DCAT-aligned field names, not RDF |
| [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html) and [RFC 9111](https://www.rfc-editor.org/rfc/rfc9111.html) | Revalidation with strong validators and freshness lifetimes, applied through the extension's conditional reads |
| SurrealDB 3.3 | Catalog and index records in the platform store; `FULLTEXT` BM25 and `HNSW` vector indexes |
| [vLLM 0.30.0](https://github.com/vllm-project/vllm/releases/tag/v0.30.0) | The official `vllm/vllm-openai:v0.30.0` image, pinned by the same OCI digest `reason-mcp` uses, serving the embedding model with the pooling runner |
| [OpenAI Embeddings API](https://platform.openai.com/docs/api-reference/embeddings), as implemented by vLLM | Internal adapter protocol between `knowledge-mcp` and the embedding runtime: `POST /v1/embeddings` over cluster-internal HTTP; not a public contract |
| [`Qwen/Qwen3-Embedding-0.6B`](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B), revision `97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3` | Apache-2.0 embedding model: 28 layers, 1024-dimension output, 32,768-token context, last-token pooling, L2 normalization |
| NVIDIA CUDA | The embedding runtime runs on a hardware GPU and fails closed without one; `knowledge-mcp` needs no GPU |
| [JSON Schema 2020-12](https://json-schema.org/draft/2020-12/) | Generated schemas for every tool input, output, and resource body |

## Sources And Approval

The service discovers candidate sources from the gateway catalog: every server in
its profile whose `server/discover` result declares `ai.veoveo/knowledge-source`.
Discovery alone does not admit a source.

The control-plane document approves each collection with a typed knowledge entry.
The entry names the collection, whether the service may index it, the groups that
steward it, the subjects for which it is authoritative, and the data labels it may
contain. An installation can therefore catalog a collection without indexing it,
or refuse a collection whose labels exceed the service's clearance.

The service reads sources through the gateway as its own registered machine client.
Control-plane policy grants that client read access to approved collections only.
The gateway authorizes and audits those reads exactly as it does for any other
caller.

## Catalog

| Resource | Content |
|---|---|
| `knowledge://sources{?cursor}` | Paged `DataService` entries: server slug, contract revision, and declared collections |
| `knowledge://source/{server}` | One source with its collections and approval state |
| `knowledge://collection/{collection}` | One `Dataset`: descriptor, approval, index generation, member count, newest observation, and last change event |
| `knowledge://docs`, `knowledge://docs/{doc_id}`, `knowledge://contract` | Well-known surface |

Collection resources are subscribable, and the service notifies them as indexing
progresses.

## Index

The index is a cache over approved `content` and `metadata` collections. For each
collection the service enumerates members through the declared `enumerate` pages,
reads each member with the extension declared, and stores chunks keyed by member URI
and revision. `listen` collections then stay current through `subscriptions/listen`.
`revalidate` collections are revalidated after their `maxAgeSeconds`. Immutable
members are read once per revision.

A change notification marks the member's chunks stale. The service then re-reads the
member and replaces its chunks with ones keyed by the new revision. A lost event
stream triggers reconciliation by conditional reads over the collection's pages. A
`not_found` or unavailable read leaves the chunks marked stale; the service removes
them only after a change event, a newer revision, or a definitive deletion from the
owning server. The chunk records are the service's whole account of what it cached
and from which revision.

Chunks carry text, member URI, revision, collection, and the observation's access
descriptor. Each chunk has a BM25 `FULLTEXT` entry and a 1024-dimension `HNSW`
entry with cosine distance. Search runs both indexes and fuses their rankings with
reciprocal rank fusion.

### Embedding

A dedicated embedding runtime serves `Qwen/Qwen3-Embedding-0.6B` with vLLM, and
`knowledge-mcp` calls it over cluster-internal HTTP. vLLM owns tokenization, batching,
padding, pooling, and GPU scheduling, so Veoveo writes no model code. The runtime is a
separate deployment because it is a GPU workload with its own image, scaling, and
failure behavior, and other components can share it later.

The runtime runs the official `vllm/vllm-openai` image at the digest `reason-mcp`
pins, with no Veoveo code in it:

```text
VLLM_API_KEY=<from Secret> HF_HUB_OFFLINE=1 \
vllm serve /models/qwen3-embedding-0.6b --runner pooling \
  --served-model-name qwen3-embedding-0.6b
```

`--runner pooling` is required because the checkpoint declares `Qwen3ForCausalLM`;
without it vLLM loads a generative model and mounts no `/v1/embeddings` route. The
pooler uses last-token pooling with L2 normalization, which the checkpoint's
sentence-transformers configuration declares. Qualification confirms that vLLM applies
it, and the deployment sets the pooler configuration explicitly if it does not.

The checkpoint follows the `reason-mcp` model-cache pattern: the installation supplies
the Hugging Face layout at revision `97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3` on a
model-cache volume, an init container checks every file against its pinned SHA-256
with `sha256sum -c` before vLLM starts, and `HF_HUB_OFFLINE=1` forbids downloads. The Pod requests `nvidia.com/gpu`, and vLLM
fails at startup without a CUDA device, so the runtime has no CPU path.

`knowledge-mcp` owns the inputs. Documents embed as plain text. A query embeds as
`Instruct: {task}\nQuery:{query}`, with no space after `Query:`. The task is
`Given a web search query, retrieve relevant passages that answer the query` unless
qualification selects another, and the index generation records it. The service
sends chunks in requests of bounded size, and vLLM batches concurrent requests on the
GPU.

Only `knowledge-mcp` may reach the runtime. A NetworkPolicy admits its Pods, and each
request carries the runtime's API key from an installation Secret. The runtime has no
public route. Its `/health` endpoint backs readiness, and its `/metrics` endpoint
exports vLLM's Prometheus metrics.

An index generation records the model ID, revision, file digests, dimension, query
instruction, vLLM image digest, and chunker version. A change to any of them builds a
new generation beside the active one. The active pointer moves when the new generation
covers every approved collection, and vectors from different generations never share
an index.

### Model qualification

Qualification compares `Qwen3-Embedding-0.6B`, `4B`, and `8B` through the same
runtime by changing only the checkpoint. Their outputs have 1024, 2560, and 4096
dimensions, and each needs a matching index generation. The comparison records recall
at 10 on the evaluation set, GPU memory, and indexing throughput in chunks per second.
The service ships 0.6B unless a larger model shows a retrieval gain that justifies its
memory on the installation's shared GPUs.

## Search

`search` is a direct tool. Its input takes a query, optional collections and entity
kinds, and a result limit of at most 20. Its output lists results with member URI,
title, a snippet of at most 320 characters, the fused score, and a freshness summary: revision,
`lastModified`, `observedAt`, and whether the cached revision has outlived its
collection's freshness. Its content carries one `resource_link` per result.

Before returning a result, the service applies the caller's effective access to the
chunk's access descriptor: same tenant, Work Context membership or a grant with
`read`, and clearance for every data label. It evaluates the same predicate that the
Artifact store applies. `profile` collections require only that the caller's profile
exposes the source server. A result the caller may not read never appears, including
its title and snippet.

A caller who needs current content reads the member URI. That read goes to the
owning server, which applies its own authorization and returns a fresh observation.

## Knowledge Reads In The Audit Log

Veoveo keeps [one audit log](AUDIT.md). A read of a declared collection is an ordinary
audit record for `resources/read`, and the event carries the typed observation the owning
server returned: collection, revision, `contentSha256`, `lastModified`, `modifiedBy`,
and the read outcome (`full`, `not_modified`, `denied`, `not_found`, or
`unavailable`). The event already names the actor, delegating principal, managed
agent, Work Context, profile, and trace. "What did this answer rely on?" is therefore
an audit query by trace or agent episode.

The gateway declares the extension on each upstream read of a declaring server and
forwards the observation only to callers that declared the extension themselves. For
these reads it commits the audit event after the upstream response arrives and before
it returns the result, so no caller receives content whose revision the log lacks.
Denied reads are audited before any upstream call, as every denial is. The event holds
digests and identities, never member content. Retention follows the audit log's
policy for its event kind.

## Agent Context

The governed agent read adapter keeps each observation beside the text it admits.
The model receives one compact provenance line per item: collection, revision,
`lastModified`, and `observedAt`. It can therefore state how current its evidence
is. The adapter's byte and read budgets count those lines.

## Implementation Map

| Path | Responsibility |
|---|---|
| `src/catalog/` | gateway discovery, control-plane approval, and DCAT-shaped resources |
| `src/index/` | enumeration, change subscriptions, reconciliation, chunking, and index generations |
| `src/embed/` | typed client for the embedding runtime, query instruction formatting, request sizing, and response validation |
| `deploy/helm/veoveo` | the embedding runtime Deployment, model cache, GPU request, NetworkPolicy, and API key Secret |
| `src/search.rs` | hybrid query, rank fusion, effective-access filtering, and result links |
| `platform/store/src/knowledge.rs` | typed catalog, chunk, and index-generation records |
| `platform/gateway/src/mcp/resources.rs` | observation attached to the read's audit event |
| `agents/kernel/src/resource.rs` | observation retention and provenance lines in model context |

## Verification

- Contract rules K01 through K10 pass against this server's `knowledge.docs`
  collection.
- Unit and store tests prove that a caller never receives a result outside its
  effective access, including title and snippet.
- Gateway tests prove that a declared read's audit event carries its observation and
  outcome, and that it commits before the result returns.
- Change-event tests prove invalidation, re-read, and reconciliation after a lost
  stream.
- The embedding runtime runs on a hardware GPU. A test proves that it fails to become
  ready without a CUDA device or with a checkpoint whose digests differ from the pin.
- A reference test embeds a fixed set of queries and documents through the runtime and
  compares each vector with one produced by the model card's `transformers` recipe at
  the pinned revision. Each pair reaches cosine similarity of at least 0.999. The
  reference vectors are a committed fixture, generated once with `uv run`.
- The throughput measurement records chunks per second for each Phase 7 collection and
  for a full rebuild.
- The evaluation set measures recall at 10 for the qualified chunk settings, and
  each index generation records its result.
