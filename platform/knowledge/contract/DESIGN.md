# Knowledge Domain Contract

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| RFC 9562 | canonical RFC UUIDv7 index-generation identities |
| JSON and JSON Schema 2020-12 | closed collection approvals, generation specifications, retrieval datasets and measurements |
| SHA-256 | immutable generation and approval fingerprints |
| `ai.veoveo/knowledge-source` | collection descriptors and source observations from the extension's protocol-independent contract feature |
| [Embedding contract](../../runtimes/embedding/contract/DESIGN.md) | complete embedding-space identity and normalized vectors |

## Ownership

The knowledge service and Store share catalog and indexing types through this crate.
A separate contract prevents a dependency cycle between the MCP server and
Store, while clients can import its types without a database driver or MCP runtime.
The MCP server re-exports this contract from its library. Server-owned routes,
tools and scopes belong to that server; MCP core imports none of them.

## Catalog And Generations

`CollectionRegistration` binds a tenant, source descriptor, positive source contract
revision, installation approval and control-plane revision. Discovery obtains the source
revision from the server's contract declaration. Its SHA-256 fingerprint covers the
tenant, descriptor, source contract revision and full approval. The Store compares
this fingerprint in source-read and activation transactions. Catalog publication
separately checks the active control revision, allowing unrelated installation edits
to preserve cached chunks and generation identity.
Approval authorizes indexing by the service; it grants no caller permission to read.

`KnowledgeCollectionApproval` names the collection, its `index` or `catalog-only`
mode, 1–64 steward groups, up to 64 authoritative subjects and up to 64 allowed data
labels. `KnowledgeSubject` checks a nonempty printable topic of at most 256 UTF-8
bytes. An empty label set permits unlabelled records only. Registration validates
collection identity and rejects indexing when the source declares `none`.
`admit_observation` checks the source descriptor, tenant and label ceiling before
embedding; member construction repeats the check before storage. All approval fields
contribute to the registration fingerprint.

`KnowledgeIndexingRegistration` names the 1–1,024 collections that a machine client
may index. The gateway control-plane contract imports these generic domain types and
validates the client's authentication and profile. It imports no server-owned scope,
resource route or tool schema.

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
This constructor accepts `content` indexing. `IndexedMember::metadata` checks the same
source digest and admits chunks only from `metadata_text`: title, collection, revision,
modification time and external system/native identity. Source bodies and navigation URLs
cannot enter those chunks. `MemberTitle` bounds titles to 256 printable characters.
Generation query instructions use the embedding contract's 1,024-byte task bound.

`CollectionStatistics` carries caller-visible indexed member and chunk counts with
optional source observation and modification timestamps. Construction and JSON decoding
check that each member accounts for 1–256 chunks, that populated statistics have an
observation timestamp, and that empty statistics have no timestamps. Store computes
these values after admission; the type itself grants no read authority.

## Retrieval Evaluation

`RetrievalDataset` binds up to 4,096 corpus members and 256 judged queries. A member
names its owning collection, resource URI, source revision and content digest.
Judgments identify relevant members, and collection filters can narrow a query.
Construction rejects duplicate identities, missing judgments and references outside
the corpus. Sorting members and cases gives equivalent input orders one fingerprint.

`RetrievalEvaluation` binds the dataset to a generation specification, the evaluated
caller's policy fingerprint and the run interval. Each query records up to ten distinct
ranked members and its elapsed time. A builder requires one measurement per query;
JSON decoding applies the same checks. Recall at ten is the arithmetic mean of each
query's retrieved-relevant count divided by its judged-relevant count. Empty result
lists score zero. The retained judgments and ranks determine the score on every read.
Evaluation cannot authorize index activation or promise quality after source changes.

## Persistence

[Knowledge storage](../../store/src/knowledge/DESIGN.md) owns schema, transactional
fencing, activation and SQL candidate admission. The service owns source discovery,
enumeration, change recovery, chunk selection and the final canonical access decision.
The [service design](../../../servers/knowledge-mcp/DESIGN.md) specifies that service work.
