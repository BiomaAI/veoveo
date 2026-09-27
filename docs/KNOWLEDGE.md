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
[implementation plan](KNOWLEDGE_AND_IDENTIFIERS_PLAN.md) sequences delivery. When the
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
| candle `0.11.0`: `candle-core`, `candle-nn`, `candle-transformers` | Unmodified `candle_transformers::models::qwen3::Model` forward pass in BF16, with the `cuda` feature on `candle-core` and `candle-nn` |
| `tokenizers` `0.23.2` | The Qwen3 tokenizer from the pinned model revision |
| [`Qwen/Qwen3-Embedding-0.6B`](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B), revision `97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3` | Apache-2.0 embedding model: 28 layers, 1024-dimension output, 32,768-token context, last-token pooling, L2 normalization |
| NVIDIA CUDA | Embedding runs on a hardware GPU and fails closed without one |
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

The service embeds with `Qwen/Qwen3-Embedding-0.6B` through candle's unmodified Qwen3
model. The service owns a small embedder module around it and no model code. The
candle example
[`gte-qwen`](https://github.com/huggingface/candle/blob/main/candle-examples/examples/gte-qwen/main.rs)
implements the same recipe for the Qwen2-based gte models and is the reference for
this module. The recipe follows the
[model card](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B):

1. Load `config.json` into `candle_transformers::models::qwen3::Config` and the
   BF16 `model.safetensors` into `qwen3::Model::new`. The checkpoint stores keys
   without the `model.` prefix that `Model::new` requests, such as
   `embed_tokens.weight` and `norm.weight`, so the loader maps the prefix away when
   it builds the `VarBuilder`. The model's tied `lm_head` is never loaded.
2. Tokenize with `tokenizer.json` from the same revision. Every input ends with
   exactly one `<|endoftext|>` token, ID 151643. The embedder appends it unless the
   pinned tokenizer's post-processor already does, and it asserts the result.
3. Documents embed as plain text. A query embeds as
   `Instruct: {task}\nQuery:{query}`, with no space after `Query:`. The task is
   `Given a web search query, retrieve relevant passages that answer the query`
   unless qualification selects another, and the index generation records it.
4. Group chunks into batches of identical token length. Candle's `qwen3::Model::forward`
   takes no padding mask, so a padded batch would let real tokens attend to padding.
   Equal-length batches need no padding and produce the same vectors as single-input
   calls. A query always embeds alone.
5. Call `clear_kv_cache()` before each batch, then `forward(&input_ids, 0)`. The
   result has shape `[batch, tokens, 1024]` after the model's final RMSNorm.
6. Take the hidden state at the last position, which is the appended
   `<|endoftext|>` token, and L2-normalize it into the stored vector.

The chunker cuts text to a fixed token budget, so most chunks share one length and
fill whole batches. Only the final chunk of each member varies. Short, varied records
such as events and features form smaller batches, which lowers indexing throughput but
leaves vectors unchanged.

If measured throughput cannot keep up with a full index rebuild, the fix is a padding
mask for `qwen3::Model::forward`, contributed upstream to candle to match
`qwen2::Model`. Padded batches then replace equal-length batches behind the same
embedder interface. Maintaining a private copy of the model code is not the fix.

The container requests `nvidia.com/gpu` and opens the CUDA device before it reports
ready. A missing device, driver capability, or CUDA context is a startup failure.
The service has no CPU embedding path. The image build compiles candle's CUDA kernels
with the CUDA toolkit and sets `CUDA_COMPUTE_CAP` for the target GPUs, because the
builder has no GPU to detect.

Model files are pinned by Hugging Face revision and SHA-256 and packaged into the
image. The service never downloads a model at runtime and depends on no Hugging Face
client crate.

An index generation records the model ID, revision, file digests, dimension, query
instruction, and chunker version. A change to any of them builds a new generation
beside the active one. The active pointer moves when the new generation covers every
approved collection, and vectors from different generations never share an index.

### Model qualification

Qualification compares `Qwen3-Embedding-0.6B` with `Qwen3-Embedding-4B` through the
same embedder. The 4B model has a 2560-dimension output and needs a matching index
generation. The comparison records recall at 10 on the evaluation set, GPU memory,
and indexing throughput in chunks per second. The service ships 0.6B unless 4B shows a
retrieval gain that justifies its memory on the installation's shared GPUs. A model of
8B or more would move embedding to a batching inference server such as the vLLM runner
that `reason-mcp` already uses. That move requires its own decision.

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
| `src/embed/` | CUDA device admission, pinned model loading, instruction formatting, equal-length batching, last-token pooling, and normalization |
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
- The embedding tests run on a hardware GPU. They prove that startup fails without
  a CUDA device and that the loaded model matches its pinned digests.
- A reference test embeds a fixed set of queries and documents and compares each
  vector with one produced by the model card's `transformers` recipe at the pinned
  revision. Each pair reaches cosine similarity of at least 0.999. The reference
  vectors are a committed fixture, generated once with `uv run`.
- A batching test proves that equal-length batches and single-input calls produce the
  same vectors within that tolerance.
- The throughput measurement records chunks per second for each Phase 7 collection and
  for a full rebuild.
- The evaluation set measures recall at 10 for the qualified chunk settings, and
  each index generation records its result.
