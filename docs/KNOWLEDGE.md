# Knowledge Sharing

Veoveo servers can share knowledge through MCP resources. An adopting server stays
the system of record for its domain and publishes selected collections through the
[`ai.veoveo/knowledge-source`](../mcp/knowledge-extension/DESIGN.md) extension. The
gateway records what each caller observed. The `knowledge-mcp` server catalogs every
declared collection and indexes the approved ones for search.

Knowledge publication is optional. Choose sources for the questions users need to
answer across tasks; operational resources do not need a knowledge collection merely
because they exist. Every declared collection must satisfy the extension's access,
provenance and freshness rules.

`knowledge-mcp` answers two questions for agents and people: where does the
knowledge about a subject live, and how current is what Veoveo has seen of it.
Search returns links to the owning server's resources, and every result states the
freshness of the cached revision behind it.

## Status

The extension crate, Rust, Python and Node document adapters, gateway read auditing,
and kernel provenance are implemented. Store implements approval fingerprints,
generation-specific indexes, fenced member replacement and SQL-selected candidate
pages through the [knowledge storage contract](../platform/store/src/knowledge/DESIGN.md).
Installed qualification is in progress.
The `knowledge-mcp` library implements chunking, generation building, hybrid search and
source-listener reconciliation under Store leases. Its authenticated HTTP adapter serves
search, embeddings and catalog resources. Its gateway adapter discovers approved
collections and waits for source observation readiness. Machine connection wiring,
atomic catalog publication and reconnection run in the binary's tenant workers.
Machine-client provisioning, packaging and installed qualification remain in the
[implementation plan](PLATFORM_FOUNDATIONS_PLAN.md).

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
| [Embedding runtime](../platform/runtimes/embedding/DESIGN.md) | Shared vLLM service for `Qwen/Qwen3-Embedding-0.6B`, reached through `veoveo-embedding-client` |
| [JSON Schema 2020-12](https://json-schema.org/draft/2020-12/) | Generated schemas for every tool input, output, and resource body |

## Service Ownership

The [Knowledge service design](../servers/knowledge-mcp/DESIGN.md) owns source approval,
catalog resources, indexing, retrieval, embedding tools and service qualification.
Domain servers retain their data and read authorization. The service reads approved
collections through the gateway and maintains a searchable cache of observed revisions.

## Knowledge Reads In The Audit Log

Veoveo keeps [one audit log](AUDIT.md). A read of a declared collection is an ordinary
audit record for `resources/read`, and the record carries the typed observation the owning
server returned: collection, revision, `contentSha256`, `modifiedAt`, `modifiedBy`,
and whether the read returned content or confirmed an unchanged revision. Denied
and failed reads record an outcome and reason without claiming source provenance.
The record already names the actor, delegating principal, managed
agent, Work Context, profile, and trace. "What did this answer rely on?" is therefore
an audit query by trace or agent episode.

The gateway declares the extension on each upstream read of a declaring server and
forwards the observation only to callers that declared the extension themselves. For
these reads it commits the audit record after the upstream response arrives and before
it returns the result, so no caller receives content whose revision the log lacks.
Denied reads are audited before any upstream call, as every denial is. The record holds
digests and identities, never member content, and follows the audit log's retention.
The audit observation excludes external navigation URLs, which can carry signed
credentials; external system and native record identities remain available for lookup.
Reads by the knowledge service itself use the indexing window described in
[source approval](../servers/knowledge-mcp/DESIGN.md#sources-and-approval).

## Agent Context

The governed agent read adapter declares the extension on its reads and keeps each
observation beside the text it admits.
The model receives one compact provenance line per item: collection, revision,
`modifiedAt`, and `observedAt`. It can therefore state how current its evidence
is. The adapter's byte and read budgets count those lines.
