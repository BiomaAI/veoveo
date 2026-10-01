# Audit Record Contract

## Standards And Protocols

| Boundary | Profile |
|---|---|
| RFC 9562 | Canonical UUIDv7 record and HTTP request identities |
| W3C Trace Context | Nonzero lowercase 32-hex trace IDs and 16-hex span IDs |
| RFC 9162, RFC 8785 and RFC 8032 | Typed block heads, decimal-string versionstamps, Merkle roots and Ed25519 signatures |
| RFC 3339 and JSON | Typed timestamps, tagged closed enums and checked deserialization |
| `veoveo.ai/audit-record/v1` | One reviewed record shape across platform producers and readers |
| `ai.veoveo/knowledge-source` | Reviewed observation fields from the protocol-independent extension contract |

This library owns the protocol-independent audit vocabulary. It depends on foundational
names and the lightweight Artifact and Computers identities. Store and MCP both depend
on this crate. The writer depends on Store; separating this contract avoids a Cargo
cycle and prevents persistence from importing the MCP runtime.

The draft builder derives the partition from the authenticated actor. Accepted outcomes
require the accepted reason. Serialization and input admission share these checks.
Details expose only reviewed enums, identifiers, digests, counters and timestamps.
`tests/detail_schema.rs` walks the generated detail schema and rejects free text,
open maps and unreviewed string wrappers. It also recognizes the reviewed, inlined
SHA-256 pattern. These identifier allowlists belong to this contract and must be
reviewed when a detail gains a new identifier type.
Numeric counters must fit the exact I-JSON integer range before a draft can be stored.
Tool, discovery and Task details must match the corresponding typed target. A platform
Task uses its UUID record link; an external Task uses its opaque gateway route and
server identity. These identities have separate variants.
Knowledge read details bind the member and collection owner to their resource target.
Successful reads require an observation whose conditional status matches the outcome.
Denied or failed reads cannot claim a returned revision. `KnowledgeReadObservation`
copies source identity, revision, digest, times, attribution and access fields. Its
external identity excludes navigation URLs because they can carry signed query
credentials. The closed audit schema rejects those URLs on input as well.
Store owns conversion to compound record IDs and native record links. A partition key
distinguishes the installation partition from a tenant literally named `installation`.

The reader's policy owner supplies an `AuditReadScope`. Store checks it before query
execution and includes its admitted partition in SQL before decoding or limiting rows.
Cursors carry their partition and ordering and cannot move a reader to another tenant. Daily counts
use UTC day bounds and a keyset of day, class and outcome; readers can traverse every
page without silently truncating at a fixed total.

The audit domain owns `AuditScope::Read`. Protocol cores carry its `ScopeName` without
defining the domain vocabulary. Gateway readers check that scope on the current actor.
The actor's tenant selects its tenant partition; the `administrator` and `auditor`
roles also admit installation records.

`AuditRecordSummary` keeps typed targets and activities through browser delivery.
`reader_schema` exports this projection, page queries, daily counts and view receipts
for the existing client generator. A view receipt refers to a committed access record;
it grants no authority. Each read verifies the current actor, profile, selected
partition and receipt lifetime. JSON Lines exports contain a header, records and a
completion footer. Readers require that footer before accepting a complete download.

`AuditContext` carries verified actor, request correlation and invocation authority into
a domain operation. The MCP adapter derives it from `GatewayRequestContext` only after
validating the source-principal, actor and invocation relationship. Domain services keep
that attribution for background expiry and terminal records. Dictation summaries expose
an end reason, accepted chunk count and audio duration; they cannot carry transcript text.

Implementation and qualification are in progress under the
[foundations audit pass](../../../docs/PLATFORM_FOUNDATIONS_PLAN.md#phase-4-unified-audit-log).

Destination IDs wrap configuration hashes separately from content digests. Export
intents bind both content and signed-block hashes. Closed rejection codes carry no
provider response text and survive replica changes.
