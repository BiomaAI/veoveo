# Knowledge Domain Contract

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| RFC 9562 | canonical RFC UUIDv7 index-generation identities |
| JSON and JSON Schema 2020-12 | closed collection approvals and generation specifications |
| SHA-256 | immutable generation and approval fingerprints |
| `ai.veoveo/knowledge-source` | collection descriptors and source observations from the extension's protocol-independent contract feature |
| [Embedding contract](../../runtimes/embedding/contract/DESIGN.md) | complete embedding-space identity and normalized vectors |

## Ownership

The knowledge service and Store share catalog and indexing types through this crate.
A separate contract prevents a dependency cycle between the future MCP server and
Store, while clients can import its types without a database driver or MCP runtime.
The MCP server will re-export this contract from its library. Server-owned routes,
tools and scopes belong to that server; MCP core imports none of them.

## Catalog And Generations

`CollectionRegistration` binds a tenant, source descriptor, installation approval and
control-plane revision. Its SHA-256 fingerprint changes when any of these values
changes. The Store compares this fingerprint in source-read and activation transactions.
Approval authorizes indexing by the service; it grants no caller permission to read.

`GenerationSpec` binds an embedding space, query task, chunker version and character
settings, and 1–1,024 collection fingerprints. A new specification creates a new
generation. Chunk limits allow at most 8,192 characters, and overlap must be smaller
than the cap. Runtime model qualification may select tighter settings.

`IndexedChunk::from_range` selects a complete UTF-8 range within a source body and
checks its size and vector space. Chunks have no deserialization shortcut around
that constructor. `IndexedMember::new` admits a complete source
observation, verifies its content digest and collection/tenant relationships, and
checks every chunk against the source and generation. It accepts at most 256 chunks
from a 64 KiB source item. Its retained generation fingerprint prevents attaching an
admitted member to a read ticket for another embedding space or chunker configuration.
This constructor accepts `content` indexing. Metadata-only ingestion requires its own
checked construction when that collection profile is implemented.

## Persistence

[Knowledge storage](../../store/src/knowledge/DESIGN.md) owns schema, transactional
fencing, activation and SQL candidate admission. The service owns source discovery,
enumeration, change recovery, chunk selection and the final canonical access decision.
The [knowledge design](../../../docs/KNOWLEDGE.md) specifies that service work.
